//! # SolarFocus OS — App Principal
//!
//! v1.2.0-alpha:
//! - Engine Pomodoro from `solar-focus-core` (unchanged from v1.1.21).
//! - SQLite session persistence via `infra::persistence` (unchanged).
//! - NEW: AI-coaching slot in the UI driven by a `solar_focus_intelligence::Coach`
//!        (Phase 1 ships `MockCoach` — real LLM lands in Phase 3).
//! - NEW: Window watcher polled every N seconds during focus sessions.
//! - NEW: User settings persisted to JSON.

use iced::alignment::Horizontal;
use iced::widget::{button, column, container, progress_bar, row, text};
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

    // Phase 3.5b — model download flow
    StartModelDownload,
    SkipModelDownload,
    DownloadPoll,
    DownloadFinished(Result<String, String>), // Ok(path) | Err(message)
    DismissDownloadModal,
    /// Phase 3.5c — engine ready after async load post-download.
    LlmEngineLoaded(Result<(), String>),
    /// Phase 3.5c — manual debug trigger to generate today's recap now.
    GenerateRecapNow,

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

    // Phase 3.5b — download lifecycle
    download_modal_open: bool,
    download_active: Arc<AtomicBool>,
    download_progress: Arc<StdMutex<Option<DownloadSnapshot>>>,
    download_error: Option<String>,

    // Phase 3.5b — daily summary scheduler
    last_summary_date: Option<String>, // ISO YYYY-MM-DD of the last day we summarized
    recap: Option<(String, String)>,   // (date, text) — shown as a card if Some

    // UI-1 — current canvas route
    route: Route,
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
            "v1.2.0-beta2 boot — ai_enabled={}, language={:?}, poll={}s, classifier={:?}, coach_ready={}",
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

        // First-run download check — only when LLM feature is on.
        let download_modal_open = first_run_should_offer_download(&settings);

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
                sessions_today: 0,
                settings_open: false,
                session_started_at: None,
                focus_rules: FocusRulesEngine::new(),
                consecutive_distraction_samples: 0,
                toast: None,
                download_modal_open,
                download_active: Arc::new(AtomicBool::new(false)),
                download_progress: Arc::new(StdMutex::new(None)),
                download_error: None,
                last_summary_date: today_iso_local(),
                recap,
                route: Route::default(),
            },
            Task::none(),
        )
    }

    fn rebuild_classifier(&mut self) {
        self.classifier = build_classifier(&self.settings);
        log::info!("Classifier rebuilt: mode={:?}", self.settings.classifier_mode);
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
        let fut = async move {
            let result = download_model(manifest, move |evt| {
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
                Ok(_) => Ok(()), // We discard the runtime here — build_coach
                                  // re-loads it. Cheap because mmap'd file is in
                                  // OS page cache after the first load.
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

#[cfg(feature = "llm")]
fn first_run_should_offer_download(settings: &Settings) -> bool {
    use infra::model_download::{manifest_for, model_present};
    if !settings.ai_enabled || settings.model_download_skipped {
        return false;
    }
    match manifest_for(settings.model_choice) {
        Some(m) => !model_present(m),
        None => false,
    }
}

#[cfg(not(feature = "llm"))]
fn first_run_should_offer_download(_settings: &Settings) -> bool {
    false
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
                        log::warn!(
                            "Distraction confirmed (consecutive={}, rule={:?})",
                            self.consecutive_distraction_samples,
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
                    self.last_coaching = Some(s);
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
                self.download_modal_open = true; // keep modal showing while progress flows
                self.download_error = None;
                self.spawn_download()
            }
            Message::SkipModelDownload => {
                log::info!("User skipped first-run model download");
                self.settings.model_download_skipped = true;
                self.settings.save();
                self.download_modal_open = false;
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
                        self.download_modal_open = false;
                        self.download_error = None;
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
                        self.download_error = Some(e);
                    }
                }
                Task::none()
            }
            Message::LlmEngineLoaded(result) => {
                match result {
                    Ok(()) => {
                        // Re-resolve coach + summarizer from the now-present model.
                        self.coach = build_coach(&self.settings);
                        self.summarizer = build_summarizer(&self.settings);
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
                    Err(e) => {
                        log::warn!("LLM load failed after download: {e}");
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
                    // Resetting `model_download_skipped` lets the modal
                    // re-prompt for the newly chosen model file.
                    self.settings.model_download_skipped = false;
                    self.settings.save();
                    log::info!("Model choice → {:?}", choice);
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
            Message::SwitchRoute(r) => {
                if r != self.route {
                    log::info!("Route → {:?}", r);
                    self.route = r;
                    // Closing settings_open ensures the legacy settings modal
                    // doesn't shadow the new Setup canvas. Same for the
                    // download modal — only stays for first-run.
                    if self.route != Route::Setup {
                        self.settings_open = false;
                    }
                }
                Task::none()
            }
            Message::ThumbsUp | Message::ThumbsDown => {
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
            Message::DismissDownloadModal => {
                self.download_modal_open = false;
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
                self.last_summary_date = Some(today.clone());

                let yesterday = match yesterday_iso_local() {
                    Some(s) => s,
                    None => return Task::none(),
                };

                self.dispatch_summary_for(yesterday)
            }
            Message::DailySummaryReady { date, text } => {
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
        // First-run download still hijacks the whole window — UI-4 will
        // fold it into a Wizard route. For now, pre-empt the layout.
        if self.download_modal_open {
            return self.view_download_modal();
        }

        let status = self.status_pill();
        let sidebar = ui::sidebar::view(self.route, status, Message::SwitchRoute);

        let canvas: Element<'_, Message> = match self.route {
            Route::Focus => self.view_main(),
            Route::Stats => self.view_stats_placeholder(),
            Route::Coach => self.view_coach_placeholder(),
            Route::Setup => self.view_settings(),
        };

        iced::widget::row![sidebar, canvas].into()
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

    fn view_stats_placeholder(&self) -> Element<'_, Message> {
        use ui::palette::*;
        let (up, down) = self
            .session_repo
            .as_ref()
            .and_then(|r| r.feedback_counts().ok())
            .unwrap_or((0, 0));

        let recap_card: Element<'_, Message> = if let Some((d, t)) = &self.recap {
            container(
                column![
                    text(format!("Resumen de {}", d))
                        .size(FONT_SMALL)
                        .color(TEXT_SECONDARY),
                    text(t.clone()).size(FONT_BODY).color(TEXT_PRIMARY),
                ]
                .spacing(SPACE_XS as u16),
            )
            .padding(SPACE_MD as u16)
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
            iced::widget::Space::with_height(Length::Fixed(0.0)).into()
        };

        let body = column![
            text("Estadísticas").size(FONT_TITLE).color(TEXT_PRIMARY),
            recap_card,
            text(format!("Sesiones hoy: {}", self.sessions_today))
                .size(FONT_LEAD)
                .color(TEXT_PRIMARY),
            text(format!("Coaching feedback: 👍 {} · 👎 {}", up, down))
                .size(FONT_SMALL)
                .color(TEXT_SECONDARY),
            text("(Más stats llegan en v1.2.0-rc2 UI-3)")
                .size(FONT_SMALL)
                .color(TEXT_MUTED),
        ]
        .spacing(SPACE_MD as u16)
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

    fn view_coach_placeholder(&self) -> Element<'_, Message> {
        use ui::palette::*;
        let last = self
            .last_coaching
            .clone()
            .unwrap_or_else(|| "(Aún no hay mensajes del coach)".to_string());
        let model_badge = if cfg!(feature = "llm") && self.coach.is_ready() {
            format!("Coach: LLM · modelo {:?}", self.settings.model_choice)
        } else {
            "Coach: Mock (sin LLM cargado)".to_string()
        };

        let body = column![
            text("Coach").size(FONT_TITLE).color(TEXT_PRIMARY),
            text(model_badge).size(FONT_SMALL).color(TEXT_SECONDARY),
            container(text(last).size(FONT_BODY).color(TEXT_PRIMARY))
                .padding(SPACE_MD as u16)
                .style(|_| container::Style {
                    background: Some(iced::Background::Color(SURFACE)),
                    border: iced::Border {
                        radius: 8.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            iced::widget::row![
                button(text("👍").size(FONT_SMALL))
                    .on_press(Message::ThumbsUp)
                    .padding([4, 10]),
                iced::widget::Space::with_width(8),
                button(text("👎").size(FONT_SMALL))
                    .on_press(Message::ThumbsDown)
                    .padding([4, 10]),
            ],
            text("(Historial completo llega en v1.2.0-rc2 UI-3)")
                .size(FONT_SMALL)
                .color(TEXT_MUTED),
        ]
        .spacing(SPACE_MD as u16)
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

    fn view_download_modal(&self) -> Element<'_, Message> {
        let snap = self.download_progress.lock().unwrap().clone();
        let downloading = self.download_active.load(Ordering::Relaxed);

        let (lang_title, lang_desc, lang_dl, lang_skip, lang_dl_progress, lang_verify, lang_close) =
            match self.settings.language {
                Language::Es => (
                    "Descarga del modelo IA",
                    "SolarFocus puede usar un modelo local pequeño (~1 GB) para ofrecer coaching contextual. Todo el procesamiento ocurre en tu equipo. ¿Lo descargo ahora?",
                    "Descargar (~1 GB)",
                    "Saltar — usar coaching básico",
                    "Descargando…",
                    "Verificando…",
                    "Cerrar",
                ),
                Language::En => (
                    "AI model download",
                    "SolarFocus can use a small local model (~1 GB) for contextual coaching. All processing stays on your machine. Download now?",
                    "Download (~1 GB)",
                    "Skip — use basic coaching",
                    "Downloading…",
                    "Verifying…",
                    "Close",
                ),
            };

        let title = text(lang_title).size(28).color(Color::WHITE);
        let desc = text(lang_desc)
            .size(15)
            .color(Color::from_rgb(0.85, 0.9, 0.85))
            .width(Length::Fixed(560.0));

        let body: Element<'_, Message> = if let Some(s) = snap {
            // Active download
            let pct = if s.total > 0 {
                (s.downloaded as f64 / s.total as f64).min(1.0)
            } else {
                0.0
            };
            let mb_done = s.downloaded as f64 / 1_048_576.0;
            let mb_total = s.total as f64 / 1_048_576.0;
            let kbps = s.bytes_per_sec / 1024;
            let label_state = if s.verifying {
                lang_verify.to_string()
            } else if downloading {
                format!(
                    "{} {:.1}/{:.1} MB · {} KB/s",
                    lang_dl_progress, mb_done, mb_total, kbps
                )
            } else if let Some(ref e) = self.download_error {
                format!("Error: {}", e)
            } else {
                format!("{:.1}/{:.1} MB", mb_done, mb_total)
            };
            column![
                progress_bar(0.0..=1.0, pct as f32).width(Length::Fixed(560.0)),
                text(label_state)
                    .size(13)
                    .color(Color::from_rgb(0.7, 0.85, 0.7)),
            ]
            .spacing(8)
            .into()
        } else {
            // Initial state — offer Download / Skip
            row![
                button(text(lang_dl).size(15))
                    .on_press(Message::StartModelDownload)
                    .padding([10, 22])
                    .style(|_, _| button::Style {
                        background: Some(iced::Background::Color(Color::from_rgb(0.2, 0.6, 0.3))),
                        text_color: Color::WHITE,
                        border: iced::Border {
                            radius: 6.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }),
                iced::widget::Space::with_width(12),
                button(text(lang_skip).size(15))
                    .on_press(Message::SkipModelDownload)
                    .padding([10, 22])
                    .style(|_, _| button::Style {
                        background: Some(iced::Background::Color(Color::from_rgb(0.30, 0.35, 0.32))),
                        text_color: Color::WHITE,
                        border: iced::Border {
                            radius: 6.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }),
            ]
            .into()
        };

        let close: Element<'_, Message> = if !downloading {
            button(text(lang_close).size(13))
                .on_press(Message::DismissDownloadModal)
                .padding([6, 14])
                .into()
        } else {
            iced::widget::Space::with_height(Length::Fixed(0.0)).into()
        };

        let content = column![title, desc, body, close]
            .spacing(18)
            .align_x(Horizontal::Center)
            .max_width(640);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .padding(40)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(Color::from_rgb(0.04, 0.06, 0.05))),
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

    fn view_settings(&self) -> Element<'_, Message> {
        let title = text("Ajustes").size(32).color(Color::WHITE);

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
        let model_row = row![
            text(format!("Modelo IA: {:?}", self.settings.model_choice))
                .size(18)
                .color(Color::WHITE),
            iced::widget::horizontal_space(),
            button(text("SmolLM2").size(14))
                .on_press(Message::SetModelChoice(ModelChoice::SmolLM2))
                .padding([6, 14]),
            iced::widget::Space::with_width(8),
            button(text("Llama1B").size(14))
                .on_press(Message::SetModelChoice(ModelChoice::Llama1B))
                .padding([6, 14]),
            iced::widget::Space::with_width(8),
            button(text("Qwen15").size(14))
                .on_press(Message::SetModelChoice(ModelChoice::Qwen15))
                .padding([6, 14]),
        ]
        .padding(8);

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

        // Phase 4 — feedback summary line.
        let (up, down) = self
            .session_repo
            .as_ref()
            .and_then(|r| r.feedback_counts().ok())
            .unwrap_or((0, 0));
        let feedback_summary = text(format!("Coaching feedback: 👍 {} · 👎 {}", up, down))
            .size(13)
            .color(Color::from_rgb(0.7, 0.85, 0.7));

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

        let content = column![
            title,
            ai_toggle,
            watch_toggle,
            lang_row,
            classifier_row,
            model_row,
            ram_row,
            thresholds,
            feedback_summary,
            recap_now,
            close,
        ]
        .spacing(12)
        .align_x(Horizontal::Center)
        .max_width(620);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .padding(40)
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

/// Select the active Coach based on settings + compile-time `llm` feature
/// + whether a model file is on disk. Falls back to MockCoach gracefully.
fn build_coach(settings: &Settings) -> Arc<dyn Coach> {
    if !settings.ai_enabled {
        log::info!("AI disabled in settings → using MockCoach");
        return Arc::new(MockCoach);
    }

    #[cfg(feature = "llm")]
    {
        use infra::llm::{LlmRuntime, LoadOpts};
        use infra::llm_coach::LlmCoach;
        use infra::model_download::{manifest_for, model_path, model_present};

        if let Some(manifest) = manifest_for(settings.model_choice) {
            if model_present(manifest) {
                let path = model_path(manifest);
                log::info!("Loading LLM from {} (this may take a few seconds)", path.display());
                // Block on load. Done at startup, so OK.
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .ok()
                    .and_then(|rt| {
                        rt.block_on(async { LlmRuntime::load(&path, LoadOpts::default()).await }).ok()
                    });
                if let Some(rt) = rt {
                    log::info!("LLM ready — using LlmCoach");
                    return Arc::new(LlmCoach::new(Arc::new(rt)));
                }
                log::warn!("LLM failed to load — falling back to MockCoach");
            } else {
                log::info!(
                    "Model file {} not present — falling back to MockCoach (run downloader first)",
                    manifest.filename
                );
            }
        }
    }

    Arc::new(MockCoach)
}

/// Mirror of `build_coach` for the daily-summary tier.
fn build_summarizer(settings: &Settings) -> Arc<dyn Summarizer> {
    if !settings.ai_enabled {
        return Arc::new(MockSummarizer);
    }
    #[cfg(feature = "llm")]
    {
        use infra::llm::{LlmRuntime, LoadOpts};
        use infra::llm_coach::LlmSummarizer;
        use infra::model_download::{manifest_for, model_path, model_present};
        if let Some(manifest) = manifest_for(settings.model_choice) {
            if model_present(manifest) {
                let path = model_path(manifest);
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .ok()
                    .and_then(|rt| {
                        rt.block_on(async { LlmRuntime::load(&path, LoadOpts::default()).await })
                            .ok()
                    });
                if let Some(rt) = rt {
                    log::info!("LlmSummarizer ready");
                    return Arc::new(LlmSummarizer::new(Arc::new(rt)));
                }
            }
        }
    }
    Arc::new(MockSummarizer)
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
