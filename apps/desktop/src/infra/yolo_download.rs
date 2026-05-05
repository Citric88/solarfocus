#![cfg(feature = "presence")]
//! v1.11.0 — YOLOv8n ONNX downloader for the cell-phone detector.
//!
//! Same pattern as `yunet_download.rs`. ~12 MB single file, fetched once
//! on user opt-in and stored at
//! `<data_dir>/SolarFocus OS/models/yolo/yolov8n.onnx`. The model is the
//! 80-class COCO export from Ultralytics (class 67 = cell phone).
//!
//! Privacy stance: when the model is absent, the phone detector is off.
//! When present and enabled, frames are processed in-memory only — no
//! disk write, no upload, same contract as YuNet.

use crate::infra::model_download::{download_file, models_dir, DownloadError};
use std::path::PathBuf;

// v1.12.2 — Ultralytics ships only the .pt PyTorch checkpoint, not the
// ONNX export. We pull the converted ONNX from a public, no-auth-required
// HuggingFace mirror; size locked to ~12.7 MB matches the stock
// yolov8n.onnx export from `ultralytics export format=onnx`.
const URL: &str =
    "https://huggingface.co/s1777/yolo-v8n-onnx/resolve/main/yolov8n.onnx";
// SHA placeholder — locked once we ship a stable revision pin.
const SHA: &str = "";

pub fn model_dir() -> PathBuf {
    models_dir().join("yolo")
}

pub fn model_path() -> PathBuf {
    model_dir().join("yolov8n.onnx")
}

pub fn is_present() -> bool {
    let p = model_path();
    p.exists()
        && std::fs::metadata(&p)
            .map(|m| m.len() > 1_000_000) // > 1 MB sanity floor (real file is ~12 MB)
            .unwrap_or(false)
}

pub async fn download() -> Result<(), DownloadError> {
    let sha = (!SHA.is_empty()).then_some(SHA);
    download_file(URL, &model_path(), sha).await
}
