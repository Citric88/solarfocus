//! User-tunable settings persisted as JSON in the OS config dir.
//!
//! macOS:   ~/Library/Application Support/SolarFocus OS/settings.json
//! Linux:   ~/.config/solarfocus-os/settings.json
//! Windows: %APPDATA%\SolarFocus OS\settings.json
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

/// Phase 4 — coarse RAM-budget profile that the App uses to derive
/// derived flags (ai_enabled, window_watch_enabled, classifier_mode).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RamMode {
    /// ≤ 50 MB — timer only, no AI, no window watch.
    Low,
    /// ≤ 120 MB — classifier + window watch, no LLM.
    Normal,
    /// ≤ 1.5 GB — full AI, classifier, window watch.
    Full,
}

impl Default for RamMode {
    fn default() -> Self {
        RamMode::Normal
    }
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
    #[serde(default = "default_classifier_mode")]
    pub classifier_mode: ClassifierMode,
    #[serde(default = "default_true")]
    pub window_watch_enabled: bool,
    #[serde(default = "default_poll")]
    pub window_poll_secs: u8,

    // v1.2 Phase 2 — distraction detection knobs
    #[serde(default = "default_min_consecutive")]
    pub min_consecutive_samples: u8,
    #[serde(default = "default_min_confidence")]
    pub min_confidence: f32,
    #[serde(default)]
    pub user_rules_path: Option<PathBuf>,

    // v1.2 Phase 3.5b — first-run download tracking.
    // True once the user clicked "Skip" on the first-run modal so we don't
    // ask again. Resets if they later change `model_choice`.
    #[serde(default)]
    pub model_download_skipped: bool,

    // v1.2 Phase 4 — coarse RAM budget profile.
    #[serde(default)]
    pub ram_mode: RamMode,

    // UI-4 — true on a brand-new install. Wizard sets to false on completion.
    #[serde(default = "default_first_run")]
    pub first_run: bool,

    // FEAT — user-configurable focus duration in minutes. Default 25 (classic Pomodoro).
    #[serde(default = "default_focus_minutes")]
    pub focus_minutes: u32,
    #[serde(default = "default_break_minutes")]
    pub break_minutes: u32,
    #[serde(default = "default_long_break_minutes")]
    pub long_break_minutes: u32,

    // v1.3 Wave A2 — last selected focus category. Persisted so the chip
    // selection survives restarts. Default "Focus" matches the legacy
    // value used by v1.2 rows after migration.
    #[serde(default = "default_category")]
    pub last_category: String,

    // v1.3 Wave B — camera-based presence detection. Off by default
    // because it needs Camera permission and we want explicit opt-in.
    #[serde(default)]
    pub presence_enabled: bool,
    /// Consecutive Absent samples (1 sample/sec) before auto-pausing
    /// the focus session. 3 = ≈3 seconds.
    #[serde(default = "default_absent_threshold")]
    pub presence_absent_threshold: u8,

    // v1.3 Wave C — manual "next deadline" stored as RFC3339 local
    // datetime + a label. v1.3.1 will replace this with a live
    // EventKit feed; the field stays as a fallback when permission
    // is denied or no calendar accounts are configured.
    #[serde(default)]
    pub next_deadline_at: Option<String>,
    #[serde(default)]
    pub next_deadline_label: String,
}

fn default_absent_threshold() -> u8 {
    3
}

fn default_category() -> String {
    "Focus".to_string()
}

fn default_focus_minutes() -> u32 {
    25
}
fn default_break_minutes() -> u32 {
    5
}
fn default_long_break_minutes() -> u32 {
    15
}

fn default_first_run() -> bool {
    true
}

fn default_true() -> bool {
    true
}
fn default_poll() -> u8 {
    10
}
fn default_classifier_mode() -> ClassifierMode {
    // v1.2.0-beta1: Rules is the new sensible default.
    ClassifierMode::Rules
}
fn default_min_consecutive() -> u8 {
    2
}
fn default_min_confidence() -> f32 {
    0.7
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: Language::default(),
            ai_enabled: true,
            coaching_tone: CoachingTone::default(),
            model_choice: ModelChoice::default(),
            classifier_mode: default_classifier_mode(),
            window_watch_enabled: true,
            window_poll_secs: 10,
            min_consecutive_samples: default_min_consecutive(),
            min_confidence: default_min_confidence(),
            user_rules_path: None,
            model_download_skipped: false,
            ram_mode: RamMode::default(),
            first_run: true,
            focus_minutes: 25,
            break_minutes: 5,
            long_break_minutes: 15,
            last_category: default_category(),
            presence_enabled: false,
            presence_absent_threshold: default_absent_threshold(),
            next_deadline_at: None,
            next_deadline_label: String::new(),
        }
    }
}

impl Settings {
    /// Apply the implications of a RAM-mode change to dependent flags.
    /// Caller must persist via `save()` afterwards.
    pub fn apply_ram_mode(&mut self) {
        match self.ram_mode {
            RamMode::Low => {
                self.ai_enabled = false;
                self.window_watch_enabled = false;
                self.classifier_mode = ClassifierMode::Mock;
            }
            RamMode::Normal => {
                self.ai_enabled = false;
                self.window_watch_enabled = true;
                self.classifier_mode = ClassifierMode::Rules;
            }
            RamMode::Full => {
                self.ai_enabled = true;
                self.window_watch_enabled = true;
                if matches!(self.classifier_mode, ClassifierMode::Mock) {
                    self.classifier_mode = ClassifierMode::Rules;
                }
            }
        }
    }

    /// Path the App uses to look up user-overridden rules.toml. Defaults to
    /// `<config_dir>/SolarFocus/rules.toml` when not explicitly set.
    pub fn effective_rules_path(&self) -> PathBuf {
        if let Some(ref p) = self.user_rules_path {
            return p.clone();
        }
        if let Some(p) = ProjectDirs::from("os", "SolarFocus", "SolarFocus") {
            p.config_dir().join("rules.toml")
        } else {
            PathBuf::from("rules.toml")
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
