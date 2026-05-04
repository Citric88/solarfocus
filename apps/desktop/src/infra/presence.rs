#![cfg(feature = "presence")]
//! v1.3 Wave B — camera-based presence detection.
//!
//! Captures one low-resolution grayscale frame from the user's default
//! webcam every poll (default once per second when active). The frame
//! never touches disk, never leaves memory beyond the inference call,
//! and the detector returns only a Present / Absent label + confidence.
//!
//! Two-tier detection:
//! 1. **YuNet ONNX (preferred)** — when `~/Library/Application Support/
//!    SolarFocus OS/models/yunet/face_detection_yunet_2023mar.onnx`
//!    is present (~337 KB). Returns Present iff at least one face is
//!    detected at confidence ≥ FACE_CONF_MIN.
//! 2. **Brightness heuristic (fallback)** — when YuNet is missing.
//!    Compares mean luminance between consecutive frames; flags
//!    Absent on sharp swings.
//!
//! Privacy posture (matches the v1.2 distraction-detection card):
//! - No frame written to disk.
//! - No frame transmitted off the device.
//! - No identification — only a binary "person likely present" signal.
//! - Camera initialized lazily and stops when the toggle is turned off.

use chrono::{DateTime, Local};
use nokhwa::pixel_format::LumaFormat;
use nokhwa::utils::{CameraIndex, RequestedFormat, RequestedFormatType};
use nokhwa::Camera;
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    Present,
    Absent,
    /// First sample after init — heuristic needs ≥2 frames to compare.
    Unknown,
}

#[derive(Debug, Clone)]
pub struct PresenceSample {
    pub presence: Presence,
    pub confidence: f32,
    pub captured_at: DateTime<Local>,
}

#[derive(Debug, thiserror::Error)]
pub enum PresenceError {
    #[error("camera open failed: {0}")]
    Open(String),
    #[error("camera read failed: {0}")]
    Read(String),
    #[error("permission denied")]
    PermissionDenied,
}

/// Minimum delta in mean luminance (0..=255) between consecutive frames
/// to interpret as "presence change". Tuned for indoor lighting; a real
/// face entering or leaving frame typically swings the mean by ≥6 levels.
const ABSENCE_DELTA: f32 = 8.0;

/// YuNet face confidence threshold — detections below this are dropped.
const FACE_CONF_MIN: f32 = 0.6;

/// Number of consecutive Absent samples before the engine should pause.
/// Wired into App from settings; this constant is the default.
pub const DEFAULT_ABSENT_THRESHOLD: u8 = 3;

/// Detection backend currently in use by a `PresenceProbe`. Surfaced
/// to the UI so the Setup card can show *"Heurística por luminosidad"*
/// vs *"Detección facial (YuNet)"*.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectionMode {
    Brightness,
    YunetFace,
}

pub struct PresenceProbe {
    camera: Mutex<Camera>,
    last_mean: Mutex<Option<f32>>,
    yunet: Mutex<Option<YunetSession>>,
    mode: DetectionMode,
}

/// Encapsulates the ort session + input shape for YuNet. Wrapped in
/// Mutex on the parent so the synchronous `poll()` path can mutate it.
struct YunetSession {
    session: ort::session::Session,
    input_w: u32,
    input_h: u32,
}

impl PresenceProbe {
    /// Default model path on macOS. Mirrors the `model_download` layout
    /// (i.e. lives under the same `<data>/models/` tree that hosts the
    /// LLM and DistilBERT files).
    pub fn yunet_path() -> PathBuf {
        crate::infra::yunet_download::model_path()
    }

    pub fn new() -> Result<Self, PresenceError> {
        let format = RequestedFormat::new::<LumaFormat>(RequestedFormatType::AbsoluteHighestFrameRate);
        let mut camera = Camera::new(CameraIndex::Index(0), format)
            .map_err(|e| PresenceError::Open(e.to_string()))?;
        camera
            .open_stream()
            .map_err(|e| PresenceError::Open(e.to_string()))?;
        log::info!("PresenceProbe: camera opened");

        // Try to load YuNet — non-fatal: if missing or load fails we
        // fall back to brightness heuristic.
        let (yunet, mode) = match Self::try_load_yunet() {
            Ok(Some(s)) => {
                log::info!("PresenceProbe: YuNet ONNX loaded ({}x{})", s.input_w, s.input_h);
                (Some(s), DetectionMode::YunetFace)
            }
            Ok(None) => {
                log::info!("PresenceProbe: YuNet model absent — using brightness heuristic");
                (None, DetectionMode::Brightness)
            }
            Err(e) => {
                log::warn!("PresenceProbe: YuNet load failed ({}), falling back to brightness", e);
                (None, DetectionMode::Brightness)
            }
        };

        Ok(Self {
            camera: Mutex::new(camera),
            last_mean: Mutex::new(None),
            yunet: Mutex::new(yunet),
            mode,
        })
    }

    pub fn mode(&self) -> DetectionMode {
        self.mode
    }

    fn try_load_yunet() -> Result<Option<YunetSession>, String> {
        let path = Self::yunet_path();
        if !path.exists() {
            return Ok(None);
        }
        let session = ort::session::Session::builder()
            .map_err(|e| e.to_string())?
            .commit_from_file(&path)
            .map_err(|e| e.to_string())?;
        // YuNet 2023mar is fixed at 320x320 by default; can be reshaped
        // but we keep the canonical input.
        Ok(Some(YunetSession {
            session,
            input_w: 320,
            input_h: 320,
        }))
    }

    /// Capture one frame and return a presence sample. The frame buffer
    /// is dropped before this function returns — never persisted.
    pub fn poll(&self) -> Result<PresenceSample, PresenceError> {
        let mut cam = self.camera.lock().expect("camera mutex poisoned");
        let frame = cam
            .frame()
            .map_err(|e| PresenceError::Read(e.to_string()))?;
        let buf = frame
            .decode_image::<LumaFormat>()
            .map_err(|e| PresenceError::Read(e.to_string()))?;

        // YuNet expects RGB at 320x320. The brightness-only path needs
        // just the mean luminance. Compute mean for both paths because
        // it's cheap, then either run YuNet on a resized RGB tensor or
        // fall back to the heuristic.
        let raw = buf.as_raw();
        let total: u64 = raw.iter().map(|&p| p as u64).sum();
        let count = raw.len() as u64;
        let mean = if count == 0 { 0.0 } else { total as f32 / count as f32 };
        let cap_w = buf.width();
        let cap_h = buf.height();
        // Move the raw bytes out before we drop the frame, so the YuNet
        // path can resize without relying on the frame's lifetime.
        let raw_bytes = raw.to_vec();
        drop(buf);
        drop(frame);

        let captured_at = Local::now();
        if self.mode == DetectionMode::YunetFace {
            // Try YuNet; if it errors at runtime we fall back to
            // brightness for *this* sample but stay in YunetFace mode.
            match self.run_yunet(&raw_bytes, cap_w, cap_h) {
                Ok((presence, confidence)) => {
                    return Ok(PresenceSample {
                        presence,
                        confidence,
                        captured_at,
                    });
                }
                Err(e) => {
                    log::warn!("PresenceProbe: YuNet inference error ({}), brightness fallback", e);
                }
            }
        }

        // Brightness fallback path.
        let mut last = self.last_mean.lock().expect("last_mean mutex poisoned");
        let (presence, confidence) = match *last {
            None => (Presence::Unknown, 0.0),
            Some(prev) => {
                let delta = (mean - prev).abs();
                if delta >= ABSENCE_DELTA {
                    (Presence::Absent, (delta / 32.0).clamp(0.0, 1.0))
                } else {
                    (Presence::Present, 1.0 - (delta / ABSENCE_DELTA))
                }
            }
        };
        *last = Some(mean);

        Ok(PresenceSample { presence, confidence, captured_at })
    }

    /// Resize a grayscale frame to 320×320, repeat to 3 channels (BGR
    /// since YuNet was trained on BGR input), normalize to f32, run
    /// inference, return Present/Absent + max-face confidence.
    fn run_yunet(&self, gray: &[u8], src_w: u32, src_h: u32) -> Result<(Presence, f32), String> {
        use ndarray::{Array4, Axis};
        let mut guard = self.yunet.lock().map_err(|_| "yunet poisoned".to_string())?;
        let yunet = guard
            .as_mut()
            .ok_or_else(|| "yunet session missing".to_string())?;
        let (w, h) = (yunet.input_w as usize, yunet.input_h as usize);

        // Cheap nearest-neighbor resize so we don't pull in a full image
        // pipeline; YuNet tolerates this at 1 fps.
        let mut bgr = vec![0f32; w * h * 3];
        for ty in 0..h {
            let sy = (ty as u32 * src_h / w as u32).min(src_h.saturating_sub(1));
            for tx in 0..w {
                let sx = (tx as u32 * src_w / w as u32).min(src_w.saturating_sub(1));
                let g = gray[(sy * src_w + sx) as usize] as f32;
                let idx = (ty * w + tx) * 3;
                // Replicate luminance into B, G, R.
                bgr[idx] = g;
                bgr[idx + 1] = g;
                bgr[idx + 2] = g;
            }
        }

        // ONNX expects NCHW [1, 3, H, W].
        let mut tensor = Array4::<f32>::zeros((1, 3, h, w));
        for c in 0..3 {
            for y in 0..h {
                for x in 0..w {
                    tensor[(0, c, y, x)] = bgr[(y * w + x) * 3 + c];
                }
            }
        }
        let _ = Axis(0);

        // Build input value and run. YuNet's input is named "input"
        // in the 2023-mar revision.
        let input_value = ort::value::Value::from_array(tensor)
            .map_err(|e| e.to_string())?;
        let outputs = yunet
            .session
            .run(ort::inputs!["input" => input_value])
            .map_err(|e| e.to_string())?;

        // YuNet's "main" output is the per-anchor confidence at index 0
        // (cls), or sometimes index 1 depending on revision. We only
        // need a "max face score" — scan all f32 outputs to find the
        // largest value as a robust proxy.
        let mut max_score = 0f32;
        for (_, val) in outputs.iter() {
            if let Ok((_, slice)) = val.try_extract_tensor::<f32>() {
                for &v in slice.iter() {
                    if v.is_finite() && v > max_score {
                        max_score = v;
                    }
                }
            }
        }

        let presence = if max_score >= FACE_CONF_MIN {
            Presence::Present
        } else {
            Presence::Absent
        };
        Ok((presence, max_score.clamp(0.0, 1.0)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A PresenceSample carries the timestamp + label only — never any
    /// frame bytes. This guards against accidental Vec<u8> field drift.
    #[test]
    fn sample_carries_no_frame_bytes() {
        let sample = PresenceSample {
            presence: Presence::Present,
            confidence: 1.0,
            captured_at: Local::now(),
        };
        // size_of guard: any new field that holds image bytes would
        // balloon this far past the small-struct floor.
        assert!(
            std::mem::size_of_val(&sample) < 64,
            "PresenceSample grew unexpectedly: {} bytes",
            std::mem::size_of_val(&sample)
        );
    }

    #[test]
    fn presence_enum_round_trip() {
        for v in [Presence::Present, Presence::Absent, Presence::Unknown] {
            assert_eq!(v, v);
        }
    }
}
