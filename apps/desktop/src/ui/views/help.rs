//! v1.6.0 — Help canvas. Privacy hero + Empezar CTA + 5 feature
//! cards + keyboard-shortcuts panel + LLM status badge.

use iced::widget::{column, container, text};
use iced::{Element, Length};

use crate::ui::palette::*;
use crate::ui::sidebar::Route;
use crate::{App, Message};
use solar_focus_intelligence::Language;

impl App {
    pub fn view_help(&self) -> Element<'_, Message> {
        let lang = self.settings.language;

        let title = text(match lang {
            Language::Es => "¿Qué es SolarFocus OS?",
            Language::En => "What is SolarFocus OS?",
        })
        .size(FONT_TITLE)
        .color(TEXT_PRIMARY);

        let pitch = text(match lang {
            Language::Es =>
                "Un cronómetro Pomodoro con coach de IA local. Todo el procesamiento ocurre en tu equipo: \
                 sin nube, sin telemetría, sin cuenta. Diseñado para concentrarte sin sacrificar tu privacidad.",
            Language::En =>
                "A Pomodoro timer with a local AI coach. Everything runs on your machine: \
                 no cloud, no telemetry, no account. Designed to help you focus without sacrificing privacy.",
        })
        .size(FONT_BODY)
        .color(TEXT_SECONDARY);

        let privacy_hero = container(
            column![
                text(match lang {
                    Language::Es => "Privacidad por diseño",
                    Language::En => "Privacy by design",
                })
                .size(FONT_LEAD)
                .color(TEXT_PRIMARY),
                text(match lang {
                    Language::Es => "Sin nube · Sin telemetría · Sin cuenta",
                    Language::En => "No cloud · No telemetry · No account",
                })
                .size(FONT_SMALL)
                .color(TEXT_SECONDARY),
            ]
            .spacing(SPACE_XS as u16),
        )
        .padding(SPACE_MD as u16)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(SURFACE_RAISED)),
            border: iced::Border {
                color: ACCENT_DIM,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        });

        let cta_label = match lang {
            Language::Es => "Empezar",
            Language::En => "Get started",
        };
        let cta = iced::widget::button(text(cta_label.to_string()).size(FONT_BODY).color(BG))
            .on_press(Message::SwitchRoute(Route::Focus))
            .padding([12, 32])
            .style(|_, _| iced::widget::button::Style {
                background: Some(iced::Background::Color(ACCENT)),
                text_color: BG,
                border: iced::Border {
                    radius: 8.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            });
        let cta_row: Element<'_, Message> = iced::widget::row![
            iced::widget::horizontal_space(),
            cta,
            iced::widget::horizontal_space(),
        ]
        .into();

        let feature = |num: &'static str,
                        glyph: crate::ui::sidebar::IconGlyph,
                        title_str: String,
                        summary_str: String,
                        howto_str: String|
         -> Element<'_, Message> {
            container(
                column![
                    iced::widget::row![
                        iced::widget::Canvas::new(crate::ui::sidebar::IconCanvas { glyph, selected: true })
                            .width(Length::Fixed(28.0))
                            .height(Length::Fixed(28.0)),
                        iced::widget::Space::with_width(SPACE_MD as f32),
                        column![
                            iced::widget::row![
                                text(num.to_string())
                                    .size(FONT_TINY)
                                    .color(TEXT_MUTED),
                                iced::widget::Space::with_width(SPACE_XS as f32),
                                text(title_str).size(FONT_BODY).color(TEXT_PRIMARY),
                            ],
                            text(summary_str).size(FONT_SMALL).color(TEXT_SECONDARY),
                        ]
                        .spacing(2),
                    ],
                    iced::widget::Space::with_height(SPACE_SM as f32),
                    text(howto_str).size(FONT_SMALL).color(TEXT_MUTED),
                ]
                .spacing(SPACE_XS as u16)
                .padding(SPACE_SM as u16),
            )
            .padding(SPACE_SM as u16)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(SURFACE)),
                border: iced::Border {
                    radius: 8.0.into(),
                    width: 1.0,
                    color: ACCENT_DIM,
                },
                ..Default::default()
            })
            .into()
        };

        let pick = |es: &str, en: &str| -> String {
            if lang == Language::Es { es.to_string() } else { en.to_string() }
        };

        let features = column![
            feature("1", crate::ui::sidebar::IconGlyph::Focus,
                pick("Cronómetro Pomodoro", "Pomodoro timer"),
                pick("Sesiones de foco + pausas configurables.",
                     "Configurable focus + break durations."),
                pick("Cómo funciona: tras 25 minutos (configurable en Setup → General · Duraciones), \
                      arranca una pausa corta de 5 min. Cada 4 sesiones la pausa se vuelve larga (15 min). \
                      Puedes terminar la sesión cuando quieras con Esc o el botón \"Terminar sesión\".",
                     "How it works: after 25 minutes (configurable in Setup → General · Durations), a 5-min short \
                      break begins. Every 4 sessions the break becomes long (15 min). You can end the session \
                      anytime with Esc or the \"End session\" button.")),

            feature("2", crate::ui::sidebar::IconGlyph::Setup,
                pick("Detección de distracciones", "Distraction detection"),
                pick("Avisa cuando te alejas del trabajo durante una sesión.",
                     "Alerts you when you drift off-task during a session."),
                pick("Cómo funciona: cada 10 segundos lee la ventana activa del sistema operativo \
                      (nombre del proceso + título de la ventana, vía macOS NSWorkspace). \
                      Compara contra una lista local de procesos/URLs (TikTok, Instagram, youtube.com/watch, etc.). \
                      Necesita 2 muestras consecutivas con confianza ≥ 0.7 antes de mostrar un aviso (evita falsos positivos por cambio rápido de pestaña). \
                      Privacidad: ni el nombre ni el título salen del equipo.",
                     "How it works: every 10 seconds reads the active OS window (process name + title, via macOS NSWorkspace). \
                      Compares against a local list of processes/URLs (TikTok, Instagram, youtube.com/watch, etc.). \
                      Requires 2 consecutive samples at confidence ≥ 0.7 before alerting (prevents false positives from quick tab switches). \
                      Privacy: neither name nor title leave your machine.")),

            feature("3", crate::ui::sidebar::IconGlyph::Coach,
                pick("Coach IA local", "Local AI coach"),
                pick("Mensajes personalizados al iniciar, terminar o pausar sesiones.",
                     "Personalized messages at session start, end, and pauses."),
                pick("Cómo funciona: usa SmolLM2 1.7B (modelo de 1 GB que vive en tu disco) más un banco de \
                      ~50 mensajes curados a mano. El coach combina hora del día, día de la semana, número \
                      de sesiones hoy, racha actual y distracciones recientes para personalizar el mensaje. \
                      Si el LLM produce algo incoherente, cae en el banco curado. Califica con Útil / No útil \
                      en la pestaña Coach para mejorar futuros mensajes.",
                     "How it works: uses SmolLM2 1.7B (a 1 GB model that lives on your disk) plus ~50 hand-curated \
                      messages. The coach combines time of day, weekday, today's session count, current streak, \
                      and recent distractions to personalize. If the LLM produces something incoherent it falls \
                      back to the curated bank. Rate with Helpful / Not helpful in the Coach tab to improve.")),

            feature("4", crate::ui::sidebar::IconGlyph::Stats,
                pick("Resumen diario", "Daily recap"),
                pick("Resumen automático con tus números reales del día anterior.",
                     "Automatic summary of yesterday's real numbers."),
                pick("Cómo funciona: una vez al día (al cambiar de fecha local), pulla todas las sesiones \
                      completadas del día anterior, calcula totales, y genera una frase de cierre con el LLM \
                      grounded en los datos reales. El resumen se guarda en la base de datos SQLite local.",
                     "How it works: once per day (on local date change), pulls all completed sessions from yesterday, \
                      computes totals, and generates a closing sentence with the LLM grounded in the real data. \
                      The summary is saved in the local SQLite database.")),

            feature("5", crate::ui::sidebar::IconGlyph::Stats,
                pick("Estadísticas", "Stats"),
                pick("Sesiones, distracciones, gráfica semanal, totales históricos.",
                     "Sessions, distractions, weekly chart, lifetime totals."),
                pick("Cómo funciona: cada sesión completada se guarda en SQLite local con timestamp y duración. \
                      La pestaña Stats agrega contadores de hoy, esta semana y total histórico, más una gráfica \
                      de minutos por día para los últimos 7 días. La base de datos vive en \
                      ~/Library/Application Support/SolarFocus OS.",
                     "How it works: each completed session is saved to local SQLite with timestamp and duration. \
                      The Stats tab aggregates today, week, and lifetime counters plus a 7-day minutes-per-day chart. \
                      The database lives in ~/Library/Application Support/SolarFocus OS.")),
        ]
        .spacing(SPACE_SM as u16);

        let shortcuts = container(
            column![
                text(match lang {
                    Language::Es => "Atajos de teclado",
                    Language::En => "Keyboard shortcuts",
                })
                .size(FONT_BODY)
                .color(TEXT_PRIMARY),
                text("Space / P  ·  Pausar  /  Pause").size(FONT_SMALL).color(TEXT_SECONDARY),
                text("R  ·  Reanudar  /  Resume").size(FONT_SMALL).color(TEXT_SECONDARY),
                text("B  ·  Tomar descanso  /  Take break").size(FONT_SMALL).color(TEXT_SECONDARY),
                text("S  ·  Setup").size(FONT_SMALL).color(TEXT_SECONDARY),
                text("?  /  5  ·  Help").size(FONT_SMALL).color(TEXT_SECONDARY),
                text("1 / 2 / 3 / 4  ·  Cambiar de pestaña / Switch tab").size(FONT_SMALL).color(TEXT_SECONDARY),
            ]
            .spacing(SPACE_XS as u16),
        )
        .padding(SPACE_MD as u16)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(SURFACE)),
            border: iced::Border { radius: 6.0.into(), ..Default::default() },
            ..Default::default()
        });

        let model_state = if cfg!(feature = "llm") {
            match (self.settings.ai_enabled, self.coach.is_ready()) {
                (true, true) => format!(
                    "{}: {:?} ({})",
                    match lang { Language::Es => "Coach IA activo", Language::En => "AI coach active" },
                    self.settings.model_choice,
                    if self.model_present_cache.unwrap_or(false) {
                        match lang { Language::Es => "modelo en disco", Language::En => "model on disk" }
                    } else {
                        match lang { Language::Es => "esperando descarga", Language::En => "awaiting download" }
                    }
                ),
                _ => match lang {
                    Language::Es => "Coach IA desactivado".to_string(),
                    Language::En => "AI coach disabled".to_string(),
                },
            }
        } else {
            match lang {
                Language::Es => "Build sin LLM (recompila con --features llm)".to_string(),
                Language::En => "Build without LLM (rebuild with --features llm)".to_string(),
            }
        };
        let status = container(text(model_state).size(FONT_SMALL).color(TEXT_PRIMARY))
            .padding(SPACE_SM as u16)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(SURFACE_RAISED)),
                border: iced::Border { radius: 6.0.into(), ..Default::default() },
                ..Default::default()
            });

        let body = column![
            title,
            pitch,
            privacy_hero,
            cta_row,
            status,
            features,
            shortcuts,
        ]
        .spacing(SPACE_LG as u16)
        .padding(SPACE_XL as u16)
        .max_width(720);

        container(iced::widget::scrollable(body))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding([SPACE_LG as u16, SPACE_XL as u16])
            .style(|_| container::Style {
                background: Some(iced::Background::Color(BG)),
                ..Default::default()
            })
            .into()
    }
}
