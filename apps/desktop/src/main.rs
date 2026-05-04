//! # SolarFocus OS — App Principal
//!
//! v1.2.0-alpha:
//! - Engine Pomodoro from `solar-focus-core` (unchanged from v1.1.21).
//! - SQLite session persistence via `infra::persistence` (unchanged).
//! - NEW: AI-coaching slot in the UI driven by a `solar_focus_intelligence::Coach`
//!        (Phase 1 ships `MockCoach` — real LLM lands in Phase 3).
//! - NEW: Window watcher polled every N seconds during focus sessions.
//! - NEW: User settings persisted to JSON.

use iced::{Element, Task, window};

pub use solar_focus_core as SolarFocusCore;

use solar_focus_intelligence::{
    ClassificationLabel, ClassificationResult, Language,
};
use solar_focus_core::focus_rules::FocusRulesEngine;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(feature = "llm")]
use infra::model_download::DownloadEvent;

mod app;
mod infra;
mod ui;

use app::builders::{build_classifier, build_coach, build_summarizer, should_attempt_llm_load};
pub use app::state::{
    App, DownloadSnapshot, LoadedEngines, PermissionStatus, SetupTab, Toast, WizardStep,
};
use ui::sidebar::{Route, StatusPill};

use infra::persistence::SessionRepository;
use infra::settings::{ClassifierMode, Settings};

#[derive(Debug, Clone)]
pub enum Message {
    StartFocus,
    Pause,
    Resume,
    TakeBreak,
    EndSession,
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
    SetFocusMinutes(u32),
    SetBreakMinutes(u32),
    SetLongBreakMinutes(u32),
    // v1.3 Wave A1 — free-form custom duration text inputs.
    SetFocusMinutesText(String),
    SetBreakMinutesText(String),
    SetLongBreakMinutesText(String),
    // v1.3 Wave A2 — named focus category for the next session.
    SetCategory(String),
    SetCategoryText(String),
    // v1.3 Wave B — camera-based presence detection (feature-gated).
    #[cfg(feature = "presence")]
    TogglePresence(bool),
    #[cfg(feature = "presence")]
    PresenceProbe,
    #[cfg(feature = "presence")]
    PresenceReady(Result<infra::presence::PresenceSample, String>),
    // v1.3.1 — YuNet ONNX downloader for face detection.
    #[cfg(feature = "presence")]
    DownloadYunet,
    #[cfg(feature = "presence")]
    YunetDownloaded(Result<(), String>),
    /// Verdict from a background YuNet inference (Present/Absent +
    /// max face confidence + when it was captured).
    #[cfg(feature = "presence")]
    YunetVerdict(Result<(infra::presence::Presence, f32, chrono::DateTime<chrono::Local>), String>),
    // v1.3 Wave C — manual next-deadline input (label + HH:MM today).
    #[cfg(feature = "calendar")]
    SetDeadlineLabel(String),
    #[cfg(feature = "calendar")]
    SetDeadlineTime(String),
    #[cfg(feature = "calendar")]
    ClearDeadline,
    // v1.3.1 — live EventKit binding (toggle + access result + refresh).
    #[cfg(feature = "calendar")]
    ToggleCalendarLive(bool),
    #[cfg(feature = "calendar")]
    CalendarAccessResult(Result<bool, String>),
    #[cfg(feature = "calendar")]
    CalendarRefresh,
    #[cfg(feature = "calendar")]
    CalendarEventsLoaded(Result<Vec<infra::calendar::CalendarEvent>, String>),
    ThumbsUp,
    ThumbsDown,

    // UI-1 — route navigation
    SwitchRoute(Route),

    // UI-4 — Setup tabs + first-run wizard
    SwitchSetupTab(SetupTab),
    WizardNext,
    WizardBack,
    WizardFinish,
    /// FIX-3 (rc14) — flip the AI-tab advanced section.
    ToggleSetupAdvanced,
    /// FIX-4 (rc14) — wipe all coaching_feedback rows.
    ClearFeedbackHistory,

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


impl App {
    pub fn new() -> (Self, Task<Message>) {
        let mut engine = SolarFocusCore::PomodoroEngine::new();
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

        // FEAT — apply user's saved focus + break durations.
        engine.config_mut().focus_duration = (settings.focus_minutes as f32) * 60.0;
        engine.config_mut().short_break_duration = (settings.break_minutes as f32) * 60.0;
        engine.config_mut().long_break_duration = (settings.long_break_minutes as f32) * 60.0;

        let coach = build_coach(&settings);
        let summarizer = build_summarizer(&settings);
        let classifier = build_classifier(&settings);

        log::info!(
            "v1.3.1 boot — ai_enabled={}, language={:?}, poll={}s, classifier={:?}, coach_ready={}",
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
        // v1.3 Wave A1 — capture initial duration strings before `settings`
        // is moved into Self.
        let custom_focus_str = settings.focus_minutes.to_string();
        let custom_break_str = settings.break_minutes.to_string();
        let custom_long_break_str = settings.long_break_minutes.to_string();
        let custom_category_str = settings.last_category.clone();
        // v1.3 Wave C — initial deadline string (only used when `calendar`
        // feature is on; computed unconditionally to keep code simple).
        #[cfg(feature = "calendar")]
        let initial_deadline_str: String = settings
            .next_deadline_at
            .as_deref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Local).format("%H:%M").to_string())
            .unwrap_or_default();

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
                session_started_at_utc: None,
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
                setup_show_advanced: false,
                custom_focus_str,
                custom_break_str,
                custom_long_break_str,
                custom_category_str,
                #[cfg(feature = "calendar")]
                deadline_time_str: initial_deadline_str,
                #[cfg(feature = "calendar")]
                calendar_reader: None,
                #[cfg(feature = "calendar")]
                calendar_events: Vec::new(),
                #[cfg(feature = "calendar")]
                calendar_error: None,
                #[cfg(feature = "presence")]
                presence_probe: None,
                #[cfg(feature = "presence")]
                consecutive_absent_samples: 0,
                #[cfg(feature = "presence")]
                last_presence: None,
                #[cfg(feature = "presence")]
                presence_error: None,
                #[cfg(feature = "presence")]
                last_yunet_at: None,
                #[cfg(feature = "presence")]
                last_yunet: None,
            },
            // PERF-1: probe permission AND kick off the background LLM load
            // (latter is a no-op if no model file present or feature off).
            // Both tasks run in parallel; window paints immediately.
            {
                let mut tasks = vec![Task::done(Message::ProbePermission)];
                if should_attempt_llm_load(&Settings::load()) {
                    tasks.push(Task::done(Message::SpawnEngineLoad));
                }
                // v1.3.1 — if the user previously enabled live calendar
                // and granted permission, kick off an initial refresh.
                #[cfg(feature = "calendar")]
                if Settings::load().calendar_live_enabled {
                    tasks.push(Task::done(Message::ToggleCalendarLive(true)));
                }
                // v1.4.0 rc11 — same boot-time auto-restore for camera
                // presence. macOS Camera permission only prompts once;
                // subsequent boots open the camera silently. Without
                // this the user had to manually Desactivar+Activar
                // every launch (caught in live test).
                #[cfg(feature = "presence")]
                if Settings::load().presence_enabled {
                    tasks.push(Task::done(Message::TogglePresence(true)));
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

pub fn today_iso_local() -> Option<String> {
    Some(chrono::Local::now().format("%Y-%m-%d").to_string())
}

pub fn yesterday_iso_local() -> Option<String> {
    let d = chrono::Local::now().date_naive() - chrono::Duration::days(1);
    Some(d.format("%Y-%m-%d").to_string())
}

impl App {
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
            Route::Help => self.view_help(),
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

    // All view_* methods extracted to ui/views/*.rs:
    //   view_main / category_picker / deadline_badge / state_label /
    //   idle_microcopy / cta_button → ui/views/focus.rs
    //   view_stats_placeholder → ui/views/stats.rs
    //   view_coach_placeholder → ui/views/coach.rs
    //   view_help → ui/views/help.rs
    //   view_setup_tabs → ui/views/setup_tabs.rs
    //   view_setup_general / lang_button / ram_card → ui/views/setup_general.rs
    //   view_settings / model_download_panel → ui/views/setup_ai.rs
    //   view_setup_privacy → ui/views/setup_privacy.rs
    //   view_setup_about → ui/views/setup_about.rs
    //   view_wizard + wizard_welcome / wizard_profile / wizard_download → ui/views/wizard.rs
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
