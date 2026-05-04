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
    ClassificationLabel, ClassificationResult, CoachingTrigger, Language,
};
use solar_focus_core::focus_rules::FocusRulesEngine;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

#[cfg(feature = "llm")]
use infra::model_download::DownloadEvent;

mod app;
mod infra;
mod ui;

use app::builders::{
    build_classifier, build_coach, build_summarizer, probe_permission_now, should_attempt_llm_load,
};
use app::helpers::{digits_only, parse_minutes, sanitize_for_display, wipe_all_local_data};
pub use app::state::{
    App, DownloadSnapshot, LoadedEngines, PermissionStatus, SetupTab, Toast, WizardStep,
};
use ui::sidebar::{Route, StatusPill};

use chrono::Utc;
use infra::persistence::SessionRepository;
use infra::settings::{ClassifierMode, Settings};
use infra::window_watch::WindowWatcher;

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

fn today_iso_local() -> Option<String> {
    Some(chrono::Local::now().format("%Y-%m-%d").to_string())
}

fn yesterday_iso_local() -> Option<String> {
    let d = chrono::Local::now().date_naive() - chrono::Duration::days(1);
    Some(d.format("%Y-%m-%d").to_string())
}

impl App {
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::StartFocus => {
                log::info!("Iniciando sesión de enfoque");
                self.pomodoro_engine.start_focus();
                self.last_state_was_completed = false;
                self.last_classification = None;
                self.session_started_at = Some(std::time::Instant::now());
                self.session_started_at_utc = Some(chrono::Utc::now());

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
            Message::EndSession => {
                log::info!("Sesión terminada por el usuario");
                self.pomodoro_engine.reset();
                self.last_state_was_completed = false;
                self.session_started_at = None;
                self.session_started_at_utc = None;
                self.toast = Some(Toast {
                    text: match self.settings.language {
                        Language::Es => "Sesión terminada.".to_string(),
                        Language::En => "Session ended.".to_string(),
                    },
                    expires_at: Instant::now() + Duration::from_secs(2),
                });
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
                    // v1.4.1 — record the actual session start, not
                    // the completion timestamp. The old code wrote
                    // Utc::now() here, which broke attention-score
                    // window queries.
                    let start_time = self
                        .session_started_at_utc
                        .unwrap_or_else(|| Utc::now() - chrono::Duration::seconds(duration as i64));
                    let record = infra::persistence::SessionRecord {
                        id: None,
                        start_time,
                        duration,
                        state: "completed".to_string(),
                        category: self.settings.last_category.clone(),
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
                        // v1.4.0 — persist confirmed distraction so the
                        // Stats canvas can show recent top offenders.
                        // The "process" we record is the rule's keyword
                        // (e.g. "deny:tiktok" → "tiktok") because rule
                        // names already cluster equivalent surfaces
                        // (e.g. tiktok web vs app vs URL match).
                        if let Some(repo) = self.session_repo.as_ref() {
                            let display_name: String = c
                                .matched_rule
                                .as_deref()
                                .map(|r| {
                                    r.splitn(2, ':')
                                        .nth(1)
                                        .unwrap_or(r)
                                        .to_string()
                                })
                                .unwrap_or_else(|| "(sin nombre)".to_string());
                            let _ = repo.save_distraction(
                                &display_name,
                                c.matched_rule.as_deref(),
                                c.confidence,
                            );
                        }
                        log::warn!(
                            "Distraction confirmed (consecutive={}, today={}, rule={:?})",
                            self.consecutive_distraction_samples,
                            self.distractions_today,
                            c.matched_rule
                        );
                        // v1.4.0 rc11 — fire a real macOS notification
                        // via osascript so the alert reaches the user
                        // even when they're on the distracting app.
                        // Toast alone was missing the moment because
                        // the user is by definition not looking at
                        // SolarFocus when a window distraction fires.
                        #[cfg(target_os = "macos")]
                        {
                            let rule = c.matched_rule.clone()
                                .unwrap_or_else(|| "?".to_string());
                            let body = match self.settings.language {
                                Language::Es => format!("Distracción: {}. Vuelve al foco.", rule),
                                Language::En => format!("Distraction: {}. Refocus.", rule),
                            };
                            let _ = std::process::Command::new("osascript")
                                .arg("-e")
                                .arg(format!(
                                    r#"display notification "{}" with title "SolarFocus OS" sound name "Submarine""#,
                                    body.replace('"', "\\\""),
                                ))
                                .spawn();
                        }
                        // v1.4.1 — auto-pause the focus session on a
                        // confirmed window distraction. Live test of
                        // v1.4.0 surfaced the gap: a notification fired
                        // and the row was logged, but the timer kept
                        // counting as if the user was focused. That's
                        // the same bug we fixed for camera absence in
                        // v1.3.x; fix it here for window distractions
                        // too. Auto-pause only when actually focusing.
                        let auto_paused = matches!(
                            self.pomodoro_engine.state(),
                            SolarFocusCore::AppState::Focusing(_)
                        ) && !self.pomodoro_engine.is_paused();
                        if auto_paused {
                            self.pomodoro_engine.pause(0.0);
                            log::warn!(
                                "Auto-paused: window distraction confirmed (rule={:?})",
                                c.matched_rule
                            );
                        }
                        let toast_text = match (self.settings.language, auto_paused) {
                            (Language::Es, true) => match &c.matched_rule {
                                Some(r) => format!("Sesión pausada por distracción ({}).", r),
                                None => "Sesión pausada por distracción.".to_string(),
                            },
                            (Language::Es, false) => match &c.matched_rule {
                                Some(r) => format!("Distracción detectada ({}).", r),
                                None => "Distracción detectada.".to_string(),
                            },
                            (Language::En, true) => match &c.matched_rule {
                                Some(r) => format!("Session paused — distraction ({}).", r),
                                None => "Session paused — distraction.".to_string(),
                            },
                            (Language::En, false) => match &c.matched_rule {
                                Some(r) => format!("Distraction detected ({}).", r),
                                None => "Distraction detected.".to_string(),
                            },
                        };
                        tasks.push(Task::done(Message::ShowToast {
                            text: toast_text,
                            expires_in_secs: 5,
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
                // v1.4.0 rc10 — no toast. LLM hot-swap completes in
                // <200 ms on M-series, so the "Cargando coach IA…"
                // toast (with its 20 s expiry) was nothing but visual
                // noise on every boot. Slow loads will be re-flagged
                // via a dedicated UI surface, not a transient bar.
                log::info!("Spawning background LLM load…");
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
                            // v1.4.0 rc10 — no toast on completion;
                            // user can verify Coach state in Setup.
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
            Message::SetFocusMinutes(mins) => {
                let mins = mins.clamp(1, 180);
                self.settings.focus_minutes = mins;
                self.custom_focus_str = mins.to_string();
                self.settings.save();
                self.pomodoro_engine.config_mut().focus_duration = (mins as f32) * 60.0;
                log::info!("Focus duration → {} min", mins);
                self.toast = Some(Toast {
                    text: match self.settings.language {
                        Language::Es => format!("Foco: {} min", mins),
                        Language::En => format!("Focus: {} min", mins),
                    },
                    expires_at: Instant::now() + Duration::from_secs(2),
                });
                Task::none()
            }
            Message::SetBreakMinutes(mins) => {
                let mins = mins.clamp(1, 180);
                self.settings.break_minutes = mins;
                self.custom_break_str = mins.to_string();
                self.settings.save();
                self.pomodoro_engine.config_mut().short_break_duration = (mins as f32) * 60.0;
                log::info!("Short break → {} min", mins);
                self.toast = Some(Toast {
                    text: match self.settings.language {
                        Language::Es => format!("Pausa: {} min", mins),
                        Language::En => format!("Break: {} min", mins),
                    },
                    expires_at: Instant::now() + Duration::from_secs(2),
                });
                Task::none()
            }
            Message::SetLongBreakMinutes(mins) => {
                let mins = mins.clamp(1, 180);
                self.settings.long_break_minutes = mins;
                self.custom_long_break_str = mins.to_string();
                self.settings.save();
                self.pomodoro_engine.config_mut().long_break_duration = (mins as f32) * 60.0;
                log::info!("Long break → {} min", mins);
                Task::none()
            }
            // v1.3 Wave A1 — accept any keystroke into the buffer; if it
            // parses to a u32 in 1..=180, apply immediately. Empty / out
            // of range / non-numeric stays in the buffer but does not
            // mutate the persisted setting.
            Message::SetFocusMinutesText(s) => {
                self.custom_focus_str = digits_only(&s, 3);
                if let Some(m) = parse_minutes(&self.custom_focus_str, 1, 180) {
                    self.settings.focus_minutes = m;
                    self.settings.save();
                    self.pomodoro_engine.config_mut().focus_duration = (m as f32) * 60.0;
                }
                Task::none()
            }
            Message::SetBreakMinutesText(s) => {
                self.custom_break_str = digits_only(&s, 3);
                if let Some(m) = parse_minutes(&self.custom_break_str, 1, 180) {
                    self.settings.break_minutes = m;
                    self.settings.save();
                    self.pomodoro_engine.config_mut().short_break_duration = (m as f32) * 60.0;
                }
                Task::none()
            }
            Message::SetLongBreakMinutesText(s) => {
                self.custom_long_break_str = digits_only(&s, 3);
                if let Some(m) = parse_minutes(&self.custom_long_break_str, 1, 180) {
                    self.settings.long_break_minutes = m;
                    self.settings.save();
                    self.pomodoro_engine.config_mut().long_break_duration = (m as f32) * 60.0;
                }
                Task::none()
            }
            // v1.3 Wave A2 — set the category for the next focus session.
            // Persisted so chip selection survives restarts.
            Message::SetCategory(c) => {
                self.settings.last_category = c.clone();
                self.custom_category_str = c.clone();
                self.settings.save();
                log::info!("Category → {}", c);
                Task::none()
            }
            Message::SetCategoryText(s) => {
                // Cap to a sane length to keep DB rows bounded.
                let trimmed: String = s.chars().take(40).collect();
                self.custom_category_str = trimmed.clone();
                if !trimmed.trim().is_empty() {
                    self.settings.last_category = trimmed.trim().to_string();
                    self.settings.save();
                }
                Task::none()
            }
            // v1.3 Wave B — toggle the presence probe. First enable opens
            // the camera (triggers macOS Camera permission prompt). On
            // failure we save the error string for the UI to surface.
            #[cfg(feature = "presence")]
            Message::TogglePresence(on) => {
                self.settings.presence_enabled = on;
                self.settings.save();
                self.presence_error = None;
                if on {
                    if self.presence_probe.is_none() {
                        match infra::presence::PresenceProbe::new() {
                            Ok(p) => {
                                self.presence_probe = Some(std::sync::Arc::new(p));
                                log::info!("Presence: probe initialized");
                            }
                            Err(e) => {
                                let msg = e.to_string();
                                log::warn!("Presence: init failed: {}", msg);
                                self.presence_error = Some(msg);
                                self.settings.presence_enabled = false;
                                self.settings.save();
                            }
                        }
                    }
                } else {
                    self.presence_probe = None;
                    self.last_presence = None;
                    self.consecutive_absent_samples = 0;
                }
                Task::none()
            }
            #[cfg(feature = "presence")]
            Message::PresenceProbe => {
                // Capture + brightness on the UI thread (~5 ms total).
                // YuNet inference is throttled to once every 3 s and
                // runs on tokio::task::spawn_blocking so it never
                // blocks the UI even when each call costs ~200 ms on
                // CPU at 640×640.
                const YUNET_THROTTLE_SECS: u64 = 3;
                if let Some(probe) = self.presence_probe.as_ref() {
                    let probe = probe.clone();
                    match probe.poll() {
                        Ok((sample, captured)) => {
                            let captured_at = sample.captured_at;
                            let immediate = Task::done(Message::PresenceReady(Ok(sample)));
                            let now = std::time::Instant::now();
                            let throttle_ok = self
                                .last_yunet_at
                                .map(|t| now.duration_since(t).as_secs() >= YUNET_THROTTLE_SECS)
                                .unwrap_or(true);
                            if let Some(engine) = probe.yunet_engine() {
                                if throttle_ok {
                                    self.last_yunet_at = Some(now);
                                    let bg = Task::perform(
                                        async move {
                                            tokio::task::spawn_blocking(move || {
                                                let mut g = match engine.lock() {
                                                    Ok(g) => g,
                                                    Err(_) => return Err("yunet poisoned".to_string()),
                                                };
                                                g.infer(
                                                    &captured.bytes,
                                                    captured.width,
                                                    captured.height,
                                                )
                                            })
                                            .await
                                            .map_err(|e| e.to_string())
                                            .and_then(|r| r)
                                            .map(|(p, c)| (p, c, captured_at))
                                        },
                                        Message::YunetVerdict,
                                    );
                                    return Task::batch(vec![immediate, bg]);
                                }
                            }
                            immediate
                        }
                        Err(e) => Task::done(Message::PresenceReady(Err(e.to_string()))),
                    }
                } else {
                    Task::none()
                }
            }
            #[cfg(feature = "presence")]
            Message::YunetVerdict(result) => {
                use infra::presence::Presence;
                match result {
                    Ok((p, conf, captured_at)) => {
                        log::info!(
                            "YuNet verdict: {:?} score={:.3} at {}",
                            p,
                            conf,
                            captured_at.format("%H:%M:%S")
                        );
                        self.last_yunet = Some((p, captured_at));
                        // YuNet is the more reliable signal — when it
                        // disagrees with brightness, prefer YuNet for
                        // the auto-pause counter.
                        match p {
                            Presence::Absent => {
                                self.consecutive_absent_samples =
                                    self.consecutive_absent_samples.saturating_add(1);
                                let threshold = self.settings.presence_absent_threshold.max(1);
                                if self.consecutive_absent_samples >= threshold
                                    && matches!(
                                        self.pomodoro_engine.state(),
                                        SolarFocusCore::AppState::Focusing(_)
                                    )
                                    && !self.pomodoro_engine.is_paused()
                                {
                                    self.pomodoro_engine.pause(0.0);
                                    log::info!(
                                        "Presence (YuNet): auto-paused after {} Absent samples",
                                        self.consecutive_absent_samples
                                    );
                                    // v1.4.0 rc5 — log camera-detected
                                    // absence as a distraction so the
                                    // attention score reflects it.
                                    if let Some(repo) = self.session_repo.as_ref() {
                                        let _ = repo.save_distraction(
                                            "ausencia (cámara)",
                                            Some("presence:absent"),
                                            1.0,
                                        );
                                    }
                                    self.toast = Some(Toast {
                                        text: match self.settings.language {
                                            Language::Es =>
                                                "Pausado: te alejaste del escritorio.".to_string(),
                                            Language::En =>
                                                "Paused: you stepped away.".to_string(),
                                        },
                                        expires_at: Instant::now() + Duration::from_secs(4),
                                    });
                                }
                            }
                            Presence::Present | Presence::Unknown => {
                                self.consecutive_absent_samples = 0;
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!("YuNet inference error (background): {}", e);
                    }
                }
                Task::none()
            }
            // v1.3 Wave C — manual next-deadline input handlers.
            #[cfg(feature = "calendar")]
            Message::SetDeadlineLabel(s) => {
                let trimmed: String = s.chars().take(60).collect();
                self.settings.next_deadline_label = trimmed;
                self.settings.save();
                Task::none()
            }
            #[cfg(feature = "calendar")]
            Message::SetDeadlineTime(s) => {
                use chrono::{NaiveTime, TimeZone, Local};
                // Accept "HH:MM" only. Anything else just buffers without
                // applying.
                let cleaned: String = s
                    .chars()
                    .filter(|c| c.is_ascii_digit() || *c == ':')
                    .take(5)
                    .collect();
                self.deadline_time_str = cleaned.clone();
                if let Ok(t) = NaiveTime::parse_from_str(&cleaned, "%H:%M") {
                    let today = Local::now().date_naive();
                    if let Some(dt) = Local
                        .from_local_datetime(&today.and_time(t))
                        .single()
                    {
                        self.settings.next_deadline_at = Some(dt.to_rfc3339());
                        self.settings.save();
                    }
                }
                Task::none()
            }
            #[cfg(feature = "calendar")]
            Message::ClearDeadline => {
                self.settings.next_deadline_at = None;
                self.settings.next_deadline_label.clear();
                self.deadline_time_str.clear();
                self.settings.save();
                Task::none()
            }
            // v1.3.1 — live EventKit toggle. macOS Calendar permission
            // prompt is synchronous via EventKit's barrier-based wait;
            // the first call typically returns within ~100 ms (or
            // longer if the user is reading the prompt). Run on UI
            // thread to avoid Send bounds on Retained<EKEventStore>.
            #[cfg(feature = "calendar")]
            Message::ToggleCalendarLive(on) => {
                self.settings.calendar_live_enabled = on;
                self.settings.save();
                self.calendar_error = None;
                if on {
                    if self.calendar_reader.is_none() {
                        self.calendar_reader = Some(std::sync::Arc::new(
                            infra::calendar::ek::CalendarReader::new(),
                        ));
                    }
                    let reader = self.calendar_reader.as_ref().unwrap().clone();
                    let result = reader.request_access().map_err(|e| e.to_string());
                    Task::done(Message::CalendarAccessResult(result))
                } else {
                    self.calendar_events.clear();
                    Task::none()
                }
            }
            #[cfg(feature = "calendar")]
            Message::CalendarAccessResult(result) => match result {
                Ok(true) => {
                    log::info!("Calendar: access granted");
                    Task::done(Message::CalendarRefresh)
                }
                Ok(false) => {
                    log::info!("Calendar: access denied");
                    self.settings.calendar_live_enabled = false;
                    self.settings.save();
                    self.calendar_error = Some(match self.settings.language {
                        Language::Es => "Permiso de calendario denegado.".into(),
                        Language::En => "Calendar permission denied.".into(),
                    });
                    Task::none()
                }
                Err(e) => {
                    log::warn!("Calendar: access error: {}", e);
                    self.calendar_error = Some(e);
                    self.settings.calendar_live_enabled = false;
                    self.settings.save();
                    Task::none()
                }
            },
            #[cfg(feature = "calendar")]
            Message::CalendarRefresh => {
                use infra::calendar::CalendarSource;
                if let Some(reader) = self.calendar_reader.as_ref() {
                    let result = reader.events_today().map_err(|e| e.to_string());
                    Task::done(Message::CalendarEventsLoaded(result))
                } else {
                    Task::none()
                }
            }
            #[cfg(feature = "calendar")]
            Message::CalendarEventsLoaded(result) => {
                match result {
                    Ok(events) => {
                        log::info!("Calendar: {} events loaded today", events.len());
                        self.calendar_events = events;
                        self.calendar_error = None;
                    }
                    Err(e) => {
                        log::warn!("Calendar: load error: {}", e);
                        self.calendar_error = Some(e);
                    }
                }
                Task::none()
            }
            // v1.3.1 — YuNet model download (337 KB). User-triggered
            // from the Setup → IA presence card.
            #[cfg(feature = "presence")]
            Message::DownloadYunet => {
                use infra::yunet_download;
                if yunet_download::is_present() {
                    return Task::done(Message::YunetDownloaded(Ok(())));
                }
                Task::perform(
                    async move { yunet_download::download().await.map_err(|e| e.to_string()) },
                    Message::YunetDownloaded,
                )
            }
            #[cfg(feature = "presence")]
            Message::YunetDownloaded(result) => {
                match result {
                    Ok(()) => {
                        log::info!("YuNet: download complete");
                        // Force probe re-init so it picks up the new model.
                        if self.settings.presence_enabled {
                            self.presence_probe = None;
                            return Task::done(Message::TogglePresence(true));
                        }
                    }
                    Err(e) => {
                        log::warn!("YuNet: download failed: {}", e);
                        self.presence_error = Some(e);
                    }
                }
                Task::none()
            }
            #[cfg(feature = "presence")]
            Message::PresenceReady(result) => {
                use infra::presence::{DetectionMode, Presence};
                match result {
                    Ok(sample) => {
                        self.last_presence = Some(sample.presence);
                        self.presence_error = None;
                        // When YuNet is the active mode, the brightness
                        // path is informational only — YuNet drives the
                        // auto-pause counter via Message::YunetVerdict
                        // because brightness is too noisy to act on.
                        let yunet_active = self
                            .presence_probe
                            .as_ref()
                            .map(|p| p.mode() == DetectionMode::YunetFace)
                            .unwrap_or(false);
                        if yunet_active {
                            return Task::none();
                        }
                        match sample.presence {
                            Presence::Absent => {
                                self.consecutive_absent_samples =
                                    self.consecutive_absent_samples.saturating_add(1);
                                let threshold = self.settings.presence_absent_threshold.max(1);
                                if self.consecutive_absent_samples >= threshold
                                    && matches!(
                                        self.pomodoro_engine.state(),
                                        SolarFocusCore::AppState::Focusing(_)
                                    )
                                    && !self.pomodoro_engine.is_paused()
                                {
                                    self.pomodoro_engine.pause(0.0);
                                    log::info!(
                                        "Presence: auto-paused after {} Absent samples",
                                        self.consecutive_absent_samples
                                    );
                                    // v1.4.0 rc5 — log brightness-detected
                                    // absence as a distraction event too.
                                    if let Some(repo) = self.session_repo.as_ref() {
                                        let _ = repo.save_distraction(
                                            "ausencia (luminancia)",
                                            Some("presence:absent"),
                                            sample.confidence.max(0.5),
                                        );
                                    }
                                    self.toast = Some(Toast {
                                        text: match self.settings.language {
                                            Language::Es =>
                                                "Pausado: te alejaste del escritorio.".to_string(),
                                            Language::En =>
                                                "Paused: you stepped away.".to_string(),
                                        },
                                        expires_at: Instant::now() + Duration::from_secs(4),
                                    });
                                }
                            }
                            Presence::Present | Presence::Unknown => {
                                self.consecutive_absent_samples = 0;
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!("Presence: probe error: {}", e);
                        self.presence_error = Some(e);
                    }
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
            Message::ToggleSetupAdvanced => {
                self.setup_show_advanced = !self.setup_show_advanced;
                Task::none()
            }
            Message::ClearFeedbackHistory => {
                let removed = self
                    .session_repo
                    .as_ref()
                    .and_then(|r| r.clear_feedback().ok())
                    .unwrap_or(0);
                log::info!("Cleared {} coaching_feedback rows", removed);
                self.refresh_setup_caches();
                self.toast = Some(Toast {
                    text: match self.settings.language {
                        Language::Es => format!("Historial limpiado ({}).", removed),
                        Language::En => format!("History cleared ({}).", removed),
                    },
                    expires_at: Instant::now() + Duration::from_secs(3),
                });
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
