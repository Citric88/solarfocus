#![cfg(feature = "presence")]
//! v1.3.1 — YuNet ONNX face detector downloader for the Wave B
//! presence module. ~337 KB single file, fetched once on user opt-in
//! and stored at `<data_dir>/SolarFocus OS/models/yunet/face_detection_yunet_2023mar.onnx`.
//!
//! When the file is present, `PresenceProbe` upgrades from the
//! brightness-change heuristic to actual face yes/no. When it's
//! missing, presence detection still works (falls back to brightness).

use crate::infra::model_download::{download_file, models_dir, DownloadError};
use std::path::PathBuf;

const URL: &str =
    "https://github.com/opencv/opencv_zoo/raw/main/models/face_detection_yunet/face_detection_yunet_2023mar.onnx";
// SHA placeholder — locked once we ship a stable revision pin.
const SHA: &str = "";

pub fn model_dir() -> PathBuf {
    models_dir().join("yunet")
}

pub fn model_path() -> PathBuf {
    model_dir().join("face_detection_yunet_2023mar.onnx")
}

pub fn is_present() -> bool {
    let p = model_path();
    p.exists() && std::fs::metadata(&p).map(|m| m.len() > 100_000).unwrap_or(false)
}

pub async fn download() -> Result<(), DownloadError> {
    let sha = (!SHA.is_empty()).then_some(SHA);
    download_file(URL, &model_path(), sha).await
}
