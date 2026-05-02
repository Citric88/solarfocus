#![cfg(feature = "llm")]
//! Model file downloader for the Phase 3 LLM tier.
//!
//! - HTTP `Range` header for resume.
//! - SHA-256 verification before promoting `.partial` → final filename.
//! - Progress events at most every 250 ms (avoid UI spam).
//! - Cancellation by simply dropping the future.
//!
//! Manifests live in `MANIFESTS`. **SHA-256 hashes are placeholders** until
//! we lock the exact HuggingFace revisions for v1.2 release; until then,
//! `verify_only_when_present()` skips the check on dummy hashes so dev
//! testing isn't blocked.

use crate::infra::settings::ModelChoice;
use directories::ProjectDirs;
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::Instant;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone, Copy)]
pub struct ModelManifest {
    pub id: ModelChoice,
    pub url: &'static str,
    pub sha256: &'static str, // empty string => skip verification (placeholder)
    pub size_bytes: u64,
    pub filename: &'static str,
}

pub const MANIFESTS: &[ModelManifest] = &[
    ModelManifest {
        id: ModelChoice::SmolLM2,
        url: "https://huggingface.co/HuggingFaceTB/SmolLM2-1.7B-Instruct-GGUF/resolve/main/smollm2-1.7b-instruct-q4_k_m.gguf",
        sha256: "",
        size_bytes: 1_055_000_000,
        filename: "smollm2-1.7b-instruct-q4_k_m.gguf",
    },
    ModelManifest {
        id: ModelChoice::Llama1B,
        url: "https://huggingface.co/bartowski/Llama-3.2-1B-Instruct-GGUF/resolve/main/Llama-3.2-1B-Instruct-Q4_K_M.gguf",
        sha256: "",
        size_bytes: 770_000_000,
        filename: "llama-3.2-1b-instruct-q4_k_m.gguf",
    },
    ModelManifest {
        id: ModelChoice::Qwen15,
        url: "https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF/resolve/main/qwen2.5-1.5b-instruct-q4_k_m.gguf",
        sha256: "",
        size_bytes: 935_000_000,
        filename: "qwen2.5-1.5b-instruct-q4_k_m.gguf",
    },
];

pub fn manifest_for(choice: ModelChoice) -> Option<&'static ModelManifest> {
    MANIFESTS.iter().find(|m| m.id == choice)
}

/// `~/Library/Application Support/SolarFocus/models/` on macOS.
pub fn models_dir() -> PathBuf {
    if let Some(p) = ProjectDirs::from("os", "SolarFocus", "SolarFocus") {
        p.data_dir().join("models")
    } else {
        PathBuf::from("models")
    }
}

pub fn model_path(manifest: &ModelManifest) -> PathBuf {
    models_dir().join(manifest.filename)
}

/// Returns true if the file is on disk AND (SHA matches, or hash is empty placeholder).
pub fn model_present(manifest: &ModelManifest) -> bool {
    let path = model_path(manifest);
    if !path.exists() {
        return false;
    }
    if manifest.sha256.is_empty() {
        return true; // placeholder period — accept any non-empty file
    }
    matches!(verify_sha256(&path, manifest.sha256), Ok(true))
}

#[derive(Debug, Clone)]
pub enum DownloadEvent {
    Started { total_bytes: u64 },
    Progress { downloaded: u64, total: u64, bytes_per_sec: u64 },
    Verifying,
    Complete { path: PathBuf },
    Error(String),
}

#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("http: {0}")]
    Http(String),
    #[error("checksum mismatch (expected {expected}, got {got})")]
    Checksum { expected: String, got: String },
}

/// Downloads `manifest` into `models_dir()` with resume support.
/// Calls `on_event` with progress; cancellation = drop the future.
pub async fn download_model<F>(
    manifest: &'static ModelManifest,
    mut on_event: F,
) -> Result<PathBuf, DownloadError>
where
    F: FnMut(DownloadEvent) + Send,
{
    let dir = models_dir();
    std::fs::create_dir_all(&dir)?;
    let final_path = dir.join(manifest.filename);
    let partial_path = dir.join(format!("{}.partial", manifest.filename));

    // Check if final file already exists and is valid.
    if model_present(manifest) {
        on_event(DownloadEvent::Complete {
            path: final_path.clone(),
        });
        return Ok(final_path);
    }

    let mut already_downloaded: u64 = match std::fs::metadata(&partial_path) {
        Ok(m) => m.len(),
        Err(_) => 0,
    };

    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| DownloadError::Http(e.to_string()))?;

    let mut req = client.get(manifest.url);
    if already_downloaded > 0 {
        req = req.header("Range", format!("bytes={}-", already_downloaded));
        log::info!(
            "Resuming download from byte {} ({:.1} MB)",
            already_downloaded,
            already_downloaded as f64 / 1_048_576.0
        );
    }

    let resp = req.send().await.map_err(|e| DownloadError::Http(e.to_string()))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(DownloadError::Http(format!("HTTP {}", status)));
    }

    let content_length = resp.content_length().unwrap_or(manifest.size_bytes);
    let total = content_length + already_downloaded;
    on_event(DownloadEvent::Started { total_bytes: total });

    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&partial_path)
        .await?;

    let mut stream = resp.bytes_stream();
    let mut last_event = Instant::now();
    let session_start = Instant::now();
    let mut session_bytes: u64 = 0;

    while let Some(chunk_res) = stream.next().await {
        let chunk = chunk_res.map_err(|e| DownloadError::Http(e.to_string()))?;
        file.write_all(&chunk).await?;
        already_downloaded += chunk.len() as u64;
        session_bytes += chunk.len() as u64;

        if last_event.elapsed() >= std::time::Duration::from_millis(250) {
            let elapsed = session_start.elapsed().as_secs_f64().max(0.001);
            let bps = (session_bytes as f64 / elapsed) as u64;
            on_event(DownloadEvent::Progress {
                downloaded: already_downloaded,
                total,
                bytes_per_sec: bps,
            });
            last_event = Instant::now();
        }
    }

    file.flush().await?;
    drop(file);

    // Verify checksum before promoting to final filename.
    on_event(DownloadEvent::Verifying);
    if !manifest.sha256.is_empty() {
        match verify_sha256(&partial_path, manifest.sha256)? {
            true => {}
            false => {
                let got = compute_sha256(&partial_path)?;
                let _ = std::fs::remove_file(&partial_path);
                return Err(DownloadError::Checksum {
                    expected: manifest.sha256.to_string(),
                    got,
                });
            }
        }
    }

    std::fs::rename(&partial_path, &final_path)?;
    on_event(DownloadEvent::Complete {
        path: final_path.clone(),
    });
    Ok(final_path)
}

fn verify_sha256(path: &Path, expected_hex: &str) -> std::io::Result<bool> {
    let got = compute_sha256(path)?;
    Ok(got.eq_ignore_ascii_case(expected_hex))
}

fn compute_sha256(path: &Path) -> std::io::Result<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifests_cover_all_choices() {
        for choice in [ModelChoice::SmolLM2, ModelChoice::Llama1B, ModelChoice::Qwen15] {
            assert!(
                manifest_for(choice).is_some(),
                "missing manifest for {:?}",
                choice
            );
        }
    }

    #[test]
    fn models_dir_is_under_data_dir() {
        let p = models_dir();
        assert!(
            p.to_string_lossy().contains("SolarFocus"),
            "expected models dir under SolarFocus app data, got {}",
            p.display()
        );
        assert!(p.ends_with("models"));
    }

    #[test]
    fn sha256_of_known_string() {
        let p = std::env::temp_dir().join(format!("sf_sha_{}.txt", std::process::id()));
        std::fs::write(&p, b"hello").unwrap();
        let h = compute_sha256(&p).unwrap();
        assert_eq!(
            h,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
        let _ = std::fs::remove_file(&p);
    }
}
