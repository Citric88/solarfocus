//! v1.6.0 — Setup → General tab: language, RAM mode, durations,
//! shortcuts, optional calendar deadline (when `calendar` feature on).

use iced::widget::{column, container, text};
use iced::{Element, Length};

use crate::ui::components::chip_local;
use crate::ui::palette::*;
use crate::{App, Message};
use solar_focus_intelligence::Language;

impl App {
    pub fn view_setup_general(&self) -> Element<'_, Message> {
        use crate::infra::settings::RamMode;
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

        let chip = |label: String, selected: bool, msg: Message| -> Element<'_, Message> {
            chip_local(label, selected, msg)
        };
        let row_chips = |label: String,
                         opts: &[u32],
                         current: u32,
                         msg: fn(u32) -> Message,
                         input_buf: &str,
                         text_msg: fn(String) -> Message,
                         placeholder: &str|
         -> Element<'_, Message> {
            let mut row = iced::widget::Row::new();
            for &m in opts {
                row = row.push(chip(format!("{}", m), current == m, msg(m)));
                row = row.push(iced::widget::Space::with_width(SPACE_XS as f32));
            }
            let input = iced::widget::text_input(placeholder, input_buf)
                .on_input(text_msg)
                .width(Length::Fixed(72.0))
                .padding([4, 8])
                .size(FONT_SMALL);
            row = row.push(input);
            row = row.push(iced::widget::Space::with_width(SPACE_XS as f32));
            row = row.push(text("min").size(FONT_TINY).color(TEXT_MUTED));
            column![
                text(label).size(FONT_SMALL).color(TEXT_MUTED),
                row,
            ]
            .spacing(4)
            .into()
        };

        let duration_card = container(
            column![
                text(match self.settings.language {
                    Language::Es => "Duraciones",
                    Language::En => "Durations",
                })
                .size(FONT_BODY)
                .color(TEXT_PRIMARY),
                row_chips(
                    format!(
                        "{} · {} min",
                        match self.settings.language {
                            Language::Es => "Foco",
                            Language::En => "Focus",
                        },
                        self.settings.focus_minutes,
                    ),
                    &[1, 5, 15, 25, 50],
                    self.settings.focus_minutes,
                    Message::SetFocusMinutes,
                    &self.custom_focus_str,
                    Message::SetFocusMinutesText,
                    "25",
                ),
                row_chips(
                    format!(
                        "{} · {} min",
                        match self.settings.language {
                            Language::Es => "Pausa corta",
                            Language::En => "Short break",
                        },
                        self.settings.break_minutes,
                    ),
                    &[1, 3, 5, 10, 15],
                    self.settings.break_minutes,
                    Message::SetBreakMinutes,
                    &self.custom_break_str,
                    Message::SetBreakMinutesText,
                    "5",
                ),
                row_chips(
                    format!(
                        "{} · {} min",
                        match self.settings.language {
                            Language::Es => "Pausa larga",
                            Language::En => "Long break",
                        },
                        self.settings.long_break_minutes,
                    ),
                    &[5, 10, 15, 20, 30],
                    self.settings.long_break_minutes,
                    Message::SetLongBreakMinutes,
                    &self.custom_long_break_str,
                    Message::SetLongBreakMinutesText,
                    "15",
                ),
                text(match self.settings.language {
                    Language::Es =>
                        "Pomodoro clásico: 25 / 5 / 15 (después de 4 sesiones). El campo numérico acepta valores personalizados (1–180).",
                    Language::En =>
                        "Classic Pomodoro: 25 / 5 / 15 (after 4 sessions). The numeric field accepts custom values (1–180).",
                })
                .size(FONT_TINY)
                .color(TEXT_MUTED),
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

        #[cfg(feature = "calendar")]
        let live_toggle_row: Element<'_, Message> = iced::widget::row![
            text(if self.settings.calendar_live_enabled {
                match self.settings.language {
                    Language::Es => "Lectura de Calendar (iCloud / Google / Local): activa",
                    Language::En => "Calendar (iCloud / Google / Local) live: enabled",
                }
            } else {
                match self.settings.language {
                    Language::Es => "Lectura de Calendar (iCloud / Google / Local): desactivada",
                    Language::En => "Calendar (iCloud / Google / Local) live: disabled",
                }
            })
            .size(FONT_SMALL)
            .color(TEXT_PRIMARY),
            iced::widget::horizontal_space(),
            chip_local(
                if self.settings.calendar_live_enabled {
                    match self.settings.language {
                        Language::Es => "Desactivar".to_string(),
                        Language::En => "Disable".to_string(),
                    }
                } else {
                    match self.settings.language {
                        Language::Es => "Activar".to_string(),
                        Language::En => "Enable".to_string(),
                    }
                },
                false,
                Message::ToggleCalendarLive(!self.settings.calendar_live_enabled),
            ),
        ]
        .into();

        #[cfg(feature = "calendar")]
        let calendar_status: Element<'_, Message> = if let Some(err) = &self.calendar_error {
            text(err.clone()).size(FONT_TINY).color(DANGER).into()
        } else if self.settings.calendar_live_enabled {
            text(format!(
                "{} {} {}",
                match self.settings.language {
                    Language::Es => "Eventos de hoy:",
                    Language::En => "Today's events:",
                },
                self.calendar_events.len(),
                match self.settings.language {
                    Language::Es => if self.calendar_events.len() == 1 { "evento" } else { "eventos" },
                    Language::En => if self.calendar_events.len() == 1 { "event" } else { "events" },
                },
            ))
            .size(FONT_TINY)
            .color(TEXT_MUTED)
            .into()
        } else {
            iced::widget::Space::with_height(Length::Fixed(0.0)).into()
        };

        #[cfg(feature = "calendar")]
        let deadline_card: Element<'_, Message> = container(
            column![
                text(match self.settings.language {
                    Language::Es => "Próxima reunión / deadline",
                    Language::En => "Next meeting / deadline",
                })
                .size(FONT_SMALL)
                .color(TEXT_MUTED),
                live_toggle_row,
                calendar_status,
                text(match self.settings.language {
                    Language::Es => "O introduce un deadline manual (se usa si el calendario está desactivado o no hay eventos):",
                    Language::En => "Or enter a manual deadline (used when live calendar is off or empty):",
                })
                .size(FONT_TINY)
                .color(TEXT_MUTED),
                iced::widget::row![
                    iced::widget::text_input(
                        match self.settings.language {
                            Language::Es => "Etiqueta (ej. Standup)",
                            Language::En => "Label (e.g. Standup)",
                        },
                        &self.settings.next_deadline_label,
                    )
                    .on_input(Message::SetDeadlineLabel)
                    .padding([4, 8])
                    .size(FONT_SMALL)
                    .width(Length::Fixed(220.0)),
                    iced::widget::Space::with_width(SPACE_SM as f32),
                    iced::widget::text_input("HH:MM", &self.deadline_time_str)
                        .on_input(Message::SetDeadlineTime)
                        .padding([4, 8])
                        .size(FONT_SMALL)
                        .width(Length::Fixed(80.0)),
                    iced::widget::Space::with_width(SPACE_SM as f32),
                    chip_local(
                        match self.settings.language {
                            Language::Es => "Borrar".to_string(),
                            Language::En => "Clear".to_string(),
                        },
                        false,
                        Message::ClearDeadline,
                    ),
                ],
            ]
            .spacing(SPACE_SM as u16),
        )
        .padding(SPACE_MD as u16)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(SURFACE)),
            border: iced::Border {
                radius: 8.0.into(),
                width: 1.0,
                color: ACCENT_DIM,
            },
            ..Default::default()
        })
        .into();

        let mut col = iced::widget::Column::new().spacing(SPACE_MD as u16);
        col = col.push(lang_card).push(duration_card).push(ram_card);
        #[cfg(feature = "calendar")]
        {
            col = col.push(deadline_card);
        }
        col = col.push(shortcuts);
        col.into()
    }

    pub(crate) fn lang_button(&self, lang: Language, label: &'static str) -> Element<'_, Message> {
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

    pub(crate) fn ram_card(
        &self,
        mode: crate::infra::settings::RamMode,
        title: &'static str,
        desc_es: &'static str,
        desc_en: &'static str,
    ) -> Element<'_, Message> {
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
}
