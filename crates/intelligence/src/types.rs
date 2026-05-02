//! Wire types — serializable, no logic, no infra deps.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CoachingTrigger {
    SessionStart,
    BreakStart,
    SessionComplete,
    LongPauseDetected,
    StreakMilestone(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    Es,
    En,
}

impl Default for Language {
    fn default() -> Self {
        Language::Es
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FocusContext {
    pub sessions_today: u8,
    pub streak: u8,
    pub xp_today: u32,
    pub focus_duration_secs: u32,
    pub language: Language,
}

impl FocusContext {
    /// Empty context useful for early-boot calls before stats are loaded.
    pub fn empty(language: Language, focus_duration_secs: u32) -> Self {
        Self {
            sessions_today: 0,
            streak: 0,
            xp_today: 0,
            focus_duration_secs,
            language,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowSample {
    pub process_name: String,
    /// `None` when the OS hides the title (macOS without Screen Recording perm).
    pub window_title: Option<String>,
    pub elapsed_in_session_secs: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClassificationLabel {
    Focus,
    Distraction,
    Neutral,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassificationResult {
    pub label: ClassificationLabel,
    pub confidence: f32,
    /// Explainability hook: `Some("blocklist:tiktok")` etc.
    pub matched_rule: Option<String>,
}

impl ClassificationResult {
    pub fn neutral() -> Self {
        Self {
            label: ClassificationLabel::Neutral,
            confidence: 0.5,
            matched_rule: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaySummaryContext {
    pub date: String, // ISO YYYY-MM-DD
    pub sessions_completed: u8,
    pub total_focus_secs: u32,
    pub longest_streak: u8,
    pub level: u8,
    pub xp_gained: u32,
    pub language: Language,
}
