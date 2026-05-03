//! # SolarFocus OS — App Principal
//!
//! v1.2.0-alpha:
//! - Engine Pomodoro from `solar-focus-core` (unchanged from v1.1.21).
//! - SQLite session persistence via `infra::persistence` (unchanged).
//! - NEW: AI-coaching slot in the UI driven by a `solar_focus_intelligence::Coach`
//!        (Phase 1 ships `MockCoach` — real LLM lands in Phase 3).
//! - NEW: Window watcher polled every N seconds during focus sessions.
//! - NEW: User settings persisted to JSON.

use iced::widget::{button, column, container, row, text};
use iced::{Color, Element, Length, Subscription, Task, window};

pub use solar_focus_core as SolarFocusCore;

use solar_focus_intelligence::{
    ClassificationLabel, ClassificationResult, Coach, CoachingTrigger, DistractionClassifier,
    FocusContext, Language, MockClassifier, MockCoach, MockSummarizer, RulesClassifier,
    Summarizer,
};
use solar_focus_core::focus_rules::FocusRulesEngine;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

#[cfg(feature = "llm")]
use infra::model_download::DownloadEvent;

mod infra;
mod ui;

use ui::sidebar::{Route, StatusPill};

use chrono::{Datelike, Utc};
use infra::persistence::SessionRepository;
use infra::settings::{ClassifierMode, Settings};
use infra::window_watch::WindowWatcher;

#[derive(Debug, Clone)]
pub enum Message {
    StartFocus,
    Pause,
    Resume,
    TakeBreak,
    TimerTick(f32),
    SessionCompleted,

    // v1.2 additions
    WindowProbe,
    ClassificationReady(ClassificationResult),
    CoachingReady(String),
    OpenSettings,
    CloseSettings,
    ToggleAi(bool),
    ToggleWindowWatch(bool),
    SetLanguage(Language),

    // Phase 2 additions
    SetClassifierMode(ClassifierMode),
    ShowToast { text: String, expires_in_secs: u64 },
    DismissToast,
    ToastTick, // periodic check to expire toasts

    // Phase 4 — model picker + thumbs feedback + RAM mode
    SetModelChoice(infra::settings::ModelChoice),
    SetRamMode(infra::settings::RamMode),
    ThumbsUp,
    ThumbsDown,

    // UI-1 — route navigation
    SwitchRoute(Route),

    // UI-4 — Setup tabs + first-run wizard
    SwitchSetupTab(SetupTab),
    WizardNext,
    WizardBack,
    WizardFinish,

    // WIRE-2 — runtime permission probe
    ProbePermission,
    PermissionProbed(PermissionStatus),

    // WIRE-3 — privacy tab actions
    OpenSystemSettings,
    RequestClearData,
    ConfirmClearData,
    CancelClearData,

    // Phase 3.5b — model download flow
    StartModelDownload,
    SkipModelDownload,
    DownloadPoll,
    DownloadFinished(Result<String, String>), // Ok(path) | Err(message)
    /// PERF-1 — kick off the background LLM load (boot-time + post-download).
    SpawnEngineLoad,
    /// Phase 3.5c + PERF-1 — engine ready after async load. Carries the
    /// loaded runtime so the handler can hot-swap it into App.coach +
    /// App.summarizer without re-loading.
    LlmEngineLoaded(Result<LoadedEngines, String>),
    /// Phase 3.5c — manual debug trigger to generate today's recap now.
    GenerateRecapNow,
    /// WIRE-1 — delete the model file currently selected.
    DeleteModel,
    /// ENH-2 — cancel an in-flight download.
    CancelDownload,
    /// ENH-7 — fetch DistilBERT model + tokenizer.
    StartDistilbertDownload,
    DistilbertDownloadFinished(Result<(), String>),

    // Phase 3.5b — daily summary scheduler
    DailyRollCheck,
    DailySummaryReady { date: String, text: String },
    DismissRecap,
}

pub struct App {
    pomodoro_engine: SolarFocusCore::PomodoroEngine,
    session_repo: Option<SessionRepository>,
    last_state_was_completed: bool,

    // v1.2 fields
    settings: Settings,
    coach: Arc<dyn Coach>,
    summarizer: Arc<dyn Summarizer>,
    classifier: Arc<dyn DistractionClassifier>,
    last_coaching: Option<String>,
    last_classification: Option<ClassificationResult>,
    sessions_today: u8,
    settings_open: bool,
    session_started_at: Option<Instant>,

    // Phase 2 fields
    focus_rules: FocusRulesEngine,
    consecutive_distraction_samples: u8,
    toast: Option<Toast>,

    // WIRE-2 — counters reset at midnight via DailyRollCheck
    distractions_today: u32,

    // WIRE-2 — cached macOS Screen Recording permission probe
    permission_status: PermissionStatus,

    // WIRE-3 — destructive confirm gate for "Borrar todos los datos"
    confirming_clear: bool,

    // Phase 3.5b — download lifecycle.
    // download_progress is only *read* on llm builds; mark allow on default
    // builds to avoid the dead_code warning.
    download_active: Arc<AtomicBool>,
    #[allow(dead_code)]
    download_progress: Arc<StdMutex<Option<DownloadSnapshot>>>,
    download_error: Option<String>,
    /// ENH-2 — flag flipped by `Message::CancelDownload`. The download
    /// future polls this between chunks and bails when set.
    download_cancel: Arc<AtomicBool>,

    /// BUG-B — cached `model_present()` per ModelChoice + `feedback_counts()`.
    /// Reading the filesystem and querying SQLite on every render makes
    /// the AI tab visibly laggy. We refresh on Setup-tab open and after
    /// download/delete completion only.
    model_present_cache: Option<bool>,
    feedback_counts_cache: (u32, u32),

    // Phase 3.5b — daily summary scheduler
    last_summary_date: Option<String>, // ISO YYYY-MM-DD of the last day we summarized
    recap: Option<(String, String)>,   // (date, text) — shown as a card if Some

    // UI-1 — current canvas route
    route: Route,

    // UI-4 — Setup tabs + wizard
    setup_tab: SetupTab,
    wizard_step: WizardStep,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupTab {
    General,
    Ai,
    Privacy,
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

#[derive(Debug, Clone)]
pub struct DownloadSnapshot {
    pub downloaded: u64,
    pub total: u64,
    pub bytes_per_sec: u64,
    pub verifying: bool,
}

#[derive(Debug, Clone)]
struct Toast {
    text: String,
    expires_at: Instant,
}

/// PERF-1 — payload for `LlmEngineLoaded` carrying the freshly-loaded
/// LlmRuntime. Wrapped in Arc so the Message can be Clone (iced requires).
/// On non-llm builds this is a unit-like marker.
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

/// Result of probing the foreground-window API. Drives the Privacy / Stats
/// "permission status" badges. Cached on App; refreshed lazily.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionStatus {
    /// Haven't probed yet this session.
    Unknown,
    /// Got both process_name and a non-empty title — permission granted.
    Granted,
    /// Got process_name but no title — typical of macOS without Screen
    /// Recording permission.
    NameOnly,
    /// `active_win_pos_rs` returned Err — no window readable at all.
    Denied,
}

impl Default for PermissionStatus {
    fn default() -> Self {
        PermissionStatus::Unknown
    }
}

impl App {
    pub fn new() -> (Self, Task<Message>) {
        let engine = SolarFocusCore::PomodoroEngine::new();
        let session_repo = match SessionRepository::new() {
            Ok(r) => Some(r),
            Err(e) => {
                log::warn!("SQLite no disponible: {}", e);
                None
            }
        };

        let mut settings = Settings::load();

        // Phase 3.5c — auto-migrate Mock → Rules so the toast classifier
        // path actually runs for users whose settings.json was written
        // by alpha (which had Mock as default).
        if matches!(settings.classifier_mode, ClassifierMode::Mock) {
            log::info!("Migrating classifier_mode Mock → Rules (one-time)");
            settings.classifier_mode = ClassifierMode::Rules;
            settings.save();
        }

        let coach = build_coach(&settings);
        let summarizer = build_summarizer(&settings);
        let classifier = build_classifier(&settings);

        log::info!(
            "v1.2.0-rc3 boot — ai_enabled={}, language={:?}, poll={}s, classifier={:?}, coach_ready={}",
            settings.ai_enabled,
            settings.language,
            settings.window_poll_secs,
            settings.classifier_mode,
            coach.is_ready(),
        );

        // Pre-load the most recent recap so it can render on first paint.
        let recap = session_repo
            .as_ref()
            .and_then(|r| r.latest_summary().ok().flatten());

        // FIX-1: derive `last_summary_date` from the actual most-recent
        // summary instead of today_iso_local(). Otherwise a launch on day
        // N+1 immediately marks today as "already summarized" and yesterday's
        // recap never fires from DailyRollCheck.
        let last_summary_date = recap.as_ref().map(|(d, _)| d.clone());

        // FIX-2: initialize today's counters from DB so same-day relaunches
        // preserve the running count.
        let today_iso = today_iso_local().unwrap_or_default();
        let sessions_today = session_repo
            .as_ref()
            .and_then(|r| r.sessions_for_date(&today_iso).ok())
            .map(|rows| {
                rows.iter()
                    .filter(|r| r.state == "completed")
                    .count() as u8
            })
            .unwrap_or(0);

        // distractions_today is intentionally ephemeral — we don't persist
        // them to DB (would require a new schema). Restarting the app
        // resets this counter; documented behavior for v1.2.0.
        let distractions_today = 0;

        // BUG-B: pre-compute cache values before moving session_repo into Self.
        let model_present_cache: Option<bool> = {
            #[cfg(feature = "llm")]
            {
                use infra::model_download::{manifest_for, model_present};
                manifest_for(settings.model_choice).map(model_present)
            }
            #[cfg(not(feature = "llm"))]
            {
                Some(false)
            }
        };
        let feedback_counts_cache: (u32, u32) = session_repo
            .as_ref()
            .and_then(|r| r.feedback_counts().ok())
            .unwrap_or((0, 0));

        (
            Self {
                pomodoro_engine: engine,
                session_repo,
                last_state_was_completed: false,
                settings,
                coach,
                summarizer,
                classifier,
                last_coaching: None,
                last_classification: None,
                sessions_today,
                settings_open: false,
                session_started_at: None,
                focus_rules: FocusRulesEngine::new(),
                consecutive_distraction_samples: 0,
                toast: None,
                distractions_today,
                permission_status: PermissionStatus::Unknown,
                confirming_clear: false,
                download_active: Arc::new(AtomicBool::new(false)),
                download_progress: Arc::new(StdMutex::new(None)),
                download_error: None,
                download_cancel: Arc::new(AtomicBool::new(false)),
                model_present_cache,
                feedback_counts_cache,
                last_summary_date,
                recap,
                route: Route::default(),
                setup_tab: SetupTab::default(),
                wizard_step: WizardStep::Welcome,
            },
            // PERF-1: probe permission AND kick off the background LLM load
            // (latter is a no-op if no model file present or feature off).
            // Both tasks run in parallel; window paints immediately.
            {
                let mut tasks = vec![Task::done(Message::ProbePermission)];
                if should_attempt_llm_load(&Settings::load()) {
                    tasks.push(Task::done(Message::SpawnEngineLoad));
                }
                Task::batch(tasks)
            },
        )
    }

    fn rebuild_classifier(&mut self) {
        self.classifier = build_classifier(&self.settings);
        log::info!("Classifier rebuilt: mode={:?}", self.settings.classifier_mode);
    }

    /// BUG-B — refresh the cached probes used by Setup → AI panel.
    /// Cheap when called once per route switch / download finish.
    fn refresh_setup_caches(&mut self) {
        #[cfg(feature = "llm")]
        {
            use infra::model_download::{manifest_for, model_present};
            self.model_present_cache = manifest_for(self.settings.model_choice).map(model_present);
        }
        #[cfg(not(feature = "llm"))]
        {
            self.model_present_cache = Some(false);
        }
        if let Some(repo) = self.session_repo.as_ref() {
            self.feedback_counts_cache = repo.feedback_counts().unwrap_or((0, 0));
        }
    }

    /// Build the DaySummaryContext for `date` and dispatch it to the
    /// active Summarizer (real LLM if loaded, MockSummarizer otherwise).
    fn dispatch_summary_for(&self, date: String) -> Task<Message> {
        let repo = match self.session_repo.as_ref() {
            Some(r) => r,
            None => return Task::none(),
        };
        let rows = repo.sessions_for_date(&date).unwrap_or_default();
        if rows.is_empty() {
            log::info!("No sessions for {} — skipping summary", date);
            return Task::none();
        }
        let sessions_completed = rows
            .iter()
            .filter(|r| r.state == "completed")
            .count() as u8;
        let total_focus_secs = rows
            .iter()
            .filter(|r| r.state == "completed")
            .map(|r| r.duration as u32)
            .sum::<u32>();
        let ctx = solar_focus_intelligence::DaySummaryContext {
            date: date.clone(),
            sessions_completed,
            total_focus_secs,
            longest_streak: sessions_completed,
            level: 1,
            xp_gained: total_focus_secs / 10,
            language: self.settings.language,
        };
        let fut = self.summarizer.daily_summary(&ctx);
        let date_for_msg = date.clone();
        let canned_fallback = solar_focus_intelligence::prompts::summary_canned(&ctx);
        Task::perform(fut, move |result| match result {
            Ok(text) => Message::DailySummaryReady {
                date: date_for_msg.clone(),
                text,
            },
            Err(e) => {
                log::warn!("Summarizer failed: {e} — using canned fallback");
                Message::DailySummaryReady {
                    date: date_for_msg.clone(),
                    text: canned_fallback.clone(),
                }
            }
        })
    }

    #[cfg(feature = "llm")]
    fn spawn_download(&mut self) -> Task<Message> {
        use infra::model_download::{download_model, manifest_for};
        let Some(manifest) = manifest_for(self.settings.model_choice) else {
            log::error!("No manifest for {:?}", self.settings.model_choice);
            return Task::none();
        };
        let progress = self.download_progress.clone();
        let active = self.download_active.clone();
        let cancel = self.download_cancel.clone();
        cancel.store(false, Ordering::Relaxed);
        active.store(true, Ordering::Relaxed);
        *progress.lock().unwrap() = Some(DownloadSnapshot {
            downloaded: 0,
            total: manifest.size_bytes,
            bytes_per_sec: 0,
            verifying: false,
        });
        log::info!(
            "Starting download: {} ({:.1} MB)",
            manifest.filename,
            manifest.size_bytes as f64 / 1_048_576.0
        );

        let progress_for_cb = progress.clone();
        let cancel_for_fut = cancel.clone();
        let fut = async move {
            let result = download_model(manifest, cancel_for_fut, move |evt| {
                let mut snap = progress_for_cb.lock().unwrap();
                let cur = snap.clone().unwrap_or(DownloadSnapshot {
                    downloaded: 0,
                    total: manifest.size_bytes,
                    bytes_per_sec: 0,
                    verifying: false,
                });
                let new = match evt {
                    DownloadEvent::Started { total_bytes } => DownloadSnapshot {
                        downloaded: 0,
                        total: total_bytes,
                        bytes_per_sec: 0,
                        verifying: false,
                    },
                    DownloadEvent::Progress {
                        downloaded,
                        total,
                        bytes_per_sec,
                    } => DownloadSnapshot {
                        downloaded,
                        total,
                        bytes_per_sec,
                        verifying: false,
                    },
                    DownloadEvent::Verifying => DownloadSnapshot {
                        verifying: true,
                        ..cur
                    },
                    DownloadEvent::Complete { .. } => DownloadSnapshot {
                        downloaded: cur.total,
                        verifying: false,
                        ..cur
                    },
                    DownloadEvent::Error(_) => cur,
                };
                *snap = Some(new);
            })
            .await;
            active.store(false, Ordering::Relaxed);
            match result {
                Ok(p) => Message::DownloadFinished(Ok(p.display().to_string())),
                Err(e) => Message::DownloadFinished(Err(e.to_string())),
            }
        };
        Task::perform(fut, |m| m)
    }

    #[cfg(not(feature = "llm"))]
    fn spawn_download(&mut self) -> Task<Message> {
        log::warn!("Download requested but binary built without `llm` feature");
        Task::none()
    }

    #[cfg(feature = "llm")]
    fn spawn_engine_load(&mut self) -> Task<Message> {
        use infra::llm::{LlmRuntime, LoadOpts};
        use infra::model_download::{manifest_for, model_path};
        let Some(manifest) = manifest_for(self.settings.model_choice) else {
            return Task::done(Message::LlmEngineLoaded(Err("no manifest".into())));
        };
        let path = model_path(manifest);
        let fut = async move {
            match LlmRuntime::load(&path, LoadOpts::default()).await {
                // PERF-1: keep the runtime alive in the Message; the handler
                // wraps it in LlmCoach + LlmSummarizer and stores both.
                Ok(rt) => Ok(LoadedEngines(std::sync::Arc::new(rt))),
                Err(e) => Err(e.to_string()),
            }
        };
        Task::perform(fut, Message::LlmEngineLoaded)
    }

    #[cfg(not(feature = "llm"))]
    fn spawn_engine_load(&mut self) -> Task<Message> {
        Task::done(Message::LlmEngineLoaded(Err(
            "binary built without llm feature".into(),
        )))
    }
}

fn today_iso_local() -> Option<String> {
    Some(chrono::Local::now().format("%Y-%m-%d").to_string())
}

fn yesterday_iso_local() -> Option<String> {
    let d = chrono::Local::now().date_naive() - chrono::Duration::days(1);
    Some(d.format("%Y-%m-%d").to_string())
}

impl App {

    pub fn title(&self) -> String {
        match self.pomodoro_engine.state() {
            SolarFocusCore::AppState::Idle => "SolarFocus OS - Esperando...".to_string(),
            SolarFocusCore::AppState::Focusing(_) => "SolarFocus OS - En Foco".to_string(),
            SolarFocusCore::AppState::Break => "SolarFocus OS - Descanso".to_string(),
            SolarFocusCore::AppState::Completed => "SolarFocus OS - Completado".to_string(),
        }
    }

    fn focus_context(&self) -> FocusContext {
        FocusContext {
            sessions_today: self.sessions_today,
            streak: self.pomodoro_engine.sessions_completed(),
            xp_today: 0, // wired in Phase 4 alongside RewardsSystem
            focus_duration_secs: self.pomodoro_engine.config().focus_duration as u32,
            language: self.settings.language,
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        let mut subs: Vec<Subscription<Message>> = Vec::new();

        if !self.pomodoro_engine.is_paused()
            && matches!(
                self.pomodoro_engine.state(),
                SolarFocusCore::AppState::Focusing(_) | SolarFocusCore::AppState::Break
            )
        {
            subs.push(
                iced::time::every(Duration::from_millis(100))
                    .map(|_| Message::TimerTick(0.1)),
            );
        }

        if self.settings.window_watch_enabled
            && matches!(
                self.pomodoro_engine.state(),
                SolarFocusCore::AppState::Focusing(_)
            )
        {
            let secs = self.settings.window_poll_secs.max(1) as u64;
            subs.push(
                iced::time::every(Duration::from_secs(secs)).map(|_| Message::WindowProbe),
            );
        }

        // Toast lifecycle ticks at 1 Hz while a toast is showing.
        if self.toast.is_some() {
            subs.push(iced::time::every(Duration::from_secs(1)).map(|_| Message::ToastTick));
        }

        // Download progress poller: 4 Hz while a download is active.
        if self.download_active.load(Ordering::Relaxed) {
            subs.push(
                iced::time::every(Duration::from_millis(250)).map(|_| Message::DownloadPoll),
            );
        }

        // Daily roll-over check: once per minute, always on.
        subs.push(iced::time::every(Duration::from_secs(60)).map(|_| Message::DailyRollCheck));

        // UI-1 — keyboard shortcuts.
        subs.push(iced::keyboard::on_key_press(|key, _modifiers| {
            use iced::keyboard::key::{Key, Named};
            match key.as_ref() {
                Key::Named(Named::Space) => Some(Message::Pause),
                Key::Character("r") | Key::Character("R") => Some(Message::Resume),
                Key::Character("p") | Key::Character("P") => Some(Message::Pause),
                Key::Character("b") | Key::Character("B") => Some(Message::TakeBreak),
                Key::Character("s") | Key::Character("S") => {
                    Some(Message::SwitchRoute(Route::Setup))
                }
                Key::Character("1") => Some(Message::SwitchRoute(Route::Focus)),
                Key::Character("2") => Some(Message::SwitchRoute(Route::Stats)),
                Key::Character("3") => Some(Message::SwitchRoute(Route::Coach)),
                Key::Character("4") => Some(Message::SwitchRoute(Route::Setup)),
                _ => None,
            }
        }));

        Subscription::batch(subs)
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::StartFocus => {
                log::info!("Iniciando sesión de enfoque");
                self.pomodoro_engine.start_focus();
                self.last_state_was_completed = false;
                self.last_classification = None;
                self.session_started_at = Some(std::time::Instant::now());

                if self.settings.ai_enabled {
                    let fut =
                        self.coach
                            .coaching_message(CoachingTrigger::SessionStart, &self.focus_context());
                    Task::perform(fut, |result| match result {
                        Ok(s) => Message::CoachingReady(s),
                        Err(e) => {
                            log::warn!("Coach error: {e}");
                            Message::CoachingReady(String::new())
                        }
                    })
                } else {
                    Task::none()
                }
            }
            Message::Pause => {
                self.pomodoro_engine.pause(0.0);
                log::info!("Sesión pausada");
                Task::none()
            }
            Message::Resume => {
                self.pomodoro_engine.resume();
                log::info!("Sesión reanudada");
                Task::none()
            }
            Message::TakeBreak => {
                self.pomodoro_engine.transition_to_break();
                log::info!("Tomando descanso manual");
                if self.settings.ai_enabled {
                    let fut =
                        self.coach
                            .coaching_message(CoachingTrigger::BreakStart, &self.focus_context());
                    Task::perform(fut, |r| match r {
                        Ok(s) => Message::CoachingReady(s),
                        Err(_) => Message::CoachingReady(String::new()),
                    })
                } else {
                    Task::none()
                }
            }
            Message::TimerTick(delta) => {
                let was_focusing = matches!(
                    self.pomodoro_engine.state(),
                    SolarFocusCore::AppState::Focusing(_)
                );

                self.pomodoro_engine.tick(delta);

                let now_break = matches!(
                    self.pomodoro_engine.state(),
                    SolarFocusCore::AppState::Break
                );
                if was_focusing && now_break && !self.last_state_was_completed {
                    self.last_state_was_completed = true;
                    return Task::done(Message::SessionCompleted);
                }
                Task::none()
            }
            Message::SessionCompleted => {
                log::info!("Sesión completada — guardando en SQLite");
                self.sessions_today = self.sessions_today.saturating_add(1);
                if let Some(ref repo) = self.session_repo {
                    let duration = self.pomodoro_engine.config().focus_duration;
                    let record = infra::persistence::SessionRecord {
                        id: None,
                        start_time: Utc::now(),
                        duration,
                        state: "completed".to_string(),
                    };
                    match repo.save_session(&record) {
                        Ok(id) => log::info!("Sesión #{} guardada", id),
                        Err(e) => log::error!("Fallo guardando sesión: {}", e),
                    }
                }
                if self.settings.ai_enabled {
                    let fut = self
                        .coach
                        .coaching_message(CoachingTrigger::SessionComplete, &self.focus_context());
                    Task::perform(fut, |r| match r {
                        Ok(s) => Message::CoachingReady(s),
                        Err(_) => Message::CoachingReady(String::new()),
                    })
                } else {
                    Task::none()
                }
            }
            Message::WindowProbe => {
                let elapsed = self
                    .session_started_at
                    .map(|i| i.elapsed().as_secs() as u32)
                    .unwrap_or(0);
                if let Some(sample) = WindowWatcher::poll(elapsed) {
                    log::info!(
                        "Window probe: process='{}' title={:?}",
                        sample.process_name,
                        sample.window_title
                    );
                    let fut = self.classifier.classify(&sample);
                    return Task::perform(fut, |r| match r {
                        Ok(c) => Message::ClassificationReady(c),
                        Err(e) => {
                            log::warn!("Classifier error: {e}");
                            Message::ClassificationReady(ClassificationResult::neutral())
                        }
                    });
                }
                Task::none()
            }
            Message::ClassificationReady(c) => {
                log::info!(
                    "Classification: {:?} conf={:.2} rule={:?}",
                    c.label,
                    c.confidence,
                    c.matched_rule
                );
                let mut tasks: Vec<Task<Message>> = Vec::new();

                if c.label == ClassificationLabel::Distraction
                    && c.confidence >= self.settings.min_confidence
                {
                    self.consecutive_distraction_samples =
                        self.consecutive_distraction_samples.saturating_add(1);
                    if self.consecutive_distraction_samples >= self.settings.min_consecutive_samples
                    {
                        self.focus_rules.record_distraction();
                        self.distractions_today =
                            self.distractions_today.saturating_add(1);
                        log::warn!(
                            "Distraction confirmed (consecutive={}, today={}, rule={:?})",
                            self.consecutive_distraction_samples,
                            self.distractions_today,
                            c.matched_rule
                        );
                        let toast_text = match self.settings.language {
                            Language::Es => match &c.matched_rule {
                                Some(r) => format!("Distracción detectada ({}). ¿Pausa o vuelves?", r),
                                None => "Distracción detectada. ¿Pausa o vuelves?".to_string(),
                            },
                            Language::En => match &c.matched_rule {
                                Some(r) => format!("Distraction detected ({}). Pause or refocus?", r),
                                None => "Distraction detected. Pause or refocus?".to_string(),
                            },
                        };
                        tasks.push(Task::done(Message::ShowToast {
                            text: toast_text,
                            expires_in_secs: 4,
                        }));
                        self.consecutive_distraction_samples = 0;
                    }
                } else {
                    // Streak broken — reset the gate
                    self.consecutive_distraction_samples = 0;
                }

                self.last_classification = Some(c);
                Task::batch(tasks)
            }
            Message::CoachingReady(s) => {
                if !s.is_empty() {
                    log::info!("Coach: {}", s);
                    self.last_coaching = Some(sanitize_for_display(&s));
                }
                Task::none()
            }
            Message::OpenSettings => {
                self.settings_open = true;
                Task::none()
            }
            Message::CloseSettings => {
                self.settings_open = false;
                self.settings.save();
                Task::none()
            }
            Message::ToggleAi(v) => {
                self.settings.ai_enabled = v;
                Task::none()
            }
            Message::ToggleWindowWatch(v) => {
                self.settings.window_watch_enabled = v;
                Task::none()
            }
            Message::SetLanguage(lang) => {
                self.settings.language = lang;
                Task::none()
            }
            Message::SetClassifierMode(mode) => {
                self.settings.classifier_mode = mode;
                self.rebuild_classifier();
                Task::none()
            }
            Message::ShowToast { text, expires_in_secs } => {
                let expires_at = Instant::now() + Duration::from_secs(expires_in_secs);
                log::info!("Toast: {} (TTL {}s)", text, expires_in_secs);
                self.toast = Some(Toast { text, expires_at });
                Task::none()
            }
            Message::DismissToast => {
                self.toast = None;
                Task::none()
            }
            Message::ToastTick => {
                if let Some(t) = &self.toast {
                    if Instant::now() >= t.expires_at {
                        self.toast = None;
                    }
                }
                Task::none()
            }

            Message::StartModelDownload => {
                self.download_error = None;
                self.spawn_download()
            }
            Message::SkipModelDownload => {
                log::info!("User skipped first-run model download");
                self.settings.model_download_skipped = true;
                self.settings.save();
                Task::none()
            }
            Message::DownloadPoll => {
                // Force a re-render so the progress bar refreshes from the
                // shared `download_progress` state. No state change needed.
                Task::none()
            }
            Message::DownloadFinished(result) => {
                match result {
                    Ok(p) => {
                        log::info!("Model downloaded: {}", p);
                        self.download_error = None;
                        self.refresh_setup_caches();
                        self.toast = Some(Toast {
                            text: match self.settings.language {
                                Language::Es => "Modelo descargado. Cargando coach IA…".to_string(),
                                Language::En => "Model downloaded. Loading AI coach…".to_string(),
                            },
                            expires_at: Instant::now() + Duration::from_secs(8),
                        });
                        return self.spawn_engine_load();
                    }
                    Err(e) => {
                        log::error!("Model download failed: {}", e);
                        let user_facing = if e.contains("cancelled") || e.contains("Cancelled") {
                            match self.settings.language {
                                Language::Es => "Descarga cancelada (puedes reanudar desde donde quedaste).".to_string(),
                                Language::En => "Download cancelled (you can resume from where you left off).".to_string(),
                            }
                        } else if e.contains("disk space") || e.contains("space") {
                            match self.settings.language {
                                Language::Es => format!("Sin espacio: {}", e),
                                Language::En => format!("Out of space: {}", e),
                            }
                        } else {
                            e
                        };
                        self.download_error = Some(user_facing);
                    }
                }
                Task::none()
            }
            Message::SpawnEngineLoad => {
                log::info!("Spawning background LLM load…");
                self.toast = Some(Toast {
                    text: match self.settings.language {
                        Language::Es => "Cargando coach IA…".to_string(),
                        Language::En => "Loading AI coach…".to_string(),
                    },
                    expires_at: Instant::now() + Duration::from_secs(20),
                });
                self.spawn_engine_load()
            }
            Message::LlmEngineLoaded(result) => {
                match result {
                    Ok(_loaded) => {
                        #[cfg(feature = "llm")]
                        {
                            use infra::llm_coach::{LlmCoach, LlmSummarizer};
                            let runtime = _loaded.0;
                            self.coach = Arc::new(LlmCoach::new(runtime.clone()));
                            self.summarizer = Arc::new(LlmSummarizer::new(runtime));
                            log::info!(
                                "Hot-swap complete: coach_ready={}, summarizer_ready={}",
                                self.coach.is_ready(),
                                self.summarizer.is_ready()
                            );
                            self.toast = Some(Toast {
                                text: match self.settings.language {
                                    Language::Es => "Coach IA listo".to_string(),
                                    Language::En => "AI coach ready".to_string(),
                                },
                                expires_at: Instant::now() + Duration::from_secs(4),
                            });
                        }
                    }
                    Err(e) => {
                        log::warn!("LLM load failed: {e}");
                        self.toast = Some(Toast {
                            text: format!("LLM load failed: {e}"),
                            expires_at: Instant::now() + Duration::from_secs(6),
                        });
                    }
                }
                Task::none()
            }
            Message::GenerateRecapNow => {
                let target = today_iso_local().unwrap_or_else(|| "today".into());
                self.dispatch_summary_for(target)
            }
            Message::SetModelChoice(choice) => {
                if self.settings.model_choice != choice {
                    self.settings.model_choice = choice;
                    self.settings.model_download_skipped = false;
                    self.settings.save();
                    log::info!("Model choice → {:?}", choice);

                    // ENH-5: if the chosen model is missing, auto-trigger
                    // the download instead of forcing the user to click again.
                    #[cfg(feature = "llm")]
                    {
                        use infra::model_download::{manifest_for, model_present};
                        if let Some(m) = manifest_for(choice) {
                            if !model_present(m) {
                                self.toast = Some(Toast {
                                    text: match self.settings.language {
                                        Language::Es => format!("Descargando {:?}…", choice),
                                        Language::En => format!("Downloading {:?}…", choice),
                                    },
                                    expires_at: Instant::now() + Duration::from_secs(4),
                                });
                                return self.spawn_download();
                            }
                        }
                    }

                    // Already present (or non-llm build) — just notify.
                    self.toast = Some(Toast {
                        text: match self.settings.language {
                            Language::Es => format!("Modelo: {:?} (reinicia para aplicar)", choice),
                            Language::En => format!("Model: {:?} (restart to apply)", choice),
                        },
                        expires_at: Instant::now() + Duration::from_secs(5),
                    });
                }
                Task::none()
            }
            Message::SetRamMode(mode) => {
                self.settings.ram_mode = mode;
                self.settings.apply_ram_mode();
                self.settings.save();
                self.rebuild_classifier();
                self.coach = build_coach(&self.settings);
                self.summarizer = build_summarizer(&self.settings);
                log::info!("RAM mode → {:?} (applied)", mode);
                Task::none()
            }
            Message::SwitchSetupTab(t) => {
                self.setup_tab = t;
                self.refresh_setup_caches();
                Task::none()
            }
            Message::WizardNext => {
                self.wizard_step = match self.wizard_step {
                    WizardStep::Welcome => WizardStep::Profile,
                    WizardStep::Profile => WizardStep::Download,
                    WizardStep::Download | WizardStep::Done => WizardStep::Done,
                };
                Task::none()
            }
            Message::WizardBack => {
                self.wizard_step = match self.wizard_step {
                    WizardStep::Welcome => WizardStep::Welcome,
                    WizardStep::Profile => WizardStep::Welcome,
                    WizardStep::Download => WizardStep::Profile,
                    WizardStep::Done => WizardStep::Done,
                };
                Task::none()
            }
            Message::WizardFinish => {
                self.settings.first_run = false;
                self.settings.save();
                self.wizard_step = WizardStep::Done;

                // FIX-3: if user picked Full mode but never triggered the
                // download in the wizard (and the model isn't already on
                // disk), land them on Setup → AI so the next visible step
                // is obvious. Otherwise → Focus.
                let needs_model_setup = self.settings.ram_mode
                    == infra::settings::RamMode::Full
                    && {
                        #[cfg(feature = "llm")]
                        {
                            use infra::model_download::{manifest_for, model_present};
                            manifest_for(self.settings.model_choice)
                                .map(|m| !model_present(m))
                                .unwrap_or(false)
                        }
                        #[cfg(not(feature = "llm"))]
                        {
                            false
                        }
                    };
                if needs_model_setup {
                    self.route = Route::Setup;
                    self.setup_tab = SetupTab::Ai;
                    self.toast = Some(Toast {
                        text: match self.settings.language {
                            Language::Es => "Descarga el modelo IA cuando estés listo.".to_string(),
                            Language::En => "Download the AI model when you're ready.".to_string(),
                        },
                        expires_at: Instant::now() + Duration::from_secs(6),
                    });
                } else {
                    self.route = Route::Focus;
                }
                Task::none()
            }
            Message::SwitchRoute(r) => {
                if r != self.route {
                    log::info!("Route → {:?}", r);
                    self.route = r;
                    if self.route != Route::Setup {
                        self.settings_open = false;
                    }
                    // BUG-B: refresh caches when entering Setup or Stats.
                    if matches!(self.route, Route::Setup | Route::Stats) {
                        self.refresh_setup_caches();
                    }
                    // PERF-2: only re-probe permission if it's still Unknown
                    // (initial probe may not have landed yet). Manual
                    // "Re-verificar" button in Privacy tab handles refresh
                    // after the user grants permission in System Settings.
                    if matches!(self.route, Route::Stats | Route::Setup)
                        && self.permission_status == PermissionStatus::Unknown
                    {
                        return Task::done(Message::ProbePermission);
                    }
                }
                Task::none()
            }
            Message::ProbePermission => {
                // PERF-2: never block the UI thread on the OS window query.
                // active-win-pos-rs can be slow under contention (LLM load
                // running, many windows open). spawn_blocking offloads it.
                Task::perform(
                    async {
                        tokio::task::spawn_blocking(probe_permission_now)
                            .await
                            .unwrap_or(PermissionStatus::Unknown)
                    },
                    Message::PermissionProbed,
                )
            }
            Message::PermissionProbed(status) => {
                if status != self.permission_status {
                    log::info!("Permission → {:?}", status);
                    self.permission_status = status;
                }
                Task::none()
            }
            Message::OpenSystemSettings => {
                #[cfg(target_os = "macos")]
                {
                    let _ = std::process::Command::new("open")
                        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
                        .spawn();
                    log::info!("Opened macOS Privacy → Screen Recording");
                }
                #[cfg(not(target_os = "macos"))]
                {
                    log::info!("Open System Settings is macOS-only");
                }
                Task::none()
            }
            Message::RequestClearData => {
                self.confirming_clear = true;
                Task::none()
            }
            Message::CancelClearData => {
                self.confirming_clear = false;
                Task::none()
            }
            Message::ConfirmClearData => {
                self.confirming_clear = false;
                let removed = wipe_all_local_data();
                log::warn!("Cleared local data: {} files/dirs removed", removed);
                self.toast = Some(Toast {
                    text: match self.settings.language {
                        Language::Es => format!("Datos borrados ({} entradas).", removed),
                        Language::En => format!("Data cleared ({} entries).", removed),
                    },
                    expires_at: Instant::now() + Duration::from_secs(5),
                });
                // Re-init persistence + settings to defaults so the running app
                // doesn't crash on stale handles.
                self.session_repo = SessionRepository::new().ok();
                self.settings = Settings::default();
                self.settings.save();
                self.recap = None;
                self.last_coaching = None;
                self.distractions_today = 0;
                self.sessions_today = 0;
                Task::none()
            }
            Message::ThumbsUp | Message::ThumbsDown => {
                // After persisting we'll refresh the cache so AI tab counts stay current.
                let rating = if matches!(message, Message::ThumbsUp) { 1 } else { -1 };
                let trigger = "session"; // generic — could differentiate per CoachingTrigger later
                let msg_text = self.last_coaching.clone().unwrap_or_default();
                if msg_text.is_empty() {
                    return Task::none();
                }
                let model_id = if cfg!(feature = "llm") && self.settings.ai_enabled {
                    match self.settings.model_choice {
                        infra::settings::ModelChoice::SmolLM2 => "smollm2-1.7b-instruct-q4_k_m",
                        infra::settings::ModelChoice::Llama1B => "llama-3.2-1b-instruct-q4_k_m",
                        infra::settings::ModelChoice::Qwen15 => "qwen2.5-1.5b-instruct-q4_k_m",
                    }
                } else {
                    "mock"
                };
                if let Some(repo) = self.session_repo.as_ref() {
                    if let Err(e) = repo.save_feedback(trigger, &msg_text, rating, model_id) {
                        log::warn!("save_feedback failed: {e}");
                    }
                }
                self.refresh_setup_caches();
                self.toast = Some(Toast {
                    text: match (rating, self.settings.language) {
                        (1, Language::Es) => "Gracias 👍".to_string(),
                        (1, Language::En) => "Thanks 👍".to_string(),
                        (_, Language::Es) => "Anotado 👎".to_string(),
                        (_, Language::En) => "Noted 👎".to_string(),
                    },
                    expires_at: Instant::now() + Duration::from_secs(2),
                });
                Task::none()
            }
            Message::StartDistilbertDownload => {
                #[cfg(feature = "classifier")]
                {
                    use infra::distilbert_download::download_distilbert;
                    let lang = self.settings.language;
                    let fut = async move {
                        match download_distilbert().await {
                            Ok(()) => Ok(()),
                            Err(e) => Err(format!("{}", e)),
                        }
                    };
                    let _ = lang; // placate compiler if classifier disabled
                    return Task::perform(fut, Message::DistilbertDownloadFinished);
                }
                #[cfg(not(feature = "classifier"))]
                {
                    log::warn!("DistilBERT download requested without classifier feature");
                    Task::none()
                }
            }
            Message::DistilbertDownloadFinished(result) => {
                let msg = match result {
                    Ok(()) => match self.settings.language {
                        Language::Es => "DistilBERT descargado.".to_string(),
                        Language::En => "DistilBERT downloaded.".to_string(),
                    },
                    Err(e) => format!("Error: {}", e),
                };
                log::info!("DistilBERT download → {msg}");
                self.toast = Some(Toast {
                    text: msg,
                    expires_at: Instant::now() + Duration::from_secs(6),
                });
                // Rebuild classifier so Distilbert mode tries the new file.
                if matches!(
                    self.settings.classifier_mode,
                    infra::settings::ClassifierMode::Distilbert
                ) {
                    self.rebuild_classifier();
                }
                Task::none()
            }
            Message::CancelDownload => {
                self.download_cancel.store(true, Ordering::Relaxed);
                log::info!("Download cancellation requested");
                Task::none()
            }
            Message::DeleteModel => {
                #[cfg(feature = "llm")]
                {
                    use infra::model_download::{manifest_for, model_path};
                    if let Some(m) = manifest_for(self.settings.model_choice) {
                        let p = model_path(m);
                        match std::fs::remove_file(&p) {
                            Ok(()) => {
                                log::info!("Deleted model file: {}", p.display());
                                self.coach = build_coach(&self.settings);
                                self.summarizer = build_summarizer(&self.settings);
                                self.refresh_setup_caches();
                                self.toast = Some(Toast {
                                    text: match self.settings.language {
                                        Language::Es => "Modelo eliminado.".to_string(),
                                        Language::En => "Model deleted.".to_string(),
                                    },
                                    expires_at: Instant::now() + Duration::from_secs(3),
                                });
                            }
                            Err(e) => log::warn!("Could not delete model file: {e}"),
                        }
                    }
                }
                Task::none()
            }

            Message::DailyRollCheck => {
                let today = match today_iso_local() {
                    Some(s) => s,
                    None => return Task::none(),
                };
                let last = self.last_summary_date.clone();
                if last.as_deref() == Some(today.as_str()) {
                    return Task::none();
                }
                // Date rollover: reset per-day counters BEFORE summarizing.
                self.distractions_today = 0;
                self.sessions_today = 0;
                self.last_summary_date = Some(today.clone());

                let yesterday = match yesterday_iso_local() {
                    Some(s) => s,
                    None => return Task::none(),
                };

                self.dispatch_summary_for(yesterday)
            }
            Message::DailySummaryReady { date, text } => {
                let text = sanitize_for_display(&text);
                if let Some(repo) = self.session_repo.as_ref() {
                    let model_id = if cfg!(feature = "llm") {
                        match self.settings.model_choice {
                            infra::settings::ModelChoice::SmolLM2 => "smollm2-1.7b-instruct-q4_k_m",
                            infra::settings::ModelChoice::Llama1B => "llama-3.2-1b-instruct-q4_k_m",
                            infra::settings::ModelChoice::Qwen15 => "qwen2.5-1.5b-instruct-q4_k_m",
                        }
                    } else {
                        "canned-v1"
                    };
                    let _ = repo.save_summary(&date, &text, model_id);
                }
                self.recap = Some((date, text));
                Task::none()
            }
            Message::DismissRecap => {
                self.recap = None;
                Task::none()
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        // UI-4: first-run wizard takes the whole window until completed.
        if self.settings.first_run && self.wizard_step != WizardStep::Done {
            return self.view_wizard();
        }

        let status = self.status_pill();
        let download_pct = self.sidebar_download_pct();
        let sidebar = ui::sidebar::view(self.route, status, download_pct, Message::SwitchRoute);

        let canvas: Element<'_, Message> = match self.route {
            Route::Focus => self.view_main(),
            Route::Stats => self.view_stats_placeholder(),
            Route::Coach => self.view_coach_placeholder(),
            Route::Setup => self.view_setup_tabs(),
        };

        iced::widget::row![sidebar, canvas].into()
    }

    /// ENH-4 — percentage to show on the Setup sidebar icon during download.
    fn sidebar_download_pct(&self) -> Option<u8> {
        if !self.download_active.load(Ordering::Relaxed) {
            return None;
        }
        let snap = self.download_progress.lock().unwrap().clone()?;
        if snap.total == 0 {
            return Some(0);
        }
        Some(((snap.downloaded as f64 / snap.total as f64) * 100.0).round() as u8)
    }

    fn status_pill(&self) -> StatusPill {
        // Distraction wins if we just flagged one and toast still showing.
        if let Some(ref c) = self.last_classification {
            if c.label == ClassificationLabel::Distraction
                && self.toast.is_some()
            {
                return StatusPill::Distraction;
            }
        }
        match self.pomodoro_engine.state() {
            SolarFocusCore::AppState::Idle => StatusPill::Idle,
            SolarFocusCore::AppState::Focusing(_) => StatusPill::Focusing,
            SolarFocusCore::AppState::Break => StatusPill::Break,
            SolarFocusCore::AppState::Completed => StatusPill::Idle,
        }
    }

    /// UI-3: Stats canvas — three cards (Today / Week / All-time) +
    /// weekly bar chart + recap card.
    fn view_stats_placeholder(&self) -> Element<'_, Message> {
        use ui::palette::*;

        // BUG-B: use cached counts.
        let (up, down) = self.feedback_counts_cache;
        let week = self
            .session_repo
            .as_ref()
            .and_then(|r| r.weekly_focus_seconds().ok())
            .unwrap_or_default();
        let (lifetime_n, lifetime_secs) = self
            .session_repo
            .as_ref()
            .and_then(|r| r.lifetime_totals().ok())
            .unwrap_or((0, 0));

        let today_secs: u32 = week.last().map(|(_, s)| *s).unwrap_or(0);
        let week_secs: u32 = week.iter().map(|(_, s)| *s).sum();

        let card = |title: &str, primary: String, secondary: &str| -> Element<'_, Message> {
            container(
                column![
                    text(title.to_string()).size(FONT_SMALL).color(TEXT_MUTED),
                    text(primary).size(FONT_TITLE).color(TEXT_PRIMARY),
                    text(secondary.to_string())
                        .size(FONT_SMALL)
                        .color(TEXT_SECONDARY),
                ]
                .spacing(SPACE_XS as u16),
            )
            .padding(SPACE_MD as u16)
            .width(Length::Fixed(200.0))
            .style(|_| container::Style {
                background: Some(iced::Background::Color(SURFACE)),
                border: iced::Border {
                    radius: 8.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
        };

        let cards = iced::widget::row![
            card(
                match self.settings.language {
                    Language::Es => "HOY",
                    Language::En => "TODAY",
                },
                format!("{}", self.sessions_today),
                &format!("{} min de foco", today_secs / 60),
            ),
            iced::widget::Space::with_width(SPACE_MD as f32),
            card(
                match self.settings.language {
                    Language::Es => "DISTRACCIONES",
                    Language::En => "DISTRACTIONS",
                },
                format!("{}", self.distractions_today),
                match self.settings.language {
                    Language::Es => "hoy",
                    Language::En => "today",
                },
            ),
            iced::widget::Space::with_width(SPACE_MD as f32),
            card(
                match self.settings.language {
                    Language::Es => "ESTA SEMANA",
                    Language::En => "THIS WEEK",
                },
                format!("{} min", week_secs / 60),
                &format!("{} sesiones de coaching", up + down),
            ),
            iced::widget::Space::with_width(SPACE_MD as f32),
            card(
                match self.settings.language {
                    Language::Es => "TOTAL",
                    Language::En => "ALL-TIME",
                },
                format!("{}", lifetime_n),
                &format!("{} h totales", lifetime_secs / 3600),
            ),
        ];

        // WIRE-2: permission status indicator.
        let (perm_color, perm_text_es, perm_text_en) = match self.permission_status {
            PermissionStatus::Granted => (
                ACCENT,
                "Permiso concedido — vigilancia completa",
                "Permission granted — full window watching",
            ),
            PermissionStatus::NameOnly => (
                WARNING,
                "Permiso parcial — solo nombre del proceso (concede Screen Recording para títulos)",
                "Partial permission — process names only (grant Screen Recording for titles)",
            ),
            PermissionStatus::Denied => (
                DANGER,
                "Sin permiso — no se puede leer la ventana activa",
                "No permission — can't read active window",
            ),
            PermissionStatus::Unknown => (
                TEXT_MUTED,
                "Verificando permiso…",
                "Checking permission…",
            ),
        };
        let perm_card: Element<'_, Message> = container(
            iced::widget::row![
                text("●").size(FONT_LEAD).color(perm_color),
                iced::widget::Space::with_width(SPACE_SM as f32),
                text(if self.settings.language == Language::Es {
                    perm_text_es
                } else {
                    perm_text_en
                })
                .size(FONT_SMALL)
                .color(TEXT_PRIMARY),
            ]
            .padding(SPACE_SM as u16),
        )
        .padding(SPACE_XS as u16)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(SURFACE)),
            border: iced::Border {
                radius: 6.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into();

        // WIRE-2: today's session list (drill-down).
        let today_iso = today_iso_local().unwrap_or_default();
        let today_sessions = self
            .session_repo
            .as_ref()
            .and_then(|r| r.sessions_for_date(&today_iso).ok())
            .unwrap_or_default();
        let sessions_list: Element<'_, Message> = if today_sessions.is_empty() {
            container(
                text(match self.settings.language {
                    Language::Es => "Aún no has completado sesiones hoy.",
                    Language::En => "No completed sessions yet today.",
                })
                .size(FONT_SMALL)
                .color(TEXT_MUTED),
            )
            .padding(SPACE_MD as u16)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(SURFACE)),
                border: iced::Border {
                    radius: 6.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
        } else {
            let rows: Vec<Element<'_, Message>> = today_sessions
                .into_iter()
                .map(|s| {
                    let when = s.start_time.format("%H:%M").to_string();
                    let mins = (s.duration / 60.0).round() as u32;
                    container(
                        iced::widget::row![
                            text(when).size(FONT_SMALL).color(TEXT_SECONDARY),
                            iced::widget::Space::with_width(SPACE_MD as f32),
                            text(format!("{} min · {}", mins, s.state))
                                .size(FONT_SMALL)
                                .color(TEXT_PRIMARY),
                        ]
                        .padding(SPACE_XS as u16),
                    )
                    .padding(SPACE_XS as u16)
                    .into()
                })
                .collect();
            container(column(rows).spacing(SPACE_XS as u16))
                .padding(SPACE_SM as u16)
                .width(Length::Fixed(560.0))
                .style(|_| container::Style {
                    background: Some(iced::Background::Color(SURFACE)),
                    border: iced::Border {
                        radius: 6.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .into()
        };

        // Weekly chart — convert ISO dates to single-letter weekday labels.
        let chart_bars: Vec<(String, u32)> = week
            .iter()
            .map(|(d, s)| {
                let parsed = chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok();
                let label = parsed
                    .map(|d| weekday_short(d.weekday()))
                    .unwrap_or("?".to_string());
                (label, s / 60)
            })
            .collect();

        let chart: Element<'_, Message> = iced::widget::Canvas::new(ui::chart::WeeklyChart::new(chart_bars))
            .width(Length::Fixed(560.0))
            .height(Length::Fixed(160.0))
            .into();
        let chart_card = container(
            column![
                text(match self.settings.language {
                    Language::Es => "Minutos de foco por día",
                    Language::En => "Focus minutes per day",
                })
                .size(FONT_SMALL)
                .color(TEXT_MUTED),
                chart,
            ]
            .spacing(SPACE_SM as u16),
        )
        .padding(SPACE_MD as u16)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(SURFACE)),
            border: iced::Border {
                radius: 8.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });

        let recap_card: Element<'_, Message> = if let Some((d, t)) = &self.recap {
            container(
                column![
                    iced::widget::row![
                        text(format!(
                            "{} {}",
                            match self.settings.language {
                                Language::Es => "Resumen de",
                                Language::En => "Recap of",
                            },
                            d
                        ))
                        .size(FONT_SMALL)
                        .color(TEXT_MUTED),
                        iced::widget::horizontal_space(),
                        ghost_button(
                            match self.settings.language {
                                Language::Es => "Regenerar",
                                Language::En => "Regenerate",
                            },
                            Message::GenerateRecapNow,
                        ),
                    ],
                    text(t.clone()).size(FONT_BODY).color(TEXT_PRIMARY),
                ]
                .spacing(SPACE_XS as u16),
            )
            .padding(SPACE_MD as u16)
            .width(Length::Fixed(560.0))
            .style(|_| container::Style {
                background: Some(iced::Background::Color(SURFACE_RAISED)),
                border: iced::Border {
                    radius: 8.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
        } else {
            // No recap yet — offer "Generate today's recap" button.
            container(
                iced::widget::row![
                    text(match self.settings.language {
                        Language::Es => "Aún no hay resumen del día.",
                        Language::En => "No daily recap yet.",
                    })
                    .size(FONT_SMALL)
                    .color(TEXT_MUTED),
                    iced::widget::horizontal_space(),
                    ghost_button(
                        match self.settings.language {
                            Language::Es => "Generar ahora",
                            Language::En => "Generate now",
                        },
                        Message::GenerateRecapNow,
                    ),
                ]
                .padding(SPACE_SM as u16),
            )
            .padding(SPACE_XS as u16)
            .width(Length::Fixed(560.0))
            .style(|_| container::Style {
                background: Some(iced::Background::Color(SURFACE)),
                border: iced::Border {
                    radius: 6.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
        };

        let sessions_title = text(match self.settings.language {
            Language::Es => "Sesiones de hoy",
            Language::En => "Today's sessions",
        })
        .size(FONT_BODY)
        .color(TEXT_SECONDARY);

        let body = column![
            text(match self.settings.language {
                Language::Es => "Estadísticas",
                Language::En => "Stats",
            })
            .size(FONT_TITLE)
            .color(TEXT_PRIMARY),
            perm_card,
            cards,
            chart_card,
            recap_card,
            sessions_title,
            sessions_list,
        ]
        .spacing(SPACE_LG as u16)
        .padding(SPACE_XL as u16)
        .max_width(680);

        container(body)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(BG)),
                ..Default::default()
            })
            .into()
    }

    /// UI-3: Coach canvas — model badge + most recent coaching message
    /// (large) with ghost thumbs + scrolling history of past feedback.
    fn view_coach_placeholder(&self) -> Element<'_, Message> {
        use ui::palette::*;

        let last = self
            .last_coaching
            .clone()
            .unwrap_or_else(|| match self.settings.language {
                Language::Es => "(Aún no hay mensajes del coach)".to_string(),
                Language::En => "(No coach messages yet)".to_string(),
            });
        let model_badge = if cfg!(feature = "llm") && self.coach.is_ready() {
            format!(
                "{} · {:?}",
                match self.settings.language {
                    Language::Es => "Modelo activo",
                    Language::En => "Active model",
                },
                self.settings.model_choice
            )
        } else {
            match self.settings.language {
                Language::Es => "Coach básico (sin LLM cargado)".to_string(),
                Language::En => "Basic coach (no LLM loaded)".to_string(),
            }
        };

        let live = container(
            column![
                text(model_badge).size(FONT_SMALL).color(TEXT_MUTED),
                text(last.clone()).size(FONT_LEAD).color(TEXT_PRIMARY),
                iced::widget::row![
                    iced::widget::horizontal_space(),
                    ghost_button("👍", Message::ThumbsUp),
                    iced::widget::Space::with_width(SPACE_XS as f32),
                    ghost_button("👎", Message::ThumbsDown),
                ],
            ]
            .spacing(SPACE_SM as u16),
        )
        .padding(SPACE_LG as u16)
        .width(Length::Fixed(640.0))
        .style(|_| container::Style {
            background: Some(iced::Background::Color(SURFACE)),
            border: iced::Border {
                radius: 10.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });

        let recent = self
            .session_repo
            .as_ref()
            .and_then(|r| r.recent_feedback(20).ok())
            .unwrap_or_default();

        let history_title = text(match self.settings.language {
            Language::Es => "Historial de feedback",
            Language::En => "Feedback history",
        })
        .size(FONT_BODY)
        .color(TEXT_SECONDARY);

        let history_items: Vec<Element<'_, Message>> = if recent.is_empty() {
            vec![text(match self.settings.language {
                Language::Es => "(Aún no has dejado feedback.)",
                Language::En => "(No feedback yet.)",
            })
            .size(FONT_SMALL)
            .color(TEXT_MUTED)
            .into()]
        } else {
            recent
                .into_iter()
                .map(|(when, rating, msg)| {
                    let glyph = if rating > 0 { "+" } else { "−" };
                    let glyph_color = if rating > 0 { ACCENT } else { DANGER };
                    container(
                        iced::widget::row![
                            text(glyph.to_string()).size(FONT_LEAD).color(glyph_color),
                            iced::widget::Space::with_width(SPACE_SM as f32),
                            column![
                                text(msg).size(FONT_SMALL).color(TEXT_PRIMARY),
                                text(when).size(FONT_TINY).color(TEXT_MUTED),
                            ]
                            .spacing(2),
                        ]
                        .padding(SPACE_SM as u16),
                    )
                    .padding(SPACE_XS as u16)
                    .width(Length::Fixed(640.0))
                    .style(|_| container::Style {
                        background: Some(iced::Background::Color(SURFACE)),
                        border: iced::Border {
                            radius: 6.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                    .into()
                })
                .collect()
        };

        let history_col = column(history_items).spacing(SPACE_XS as u16);

        let body = column![
            text(match self.settings.language {
                Language::Es => "Coach",
                Language::En => "Coach",
            })
            .size(FONT_TITLE)
            .color(TEXT_PRIMARY),
            live,
            history_title,
            history_col,
        ]
        .spacing(SPACE_LG as u16)
        .padding(SPACE_XL as u16)
        .max_width(720);

        container(body)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(BG)),
                ..Default::default()
            })
            .into()
    }


    /// UI-2: Focus canvas — hero timer, ring progress, single context-aware
    /// CTA, microcopy slot. No top bar (sidebar handles nav). No recap card
    /// (lives in Stats canvas). Toast overlays the top of the canvas.
    fn view_main(&self) -> Element<'_, Message> {
        use iced::widget::stack;
        use ui::palette::*;

        let progress = self.pomodoro_engine.progress();
        let is_paused = self.pomodoro_engine.is_paused();

        let (ring_color, time_color) = match self.pomodoro_engine.state() {
            SolarFocusCore::AppState::Focusing(_) => (ACCENT, TEXT_PRIMARY),
            SolarFocusCore::AppState::Break => (ON_BREAK, ON_BREAK),
            SolarFocusCore::AppState::Completed => (ACCENT, ACCENT),
            SolarFocusCore::AppState::Idle => (TEXT_MUTED, TEXT_MUTED),
        };

        // Hero ring + timer (320×320 stack).
        let timer_text = if matches!(
            self.pomodoro_engine.state(),
            SolarFocusCore::AppState::Idle
        ) {
            (self.pomodoro_engine.config().focus_duration as u32 / 60).to_string()
                + ":00"
        } else {
            self.pomodoro_engine.remaining_time_formatted()
        };

        let ring: Element<'_, Message> = iced::widget::Canvas::new(ui::ring::Ring::new(progress, ring_color))
            .width(Length::Fixed(320.0))
            .height(Length::Fixed(320.0))
            .into();
        let time_label: Element<'_, Message> = container(
            column![
                text(timer_text)
                    .size(72)
                    .color(time_color)
                    .font(iced::Font::MONOSPACE),
                text(self.state_label())
                    .size(FONT_SMALL)
                    .color(TEXT_SECONDARY),
            ]
            .spacing(SPACE_XS as u16)
            .align_x(iced::alignment::Horizontal::Center),
        )
        .width(Length::Fixed(320.0))
        .height(Length::Fixed(320.0))
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into();
        let hero: Element<'_, Message> = stack![ring, time_label].into();

        // Single context-aware CTA below the ring.
        let cta = self.cta_button(is_paused);

        // Microcopy or toast (toast wins).
        let microcopy: Element<'_, Message> = if let Some(t) = &self.toast {
            container(
                iced::widget::row![
                    text(t.text.clone()).size(FONT_BODY).color(BG),
                    iced::widget::horizontal_space(),
                    button(text("×").size(FONT_SMALL))
                        .on_press(Message::DismissToast)
                        .padding([2, 8])
                        .style(|_, _| button::Style {
                            background: Some(iced::Background::Color(WARNING)),
                            text_color: BG,
                            border: iced::Border {
                                radius: 3.0.into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        }),
                ]
                .padding(SPACE_SM as u16),
            )
            .padding(SPACE_SM as u16)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(WARNING)),
                border: iced::Border {
                    radius: 6.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .width(Length::Fixed(420.0))
            .into()
        } else if let Some(c) = &self.last_coaching {
            text(c.clone())
                .size(FONT_BODY)
                .color(TEXT_SECONDARY)
                .into()
        } else {
            text(self.idle_microcopy())
                .size(FONT_BODY)
                .color(TEXT_MUTED)
                .into()
        };

        let content = column![
            hero,
            iced::widget::Space::with_height(Length::Fixed(SPACE_LG as f32)),
            cta,
            iced::widget::Space::with_height(Length::Fixed(SPACE_MD as f32)),
            microcopy,
        ]
        .spacing(SPACE_MD as u16)
        .align_x(iced::alignment::Horizontal::Center);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .padding(SPACE_XL as u16)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(BG)),
                ..Default::default()
            })
            .into()
    }

    fn state_label(&self) -> String {
        match (self.pomodoro_engine.state(), self.settings.language) {
            (SolarFocusCore::AppState::Idle, Language::Es) => "Listo para comenzar".to_string(),
            (SolarFocusCore::AppState::Idle, Language::En) => "Ready to start".to_string(),
            (SolarFocusCore::AppState::Focusing(_), Language::Es) if self.pomodoro_engine.is_paused() => "EN PAUSA".to_string(),
            (SolarFocusCore::AppState::Focusing(_), Language::En) if self.pomodoro_engine.is_paused() => "PAUSED".to_string(),
            (SolarFocusCore::AppState::Focusing(_), Language::Es) => "EN FOCO".to_string(),
            (SolarFocusCore::AppState::Focusing(_), Language::En) => "IN FOCUS".to_string(),
            (SolarFocusCore::AppState::Break, Language::Es) => "DESCANSO".to_string(),
            (SolarFocusCore::AppState::Break, Language::En) => "BREAK".to_string(),
            (SolarFocusCore::AppState::Completed, Language::Es) => "COMPLETADO".to_string(),
            (SolarFocusCore::AppState::Completed, Language::En) => "DONE".to_string(),
        }
    }

    fn idle_microcopy(&self) -> &'static str {
        match (self.pomodoro_engine.state(), self.settings.language) {
            (SolarFocusCore::AppState::Idle, Language::Es) => "Pomodoro de 25 minutos.",
            (SolarFocusCore::AppState::Idle, Language::En) => "25-minute pomodoro.",
            (SolarFocusCore::AppState::Break, Language::Es) => "Tomate un respiro.",
            (SolarFocusCore::AppState::Break, Language::En) => "Take a breath.",
            _ => "",
        }
    }

    fn cta_button(&self, is_paused: bool) -> Element<'_, Message> {
        use ui::palette::*;
        let lang = self.settings.language;
        let (label, msg, bg) = match (self.pomodoro_engine.state(), is_paused, lang) {
            (SolarFocusCore::AppState::Idle, _, Language::Es) => ("EMPEZAR ENFOQUE", Message::StartFocus, ACCENT),
            (SolarFocusCore::AppState::Idle, _, Language::En) => ("START FOCUS", Message::StartFocus, ACCENT),
            (SolarFocusCore::AppState::Completed, _, Language::Es) => ("EMPEZAR DE NUEVO", Message::StartFocus, ACCENT),
            (SolarFocusCore::AppState::Completed, _, Language::En) => ("START AGAIN", Message::StartFocus, ACCENT),
            (SolarFocusCore::AppState::Focusing(_), true, Language::Es) => ("REANUDAR", Message::Resume, ACCENT),
            (SolarFocusCore::AppState::Focusing(_), true, Language::En) => ("RESUME", Message::Resume, ACCENT),
            (SolarFocusCore::AppState::Focusing(_), false, Language::Es) => ("PAUSAR", Message::Pause, ACCENT_DIM),
            (SolarFocusCore::AppState::Focusing(_), false, Language::En) => ("PAUSE", Message::Pause, ACCENT_DIM),
            (SolarFocusCore::AppState::Break, true, Language::Es) => ("REANUDAR", Message::Resume, ACCENT),
            (SolarFocusCore::AppState::Break, true, Language::En) => ("RESUME", Message::Resume, ACCENT),
            (SolarFocusCore::AppState::Break, false, Language::Es) => ("VOLVER AL FOCO", Message::StartFocus, ACCENT),
            (SolarFocusCore::AppState::Break, false, Language::En) => ("BACK TO FOCUS", Message::StartFocus, ACCENT),
        };
        button(
            text(label)
                .size(FONT_LEAD)
                .color(BG),
        )
        .on_press(msg)
        .padding([14, 32])
        .style(move |_, status| {
            let bg = match status {
                button::Status::Hovered => SURFACE_RAISED,
                _ => bg,
            };
            button::Style {
                background: Some(iced::Background::Color(bg)),
                text_color: TEXT_PRIMARY,
                border: iced::Border {
                    radius: 8.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
        .into()
    }

    /// UI-4: tabbed Setup canvas. The legacy `view_settings` is kept as
    /// the AI tab content for now; UI-4 wraps it with tab navigation.
    fn view_setup_tabs(&self) -> Element<'_, Message> {
        use ui::palette::*;

        let make_tab = |t: SetupTab, label: &'static str| -> Element<'_, Message> {
            let selected = t == self.setup_tab;
            iced::widget::button(
                text(label.to_string())
                    .size(FONT_BODY)
                    .color(if selected { TEXT_PRIMARY } else { TEXT_SECONDARY }),
            )
            .on_press(Message::SwitchSetupTab(t))
            .padding([8, 16])
            .style(move |_, _| iced::widget::button::Style {
                background: Some(iced::Background::Color(if selected {
                    SURFACE
                } else {
                    iced::Color::TRANSPARENT
                })),
                text_color: TEXT_PRIMARY,
                border: iced::Border {
                    radius: 6.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
        };

        let tab_bar = iced::widget::row![
            make_tab(SetupTab::General, match self.settings.language {
                Language::Es => "General",
                Language::En => "General",
            }),
            iced::widget::Space::with_width(SPACE_XS as f32),
            make_tab(SetupTab::Ai, "IA"),
            iced::widget::Space::with_width(SPACE_XS as f32),
            make_tab(SetupTab::Privacy, match self.settings.language {
                Language::Es => "Privacidad",
                Language::En => "Privacy",
            }),
            iced::widget::Space::with_width(SPACE_XS as f32),
            make_tab(SetupTab::About, match self.settings.language {
                Language::Es => "Acerca",
                Language::En => "About",
            }),
        ]
        .spacing(SPACE_XS as u16);

        let panel: Element<'_, Message> = match self.setup_tab {
            SetupTab::General => self.view_setup_general(),
            SetupTab::Ai => self.view_settings(),
            SetupTab::Privacy => self.view_setup_privacy(),
            SetupTab::About => self.view_setup_about(),
        };

        let body = column![
            text(match self.settings.language {
                Language::Es => "Ajustes",
                Language::En => "Setup",
            })
            .size(FONT_TITLE)
            .color(TEXT_PRIMARY),
            tab_bar,
            panel,
        ]
        .spacing(SPACE_LG as u16)
        .padding(SPACE_XL as u16)
        .max_width(720);

        container(body)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(BG)),
                ..Default::default()
            })
            .into()
    }

    fn view_setup_general(&self) -> Element<'_, Message> {
        use infra::settings::RamMode;
        use ui::palette::*;
        let lang_card = container(
            column![
                text(match self.settings.language {
                    Language::Es => "Idioma",
                    Language::En => "Language",
                })
                .size(FONT_SMALL)
                .color(TEXT_MUTED),
                iced::widget::row![
                    self.lang_button(Language::Es, "Español"),
                    iced::widget::Space::with_width(SPACE_SM as f32),
                    self.lang_button(Language::En, "English"),
                ],
            ]
            .spacing(SPACE_SM as u16),
        )
        .padding(SPACE_MD as u16)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(SURFACE)),
            border: iced::Border {
                radius: 8.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });

        let ram_card = container(
            column![
                text(match self.settings.language {
                    Language::Es => "Modo de RAM",
                    Language::En => "RAM mode",
                })
                .size(FONT_SMALL)
                .color(TEXT_MUTED),
                self.ram_card(
                    RamMode::Low,
                    "Low",
                    "Solo timer · ≤ 50 MB",
                    "Timer only · ≤ 50 MB",
                ),
                self.ram_card(
                    RamMode::Normal,
                    "Normal",
                    "Detección de distracciones · ≤ 120 MB",
                    "Distraction detection · ≤ 120 MB",
                ),
                self.ram_card(
                    RamMode::Full,
                    "Full",
                    "Coaching IA + clasificador · ≤ 1.5 GB",
                    "AI coaching + classifier · ≤ 1.5 GB",
                ),
            ]
            .spacing(SPACE_SM as u16),
        )
        .padding(SPACE_MD as u16)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(SURFACE)),
            border: iced::Border {
                radius: 8.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });

        let shortcuts = container(
            column![
                text(match self.settings.language {
                    Language::Es => "Atajos de teclado",
                    Language::En => "Keyboard shortcuts",
                })
                .size(FONT_SMALL)
                .color(TEXT_MUTED),
                text("Space / P · Pausa").size(FONT_SMALL).color(TEXT_PRIMARY),
                text("R · Reanudar").size(FONT_SMALL).color(TEXT_PRIMARY),
                text("B · Tomar descanso")
                    .size(FONT_SMALL)
                    .color(TEXT_PRIMARY),
                text("S · Abrir Setup").size(FONT_SMALL).color(TEXT_PRIMARY),
                text("1 / 2 / 3 / 4 · Cambiar de pestaña")
                    .size(FONT_SMALL)
                    .color(TEXT_PRIMARY),
            ]
            .spacing(SPACE_XS as u16),
        )
        .padding(SPACE_MD as u16)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(SURFACE)),
            border: iced::Border {
                radius: 8.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });

        column![lang_card, ram_card, shortcuts]
            .spacing(SPACE_MD as u16)
            .into()
    }

    fn lang_button(&self, lang: Language, label: &'static str) -> Element<'_, Message> {
        use ui::palette::*;
        let selected = self.settings.language == lang;
        iced::widget::button(text(label.to_string()).size(FONT_BODY).color(BG))
            .on_press(Message::SetLanguage(lang))
            .padding([6, 16])
            .style(move |_, _| iced::widget::button::Style {
                background: Some(iced::Background::Color(if selected {
                    ACCENT
                } else {
                    SURFACE_RAISED
                })),
                text_color: if selected { BG } else { TEXT_PRIMARY },
                border: iced::Border {
                    radius: 6.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
    }

    fn ram_card(
        &self,
        mode: infra::settings::RamMode,
        title: &'static str,
        desc_es: &'static str,
        desc_en: &'static str,
    ) -> Element<'_, Message> {
        use ui::palette::*;
        let selected = self.settings.ram_mode == mode;
        let desc = if self.settings.language == Language::Es {
            desc_es
        } else {
            desc_en
        };
        iced::widget::button(
            iced::widget::row![
                column![
                    text(title.to_string())
                        .size(FONT_BODY)
                        .color(TEXT_PRIMARY),
                    text(desc.to_string())
                        .size(FONT_SMALL)
                        .color(TEXT_SECONDARY),
                ]
                .spacing(2),
                iced::widget::horizontal_space(),
                text(if selected { "●" } else { "○" })
                    .size(FONT_LEAD)
                    .color(if selected { ACCENT } else { TEXT_MUTED }),
            ]
            .padding(SPACE_SM as u16),
        )
        .on_press(Message::SetRamMode(mode))
        .padding(0)
        .width(Length::Fill)
        .style(move |_, _| iced::widget::button::Style {
            background: Some(iced::Background::Color(if selected {
                SURFACE_RAISED
            } else {
                SURFACE
            })),
            text_color: TEXT_PRIMARY,
            border: iced::Border {
                color: if selected { ACCENT_DIM } else { iced::Color::TRANSPARENT },
                width: 1.0,
                radius: 6.0.into(),
            },
            ..Default::default()
        })
        .into()
    }

    fn view_setup_privacy(&self) -> Element<'_, Message> {
        use ui::palette::*;
        let copy_es = "SolarFocus procesa todo localmente. Tu actividad no sale del equipo. \
                       Los modelos IA (cuando se descargan) corren en tu hardware.";
        let copy_en = "SolarFocus processes everything locally. Your activity never leaves your machine. \
                       AI models (when downloaded) run on your own hardware.";

        let banner = container(
            column![
                text(match self.settings.language {
                    Language::Es => "Privacidad",
                    Language::En => "Privacy",
                })
                .size(FONT_LEAD)
                .color(TEXT_PRIMARY),
                text(if self.settings.language == Language::Es {
                    copy_es
                } else {
                    copy_en
                })
                .size(FONT_SMALL)
                .color(TEXT_SECONDARY),
            ]
            .spacing(SPACE_SM as u16),
        )
        .padding(SPACE_MD as u16)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(SURFACE)),
            border: iced::Border {
                color: ACCENT_DIM,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        });

        // WIRE-3: live permission status with action button.
        let (badge_color, status_text_es, status_text_en) = match self.permission_status {
            PermissionStatus::Granted => (ACCENT, "Concedido", "Granted"),
            PermissionStatus::NameOnly => (WARNING, "Parcial (solo procesos)", "Partial (process names only)"),
            PermissionStatus::Denied => (DANGER, "Denegado", "Denied"),
            PermissionStatus::Unknown => (TEXT_MUTED, "Verificando…", "Checking…"),
        };
        let perm = container(
            column![
                text(match self.settings.language {
                    Language::Es => "Permiso de Grabación de Pantalla (macOS)",
                    Language::En => "Screen Recording permission (macOS)",
                })
                .size(FONT_SMALL)
                .color(TEXT_MUTED),
                iced::widget::row![
                    text("●").size(FONT_LEAD).color(badge_color),
                    iced::widget::Space::with_width(SPACE_SM as f32),
                    text(if self.settings.language == Language::Es {
                        status_text_es
                    } else {
                        status_text_en
                    })
                    .size(FONT_BODY)
                    .color(TEXT_PRIMARY),
                    iced::widget::horizontal_space(),
                    iced::widget::button(
                        text(match self.settings.language {
                            Language::Es => "Abrir Ajustes del sistema",
                            Language::En => "Open System Settings",
                        })
                        .size(FONT_SMALL),
                    )
                    .on_press(Message::OpenSystemSettings)
                    .padding([6, 14])
                    .style(|_, _| iced::widget::button::Style {
                        background: Some(iced::Background::Color(SURFACE_RAISED)),
                        text_color: TEXT_PRIMARY,
                        border: iced::Border {
                            radius: 6.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }),
                    iced::widget::Space::with_width(SPACE_XS as f32),
                    iced::widget::button(text(match self.settings.language {
                        Language::Es => "Re-verificar",
                        Language::En => "Re-check",
                    }).size(FONT_SMALL))
                    .on_press(Message::ProbePermission)
                    .padding([6, 14]),
                ],
            ]
            .spacing(SPACE_SM as u16),
        )
        .padding(SPACE_MD as u16)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(SURFACE)),
            border: iced::Border {
                radius: 6.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });

        // WIRE-3: destructive "Borrar todos los datos" with two-step confirm.
        let danger_zone: Element<'_, Message> = if self.confirming_clear {
            container(
                column![
                    text(match self.settings.language {
                        Language::Es => "¿Seguro? Esto borrará la base de datos, los ajustes y los modelos descargados.",
                        Language::En => "Are you sure? This will erase the database, settings, and any downloaded models.",
                    })
                    .size(FONT_SMALL)
                    .color(DANGER),
                    iced::widget::row![
                        iced::widget::button(text(match self.settings.language {
                            Language::Es => "Sí, borrar todo",
                            Language::En => "Yes, clear all",
                        }))
                        .on_press(Message::ConfirmClearData)
                        .padding([6, 14])
                        .style(|_, _| iced::widget::button::Style {
                            background: Some(iced::Background::Color(DANGER)),
                            text_color: BG,
                            border: iced::Border {
                                radius: 6.0.into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        }),
                        iced::widget::Space::with_width(SPACE_SM as f32),
                        iced::widget::button(text(match self.settings.language {
                            Language::Es => "Cancelar",
                            Language::En => "Cancel",
                        }))
                        .on_press(Message::CancelClearData)
                        .padding([6, 14]),
                    ],
                ]
                .spacing(SPACE_SM as u16),
            )
            .padding(SPACE_MD as u16)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(SURFACE_RAISED)),
                border: iced::Border {
                    color: DANGER,
                    width: 1.0,
                    radius: 6.0.into(),
                },
                ..Default::default()
            })
            .into()
        } else {
            container(
                iced::widget::row![
                    column![
                        text(match self.settings.language {
                            Language::Es => "Zona peligrosa",
                            Language::En => "Danger zone",
                        })
                        .size(FONT_SMALL)
                        .color(DANGER),
                        text(match self.settings.language {
                            Language::Es =>
                                "Elimina todos los datos locales (DB, ajustes, modelos).",
                            Language::En =>
                                "Erase all local data (DB, settings, models).",
                        })
                        .size(FONT_SMALL)
                        .color(TEXT_SECONDARY),
                    ]
                    .spacing(2),
                    iced::widget::horizontal_space(),
                    iced::widget::button(text(match self.settings.language {
                        Language::Es => "Borrar todos los datos",
                        Language::En => "Clear all data",
                    }))
                    .on_press(Message::RequestClearData)
                    .padding([6, 14])
                    .style(|_, _| iced::widget::button::Style {
                        background: Some(iced::Background::Color(SURFACE_RAISED)),
                        text_color: DANGER,
                        border: iced::Border {
                            color: DANGER,
                            width: 1.0,
                            radius: 6.0.into(),
                        },
                        ..Default::default()
                    }),
                ]
                .padding(SPACE_SM as u16),
            )
            .padding(SPACE_MD as u16)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(SURFACE)),
                border: iced::Border {
                    radius: 6.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
        };

        column![banner, perm, danger_zone]
            .spacing(SPACE_MD as u16)
            .into()
    }

    fn view_setup_about(&self) -> Element<'_, Message> {
        use ui::palette::*;
        column![
            text("SolarFocus OS").size(FONT_TITLE).color(TEXT_PRIMARY),
            text("v1.2.0-rc2").size(FONT_BODY).color(TEXT_SECONDARY),
            text(match self.settings.language {
                Language::Es =>
                    "Productividad enfocada con IA local. Privacidad por diseño.",
                Language::En =>
                    "Focused productivity with local AI. Privacy by design.",
            })
            .size(FONT_SMALL)
            .color(TEXT_SECONDARY),
            iced::widget::Space::with_height(SPACE_LG as f32),
            text("Apache-2.0 / MIT").size(FONT_TINY).color(TEXT_MUTED),
            text("github.com/Citric88/solarfocus")
                .size(FONT_TINY)
                .color(TEXT_MUTED),
        ]
        .spacing(SPACE_SM as u16)
        .into()
    }

    /// UI-4: First-run wizard. Three pages: Welcome / Profile / Download.
    fn view_wizard(&self) -> Element<'_, Message> {
        use ui::palette::*;

        let progress_dot = |active: bool| {
            text(if active { "●" } else { "○" })
                .size(FONT_LEAD)
                .color(if active { ACCENT } else { TEXT_MUTED })
        };
        let dots = iced::widget::row![
            progress_dot(self.wizard_step >= WizardStep::Welcome),
            iced::widget::Space::with_width(SPACE_SM as f32),
            progress_dot(self.wizard_step >= WizardStep::Profile),
            iced::widget::Space::with_width(SPACE_SM as f32),
            progress_dot(self.wizard_step >= WizardStep::Download),
        ];

        let body: Element<'_, Message> = match self.wizard_step {
            WizardStep::Welcome => self.wizard_welcome(),
            WizardStep::Profile => self.wizard_profile(),
            WizardStep::Download => self.wizard_download(),
            WizardStep::Done => iced::widget::Space::with_height(Length::Fixed(0.0)).into(),
        };

        let nav = iced::widget::row![
            iced::widget::button(text(match self.settings.language {
                Language::Es => "Atrás",
                Language::En => "Back",
            }))
            .on_press(Message::WizardBack)
            .padding([8, 18])
            .style(|_, _| iced::widget::button::Style {
                background: Some(iced::Background::Color(SURFACE_RAISED)),
                text_color: TEXT_PRIMARY,
                border: iced::Border {
                    radius: 6.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
            iced::widget::horizontal_space(),
            iced::widget::button(text(match (self.wizard_step, self.settings.language) {
                (WizardStep::Download, Language::Es) => "Empezar",
                (WizardStep::Download, Language::En) => "Get started",
                (_, Language::Es) => "Siguiente",
                (_, Language::En) => "Next",
            }))
            .on_press(if self.wizard_step == WizardStep::Download {
                Message::WizardFinish
            } else {
                Message::WizardNext
            })
            .padding([8, 22])
            .style(|_, _| iced::widget::button::Style {
                background: Some(iced::Background::Color(ACCENT)),
                text_color: BG,
                border: iced::Border {
                    radius: 6.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
        ];

        container(
            column![
                dots,
                iced::widget::Space::with_height(SPACE_LG as f32),
                body,
                iced::widget::Space::with_height(SPACE_XL as f32),
                nav,
            ]
            .padding(SPACE_XL as u16)
            .spacing(SPACE_MD as u16)
            .max_width(560),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(BG)),
            ..Default::default()
        })
        .into()
    }

    fn wizard_welcome(&self) -> Element<'_, Message> {
        use ui::palette::*;
        column![
            text("SolarFocus OS").size(FONT_TITLE).color(TEXT_PRIMARY),
            text(match self.settings.language {
                Language::Es =>
                    "Productividad enfocada con IA local. Tu actividad nunca sale del equipo.",
                Language::En =>
                    "Focused productivity with local AI. Your activity never leaves your machine.",
            })
            .size(FONT_BODY)
            .color(TEXT_SECONDARY),
            iced::widget::Space::with_height(SPACE_MD as f32),
            text(match self.settings.language {
                Language::Es => "Idioma",
                Language::En => "Language",
            })
            .size(FONT_SMALL)
            .color(TEXT_MUTED),
            iced::widget::row![
                self.lang_button(Language::Es, "Español"),
                iced::widget::Space::with_width(SPACE_SM as f32),
                self.lang_button(Language::En, "English"),
            ],
        ]
        .spacing(SPACE_SM as u16)
        .into()
    }

    fn wizard_profile(&self) -> Element<'_, Message> {
        use infra::settings::RamMode;
        use ui::palette::*;
        column![
            text(match self.settings.language {
                Language::Es => "Elige tu perfil de RAM",
                Language::En => "Pick your RAM profile",
            })
            .size(FONT_TITLE)
            .color(TEXT_PRIMARY),
            text(match self.settings.language {
                Language::Es => "Lo puedes cambiar después en Setup → General.",
                Language::En => "You can change this later in Setup → General.",
            })
            .size(FONT_SMALL)
            .color(TEXT_SECONDARY),
            iced::widget::Space::with_height(SPACE_MD as f32),
            self.ram_card(
                RamMode::Low,
                "Low",
                "Solo timer · ≤ 50 MB",
                "Timer only · ≤ 50 MB",
            ),
            self.ram_card(
                RamMode::Normal,
                "Normal",
                "Detección de distracciones · ≤ 120 MB",
                "Distraction detection · ≤ 120 MB",
            ),
            self.ram_card(
                RamMode::Full,
                "Full",
                "Coaching IA + clasificador · ≤ 1.5 GB",
                "AI coaching + classifier · ≤ 1.5 GB",
            ),
        ]
        .spacing(SPACE_SM as u16)
        .into()
    }

    fn wizard_download(&self) -> Element<'_, Message> {
        use infra::settings::RamMode;
        use ui::palette::*;

        let title = text(match self.settings.language {
            Language::Es => "Descarga del modelo IA",
            Language::En => "AI model download",
        })
        .size(FONT_TITLE)
        .color(TEXT_PRIMARY);

        // RAM mode != Full: nothing to download.
        if self.settings.ram_mode != RamMode::Full {
            return column![
                title,
                text(match self.settings.language {
                    Language::Es =>
                        "Tu perfil actual no necesita descargar el modelo IA. Listo para empezar.",
                    Language::En =>
                        "Your current profile doesn't need the AI model download. Ready to start.",
                })
                .size(FONT_BODY)
                .color(TEXT_SECONDARY),
            ]
            .spacing(SPACE_MD as u16)
            .into();
        }

        column![title, self.model_download_panel()]
            .spacing(SPACE_MD as u16)
            .into()
    }

    /// Reusable panel that shows model presence + download progress + actions.
    /// Used by both wizard download step and Setup → AI tab.
    fn model_download_panel(&self) -> Element<'_, Message> {
        use ui::palette::*;

        // When `llm` feature is off, we can't download.
        #[cfg(not(feature = "llm"))]
        {
            return container(
                text(match self.settings.language {
                    Language::Es =>
                        "Esta build no incluye el LLM. Recompila con --features llm para activarlo.",
                    Language::En =>
                        "This build was compiled without the LLM. Rebuild with --features llm to enable.",
                })
                .size(FONT_SMALL)
                .color(TEXT_MUTED),
            )
            .padding(SPACE_MD as u16)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(SURFACE)),
                border: iced::Border { radius: 8.0.into(), ..Default::default() },
                ..Default::default()
            })
            .into();
        }

        #[cfg(feature = "llm")]
        {
            use infra::model_download::{manifest_for, model_present};

            let Some(manifest) = manifest_for(self.settings.model_choice) else {
                return text("(modelo desconocido)").size(FONT_SMALL).color(TEXT_MUTED).into();
            };

            // 1) Active download in progress.
            let snap = self.download_progress.lock().unwrap().clone();
            let downloading = self.download_active.load(Ordering::Relaxed);

            if downloading {
                if let Some(s) = snap {
                    let pct = if s.total > 0 {
                        (s.downloaded as f64 / s.total as f64).min(1.0)
                    } else {
                        0.0
                    };
                    let mb_done = s.downloaded as f64 / 1_048_576.0;
                    let mb_total = s.total as f64 / 1_048_576.0;
                    let kbps = s.bytes_per_sec / 1024;
                    // ENH-3: detect resume — if we started above 0 with a 0 KB/s sample,
                    // user is resuming from a partial file.
                    let is_resume = s.downloaded > 0 && s.bytes_per_sec == 0;
                    let label = if s.verifying {
                        match self.settings.language {
                            Language::Es => "Verificando…".to_string(),
                            Language::En => "Verifying…".to_string(),
                        }
                    } else if is_resume {
                        match self.settings.language {
                            Language::Es => format!("Reanudando desde {:.1} MB", mb_done),
                            Language::En => format!("Resuming from {:.1} MB", mb_done),
                        }
                    } else {
                        format!("{:.1}/{:.1} MB · {} KB/s", mb_done, mb_total, kbps)
                    };
                    return container(
                        column![
                            text(manifest.filename.to_string())
                                .size(FONT_SMALL)
                                .color(TEXT_MUTED),
                            iced::widget::progress_bar(0.0..=1.0, pct as f32)
                                .width(Length::Fixed(420.0)),
                            iced::widget::row![
                                text(label).size(FONT_SMALL).color(TEXT_SECONDARY),
                                iced::widget::horizontal_space(),
                                // ENH-2: cancel button.
                                iced::widget::button(text(match self.settings.language {
                                    Language::Es => "Cancelar",
                                    Language::En => "Cancel",
                                }).size(FONT_SMALL))
                                .on_press(Message::CancelDownload)
                                .padding([4, 12])
                                .style(|_, _| iced::widget::button::Style {
                                    background: Some(iced::Background::Color(SURFACE_RAISED)),
                                    text_color: DANGER,
                                    border: iced::Border {
                                        color: DANGER,
                                        width: 1.0,
                                        radius: 4.0.into(),
                                    },
                                    ..Default::default()
                                }),
                            ],
                        ]
                        .spacing(SPACE_SM as u16),
                    )
                    .padding(SPACE_MD as u16)
                    .style(|_| container::Style {
                        background: Some(iced::Background::Color(SURFACE)),
                        border: iced::Border { radius: 8.0.into(), ..Default::default() },
                        ..Default::default()
                    })
                    .into();
                }
            }

            // 2) Model already on disk (BUG-B: use cached probe).
            if self.model_present_cache.unwrap_or_else(|| model_present(manifest)) {
                return container(
                    iced::widget::row![
                        column![
                            text(format!(
                                "{} {}",
                                match self.settings.language {
                                    Language::Es => "✓ Modelo presente:",
                                    Language::En => "✓ Model present:",
                                },
                                manifest.filename
                            ))
                            .size(FONT_BODY)
                            .color(ACCENT),
                            text(format!(
                                "{:.1} MB",
                                manifest.size_bytes as f64 / 1_048_576.0
                            ))
                            .size(FONT_SMALL)
                            .color(TEXT_MUTED),
                        ]
                        .spacing(2),
                        iced::widget::horizontal_space(),
                        iced::widget::button(text(match self.settings.language {
                            Language::Es => "Re-descargar",
                            Language::En => "Re-download",
                        }))
                        .on_press(Message::StartModelDownload)
                        .padding([6, 14]),
                        iced::widget::Space::with_width(SPACE_XS as f32),
                        iced::widget::button(text(match self.settings.language {
                            Language::Es => "Eliminar",
                            Language::En => "Delete",
                        }))
                        .on_press(Message::DeleteModel)
                        .padding([6, 14])
                        .style(|_, _| iced::widget::button::Style {
                            background: Some(iced::Background::Color(DANGER)),
                            text_color: BG,
                            border: iced::Border { radius: 6.0.into(), ..Default::default() },
                            ..Default::default()
                        }),
                    ]
                    .padding(SPACE_SM as u16),
                )
                .padding(SPACE_MD as u16)
                .style(|_| container::Style {
                    background: Some(iced::Background::Color(SURFACE)),
                    border: iced::Border { radius: 8.0.into(), ..Default::default() },
                    ..Default::default()
                })
                .into();
            }

            // 3) Default — offer Download / Skip + show error if any.
            let err = self.download_error.clone();
            container(
                column![
                    text(format!(
                        "{} {} (~{} MB)",
                        match self.settings.language {
                            Language::Es => "Modelo:",
                            Language::En => "Model:",
                        },
                        manifest.filename,
                        manifest.size_bytes / 1_048_576,
                    ))
                    .size(FONT_BODY)
                    .color(TEXT_PRIMARY),
                    text(match self.settings.language {
                        Language::Es => "Todo el procesamiento ocurre en tu equipo.",
                        Language::En => "All processing happens on your machine.",
                    })
                    .size(FONT_SMALL)
                    .color(TEXT_MUTED),
                    iced::widget::row![
                        iced::widget::button(text(match self.settings.language {
                            Language::Es => "Descargar",
                            Language::En => "Download",
                        }))
                        .on_press(Message::StartModelDownload)
                        .padding([8, 18])
                        .style(|_, _| iced::widget::button::Style {
                            background: Some(iced::Background::Color(ACCENT)),
                            text_color: BG,
                            border: iced::Border { radius: 6.0.into(), ..Default::default() },
                            ..Default::default()
                        }),
                        iced::widget::Space::with_width(SPACE_SM as f32),
                        iced::widget::button(text(match self.settings.language {
                            Language::Es => "Saltar",
                            Language::En => "Skip",
                        }))
                        .on_press(Message::SkipModelDownload)
                        .padding([8, 18]),
                    ],
                    if let Some(e) = err {
                        text(format!("Error: {}", e)).size(FONT_SMALL).color(DANGER).into()
                    } else {
                        Element::from(iced::widget::Space::with_height(Length::Fixed(0.0)))
                    },
                ]
                .spacing(SPACE_SM as u16),
            )
            .padding(SPACE_MD as u16)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(SURFACE)),
                border: iced::Border { radius: 8.0.into(), ..Default::default() },
                ..Default::default()
            })
            .into()
        }
    }

    /// Legacy AI-tab content. UI-4 wraps this with view_setup_tabs.
    fn view_settings(&self) -> Element<'_, Message> {
        let title = text("Ajustes IA").size(20).color(Color::WHITE);

        let ai_toggle = row![
            text(format!("Coaching IA: {}", on_off(self.settings.ai_enabled)))
                .size(18)
                .color(Color::WHITE),
            iced::widget::horizontal_space(),
            button(text(if self.settings.ai_enabled { "Desactivar" } else { "Activar" }).size(14))
                .on_press(Message::ToggleAi(!self.settings.ai_enabled))
                .padding([6, 14]),
        ]
        .padding(8);

        let watch_toggle = row![
            text(format!(
                "Vigilancia de ventana: {}",
                on_off(self.settings.window_watch_enabled)
            ))
            .size(18)
            .color(Color::WHITE),
            iced::widget::horizontal_space(),
            button(text(if self.settings.window_watch_enabled { "Desactivar" } else { "Activar" }).size(14))
                .on_press(Message::ToggleWindowWatch(!self.settings.window_watch_enabled))
                .padding([6, 14]),
        ]
        .padding(8);

        let lang_row = row![
            text(format!("Idioma: {:?}", self.settings.language))
                .size(18)
                .color(Color::WHITE),
            iced::widget::horizontal_space(),
            button(text("ES").size(14))
                .on_press(Message::SetLanguage(Language::Es))
                .padding([6, 14]),
            iced::widget::Space::with_width(8),
            button(text("EN").size(14))
                .on_press(Message::SetLanguage(Language::En))
                .padding([6, 14]),
        ]
        .padding(8);

        let classifier_row = row![
            text(format!(
                "Clasificador: {:?}",
                self.settings.classifier_mode
            ))
            .size(18)
            .color(Color::WHITE),
            iced::widget::horizontal_space(),
            button(text("Mock").size(14))
                .on_press(Message::SetClassifierMode(ClassifierMode::Mock))
                .padding([6, 14]),
            iced::widget::Space::with_width(8),
            button(text("Rules").size(14))
                .on_press(Message::SetClassifierMode(ClassifierMode::Rules))
                .padding([6, 14]),
            iced::widget::Space::with_width(8),
            button(text("DistilBERT").size(14))
                .on_press(Message::SetClassifierMode(ClassifierMode::Distilbert))
                .padding([6, 14]),
        ]
        .padding(8);

        use infra::settings::{ModelChoice, RamMode};
        // ENH-6 — recommended model for this hardware.
        let recommended = recommended_model_choice();
        let mark = |c: ModelChoice, name: &str| -> String {
            if c == recommended {
                format!("{} ★", name)
            } else {
                name.to_string()
            }
        };
        let model_row = row![
            text(format!("Modelo IA: {:?}", self.settings.model_choice))
                .size(18)
                .color(Color::WHITE),
            iced::widget::horizontal_space(),
            button(text(mark(ModelChoice::SmolLM2, "SmolLM2")).size(14))
                .on_press(Message::SetModelChoice(ModelChoice::SmolLM2))
                .padding([6, 14]),
            iced::widget::Space::with_width(8),
            button(text(mark(ModelChoice::Llama1B, "Llama1B")).size(14))
                .on_press(Message::SetModelChoice(ModelChoice::Llama1B))
                .padding([6, 14]),
            iced::widget::Space::with_width(8),
            button(text(mark(ModelChoice::Qwen15, "Qwen15")).size(14))
                .on_press(Message::SetModelChoice(ModelChoice::Qwen15))
                .padding([6, 14]),
        ]
        .padding(8);
        let recommend_hint = text(format!(
            "{} {:?} ({})",
            match self.settings.language {
                Language::Es => "Recomendado para tu hardware:",
                Language::En => "Recommended for your hardware:",
            },
            recommended,
            if cfg!(all(target_arch = "aarch64", target_os = "macos")) {
                "Apple Silicon"
            } else {
                "general"
            },
        ))
        .size(12)
        .color(Color::from_rgb(0.5, 0.7, 0.55));

        let ram_row = row![
            text(format!("Modo RAM: {:?}", self.settings.ram_mode))
                .size(18)
                .color(Color::WHITE),
            iced::widget::horizontal_space(),
            button(text("Low").size(14))
                .on_press(Message::SetRamMode(RamMode::Low))
                .padding([6, 14]),
            iced::widget::Space::with_width(8),
            button(text("Normal").size(14))
                .on_press(Message::SetRamMode(RamMode::Normal))
                .padding([6, 14]),
            iced::widget::Space::with_width(8),
            button(text("Full").size(14))
                .on_press(Message::SetRamMode(RamMode::Full))
                .padding([6, 14]),
        ]
        .padding(8);

        // Feedback summary moved to Coach canvas in WIRE-1.
        let thresholds = text(format!(
            "Umbral conf={:.2}  ·  muestras consecutivas={}  ·  poll={}s",
            self.settings.min_confidence,
            self.settings.min_consecutive_samples,
            self.settings.window_poll_secs,
        ))
        .size(13)
        .color(Color::from_rgb(0.7, 0.75, 0.7));

        let close = button(text("Guardar y cerrar").size(16))
            .on_press(Message::CloseSettings)
            .padding([10, 24])
            .style(|_, _| button::Style {
                background: Some(iced::Background::Color(Color::from_rgb(0.2, 0.6, 0.3))),
                text_color: Color::WHITE,
                border: iced::Border {
                    radius: 6.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            });

        let recap_now = button(text("Generar resumen ahora (debug)").size(13))
            .on_press(Message::GenerateRecapNow)
            .padding([6, 14])
            .style(|_, _| button::Style {
                background: Some(iced::Background::Color(Color::from_rgb(0.18, 0.30, 0.24))),
                text_color: Color::WHITE,
                border: iced::Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            });

        // WIRE-1: Modelo IA card — uses the unified model_download_panel.
        let model_card_label = text(match self.settings.language {
            Language::Es => "Estado del modelo",
            Language::En => "Model status",
        })
        .size(13)
        .color(Color::from_rgb(0.7, 0.8, 0.7));
        let model_card = self.model_download_panel();

        // ENH-7: DistilBERT downloader — only meaningful when classifier_mode = Distilbert.
        let distilbert_action: Element<'_, Message> = if matches!(
            self.settings.classifier_mode,
            infra::settings::ClassifierMode::Distilbert
        ) {
            #[cfg(feature = "classifier")]
            {
                let present = infra::distilbert_download::is_present();
                let label = match (present, self.settings.language) {
                    (true, Language::Es) => "DistilBERT presente",
                    (true, Language::En) => "DistilBERT present",
                    (false, Language::Es) => "Descargar DistilBERT (~67 MB)",
                    (false, Language::En) => "Download DistilBERT (~67 MB)",
                };
                row![
                    text(label).size(13).color(Color::from_rgb(0.7, 0.85, 0.7)),
                    iced::widget::horizontal_space(),
                    button(text(if present { "Re-descargar" } else { "Descargar" }).size(13))
                        .on_press(Message::StartDistilbertDownload)
                        .padding([4, 12]),
                ]
                .padding(8)
                .into()
            }
            #[cfg(not(feature = "classifier"))]
            {
                text("(DistilBERT requiere build con --features classifier)")
                    .size(12)
                    .color(Color::from_rgb(0.5, 0.6, 0.55))
                    .into()
            }
        } else {
            iced::widget::Space::with_height(Length::Fixed(0.0)).into()
        };

        let content = column![
            title,
            ai_toggle,
            watch_toggle,
            lang_row,
            classifier_row,
            distilbert_action,
            model_row,
            recommend_hint,
            model_card_label,
            model_card,
            ram_row,
            thresholds,
            recap_now,
            close,
        ]
        .spacing(12)
        .max_width(620);

        // Note: Length::Shrink height (default) so this can sit inside the
        // tabbed Setup canvas without violating any parent constraints.

        container(content)
            .padding(20)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(Color::from_rgb(0.04, 0.06, 0.05))),
                ..Default::default()
            })
            .into()
    }
}

fn on_off(b: bool) -> &'static str {
    if b {
        "ON"
    } else {
        "OFF"
    }
}

/// Remove the SQLite DB, settings.json, and any model files. Returns
/// the count of paths actually deleted (best-effort; missing paths skipped).
fn wipe_all_local_data() -> u32 {
    let mut n = 0u32;
    let mut try_remove = |p: std::path::PathBuf| {
        if p.is_file() {
            if std::fs::remove_file(&p).is_ok() {
                n += 1;
            }
        } else if p.is_dir() {
            if std::fs::remove_dir_all(&p).is_ok() {
                n += 1;
            }
        }
    };

    if let Some(d) = directories::ProjectDirs::from("os", "SolarFocus", "SolarFocus") {
        try_remove(d.config_dir().join("settings.json"));
        try_remove(d.config_dir().join("rules.toml"));
        try_remove(d.data_dir().join("solarfocus.db"));
        try_remove(d.data_dir().join("models"));
    }
    n
}

/// BUG-A — Strip codepoints that cosmic-text's default font can't render
/// (emojis, exotic symbols, BOMs). Keeps Latin-1 + accented Spanish
/// characters intact. Also collapses any double whitespace from the
/// removals.
fn sanitize_for_display(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        let keep = match c as u32 {
            // Basic Latin + Latin-1 Supplement (covers ¡¿áéíóúñü etc).
            0x0009 | 0x000A => true,           // tab, newline
            0x0020..=0x007E => true,           // printable ASCII
            0x00A0..=0x00FF => true,           // Latin-1 supplement (¡¿ñ accents)
            0x0100..=0x017F => true,           // Latin Extended-A
            0x2010..=0x2027 => true,           // common punctuation (— – ‘ ’ “ ” …)
            0x2030..=0x203F => true,           // ‰ ‹ › etc
            _ => false,
        };
        if keep {
            out.push(c);
        } else if !out.ends_with(' ') {
            out.push(' ');
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// ENH-6 — heuristic hardware recommendation.
///   - Apple Silicon (target_arch=aarch64 + target_os=macos) → SmolLM2 (best quality on Metal).
///   - Lower-spec systems (CPU cores < 8) → Llama-1B (lightest, fastest).
///   - Otherwise → Qwen2.5-1.5B (balanced multilingual).
fn recommended_model_choice() -> infra::settings::ModelChoice {
    use infra::settings::ModelChoice;
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    if cfg!(all(target_arch = "aarch64", target_os = "macos")) {
        ModelChoice::SmolLM2
    } else if cores < 8 {
        ModelChoice::Llama1B
    } else {
        ModelChoice::Qwen15
    }
}

/// Synchronous one-off probe of the foreground-window API. Cheap (~ms);
/// safe to call from update() since iced's update is on the main thread.
fn probe_permission_now() -> PermissionStatus {
    match infra::window_watch::WindowWatcher::poll(0) {
        Some(s) => match s.window_title {
            Some(t) if !t.trim().is_empty() => PermissionStatus::Granted,
            _ => PermissionStatus::NameOnly,
        },
        None => PermissionStatus::Denied,
    }
}

fn weekday_short(d: chrono::Weekday) -> String {
    use chrono::Weekday;
    match d {
        Weekday::Mon => "L".to_string(),
        Weekday::Tue => "M".to_string(),
        Weekday::Wed => "X".to_string(),
        Weekday::Thu => "J".to_string(),
        Weekday::Fri => "V".to_string(),
        Weekday::Sat => "S".to_string(),
        Weekday::Sun => "D".to_string(),
    }
}

fn ghost_button(label: &str, msg: Message) -> Element<'_, Message> {
    use ui::palette::*;
    iced::widget::button(text(label.to_string()).size(FONT_SMALL).color(TEXT_SECONDARY))
        .on_press(msg)
        .padding([4, 10])
        .style(|_, status| match status {
            iced::widget::button::Status::Hovered => iced::widget::button::Style {
                background: Some(iced::Background::Color(SURFACE_RAISED)),
                text_color: TEXT_PRIMARY,
                border: iced::Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            },
            _ => iced::widget::button::Style {
                background: Some(iced::Background::Color(iced::Color::TRANSPARENT)),
                text_color: TEXT_SECONDARY,
                border: iced::Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            },
        })
        .into()
}

/// Select the active Coach based on settings + compile-time `llm` feature.
///
/// PERF-1: returns MockCoach immediately. If the LLM feature is on AND
/// the model file is present, the App will dispatch a background
/// spawn_engine_load() that hot-swaps a real LlmCoach in once the
/// runtime is loaded. This keeps App::new fast (window paints in <1s
/// instead of 3-5s).
///
/// `loaded_runtime` is the optional already-loaded runtime from a recent
/// spawn_engine_load — passed in to avoid loading twice.
fn build_coach(settings: &Settings) -> Arc<dyn Coach> {
    if !settings.ai_enabled {
        log::info!("AI disabled → using MockCoach");
        return Arc::new(MockCoach);
    }
    Arc::new(MockCoach)
}

/// Same deferred semantics as `build_coach` — see PERF-1 docs above.
fn build_summarizer(settings: &Settings) -> Arc<dyn Summarizer> {
    if !settings.ai_enabled {
        return Arc::new(MockSummarizer);
    }
    Arc::new(MockSummarizer)
}

/// PERF-1: returns true if we should bother hot-loading a real LLM at boot.
fn should_attempt_llm_load(settings: &Settings) -> bool {
    if !settings.ai_enabled {
        return false;
    }
    #[cfg(feature = "llm")]
    {
        use infra::model_download::{manifest_for, model_present};
        if let Some(m) = manifest_for(settings.model_choice) {
            return model_present(m);
        }
    }
    false
}

fn build_classifier(settings: &Settings) -> Arc<dyn DistractionClassifier> {
    match settings.classifier_mode {
        ClassifierMode::Mock => Arc::new(MockClassifier),
        ClassifierMode::Rules => {
            let path = settings.effective_rules_path();
            Arc::new(RulesClassifier::bundled_with_user_override(&path))
        }
        ClassifierMode::Distilbert => {
            #[cfg(feature = "classifier")]
            {
                use infra::onnx_classifier::OnnxClassifier;
                match OnnxClassifier::try_load() {
                    Ok(c) => return Arc::new(c),
                    Err(e) => {
                        log::warn!("DistilBERT unavailable ({e}) — falling back to rules");
                    }
                }
            }
            #[cfg(not(feature = "classifier"))]
            {
                log::warn!(
                    "ClassifierMode::Distilbert requested but binary built without `classifier` feature — falling back to rules"
                );
            }
            let path = settings.effective_rules_path();
            Arc::new(RulesClassifier::bundled_with_user_override(&path))
        }
    }
}

#[allow(dead_code)]
fn primary_button(label: &str, msg: Message) -> Element<'_, Message> {
    button(text(label.to_string()).size(18))
        .on_press(msg)
        .padding([10, 24])
        .style(|_, _| button::Style {
            background: Some(iced::Background::Color(Color::from_rgb(0.2, 0.6, 0.3))),
            text_color: Color::WHITE,
            border: iced::Border {
                radius: 6.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}

#[allow(dead_code)]
fn secondary_button(label: &str, msg: Message) -> Element<'_, Message> {
    button(text(label.to_string()).size(18))
        .on_press(msg)
        .padding([10, 24])
        .style(|_, _| button::Style {
            background: Some(iced::Background::Color(Color::from_rgb(0.30, 0.50, 0.40))),
            text_color: Color::WHITE,
            border: iced::Border {
                radius: 6.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}

fn main() -> iced::Result {
    infra::init_logger(false);

    iced::application(App::title, App::update, App::view)
        .window(window::Settings {
            size: iced::Size::new(1024.0, 768.0),
            min_size: Some(iced::Size::new(800.0, 600.0)),
            ..Default::default()
        })
        .theme(|_| iced::Theme::Dark)
        .subscription(App::subscription)
        .run_with(App::new)
}
