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

    // Rich context (FEAT — smarter coach)
    /// 0..23, local time at the moment the coach is being summoned.
    #[serde(default)]
    pub hour_of_day: u8,
    /// 0=Mon..6=Sun.
    #[serde(default)]
    pub weekday: u8,
    /// Confirmed distractions today (after the gate).
    #[serde(default)]
    pub distractions_today: u32,
    /// Total focus minutes the user has logged in the last 7 days.
    #[serde(default)]
    pub focus_minutes_7d: u32,
    /// Last process name flagged as distraction, if any (e.g. "TikTok").
    #[serde(default)]
    pub last_distraction: Option<String>,
    /// v1.3 Wave A3 — current session category if the user picked one
    /// (e.g. "Coding", "Writing", "Reading", "Deep work"). `None` falls
    /// back to the generic curated bank.
    #[serde(default)]
    pub category: Option<String>,
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
            hour_of_day: 12,
            weekday: 0,
            distractions_today: 0,
            focus_minutes_7d: 0,
            last_distraction: None,
            category: None,
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
