// apps/desktop/src/main.rs

use iced::{Element, Size, Task, window};
use iced::widget::{button, column, text};

mod app_state;

#[derive(Debug)]
struct App {
    state: app_state::AppState,
}

impl App {
    fn new() -> (Self, Task<Message>) {
        (
            App { state: app_state::AppState::Idle },
            Task::none(),
        )
    }

    fn title(&self) -> String {
        "SolarFocus OS".to_string()
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::FocusStart => {
                println!("🔥 Iniciando sesión de enfoque...");
                self.state = app_state::AppState::Focusing(1500.0); // 25 min en segundos
                Task::none()
            },
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let title = match &self.state {
            app_state::AppState::Idle => "🌱 SolarFocus OS",
            app_state::AppState::Focusing(_) => "🔥 En Foco",
            app_state::AppState::Break => "☀️ Descanso",
        };

        let status_text = self.state.display();

        column![
            text(title).size(40),
            text(status_text).size(32),
            button("START FOCUS").on_press(Message::FocusStart),
            text("(Click START FOCUS para comenzar)")
                .size(14)
                .color(iced::Color::from_rgb(0.6, 0.6, 0.6)),
        ]
        .spacing(20)
        .into()
    }
}

#[derive(Debug, Clone)]
enum Message {
    FocusStart,
}

fn main() -> iced::Result {
    iced::application(App::title, App::update, App::view)
        .window(window::Settings {
            size: Size::new(1024.0, 768.0),
            min_size: Some(Size::new(800.0, 600.0)),
            ..Default::default()
        })
        .run_with(App::new)
}
