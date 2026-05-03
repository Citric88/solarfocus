//! Cross-platform foreground-window watcher. Wraps `active-win-pos-rs` and
//! converts to `solar_focus_intelligence::WindowSample`.
//!
//! macOS specifics:
//! - Reading the *title* requires Screen Recording permission.
//! - Process name is always available.
//! - We log a one-time warn when the title is empty so the user knows what to fix.

use solar_focus_intelligence::WindowSample;
use std::sync::atomic::{AtomicBool, Ordering};

static PERMISSION_WARNED: AtomicBool = AtomicBool::new(false);

pub struct WindowWatcher;

impl WindowWatcher {
    /// Returns `None` when no foreground window can be read (no error
    /// surfacing — caller can simply skip this tick).
    pub fn poll(elapsed_in_session_secs: u32) -> Option<WindowSample> {
        match active_win_pos_rs::get_active_window() {
            Ok(active) => {
                // active-win-pos-rs 0.8 exposes `app_name` and `process_path` (no `process_name`).
                // Prefer `app_name`; fall back to the basename of `process_path`.
                let process_name = if !active.app_name.is_empty() {
                    active.app_name.clone()
                } else {
                    std::path::Path::new(&active.process_path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown")
                        .to_string()
                };

                let title = active.title.clone();
                let window_title = if title.trim().is_empty() {
                    if !PERMISSION_WARNED.swap(true, Ordering::Relaxed) {
                        log::warn!(
                            "Active-window title is empty. On macOS this usually means \
                             Screen Recording permission is not granted to SolarFocus. \
                             Distraction detection will fall back to process name only."
                        );
                    }
                    None
                } else {
                    Some(title)
                };

                Some(WindowSample {
                    process_name,
                    window_title,
                    elapsed_in_session_secs,
                })
            }
            Err(()) => {
                if !PERMISSION_WARNED.swap(true, Ordering::Relaxed) {
                    log::warn!("Could not read active window — skipping this probe.");
                }
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// We can't easily mock the OS API, so this just verifies the function
    /// returns Option without panicking. CI agents typically have no
    /// foreground window — `None` is a valid result.
    #[test]
    fn poll_does_not_panic() {
        let _ = WindowWatcher::poll(0);
    }
}
