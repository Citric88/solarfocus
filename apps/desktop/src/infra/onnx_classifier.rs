#![cfg(feature = "classifier")]
//! ONNX-based DistilBERT distraction classifier (Phase 4).
//!
//! Loads a quantized INT8 ONNX file + tokenizer.json from
//! `<data_dir>/SolarFocus/models/distilbert/`. If either file is missing,
//! `OnnxClassifier::try_load()` returns Err and the App falls back to
//! `RulesClassifier`.
//!
//! The actual model + tokenizer aren't bundled in the binary; they're
//! either downloaded by `infra::model_download` (Phase 4 extension —
//! not yet wired) or dropped in by hand. This keeps the binary small.

use directories::ProjectDirs;
use ndarray::{Array1, Array2};
use ort::session::{Session, builder::GraphOptimizationLevel};
use ort::value::TensorRef;
use solar_focus_intelligence::{
    AiFuture, ClassificationLabel, ClassificationResult, DistractionClassifier, WindowSample,
};
use std::path::PathBuf;
use std::sync::Mutex;
use tokenizers::Tokenizer;

const MAX_LEN: usize = 64;

#[derive(Debug, thiserror::Error)]
pub enum OnnxError {
    #[error("ONNX model not found at {0}")]
    ModelMissing(PathBuf),
    #[error("tokenizer not found at {0}")]
    TokenizerMissing(PathBuf),
    #[error("ort: {0}")]
    Ort(String),
    #[error("tokenizer: {0}")]
    Tokenizer(String),
    #[error("inference: {0}")]
    Inference(String),
}

pub struct OnnxClassifier {
    session: Mutex<Session>,
    tokenizer: Tokenizer,
    /// Output index → semantic label. For our binary fine-tune:
    /// 0 = Focus, 1 = Distraction.
    label_map: [ClassificationLabel; 2],
}

impl OnnxClassifier {
    pub fn default_model_dir() -> PathBuf {
        if let Some(p) = ProjectDirs::from("os", "SolarFocus", "SolarFocus") {
            p.data_dir().join("models").join("distilbert")
        } else {
            PathBuf::from("models/distilbert")
        }
    }

    pub fn try_load() -> Result<Self, OnnxError> {
        let dir = Self::default_model_dir();
        let model_path = dir.join("model.onnx");
        let tok_path = dir.join("tokenizer.json");

        if !model_path.exists() {
            return Err(OnnxError::ModelMissing(model_path));
        }
        if !tok_path.exists() {
            return Err(OnnxError::TokenizerMissing(tok_path));
        }

        let session = Session::builder()
            .map_err(|e| OnnxError::Ort(e.to_string()))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| OnnxError::Ort(e.to_string()))?
            .with_intra_threads(2)
            .map_err(|e| OnnxError::Ort(e.to_string()))?
            .commit_from_file(&model_path)
            .map_err(|e| OnnxError::Ort(e.to_string()))?;

        let tokenizer = Tokenizer::from_file(&tok_path)
            .map_err(|e| OnnxError::Tokenizer(e.to_string()))?;

        log::info!("OnnxClassifier loaded from {}", dir.display());
        Ok(Self {
            session: Mutex::new(session),
            tokenizer,
            label_map: [ClassificationLabel::Focus, ClassificationLabel::Distraction],
        })
    }

    fn classify_text(&self, text: &str) -> Result<ClassificationResult, OnnxError> {
        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| OnnxError::Tokenizer(e.to_string()))?;

        let mut ids: Vec<i64> = encoding.get_ids().iter().map(|&i| i as i64).collect();
        let mut mask: Vec<i64> = encoding.get_attention_mask().iter().map(|&i| i as i64).collect();
        ids.truncate(MAX_LEN);
        mask.truncate(MAX_LEN);
        while ids.len() < MAX_LEN {
            ids.push(0);
            mask.push(0);
        }

        let ids_arr = Array2::from_shape_vec((1, MAX_LEN), ids)
            .map_err(|e| OnnxError::Inference(e.to_string()))?;
        let mask_arr = Array2::from_shape_vec((1, MAX_LEN), mask)
            .map_err(|e| OnnxError::Inference(e.to_string()))?;

        let ids_tensor = TensorRef::from_array_view(ids_arr.view())
            .map_err(|e| OnnxError::Inference(e.to_string()))?;
        let mask_tensor = TensorRef::from_array_view(mask_arr.view())
            .map_err(|e| OnnxError::Inference(e.to_string()))?;

        let mut session = self.session.lock().unwrap();
        let outputs = session
            .run(ort::inputs![
                "input_ids" => ids_tensor,
                "attention_mask" => mask_tensor,
            ])
            .map_err(|e| OnnxError::Inference(e.to_string()))?;

        // Expect first output to be logits of shape [1, 2].
        let (_shape, logits) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| OnnxError::Inference(e.to_string()))?;
        if logits.len() < 2 {
            return Err(OnnxError::Inference(format!(
                "unexpected logits len {}",
                logits.len()
            )));
        }

        // Softmax for confidence
        let logits_arr = Array1::from_iter(logits.iter().take(2).copied());
        let max = logits_arr.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let exps: Array1<f32> = logits_arr.mapv(|v| (v - max).exp());
        let sum = exps.sum();
        let probs = exps / sum;
        let (idx, conf) = probs
            .iter()
            .enumerate()
            .fold((0usize, 0.0f32), |(bi, bv), (i, &v)| {
                if v > bv {
                    (i, v)
                } else {
                    (bi, bv)
                }
            });

        Ok(ClassificationResult {
            label: self.label_map[idx],
            confidence: conf,
            matched_rule: Some(format!("distilbert:logits[{}]", idx)),
        })
    }
}

impl DistractionClassifier for OnnxClassifier {
    fn classify(&self, sample: &WindowSample) -> AiFuture<ClassificationResult> {
        // Build the text the model sees: "<process> | <title>".
        let text = match &sample.window_title {
            Some(t) => format!("{} | {}", sample.process_name, t),
            None => sample.process_name.clone(),
        };
        let result = self.classify_text(&text);
        Box::pin(async move {
            result.map_err(|e| solar_focus_intelligence::AiError::Inference(e.to_string()))
        })
    }
    fn is_ready(&self) -> bool {
        true
    }
}
