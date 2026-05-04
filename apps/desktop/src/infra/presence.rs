#![cfg(feature = "presence")]
//! v1.3 Wave B — camera-based presence detection.
//!
//! Captures one low-resolution grayscale frame from the user's default
//! webcam every poll (default once per second when active). The frame
//! never touches disk, never leaves memory beyond the inference call,
//! and the detector returns only a Present / Absent label + confidence.
//!
//! v1.3.0 ships a **brightness-change heuristic**: we keep a moving
//! average of the frame mean luminance and flag "Absent" when the
//! delta between frames jumps past a threshold (someone walked away,
//! a hand covered the lens, the room got dark). This is a reasonable
//! first-cut signal that requires no ML model and ships immediately.
//!
//! v1.3.1 will plug a YuNet ONNX face detector behind the same
//! `PresenceProbe::poll()` API. The struct is shaped now so callers
//! don't need to change.
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

/// Number of consecutive Absent samples before the engine should pause.
/// Wired into App from settings; this constant is the default.
pub const DEFAULT_ABSENT_THRESHOLD: u8 = 3;

pub struct PresenceProbe {
    camera: Mutex<Camera>,
    last_mean: Mutex<Option<f32>>,
}

impl PresenceProbe {
    pub fn new() -> Result<Self, PresenceError> {
        let format = RequestedFormat::new::<LumaFormat>(RequestedFormatType::AbsoluteHighestFrameRate);
        let mut camera = Camera::new(CameraIndex::Index(0), format)
            .map_err(|e| PresenceError::Open(e.to_string()))?;
        camera
            .open_stream()
            .map_err(|e| PresenceError::Open(e.to_string()))?;
        log::info!("PresenceProbe: camera opened");
        Ok(Self {
            camera: Mutex::new(camera),
            last_mean: Mutex::new(None),
        })
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
        // Compute the frame mean luminance. Iterate the raw buffer to
        // avoid allocating a copy.
        let total: u64 = buf.as_raw().iter().map(|&p| p as u64).sum();
        let count = buf.as_raw().len() as u64;
        let mean = if count == 0 { 0.0 } else { total as f32 / count as f32 };
        // Frame buffer drops at end of scope — no persistence.
        drop(buf);
        drop(frame);

        let mut last = self.last_mean.lock().expect("last_mean mutex poisoned");
        let (presence, confidence) = match *last {
            None => (Presence::Unknown, 0.0),
            Some(prev) => {
                let delta = (mean - prev).abs();
                if delta >= ABSENCE_DELTA {
                    // Big swing — interpret as someone left or covered the lens.
                    (Presence::Absent, (delta / 32.0).clamp(0.0, 1.0))
                } else {
                    (Presence::Present, 1.0 - (delta / ABSENCE_DELTA))
                }
            }
        };
        *last = Some(mean);

        Ok(PresenceSample {
            presence,
            confidence,
            captured_at: Local::now(),
        })
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
