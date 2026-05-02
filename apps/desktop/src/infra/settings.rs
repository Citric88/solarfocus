//! User-tunable settings persisted as JSON in the OS config dir.
//!
//! macOS:   ~/Library/Application Support/SolarFocus/settings.json
//! Linux:   ~/.config/solarfocus/settings.json
//! Windows: %APPDATA%\SolarFocus\settings.json
//!
//! Schema is additive — new fields use `#[serde(default)]` so old files keep working.

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use solar_focus_intelligence::Language;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CoachingTone {
    Terse,
    Encouraging,
    NoEmoji,
}

impl Default for CoachingTone {
    fn default() -> Self {
        CoachingTone::Terse
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelChoice {
    SmolLM2,
    Llama1B,
    Qwen15,
}

impl Default for ModelChoice {
    fn default() -> Self {
        ModelChoice::SmolLM2
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClassifierMode {
    Mock,
    Rules,
    Distilbert,
}

impl Default for ClassifierMode {
    fn default() -> Self {
        ClassifierMode::Mock
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub language: Language,
    #[serde(default = "default_true")]
    pub ai_enabled: bool,
    #[serde(default)]
    pub coaching_tone: CoachingTone,
    #[serde(default)]
    pub model_choice: ModelChoice,
    #[serde(default)]
    pub classifier_mode: ClassifierMode,
    #[serde(default = "default_true")]
    pub window_watch_enabled: bool,
    #[serde(default = "default_poll")]
    pub window_poll_secs: u8,
}

fn default_true() -> bool {
    true
}
fn default_poll() -> u8 {
    10
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: Language::default(),
            ai_enabled: true,
            coaching_tone: CoachingTone::default(),
            model_choice: ModelChoice::default(),
            classifier_mode: ClassifierMode::default(),
            window_watch_enabled: true,
            window_poll_secs: 10,
        }
    }
}

impl Settings {
    /// Loads settings from disk, or returns `Default::default()` if missing/corrupt.
    pub fn load() -> Self {
        let path = Self::default_path();
        match std::fs::read_to_string(&path) {
            Ok(s) => match serde_json::from_str(&s) {
                Ok(v) => {
                    log::info!("Settings loaded from {}", path.display());
                    v
                }
                Err(e) => {
                    log::warn!("Settings parse failed ({e}), using defaults");
                    Self::default()
                }
            },
            Err(_) => {
                log::info!("No settings file at {} — using defaults", path.display());
                Self::default()
            }
        }
    }

    /// Persists to disk. Best-effort — logs on failure but never panics.
    pub fn save(&self) {
        let path = Self::default_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match serde_json::to_string_pretty(self) {
            Ok(s) => {
                if let Err(e) = std::fs::write(&path, s) {
                    log::error!("Failed to save settings: {e}");
                } else {
                    log::info!("Settings saved to {}", path.display());
                }
            }
            Err(e) => log::error!("Failed to serialize settings: {e}"),
        }
    }

    /// OS-appropriate config path.
    pub fn default_path() -> PathBuf {
        if let Some(p) = ProjectDirs::from("os", "SolarFocus", "SolarFocus") {
            p.config_dir().join("settings.json")
        } else {
            PathBuf::from("settings.json")
        }
    }

    /// Test helper — load/save against an explicit path.
    pub fn save_at(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let s = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(path, s)
    }

    pub fn load_at(path: &std::path::Path) -> std::io::Result<Self> {
        let s = std::fs::read_to_string(path)?;
        serde_json::from_str(&s).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn temp_path() -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        std::env::temp_dir().join(format!(
            "solarfocus_settings_{}_{}.json",
            std::process::id(),
            n
        ))
    }

    #[test]
    fn roundtrip_defaults() {
        let path = temp_path();
        let s = Settings::default();
        s.save_at(&path).unwrap();
        let loaded = Settings::load_at(&path).unwrap();
        assert_eq!(loaded.language, Language::Es);
        assert!(loaded.ai_enabled);
        assert_eq!(loaded.window_poll_secs, 10);
    }

    #[test]
    fn missing_fields_use_defaults() {
        let path = temp_path();
        // Old/partial file format — only language present.
        std::fs::write(&path, r#"{"language":"En"}"#).unwrap();
        let loaded = Settings::load_at(&path).unwrap();
        assert_eq!(loaded.language, Language::En);
        assert!(loaded.ai_enabled, "default should kick in");
        assert_eq!(loaded.window_poll_secs, 10);
    }

    #[test]
    fn save_then_modify_then_reload() {
        let path = temp_path();
        let mut s = Settings::default();
        s.ai_enabled = false;
        s.window_poll_secs = 5;
        s.save_at(&path).unwrap();
        let loaded = Settings::load_at(&path).unwrap();
        assert!(!loaded.ai_enabled);
        assert_eq!(loaded.window_poll_secs, 5);
    }
}
