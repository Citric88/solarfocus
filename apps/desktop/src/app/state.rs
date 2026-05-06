//! v1.7.0 — App state struct + supporting state types.
//!
//! All fields are `pub(crate)` so view modules under `ui/views/` and
//! handler modules under `app/update/` can read them without an
//! explosion of getters. The struct is otherwise unchanged from
//! main.rs's previous home.

use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use solar_focus_intelligence::{
    ClassificationLabel, ClassificationResult, Coach, DistractionClassifier, Summarizer,
};
use solar_focus_core::focus_rules::FocusRulesEngine;

use crate::infra::persistence::SessionRepository;
use crate::infra::settings::Settings;
use crate::ui::sidebar::Route;
use crate::SolarFocusCore;
#[cfg(any(feature = "calendar", feature = "presence", feature = "llm"))]
use crate::infra;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupTab {
    General,
    Ai,
    Calibration,
    Privacy,
    Plugins,
    About,
}
impl Default for SetupTab {
    fn default() -> Self {
        SetupTab::General
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WizardStep {
    Welcome,
    Profile,
    Download,
    Done,
}

/// v1.13.0 — guided calibration wizard. Captures 10 frames per stage,
/// runs the matching ONNX inference, computes per-stage mean + stddev,
/// and proposes a threshold halfway between the two contrastive
/// distributions when their separation is statistically meaningful
/// (≥ 2σ). The wizard never overrides a default unless separation is
/// good enough — small datasets keep their defaults so a
/// noisy environment can't make the app worse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // some variants only consumed under cfg(feature = "presence")
pub enum CalibrationStage {
    Welcome,
    FaceWith,
    FaceWithout,
    PhoneWith,
    PhoneWithout,
    Summary,
}

#[derive(Debug, Clone, Default)]
pub struct CalibrationWizardState {
    pub stage: CalibrationStage,
    pub face_with: Vec<f32>,
    pub face_without: Vec<f32>,
    pub phone_with: Vec<f32>,
    pub phone_without: Vec<f32>,
    pub capturing: bool,
    pub suggested_face: Option<f32>,
    pub suggested_phone: Option<f32>,
    /// True when the suggested threshold came from a marginal
    /// separation (1σ ≤ Δ < 2σ). Surfaces a warning on the Summary
    /// so the user knows the model barely separated their data.
    pub face_marginal: bool,
    pub phone_marginal: bool,
    /// v1.13.0 — full analysis stored so the Summary can render
    /// expected error rate, overlap warnings, and an actionable
    /// recommendation instead of a vague badge. Format:
    /// `(quality_str, error_pct, overlap, m_with, m_without)`.
    pub face_quality: Option<(String, u32, bool, f32, f32)>,
    pub phone_quality: Option<(String, u32, bool, f32, f32)>,

    /// v1.13.0 — proactive per-stage warning shown immediately after
    /// a batch finishes if the data looks broken (e.g. score never
    /// rises above noise floor for "FaceWith", or stays equally high
    /// for "FaceWithout"). When set, the wizard pauses on the just-
    /// finished stage and offers Reintentar / Continuar.
    pub stage_warning: Option<String>,
}

impl Default for CalibrationStage {
    fn default() -> Self {
        CalibrationStage::Welcome
    }
}

#[derive(Debug, Clone)]
pub struct DownloadSnapshot {
    pub downloaded: u64,
    pub total: u64,
    pub bytes_per_sec: u64,
    pub verifying: bool,
}

#[derive(Debug, Clone)]
pub struct Toast {
    pub text: String,
    pub expires_at: Instant,
}

#[cfg(feature = "llm")]
#[derive(Clone)]
pub struct LoadedEngines(pub std::sync::Arc<infra::llm::LlmRuntime>);

#[cfg(not(feature = "llm"))]
#[derive(Clone, Debug)]
pub struct LoadedEngines;

#[cfg(feature = "llm")]
impl std::fmt::Debug for LoadedEngines {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "LoadedEngines(<runtime>)")
    }
}

/// Result of probing the foreground-window API.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionStatus {
    Unknown,
    Granted,
    NameOnly,
    Denied,
}

impl Default for PermissionStatus {
    fn default() -> Self {
        PermissionStatus::Unknown
    }
}

pub struct App {
    pub(crate) pomodoro_engine: SolarFocusCore::PomodoroEngine,
    pub(crate) session_repo: Option<SessionRepository>,
    pub(crate) last_state_was_completed: bool,

    pub(crate) settings: Settings,
    pub(crate) coach: Arc<dyn Coach>,
    pub(crate) summarizer: Arc<dyn Summarizer>,
    pub(crate) classifier: Arc<dyn DistractionClassifier>,
    pub(crate) last_coaching: Option<String>,
    pub(crate) last_classification: Option<ClassificationResult>,
    pub(crate) sessions_today: u8,
    pub(crate) settings_open: bool,
    pub(crate) session_started_at: Option<Instant>,
    pub(crate) session_started_at_utc: Option<chrono::DateTime<chrono::Utc>>,

    pub(crate) focus_rules: FocusRulesEngine,
    pub(crate) consecutive_distraction_samples: u8,
    pub(crate) toast: Option<Toast>,

    pub(crate) distractions_today: u32,

    // v1.9.0 — seed counter cache. Updated at boot + after each
    // SessionCompleted; reads come from `repo.total_seeds()`. The
    // `_last` field lets the toast announce *this* session's award.
    pub(crate) seeds_total_cache: u32,
    pub(crate) seeds_awarded_last: u32,

    // v1.12.2 — last export path. Surfaced inline on the Privacy card
    // because the toast only renders on the Focus canvas, so users
    // exporting from Privacy never saw confirmation that the file was
    // actually written.
    pub(crate) last_export_path: Option<std::path::PathBuf>,
    pub(crate) last_export_error: Option<String>,

    // v1.13.0 — last "Probar detección ahora" results, displayed in
    // the Calibración tab. Each entry is None until the user runs the
    // corresponding test.
    pub(crate) last_window_test:
        Option<(String, Option<String>, f32, ClassificationLabel)>,
    #[cfg(feature = "presence")]
    pub(crate) last_face_test: Option<(infra::presence::Presence, f32)>,
    #[cfg(feature = "presence")]
    pub(crate) last_phone_test: Option<f32>,

    // v1.13.0 — cooldown indicator. True when last coach call was
    // bypassed because the user voted 👎 within
    // `settings.coach_negative_cooldown_mins`.
    pub(crate) coach_in_curated_cooldown: bool,

    // v1.13.0 — guided calibration wizard. None when not active.
    pub(crate) calibration_wizard: Option<CalibrationWizardState>,

    // v1.12.0 — loaded plugins (Vec, oldest first). Each carries its
    // enable flag; a single Reload action rescans the dir.
    pub(crate) plugins: Vec<crate::infra::plugins::Plugin>,

    pub(crate) permission_status: PermissionStatus,

    pub(crate) confirming_clear: bool,

    pub(crate) download_active: Arc<AtomicBool>,
    #[allow(dead_code)]
    pub(crate) download_progress: Arc<StdMutex<Option<DownloadSnapshot>>>,
    pub(crate) download_error: Option<String>,
    pub(crate) download_cancel: Arc<AtomicBool>,

    pub(crate) model_present_cache: Option<bool>,
    pub(crate) feedback_counts_cache: (u32, u32),

    pub(crate) last_summary_date: Option<String>,
    pub(crate) recap: Option<(String, String)>,

    pub(crate) route: Route,

    pub(crate) setup_tab: SetupTab,
    pub(crate) wizard_step: WizardStep,

    pub(crate) setup_show_advanced: bool,

    pub(crate) custom_focus_str: String,
    pub(crate) custom_break_str: String,
    pub(crate) custom_long_break_str: String,
    pub(crate) custom_category_str: String,

    #[cfg(feature = "calendar")]
    pub(crate) deadline_time_str: String,
    #[cfg(feature = "calendar")]
    pub(crate) calendar_reader: Option<std::sync::Arc<infra::calendar::ek::CalendarReader>>,
    #[cfg(feature = "calendar")]
    pub(crate) calendar_events: Vec<infra::calendar::CalendarEvent>,
    #[cfg(feature = "calendar")]
    pub(crate) calendar_error: Option<String>,

    #[cfg(feature = "presence")]
    pub(crate) presence_probe: Option<std::sync::Arc<infra::presence::PresenceProbe>>,
    #[cfg(feature = "presence")]
    pub(crate) consecutive_absent_samples: u8,
    #[cfg(feature = "presence")]
    pub(crate) last_presence: Option<infra::presence::Presence>,
    #[cfg(feature = "presence")]
    pub(crate) presence_error: Option<String>,
    #[cfg(feature = "presence")]
    pub(crate) last_yunet_at: Option<std::time::Instant>,
    #[cfg(feature = "presence")]
    pub(crate) last_yunet: Option<(infra::presence::Presence, chrono::DateTime<chrono::Local>)>,

    // v1.11.0 — phone detector cache + counter for the consecutive-frames
    // gate. Same shape as the YuNet pair so the UI can show last verdict.
    #[cfg(feature = "presence")]
    pub(crate) last_yolo_at: Option<std::time::Instant>,
    #[cfg(feature = "presence")]
    pub(crate) last_yolo_score: Option<f32>,
    #[cfg(feature = "presence")]
    pub(crate) consecutive_phone_samples: u8,
}

