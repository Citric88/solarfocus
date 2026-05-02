//! # SolarFocus OS - App Principal (Orquestador Thin Controller)
//! 
//! Arquitectura Hexagonal: La UI solo orquesta eventos del Core Domain.
//! 
//! ## 🏗️ Estructura:
//! - `SolarFocusCore` - Importa lógica pura de core-domain
//! - `AppState` - Maneja estado de la ventana y mensajes
//! - `App` - Controlador thin que delega a SolarFocusCore
//! - `SessionRepository` - Persistencia SQLite para logs de sesiones

use iced::{
    Element, Length, Task, window, Color, Subscription,
};
use iced::widget::{button, column, text, container, progress_bar};
use iced::alignment::{Horizontal, Vertical};

// 📦 Importar tipos del Core Domain (Lógica pura)
pub use solar_focus_core as SolarFocusCore;

mod infra;
use infra::persistence::SessionRepository;
use chrono::{DateTime, Utc};

/// Mensajes de la aplicación (UI events)
#[derive(Debug, Clone)]
pub enum Message {
    /// Botón START FOCUS
    StartFocus,
    
    /// Botón PAUSE
    Pause,
    
    /// Botón RESUME
    Resume,
    
    /// Botón TAKE BREAK manual
    TakeBreak,
    
    /// Tick de tiempo del motor (delta desde último frame)
    TimerTick(f32),
    
    /// Sesión completada - evento del core
    SessionCompleted,
}

/// Controlador principal de la aplicación
pub struct App {
    /// Estado de la ventana (UI state)
    window_state: AppState,
    
    /// Motor Pomodoro del Core Domain (lógica pura)
    pomodoro_engine: SolarFocusCore::PomodoroEngine,
    
    /// Repositorio de persistencia SQLite
    session_repo: Option<SessionRepository>,
}

impl App {
    /// Crea una nueva instancia de la aplicación
    pub fn new() -> (Self, Task<Message>) {
        let engine = SolarFocusCore::PomodoroEngine::new();
        
        // Inicializar SQLite para persistencia de sesiones
        let session_repo = SessionRepository::new().ok();
        
        (
            Self {
                window_state: AppState::Idle,
                pomodoro_engine: engine,
                session_repo,
            },
            Task::none(),
        )
    }
    
    /// Título de la ventana
    pub fn title(&self) -> String {
        match self.pomodoro_engine.state() {
            SolarFocusCore::AppState::Idle => "🌱 SolarFocus OS - Esperando...".to_string(),
            SolarFocusCore::AppState::Focusing(_) => "🔥 SolarFocus OS - En Foco".to_string(),
            SolarFocusCore::AppState::Break => "☀️ SolarFocus OS - Descanso".to_string(),
            SolarFocusCore::AppState::Completed => "✨ SolarFocus OS - ¡Completado!".to_string(),
        }
    }
    
    /// Suscripción para ticks de tiempo precisos (independiente del loop de UI)
    fn subscription(&self) -> Subscription<Message> {
        if matches!(self.pomodoro_engine.state(), 
                   SolarFocusCore::AppState::Focusing(_) | 
                   SolarFocusCore::AppState::Break) {
            // Usar tiempo preciso: tick cada 33ms (~30 FPS para actualizaciones suaves)
            return iced::time::every(std::time::Duration::from_millis(33)).map(|_| Message::TimerTick(0.033));
        }
        
        Subscription::none()
    }
    
    /// Handle ticks de tiempo del motor Pomodoro
    fn handle_timer_tick(&mut self, delta: f32) -> Task<Message> {
        // El motor ya fue ticked en subscription, solo update UI state si necesario
        if matches!(self.pomodoro_engine.state(), 
                   SolarFocusCore::AppState::Focusing(_) | 
                   SolarFocusCore::AppState::Break) {
            self.window_state.tick(delta);
        }
        Task::none()
    }
    
    /// Actualización de estado (handle eventos)
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::StartFocus => {
                println!("🔥 Iniciando sesión de enfoque...");
                self.pomodoro_engine.start_focus();
                self.window_state = AppState::Focusing(self.pomodoro_engine.config().focus_duration);
                Task::none()
            },
            
            Message::Pause => {
                if let SolarFocusCore::AppState::Focusing(_) = self.pomodoro_engine.state() {
                    // Pausa de 30 segundos (pausa manual)
                    self.pomodoro_engine.pause(30.0);
                    
                    if matches!(self.pomodoro_engine.state(), 
                                SolarFocusCore::AppState::Idle) {
                        println!("⏸️ Sesión pausada temporalmente");
                        self.window_state = AppState::Paused;
                    }
                }
                Task::none()
            },
            
            Message::Resume => {
                if let SolarFocusCore::AppState::Focusing(_) = self.pomodoro_engine.state() {
                    self.pomodoro_engine.resume();
                    println!("▶️ Sesión reanudada");
                    self.window_state = AppState::Resuming;
                    
                    // Si resume y ya estaba en Break, transicionar de vuelta
                    if matches!(self.pomodoro_engine.state(), 
                                SolarFocusCore::AppState::Break) {
                        self.pomodoro_engine.transition_to_focus();
                    }
                }
                Task::none()
            },
            
            Message::TakeBreak => {
                // Forzar break manual (siempre 15 min por defecto)
                let _config = self.pomodoro_engine.config().clone();
                self.pomodoro_engine.transition_to_break();
                println!("☀️ Tomando descanso manual");
                self.window_state = AppState::Break;
                Task::none()
            },
            
            Message::TimerTick(delta) => {
                // El motor ya fue ticked en subscription, solo update UI state si necesario
                if matches!(self.pomodoro_engine.state(), 
                           SolarFocusCore::AppState::Focusing(_) | 
                           SolarFocusCore::AppState::Break) {
                    self.window_state.tick(delta);
                }
                Task::none()
            },
            
            Message::SessionCompleted => {
                println!("✨ Sesión finalizada - guardando en SQLite");
                
                // Guardar registro en SQLite
                if let Some(ref repo) = self.session_repo {
                    let duration = self.pomodoro_engine.config().focus_duration;
                    let record = infra::persistence::SessionRecord {
                        id: None,
                        start_time: Utc::now(),
                        duration,
                        state: "completed".to_string(),
                    };
                    
                    if let Ok(_) = repo.save_session(&record) {
                        println!("✅ Sesión guardada en base de datos local");
                    }
                }
                
                self.window_state = AppState::Completed;
                Task::none()
            },
        }
    }
    
    /// Renderizado de la interfaz de usuario
    pub fn view(&self) -> Element<Message> {
        // 🎨 Aplicar tema Solarpunk con fondo oscuro y texto verde/azul según estado
        let background_color = match self.pomodoro_engine.state() {
            SolarFocusCore::AppState::Focusing(_) => Color::from_rgb(15.0 / 255.0, 30.0 / 255.0, 20.0 / 255.0), // Verde oscuro casi negro
            SolarFocusCore::AppState::Break => Color::from_rgb(180.0 / 255.0, 220.0 / 255.0, 255.0 / 255.0),   // Azul claro para descanso
            _ => Color::from_rgb(10.0 / 255.0, 15.0 / 255.0, 12.0 / 255.0), // Gris muy oscuro (default)
        };
        
        let title = match self.pomodoro_engine.state() {
            SolarFocusCore::AppState::Idle => "🌱 SolarFocus OS",
            SolarFocusCore::AppState::Focusing(_) => "🔥 En Foco",
            SolarFocusCore::AppState::Break => "☀️ Descanso",
            SolarFocusCore::AppState::Completed => "✨ ¡Sesión Completada!",
        };
        
        let status_text = match self.pomodoro_engine.state() {
            SolarFocusCore::AppState::Idle => "Estado: Esperando inicio...".to_string(),
            SolarFocusCore::AppState::Focusing(_) => {
                let remaining = self.pomodoro_engine.remaining_time_formatted();
                format!("⏱️ FOCUS: {}", remaining)
            },
            SolarFocusCore::AppState::Break => "🌿 Descanso en curso".to_string(),
            SolarFocusCore::AppState::Completed => "🎉 ¡Excelente trabajo!".to_string(),
        };
        
        // 📊 Barra de progreso visual (usando Iced progress_bar)
        let progress = self.pomodoro_engine.progress();
        let progress_bar = progress_bar(
            0.0..=1.0,
            progress,
        )
        .width(Length::Fixed(200.0));
        
        let progress_text = match self.pomodoro_engine.state() {
            SolarFocusCore::AppState::Focusing(_) => format!("Progreso: {:.0}%", (progress * 100.0)),
            _ => "No en progreso".to_string(),
        };
        
        // 🎛️ Botones de control
        let btn_start: Element<_> = button("▶️ START FOCUS").on_press(Message::StartFocus).style(|_, _| button::Style {
            background: Some(iced::Background::Color(Color::from_rgb(0.2, 0.6, 0.3))),
            text_color: Color::WHITE,
            ..Default::default()
        }).into();

        let btn_pause: Element<_> = if matches!(self.pomodoro_engine.state(), SolarFocusCore::AppState::Focusing(_)) {
            button("⏸️ PAUSE").on_press(Message::Pause).style(|_, _| button::Style {
                background: Some(iced::Background::Color(Color::from_rgb(0.4, 0.4, 0.4))),
                text_color: Color::WHITE,
                ..Default::default()
            }).into()
        } else {
            container("N/A").style(|_| container::Style {
                background: Some(iced::Background::Color(Color::from_rgb(0.1, 0.1, 0.1))),
                text_color: Some(Color::from_rgb(0.5, 0.5, 0.5)),
                ..Default::default()
            }).into()
        };

        let btn_break: Element<_> = if matches!(self.pomodoro_engine.state(), SolarFocusCore::AppState::Focusing(_) | SolarFocusCore::AppState::Break) {
            button("☀️ TAKE BREAK").on_press(Message::TakeBreak).style(|_, _| button::Style {
                background: Some(iced::Background::Color(Color::from_rgb(0.3, 0.5, 0.4))),
                text_color: Color::WHITE,
                ..Default::default()
            }).into()
        } else {
            container("N/A").style(|_| container::Style {
                background: Some(iced::Background::Color(Color::from_rgb(0.1, 0.1, 0.1))),
                text_color: Some(Color::from_rgb(0.5, 0.5, 0.5)),
                ..Default::default()
            }).into()
        };

        let buttons = column![btn_start, btn_pause, btn_break]
        .spacing(10)
        .align_x(Horizontal::Center);
        
        // 📦 Contenido completo centrado
        let content = column![
            text(title).size(40),
            text(status_text).size(32),
            
            progress_bar,
            text(progress_text).size(16).color(Color::from_rgb(0.7, 0.8, 0.7)),
            
            buttons,
            
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
            .style(move |_theme| {
                container::Style {
                    background: Some(iced::Background::Color(background_color)),
                    ..Default::default()
                }
            })
            .into()
    }
}

/// Estado de la ventana (UI state simple)
#[derive(Debug, Clone)]
pub enum AppState {
    Idle,
    Focusing(f32), // Tiempo restante (sincronizado con core)
    Break,
    Completed,
    Paused,
    Resuming,
}

impl AppState {
    pub fn tick(&mut self, delta: f32) {
        if let AppState::Focusing(seconds_left) = self {
            if *seconds_left > 0.0 {
                *seconds_left -= delta;
            } else {
                *self = AppState::Completed;
            }
        }
    }
    
    pub fn display(&self) -> String {
        match self {
            AppState::Idle => "Estado: Idle".to_string(),
            AppState::Focusing(seconds) => {
                let mins = (*seconds / 60.0).round() as i32;
                let secs = ((*seconds % 60.0) * 100.0).round() as i32 / 100;
                format!("⏱️ FOCUS: {}m {:02}s", mins, secs)
            },
            AppState::Break => "🌿 DESCANSO".to_string(),
            AppState::Completed => "✨ COMPLETADO".to_string(),
            _ => "En transición...".to_string(),
        }
    }
}

fn main() -> iced::Result {
    // 🎨 Aplicar tema Solarpunk con colores personalizados usando Iced 0.13
    
    iced::application(App::title, App::update, App::view)
        .window(window::Settings {
            size: iced::Size::new(1024.0, 768.0),
            min_size: Some(iced::Size::new(800.0, 600.0)),
            ..Default::default()
        })
        .theme(|_| iced::Theme::Dark)
        .subscription(App::subscription) // ✅ Suscripción para ticks precisos
        .run_with(|| App::new())
}
