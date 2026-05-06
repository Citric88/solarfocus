//! v2.0.0 — Reveal a file in the OS's file manager.
//!
//! macOS    → `open -R <path>` (Finder, with the file selected)
//! Windows  → `explorer /select,<path>` (File Explorer, file selected)
//! Linux    → `xdg-open <parent_dir>` (no per-file select on Linux;
//!            opens the containing folder).
//!
//! Best-effort: silent failure. If the OS doesn't accept the command
//! the app keeps running; the inline path on the export card already
//! tells the user where the file is.

use std::path::Path;
use std::process::Command;

pub fn reveal(path: &Path) {
    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("open").arg("-R").arg(path).spawn();
    }
    #[cfg(target_os = "windows")]
    {
        let arg = format!("/select,{}", path.display());
        let _ = Command::new("explorer").arg(arg).spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let target = path.parent().unwrap_or(path);
        let _ = Command::new("xdg-open").arg(target).spawn();
    }
}
