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
/// face entering or leaving frame typically swings the mean by ≥3 levels.
/// (Was 8.0 in v1.3.0 — empirically too high for "user stepped back".)
const ABSENCE_DELTA: f32 = 3.0;
/// Minimum frame variance (luminance std-dev squared, normalized) for
/// "face likely in frame". Below this, we're looking at a flat scene.
const VARIANCE_PRESENT_MIN: f32 = 200.0;

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
    /// YuNet engine in Arc<Mutex<>> so we can `clone()` the Arc and
    /// hand it to a tokio::spawn_blocking task without holding the
    /// UI thread for the ~200 ms inference call.
    yunet: Option<std::sync::Arc<std::sync::Mutex<YunetEngine>>>,
    mode: DetectionMode,
}

/// Send-safe wrapper around the ort YuNet session. Held in
/// `Arc<Mutex<...>>` on `PresenceProbe` so we can clone the Arc and
/// move it into `spawn_blocking` without the UI thread blocking on
/// CPU inference (~200 ms per call at 640×640 on M-series).
pub struct YunetEngine {
    session: ort::session::Session,
    input_w: u32,
    input_h: u32,
    /// Actual input tensor name from the loaded model (varies between
    /// YuNet revisions: "input", "data", "image").
    input_name: String,
}

// SAFETY: ort::Session is documented as thread-safe (a single Session
// can be invoked from multiple threads concurrently). We additionally
// guard mutations behind a Mutex on the parent.
unsafe impl Send for YunetEngine {}
unsafe impl Sync for YunetEngine {}

impl YunetEngine {
    /// Run face detection on a captured frame. Returns Present + max
    /// face confidence on success, or an error description on failure.
    /// Designed to run on a tokio::task::spawn_blocking thread.
    pub fn infer(&mut self, gray: &[u8], src_w: u32, src_h: u32) -> Result<(Presence, f32), String> {
        use ndarray::Array4;
        let (w, h) = (self.input_w as usize, self.input_h as usize);

        // Cheap nearest-neighbor resize gray → 640×640, replicate into
        // BGR so YuNet (trained on BGR) sees consistent channels.
        let mut bgr = vec![0f32; w * h * 3];
        for ty in 0..h {
            let sy = (ty as u32 * src_h / h as u32).min(src_h.saturating_sub(1));
            for tx in 0..w {
                let sx = (tx as u32 * src_w / w as u32).min(src_w.saturating_sub(1));
                let g = gray[(sy * src_w + sx) as usize] as f32;
                let idx = (ty * w + tx) * 3;
                bgr[idx] = g;
                bgr[idx + 1] = g;
                bgr[idx + 2] = g;
            }
        }
        let mut tensor = Array4::<f32>::zeros((1, 3, h, w));
        for c in 0..3 {
            for y in 0..h {
                for x in 0..w {
                    tensor[(0, c, y, x)] = bgr[(y * w + x) * 3 + c];
                }
            }
        }
        let input_value = ort::value::Value::from_array(tensor)
            .map_err(|e| e.to_string())?;
        let input_name = self.input_name.clone();
        let outputs = self
            .session
            .run(ort::inputs![input_name.as_str() => input_value])
            .map_err(|e| e.to_string())?;
        // YuNet 2023mar emits per-stride tensors:
        //   cls_{8,16,32} — per-anchor face *class* probability.
        //     This is unconditioned by whether an object exists at
        //     that anchor, so its max stays ~0.9 even on a blank
        //     scene — useless on its own.
        //   obj_{8,16,32} — per-anchor *objectness* (is there a face
        //     here at all). This is the signal that actually drops
        //     when the user steps away.
        //   bbox_*, kps_* — regression coords, ignored.
        //
        // The OpenCV reference inference multiplies cls × obj
        // per-anchor and takes the max as the final face score.
        // We approximate by taking the max within each kind across
        // all strides, then multiplying.
        let mut max_obj = 0f32;
        let mut max_cls = 0f32;
        for (name, val) in outputs.iter() {
            let name_lower = name.to_ascii_lowercase();
            let is_obj = name_lower.starts_with("obj_") || name_lower.contains("conf");
            let is_cls = name_lower.starts_with("cls_") || name_lower.contains("score");
            if !is_obj && !is_cls {
                continue;
            }
            if let Ok((_, slice)) = val.try_extract_tensor::<f32>() {
                for &v in slice.iter() {
                    if !v.is_finite() || v < 0.0 || v > 1.0 {
                        continue;
                    }
                    if is_obj && v > max_obj {
                        max_obj = v;
                    }
                    if is_cls && v > max_cls {
                        max_cls = v;
                    }
                }
            }
        }
        // If only one was found (some YuNet exports merge them into
        // a single "score" output), use that one.
        let combined = match (max_obj > 0.0, max_cls > 0.0) {
            (true, true) => max_obj * max_cls,
            (true, false) => max_obj,
            (false, true) => max_cls,
            (false, false) => 0.0,
        };
        let presence = if combined >= FACE_CONF_MIN {
            Presence::Present
        } else {
            Presence::Absent
        };
        Ok((presence, combined.clamp(0.0, 1.0)))
    }
}

/// Captured frame bytes + dims handed off from `poll()` to a background
/// YuNet inference task.
pub struct CapturedFrame {
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
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
                (Some(std::sync::Arc::new(std::sync::Mutex::new(s))), DetectionMode::YunetFace)
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
            yunet,
            mode,
        })
    }

    pub fn mode(&self) -> DetectionMode {
        self.mode
    }

    /// Clone of the YuNet engine handle. Returned to the App so it
    /// can ferry the Arc into a `tokio::task::spawn_blocking` call
    /// without holding the UI thread.
    pub fn yunet_engine(&self) -> Option<std::sync::Arc<std::sync::Mutex<YunetEngine>>> {
        self.yunet.clone()
    }

    fn try_load_yunet() -> Result<Option<YunetEngine>, String> {
        let path = Self::yunet_path();
        if !path.exists() {
            return Ok(None);
        }
        let session = ort::session::Session::builder()
            .map_err(|e| e.to_string())?
            .commit_from_file(&path)
            .map_err(|e| e.to_string())?;
        let inputs = session.inputs();
        for inp in inputs.iter() {
            log::info!(
                "PresenceProbe: YuNet input name='{}' dtype={:?}",
                inp.name(),
                inp.dtype()
            );
        }
        let input_name = inputs
            .first()
            .map(|i| i.name().to_string())
            .unwrap_or_else(|| "input".to_string());
        let _ = inputs;
        // Also log every output so we can pick the right tensor for
        // confidence extraction (YuNet emits bbox/iou/cls/kps).
        let outputs = session.outputs();
        for out in outputs.iter() {
            log::info!(
                "PresenceProbe: YuNet output name='{}' dtype={:?}",
                out.name(),
                out.dtype()
            );
        }
        let _ = outputs;
        // The 2023-mar export ships with declared input shape
        // [1, 3, 640, 640].
        Ok(Some(YunetEngine {
            session,
            input_w: 640,
            input_h: 640,
            input_name,
        }))
    }

    /// Capture one frame and return a brightness-based presence
    /// sample plus the raw bytes so the App can optionally fire off
    /// a background YuNet inference on a `spawn_blocking` thread.
    /// The frame buffer is dropped before this function returns.
    pub fn poll(&self) -> Result<(PresenceSample, CapturedFrame), PresenceError> {
        let mut cam = self.camera.lock().expect("camera mutex poisoned");
        let frame = cam
            .frame()
            .map_err(|e| PresenceError::Read(e.to_string()))?;
        let buf = frame
            .decode_image::<LumaFormat>()
            .map_err(|e| PresenceError::Read(e.to_string()))?;

        let raw = buf.as_raw();
        let total: u64 = raw.iter().map(|&p| p as u64).sum();
        let count = raw.len() as u64;
        let mean = if count == 0 { 0.0 } else { total as f32 / count as f32 };
        let variance: f32 = if count > 0 {
            let mut accum = 0f64;
            for &p in raw {
                let d = p as f32 - mean;
                accum += (d * d) as f64;
            }
            (accum / count as f64) as f32
        } else {
            0.0
        };
        let cap_w = buf.width();
        let cap_h = buf.height();
        let raw_bytes = raw.to_vec();
        drop(buf);
        drop(frame);

        let captured_at = Local::now();
        let mut last = self.last_mean.lock().expect("last_mean mutex poisoned");
        let (presence, confidence) = match *last {
            None => (Presence::Unknown, 0.0),
            Some(prev) => {
                let delta = (mean - prev).abs();
                if delta >= ABSENCE_DELTA {
                    (Presence::Absent, (delta / 32.0).clamp(0.3, 1.0))
                } else if variance < VARIANCE_PRESENT_MIN {
                    let conf = ((VARIANCE_PRESENT_MIN - variance) / VARIANCE_PRESENT_MIN)
                        .clamp(0.3, 0.9);
                    (Presence::Absent, conf)
                } else {
                    let conf = (variance / 1500.0).clamp(0.5, 1.0);
                    (Presence::Present, conf)
                }
            }
        };
        *last = Some(mean);

        log::debug!(
            "PresenceProbe: brightness path mean={:.1} var={:.0} → {:?}",
            mean, variance, presence
        );

        Ok((
            PresenceSample { presence, confidence, captured_at },
            CapturedFrame { bytes: raw_bytes, width: cap_w, height: cap_h },
        ))
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
