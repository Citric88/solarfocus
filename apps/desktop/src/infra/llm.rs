#![cfg(feature = "llm")]
//! `LlmRuntime` — minimal wrapper around `llama-cpp-2` 0.1.x.
//!
//! Threading model:
//! - Heavy `LlamaModel::load_from_file` and `decode` calls run on
//!   `tokio::task::spawn_blocking` so the iced/tokio scheduler isn't blocked.
//! - We hold a single backend (cell-init) — llama.cpp requires this.
//! - Inference is serialized via a `tokio::sync::Mutex` because llama-cpp's
//!   context isn't safe to use concurrently.
//!
//! This wrapper covers what Phase 3 needs: load a model, generate ≤200
//! tokens against a fully-formed prompt string, return a String.

use llama_cpp_2::{
    context::params::LlamaContextParams,
    llama_backend::LlamaBackend,
    llama_batch::LlamaBatch,
    model::{params::LlamaModelParams, AddBos, LlamaModel},
    sampling::LlamaSampler,
};
use std::num::NonZeroU32;
use std::path::Path;
use std::sync::{Arc, OnceLock};
use tokio::sync::Mutex;

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("backend init: {0}")]
    Backend(String),
    #[error("model load: {0}")]
    ModelLoad(String),
    #[error("inference: {0}")]
    Inference(String),
    #[error("tokenization: {0}")]
    Tokenization(String),
}

#[derive(Debug, Clone, Copy)]
pub struct LoadOpts {
    pub n_ctx: u32,
    pub n_threads: i32,
    pub use_mmap: bool,
    pub gpu_layers: u32,
}

impl Default for LoadOpts {
    fn default() -> Self {
        let cores = std::thread::available_parallelism()
            .map(|n| n.get() as i32)
            .unwrap_or(4);
        Self {
            n_ctx: 2048,
            n_threads: (cores / 2).max(2),
            use_mmap: true,
            // gpu_layers: 0 on CPU build; metal/cuda features auto-offload anyway.
            // Use a high number so all layers get offloaded when GPU build is enabled.
            gpu_layers: 999,
        }
    }
}

static BACKEND: OnceLock<Arc<LlamaBackend>> = OnceLock::new();

fn shared_backend() -> Result<Arc<LlamaBackend>, LlmError> {
    if let Some(b) = BACKEND.get() {
        return Ok(b.clone());
    }
    let backend = LlamaBackend::init().map_err(|e| LlmError::Backend(e.to_string()))?;
    let arc = Arc::new(backend);
    let _ = BACKEND.set(arc.clone());
    Ok(arc)
}

/// A loaded model + a single-inference Mutex around its context creation.
pub struct LlmRuntime {
    model: Arc<LlamaModel>,
    opts: LoadOpts,
    inference_lock: Arc<Mutex<()>>,
    backend: Arc<LlamaBackend>,
}

impl LlmRuntime {
    /// Load a GGUF model from disk. Heavy — runs on a blocking task.
    pub async fn load(path: &Path, opts: LoadOpts) -> Result<Self, LlmError> {
        let path = path.to_path_buf();
        tokio::task::spawn_blocking(move || Self::load_blocking(&path, opts))
            .await
            .map_err(|e| LlmError::ModelLoad(format!("join error: {e}")))?
    }

    fn load_blocking(path: &Path, opts: LoadOpts) -> Result<Self, LlmError> {
        let backend = shared_backend()?;
        let mut params = LlamaModelParams::default();
        params = params.with_n_gpu_layers(opts.gpu_layers);
        let model = LlamaModel::load_from_file(&backend, path, &params)
            .map_err(|e| LlmError::ModelLoad(e.to_string()))?;
        Ok(Self {
            model: Arc::new(model),
            opts,
            inference_lock: Arc::new(Mutex::new(())),
            backend,
        })
    }

    /// Run inference. `prompt` is the full prompt incl. chat-template wrappers.
    /// Returns the assistant turn (token text) up to `max_tokens` or natural EOS.
    pub async fn generate(&self, prompt: String, max_tokens: usize) -> Result<String, LlmError> {
        let model = self.model.clone();
        let backend = self.backend.clone();
        let opts = self.opts;
        let lock = self.inference_lock.clone();

        let _guard = lock.lock_owned().await;
        let result = tokio::task::spawn_blocking(move || {
            generate_blocking(&backend, &model, opts, &prompt, max_tokens)
        })
        .await
        .map_err(|e| LlmError::Inference(format!("join error: {e}")))?;

        result
    }
}

fn generate_blocking(
    backend: &LlamaBackend,
    model: &LlamaModel,
    opts: LoadOpts,
    prompt: &str,
    max_tokens: usize,
) -> Result<String, LlmError> {
    let n_ctx = NonZeroU32::new(opts.n_ctx).unwrap_or(NonZeroU32::new(2048).unwrap());
    let ctx_params = LlamaContextParams::default()
        .with_n_ctx(Some(n_ctx))
        .with_n_threads(opts.n_threads);

    let mut ctx = model
        .new_context(backend, ctx_params)
        .map_err(|e| LlmError::Inference(e.to_string()))?;

    let tokens = model
        .str_to_token(prompt, AddBos::Always)
        .map_err(|e| LlmError::Tokenization(e.to_string()))?;

    let n_prompt = tokens.len();
    if n_prompt as u32 >= opts.n_ctx {
        return Err(LlmError::Inference(format!(
            "prompt too long ({} tokens) for n_ctx={}",
            n_prompt, opts.n_ctx
        )));
    }

    // Feed prompt tokens.
    let mut batch = LlamaBatch::new(opts.n_ctx as usize, 1);
    let last_idx = tokens.len() - 1;
    for (i, t) in tokens.iter().enumerate() {
        batch
            .add(*t, i as i32, &[0], i == last_idx)
            .map_err(|e| LlmError::Inference(e.to_string()))?;
    }
    ctx.decode(&mut batch)
        .map_err(|e| LlmError::Inference(e.to_string()))?;

    // Sampler: greedy + temperature; small models like SmolLM2 generate sensible
    // text with very low temperature.
    let mut sampler = LlamaSampler::chain(
        vec![
            LlamaSampler::temp(0.7),
            LlamaSampler::top_k(40),
            LlamaSampler::top_p(0.9, 1),
            LlamaSampler::dist(1234),
        ],
        true,
    );

    let mut out = String::new();
    let mut n_decoded: i32 = n_prompt as i32;

    for _ in 0..max_tokens {
        let token = sampler.sample(&ctx, -1);
        sampler.accept(token);

        if model.is_eog_token(token) {
            break;
        }

        // Convert token to text and append.
        // Convert token → bytes → utf-8 string via the non-deprecated bytes API.
        let bytes = model
            .token_to_piece_bytes(token, 32, false, None)
            .unwrap_or_default();
        let piece = String::from_utf8_lossy(&bytes).to_string();
        out.push_str(&piece);

        // BUG-C — stop on natural sentence end so coaching messages
        // never get cut mid-word. Wait until at least one sentence has
        // formed (out.len() > 30 chars), then break on . ! ? \n.
        if out.len() > 30 {
            let trimmed = out.trim_end();
            let last = trimmed.chars().last();
            let ended_with_terminator = matches!(last, Some('.') | Some('!') | Some('?') | Some('。'));
            if ended_with_terminator || piece.contains('\n') || piece.contains("</s>") {
                break;
            }
        }

        // Feed token back for next step.
        batch.clear();
        batch
            .add(token, n_decoded, &[0], true)
            .map_err(|e| LlmError::Inference(e.to_string()))?;
        ctx.decode(&mut batch)
            .map_err(|e| LlmError::Inference(e.to_string()))?;
        n_decoded += 1;
    }

    Ok(out.trim().to_string())
}
