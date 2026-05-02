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
    ClassificationResult, Coach, CoachingTrigger, DistractionClassifier, FocusContext,
    Language, MockClassifier, MockCoach,
};
use std::sync::Arc;

mod infra;

use chrono::Utc;
use infra::persistence::SessionRepository;
use infra::settings::Settings;
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
}

pub struct App {
    pomodoro_engine: SolarFocusCore::PomodoroEngine,
    session_repo: Option<SessionRepository>,
    last_state_was_completed: bool,

    // v1.2 fields
    settings: Settings,
    coach: Arc<dyn Coach>,
    classifier: Arc<dyn DistractionClassifier>,
    last_coaching: Option<String>,
    last_classification: Option<ClassificationResult>,
    sessions_today: u8,
    settings_open: bool,
    session_started_at: Option<std::time::Instant>,
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

        let settings = Settings::load();
        let coach: Arc<dyn Coach> = Arc::new(MockCoach);
        let classifier: Arc<dyn DistractionClassifier> = Arc::new(MockClassifier);

        log::info!(
            "v1.2.0-alpha boot — ai_enabled={}, language={:?}, poll={}s",
            settings.ai_enabled,
            settings.language,
            settings.window_poll_secs
        );

        (
            Self {
                pomodoro_engine: engine,
                session_repo,
                last_state_was_completed: false,
                settings,
                coach,
                classifier,
                last_coaching: None,
                last_classification: None,
                sessions_today: 0,
                settings_open: false,
                session_started_at: None,
            },
            Task::none(),
        )
    }

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
                iced::time::every(std::time::Duration::from_millis(100))
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
                iced::time::every(std::time::Duration::from_secs(secs))
                    .map(|_| Message::WindowProbe),
            );
        }

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
                self.last_classification = Some(c);
                Task::none()
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
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        if self.settings_open {
            return self.view_settings();
        }
        self.view_main()
    }

    fn view_main(&self) -> Element<'_, Message> {
        let bg = match self.pomodoro_engine.state() {
            SolarFocusCore::AppState::Focusing(_) => Color::from_rgb(0.06, 0.12, 0.08),
            SolarFocusCore::AppState::Break => Color::from_rgb(0.70, 0.86, 1.00),
            _ => Color::from_rgb(0.04, 0.06, 0.05),
        };

        let title = match self.pomodoro_engine.state() {
            SolarFocusCore::AppState::Idle => "SolarFocus OS",
            SolarFocusCore::AppState::Focusing(_) => "En Foco",
            SolarFocusCore::AppState::Break => "Descanso",
            SolarFocusCore::AppState::Completed => "Sesión Completada",
        };

        let status_text = match self.pomodoro_engine.state() {
            SolarFocusCore::AppState::Idle => "Esperando inicio...".to_string(),
            SolarFocusCore::AppState::Focusing(_) => {
                let suffix = if self.pomodoro_engine.is_paused() {
                    " (PAUSADO)"
                } else {
                    ""
                };
                format!(
                    "FOCUS: {}{}",
                    self.pomodoro_engine.remaining_time_formatted(),
                    suffix
                )
            }
            SolarFocusCore::AppState::Break => {
                format!("BREAK: {}", self.pomodoro_engine.remaining_time_formatted())
            }
            SolarFocusCore::AppState::Completed => "Excelente trabajo!".to_string(),
        };

        let progress = self.pomodoro_engine.progress();
        let bar = progress_bar(0.0..=1.0, progress).width(Length::Fixed(280.0));

        let progress_label = match self.pomodoro_engine.state() {
            SolarFocusCore::AppState::Focusing(_) | SolarFocusCore::AppState::Break => {
                format!("Progreso: {:.0}%", progress * 100.0)
            }
            _ => String::new(),
        };

        let mut buttons: Vec<Element<'_, Message>> = Vec::new();
        let in_focus = matches!(
            self.pomodoro_engine.state(),
            SolarFocusCore::AppState::Focusing(_)
        );
        let in_break = matches!(
            self.pomodoro_engine.state(),
            SolarFocusCore::AppState::Break
        );
        let is_paused = self.pomodoro_engine.is_paused();

        if matches!(
            self.pomodoro_engine.state(),
            SolarFocusCore::AppState::Idle | SolarFocusCore::AppState::Completed
        ) {
            buttons.push(primary_button("START FOCUS", Message::StartFocus));
        }
        if (in_focus || in_break) && !is_paused {
            buttons.push(secondary_button("PAUSE", Message::Pause));
        }
        if (in_focus || in_break) && is_paused {
            buttons.push(secondary_button("RESUME", Message::Resume));
        }
        if in_focus {
            buttons.push(secondary_button("TAKE BREAK", Message::TakeBreak));
        }

        let buttons_col = column(buttons).spacing(10).align_x(Horizontal::Center);

        // Coaching slot (Phase 1: MockCoach output)
        let coaching_slot: Element<'_, Message> = if let Some(ref c) = self.last_coaching {
            text(c.clone())
                .size(18)
                .color(Color::from_rgb(0.85, 0.95, 0.85))
                .into()
        } else {
            text("").size(18).into()
        };

        // Distraction indicator (small dot + label)
        let distraction_slot: Element<'_, Message> = if let Some(ref c) = self.last_classification {
            let (color, label_text) = match c.label {
                solar_focus_intelligence::ClassificationLabel::Focus => {
                    (Color::from_rgb(0.3, 0.85, 0.4), "Focus")
                }
                solar_focus_intelligence::ClassificationLabel::Distraction => {
                    (Color::from_rgb(0.95, 0.4, 0.4), "Distraction")
                }
                solar_focus_intelligence::ClassificationLabel::Neutral => {
                    (Color::from_rgb(0.7, 0.7, 0.7), "Neutral")
                }
            };
            row![
                text("●").size(16).color(color),
                text(format!(" {} ({:.0}%)", label_text, c.confidence * 100.0))
                    .size(12)
                    .color(Color::from_rgb(0.7, 0.7, 0.7)),
            ]
            .into()
        } else {
            text("").size(12).into()
        };

        let top_bar: Element<'_, Message> = row![
            text(format!("Hoy: {} sesiones", self.sessions_today))
                .size(13)
                .color(Color::from_rgb(0.6, 0.7, 0.6)),
            iced::widget::horizontal_space(),
            button(text("Settings").size(13))
                .on_press(Message::OpenSettings)
                .padding([4, 10])
                .style(|_, _| button::Style {
                    background: Some(iced::Background::Color(Color::from_rgb(0.15, 0.2, 0.18))),
                    text_color: Color::from_rgb(0.85, 0.9, 0.85),
                    border: iced::Border {
                        radius: 4.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
        ]
        .padding(10)
        .into();

        let content = column![
            top_bar,
            text(title).size(40).color(Color::from_rgb(0.92, 0.96, 0.92)),
            text(status_text)
                .size(28)
                .color(Color::from_rgb(0.85, 0.92, 0.85)),
            bar,
            text(progress_label)
                .size(16)
                .color(Color::from_rgb(0.7, 0.8, 0.7)),
            coaching_slot,
            buttons_col,
            distraction_slot,
            text("(Click START FOCUS para comenzar una sesión)")
                .size(14)
                .color(Color::from_rgb(0.6, 0.6, 0.6)),
        ]
        .spacing(20)
        .align_x(Horizontal::Center);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .style(move |_| container::Style {
                background: Some(iced::Background::Color(bg)),
                ..Default::default()
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

        let content = column![title, ai_toggle, watch_toggle, lang_row, close]
            .spacing(12)
            .align_x(Horizontal::Center)
            .max_width(560);

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
