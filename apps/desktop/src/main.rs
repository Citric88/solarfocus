// apps/desktop/src/main.rs

use iced::{window, Application, Color, Element, Settings, Command, Theme, Alignment};
use iced::widget::{Column, Text, Button, Rule};

mod app_state;

#[derive(Debug)]
struct App {
    state: app_state::AppState,
}

impl App {
    fn new() -> Self {
        App { 
            state: app_state::AppState::Idle 
        }
    }

    // --- Renderizado (UI) ---
    fn view(&self) -> Element<'_, AppMessage> {
        let title = match &self.state {
            app_state::AppState::Idle => "🌱 SolarFocus OS",
            app_state::AppState::Focusing(_) => "🔥 En Foco",
            app_state::AppState::Break => "☀️ Descanso",
        };

        let status_text = self.state.display();

        // Estilo Solarpunk: Fondo oscuro, texto verde/brillante
        Column::new()
            .spacing(20)
            .align_items(Alignment::Center)
            .push(Text::new(title).size(40))
            .push(Text::new(status_text).size(32))
            .push(Rule::horizontal(10))
            .push(Button::new(Text::new("START FOCUS")).on_press(AppMessage::FocusStart))
            .push(Text::new("(Click START FOCUS para comenzar)").size(14).style(Color::from_rgb(0.6, 0.6, 0.6)))
            .into()
    }

    // --- Actualización (Lógica) ---
    fn update(&mut self, message: AppMessage) -> Command<AppMessage> {
        match message {
            AppMessage::FocusStart => {
                // Inicia sesión de 25 min (1500 segundos)
                self.state = app_state::AppState::Focusing(1500.0);
                println!("🚀 Iniciando Timer...");
                Command::none()
            },
        }
    }
}

#[derive(Debug, Clone)]
enum AppMessage {
    FocusStart,
}

// Implementación de la Aplicación (Required by Iced)
impl Application for App {
    type Executor = iced::executor::Default;
    type Message = AppMessage;
    type Theme = Theme;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Self::Message>) {
        (App::new(), Command::none())
    }

    fn title(&self) -> String {
        "SolarFocus OS".to_string()
    }

    fn update(&mut self, message: Self::Message) -> Command<Self::Message> {
        self.update(message)
    }

    fn view(&self) -> Element<'_, Self::Message> {
        self.view()
    }

    fn theme(&self) -> Theme {
        Theme::Dark
    }
}

fn main() -> iced::Result {
    App::run(Settings {
        window: window::Settings {
            size: (1024, 768),
            min_size: Some((800, 600)),
            ..Default::default()
        },
        ..Default::default()
    })
}
