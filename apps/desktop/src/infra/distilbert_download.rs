#![cfg(feature = "classifier")]
//! ENH-7 — DistilBERT model + tokenizer downloader for the Phase 4 ONNX
//! classifier. Pulls two small files (~67 MB model + ~1 MB tokenizer)
//! into `<data_dir>/SolarFocus/models/distilbert/`.
//!
//! NOTE: URLs and SHAs are placeholders for v1.2.0 (the codebase ships
//! the plumbing; the canonical files are locked in v1.2.1 once we pick
//! a specific quantized fine-tune). When sha256 is empty, verification
//! is skipped so dev testing isn't blocked.

use crate::infra::model_download::{download_file, DownloadError};
use crate::infra::onnx_classifier::OnnxClassifier;
use std::path::PathBuf;

const MODEL_URL: &str =
    "https://huggingface.co/Xenova/distilbert-base-uncased-finetuned-sst-2-english/resolve/main/onnx/model_quantized.onnx";
const MODEL_SHA: &str = "79b45c7fd8c2673aa53615aba07caeb2de798f63a95a2393a365982429c94f7f";

const TOKENIZER_URL: &str =
    "https://huggingface.co/Xenova/distilbert-base-uncased-finetuned-sst-2-english/resolve/main/tokenizer.json";
const TOKENIZER_SHA: &str = "d241a60d5e8f04cc1b2b3e9ef7a4921b27bf526d9f6050ab90f9267a1f9e5c66";

pub fn model_dir() -> PathBuf {
    OnnxClassifier::default_model_dir()
}

pub fn model_path() -> PathBuf {
    model_dir().join("model.onnx")
}

pub fn tokenizer_path() -> PathBuf {
    model_dir().join("tokenizer.json")
}

pub fn is_present() -> bool {
    model_path().exists() && tokenizer_path().exists()
}

/// Download both files. Best-effort; on any failure, returns the error
/// from the failed step.
pub async fn download_distilbert() -> Result<(), DownloadError> {
    let m_sha = (!MODEL_SHA.is_empty()).then_some(MODEL_SHA);
    let t_sha = (!TOKENIZER_SHA.is_empty()).then_some(TOKENIZER_SHA);
    download_file(MODEL_URL, &model_path(), m_sha).await?;
    download_file(TOKENIZER_URL, &tokenizer_path(), t_sha).await?;
    Ok(())
}
