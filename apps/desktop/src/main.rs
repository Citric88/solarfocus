//! # SolarFocus OS - App Principal (Orquestador Thin Controller)
//!
//! Arquitectura Hexagonal: la UI orquesta eventos del Core Domain.

use iced::{Color, Element, Length, Subscription, Task, window};
use iced::alignment::Horizontal;
use iced::widget::{button, column, container, progress_bar, text};

pub use solar_focus_core as SolarFocusCore;

mod infra;

use chrono::Utc;
use infra::persistence::SessionRepository;

#[derive(Debug, Clone)]
pub enum Message {
    StartFocus,
    Pause,
    Resume,
    TakeBreak,
    TimerTick(f32),
    SessionCompleted,
}

pub struct App {
    pomodoro_engine: SolarFocusCore::PomodoroEngine,
    session_repo: Option<SessionRepository>,
    last_state_was_completed: bool,
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
        (
            Self {
                pomodoro_engine: engine,
                session_repo,
                last_state_was_completed: false,
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

    fn subscription(&self) -> Subscription<Message> {
        if self.pomodoro_engine.is_paused() {
            return Subscription::none();
        }
        if matches!(
            self.pomodoro_engine.state(),
            SolarFocusCore::AppState::Focusing(_) | SolarFocusCore::AppState::Break
        ) {
            return iced::time::every(std::time::Duration::from_millis(100))
                .map(|_| Message::TimerTick(0.1));
        }
        Subscription::none()
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::StartFocus => {
                log::info!("Iniciando sesión de enfoque");
                self.pomodoro_engine.start_focus();
                self.last_state_was_completed = false;
                Task::none()
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
                Task::none()
            }
            Message::TimerTick(delta) => {
                let was_focusing = matches!(
                    self.pomodoro_engine.state(),
                    SolarFocusCore::AppState::Focusing(_)
                );

                self.pomodoro_engine.tick(delta);

                // (#5) Detectar transición Focus→Break y emitir SessionCompleted
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
                Task::none()
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
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
                let suffix = if self.pomodoro_engine.is_paused() { " (PAUSADO)" } else { "" };
                format!("FOCUS: {}{}", self.pomodoro_engine.remaining_time_formatted(), suffix)
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

        // (#16) Render only the buttons that are valid for the current state
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

        if matches!(self.pomodoro_engine.state(), SolarFocusCore::AppState::Idle | SolarFocusCore::AppState::Completed) {
            buttons.push(
                button(text("START FOCUS").size(18))
                    .on_press(Message::StartFocus)
                    .padding([10, 24])
                    .style(|_, _| button::Style {
                        background: Some(iced::Background::Color(Color::from_rgb(0.2, 0.6, 0.3))),
                        text_color: Color::WHITE,
                        border: iced::Border { radius: 6.0.into(), ..Default::default() },
                        ..Default::default()
                    })
                    .into(),
            );
        }

        if (in_focus || in_break) && !is_paused {
            buttons.push(
                button(text("PAUSE").size(18))
                    .on_press(Message::Pause)
                    .padding([10, 24])
                    .style(|_, _| button::Style {
                        background: Some(iced::Background::Color(Color::from_rgb(0.45, 0.45, 0.45))),
                        text_color: Color::WHITE,
                        border: iced::Border { radius: 6.0.into(), ..Default::default() },
                        ..Default::default()
                    })
                    .into(),
            );
        }

        if (in_focus || in_break) && is_paused {
            buttons.push(
                button(text("RESUME").size(18))
                    .on_press(Message::Resume)
                    .padding([10, 24])
                    .style(|_, _| button::Style {
                        background: Some(iced::Background::Color(Color::from_rgb(0.30, 0.55, 0.40))),
                        text_color: Color::WHITE,
                        border: iced::Border { radius: 6.0.into(), ..Default::default() },
                        ..Default::default()
                    })
                    .into(),
            );
        }

        if in_focus {
            buttons.push(
                button(text("TAKE BREAK").size(18))
                    .on_press(Message::TakeBreak)
                    .padding([10, 24])
                    .style(|_, _| button::Style {
                        background: Some(iced::Background::Color(Color::from_rgb(0.30, 0.50, 0.40))),
                        text_color: Color::WHITE,
                        border: iced::Border { radius: 6.0.into(), ..Default::default() },
                        ..Default::default()
                    })
                    .into(),
            );
        }

        let buttons_col = column(buttons).spacing(10).align_x(Horizontal::Center);

        let content = column![
            text(title).size(40).color(Color::from_rgb(0.92, 0.96, 0.92)),
            text(status_text).size(28).color(Color::from_rgb(0.85, 0.92, 0.85)),
            bar,
            text(progress_label).size(16).color(Color::from_rgb(0.7, 0.8, 0.7)),
            buttons_col,
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
}

fn main() -> iced::Result {
    // (#6) Inicializar logger antes de la UI
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
