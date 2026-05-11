//! v2.0.0 — Cross-platform native notifications.
//!
//! macOS    → NSUserNotificationCenter (via notify-rust)
//! Windows  → Windows.UI.Notifications Toast (via notify-rust)
//! Linux    → libnotify (via notify-rust)
//!
//! Replaces the v1.x `osascript -e 'display notification …'` path that
//! was macOS-only. The crate handles each backend's quirks; we just
//! pass title + body and fire-and-forget.

use notify_rust::Notification;

/// Send a native OS notification. Best-effort: silently swallows any
/// platform error so a missing notification daemon (CI runner, headless)
/// never breaks the app.
pub fn send(title: &str, body: &str) {
    let mut n = Notification::new();
    n.summary(title).body(body);
    // Submarine sound is macOS-specific; ignored on other platforms.
    #[cfg(target_os = "macos")]
    {
        n.sound_name("Submarine");
    }
    if let Err(e) = n.show() {
        log::warn!("notify-rust failed: {e}");
    }
}
