//! v1.6.0 — Stats canvas: 4 summary cards, permission card,
//! weekly chart, "Por categoría" panel, "Distracciones más
//! frecuentes" panel, daily recap card, today's sessions list with
//! per-row attention badge. Whole body wrapped in a scrollable.

use iced::widget::{column, container, text};
use iced::{Element, Length};

use chrono::Datelike;

use crate::app::helpers::weekday_short;
use crate::ui::components::{badge_local, chip_local, ghost_button, BadgeVariant};
use crate::ui::palette::*;
use crate::{today_iso_local, App, Message, PermissionStatus};
use solar_focus_intelligence::Language;

impl App {
    pub fn view_stats_placeholder(&self) -> Element<'_, Message> {
        let (up, down) = self.feedback_counts_cache;
        let week = self
            .session_repo
            .as_ref()
            .and_then(|r| r.weekly_focus_seconds().ok())
            .unwrap_or_default();
        let (lifetime_n, lifetime_secs) = self
            .session_repo
            .as_ref()
            .and_then(|r| r.lifetime_totals().ok())
            .unwrap_or((0, 0));

        let today_secs: u32 = week.last().map(|(_, s)| *s).unwrap_or(0);
        let week_secs: u32 = week.iter().map(|(_, s)| *s).sum();

        let card = |title: &str, primary: String, secondary: &str| -> Element<'_, Message> {
            container(
                column![
                    text(title.to_string()).size(FONT_SMALL).color(TEXT_MUTED),
                    text(primary).size(FONT_TITLE).color(TEXT_PRIMARY),
                    text(secondary.to_string())
                        .size(FONT_SMALL)
                        .color(TEXT_SECONDARY),
                ]
                .spacing(SPACE_XS as u16),
            )
            .padding(SPACE_MD as u16)
            .width(Length::Fixed(200.0))
            .style(|_| container::Style {
                background: Some(iced::Background::Color(SURFACE)),
                border: iced::Border {
                    radius: 8.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
        };

        let cards = iced::widget::row![
            card(
                match self.settings.language {
                    Language::Es => "HOY",
                    Language::En => "TODAY",
                },
                format!("{}", self.sessions_today),
                &format!("{} min de foco", today_secs / 60),
            ),
            iced::widget::Space::with_width(SPACE_MD as f32),
            card(
                match self.settings.language {
                    Language::Es => "DISTRACCIONES",
                    Language::En => "DISTRACTIONS",
                },
                format!("{}", self.distractions_today),
                match self.settings.language {
                    Language::Es => "hoy",
                    Language::En => "today",
                },
            ),
            iced::widget::Space::with_width(SPACE_MD as f32),
            card(
                match self.settings.language {
                    Language::Es => "ESTA SEMANA",
                    Language::En => "THIS WEEK",
                },
                format!("{} min", week_secs / 60),
                &format!("{} sesiones de coaching", up + down),
            ),
            iced::widget::Space::with_width(SPACE_MD as f32),
            card(
                match self.settings.language {
                    Language::Es => "TOTAL",
                    Language::En => "ALL-TIME",
                },
                format!("{}", lifetime_n),
                &format!("{} h totales", lifetime_secs / 3600),
            ),
            iced::widget::Space::with_width(SPACE_MD as f32),
            card(
                match self.settings.language {
                    Language::Es => "SEMILLAS 🌱",
                    Language::En => "SEEDS 🌱",
                },
                format!("{}", self.seeds_total_cache),
                match self.settings.language {
                    Language::Es => "cosechadas",
                    Language::En => "harvested",
                },
            ),
        ];

        let (perm_color, perm_text_es, perm_text_en) = match self.permission_status {
            PermissionStatus::Granted => (
                ACCENT,
                "Permiso concedido — vigilancia completa",
                "Permission granted — full window watching",
            ),
            PermissionStatus::NameOnly => (
                WARNING,
                "Permiso parcial — solo nombre del proceso (concede Screen Recording para títulos)",
                "Partial permission — process names only (grant Screen Recording for titles)",
            ),
            PermissionStatus::Denied => (
                DANGER,
                "Sin permiso — no se puede leer la ventana activa",
                "No permission — can't read active window",
            ),
            PermissionStatus::Unknown => (
                TEXT_MUTED,
                "Verificando permiso…",
                "Checking permission…",
            ),
        };
        let perm_header_row: Element<'_, Message> = iced::widget::row![
            text("●").size(FONT_LEAD).color(perm_color),
            iced::widget::Space::with_width(SPACE_SM as f32),
            text(if self.settings.language == Language::Es {
                perm_text_es
            } else {
                perm_text_en
            })
            .size(FONT_SMALL)
            .color(TEXT_PRIMARY),
        ]
        .align_y(iced::alignment::Vertical::Center)
        .into();
        let perm_actions_row: Element<'_, Message> =
            if !matches!(self.permission_status, PermissionStatus::Granted) {
                iced::widget::row![
                    chip_local(
                        match self.settings.language {
                            Language::Es => "Abrir Ajustes del sistema".to_string(),
                            Language::En => "Open System Settings".to_string(),
                        },
                        false,
                        Message::OpenSystemSettings,
                    ),
                    iced::widget::Space::with_width(SPACE_XS as f32),
                    chip_local(
                        match self.settings.language {
                            Language::Es => "Re-verificar".to_string(),
                            Language::En => "Re-check".to_string(),
                        },
                        false,
                        Message::ProbePermission,
                    ),
                ]
                .into()
            } else {
                iced::widget::Space::with_height(Length::Fixed(0.0)).into()
            };
        let perm_card: Element<'_, Message> = container(
            column![perm_header_row, perm_actions_row].spacing(SPACE_SM as u16),
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

        let today_iso = today_iso_local().unwrap_or_default();
        let today_sessions = self
            .session_repo
            .as_ref()
            .and_then(|r| r.sessions_for_date(&today_iso).ok())
            .unwrap_or_default();
        let sessions_list: Element<'_, Message> = if today_sessions.is_empty() {
            container(
                text(match self.settings.language {
                    Language::Es => "Aún no has completado sesiones hoy.",
                    Language::En => "No completed sessions yet today.",
                })
                .size(FONT_SMALL)
                .color(TEXT_MUTED),
            )
            .padding(SPACE_MD as u16)
            .width(Length::Fixed(560.0))
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
        } else {
            let rows: Vec<Element<'_, Message>> = today_sessions
                .into_iter()
                .map(|s| {
                    let when = s.start_time.with_timezone(&chrono::Local).format("%H:%M").to_string();
                    let mins = (s.duration / 60.0).round() as u32;
                    let distract_count = self
                        .session_repo
                        .as_ref()
                        .and_then(|r| {
                            r.distractions_in_session_window(&s.start_time, s.duration)
                                .ok()
                        })
                        .unwrap_or(0);
                    let attention = 100u32.saturating_sub(distract_count.saturating_mul(20));
                    let attention_variant = if attention >= 80 {
                        BadgeVariant::Accent
                    } else if attention >= 50 {
                        BadgeVariant::Warning
                    } else {
                        BadgeVariant::Danger
                    };
                    let attention_label = match self.settings.language {
                        Language::Es => format!("Atención {}%", attention),
                        Language::En => format!("Focus {}%", attention),
                    };
                    let _ = &s.state;
                    let mut row = iced::widget::Row::new()
                        .push(text(when).size(FONT_SMALL).color(TEXT_SECONDARY))
                        .push(iced::widget::Space::with_width(SPACE_MD as f32))
                        .push(
                            text(format!("{} min", mins))
                                .size(FONT_SMALL)
                                .color(TEXT_PRIMARY),
                        )
                        .push(iced::widget::Space::with_width(SPACE_SM as f32))
                        .push(badge_local(s.category.clone(), BadgeVariant::Accent))
                        .push(iced::widget::Space::with_width(SPACE_SM as f32))
                        .push(badge_local(attention_label, attention_variant));
                    if !s.is_valid {
                        let invalid_label = match self.settings.language {
                            Language::Es => "No válida".to_string(),
                            Language::En => "Invalid".to_string(),
                        };
                        row = row
                            .push(iced::widget::Space::with_width(SPACE_SM as f32))
                            .push(badge_local(invalid_label, BadgeVariant::Muted));
                    }
                    row = row.push(iced::widget::horizontal_space());
                    container(
                        row.padding(SPACE_XS as u16)
                            .align_y(iced::alignment::Vertical::Center),
                    )
                    .padding(SPACE_XS as u16)
                    .into()
                })
                .collect();
            container(column(rows).spacing(SPACE_XS as u16))
                .padding(SPACE_SM as u16)
                .width(Length::Fixed(560.0))
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

        let chart_bars: Vec<(String, u32)> = week
            .iter()
            .map(|(d, s)| {
                let parsed = chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok();
                let label = parsed
                    .map(|d| weekday_short(d.weekday()))
                    .unwrap_or("?".to_string());
                (label, s / 60)
            })
            .collect();

        let chart: Element<'_, Message> =
            iced::widget::Canvas::new(crate::ui::chart::WeeklyChart::new(chart_bars))
                .width(Length::Fixed(560.0))
                .height(Length::Fixed(160.0))
                .into();
        let chart_card = container(
            column![
                text(match self.settings.language {
                    Language::Es => "Minutos de foco por día",
                    Language::En => "Focus minutes per day",
                })
                .size(FONT_SMALL)
                .color(TEXT_MUTED),
                chart,
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

        let recap_card: Element<'_, Message> = if let Some((d, t)) = &self.recap {
            container(
                column![
                    iced::widget::row![
                        text(format!(
                            "{} {}",
                            match self.settings.language {
                                Language::Es => "Resumen de",
                                Language::En => "Recap of",
                            },
                            d
                        ))
                        .size(FONT_SMALL)
                        .color(TEXT_MUTED),
                        iced::widget::horizontal_space(),
                        ghost_button(
                            match self.settings.language {
                                Language::Es => "Regenerar",
                                Language::En => "Regenerate",
                            },
                            Message::GenerateRecapNow,
                        ),
                    ],
                    text(t.clone()).size(FONT_BODY).color(TEXT_PRIMARY),
                ]
                .spacing(SPACE_XS as u16),
            )
            .padding(SPACE_MD as u16)
            .width(Length::Fixed(560.0))
            .style(|_| container::Style {
                background: Some(iced::Background::Color(SURFACE_RAISED)),
                border: iced::Border {
                    radius: 8.0.into(),
                    width: 1.0,
                    color: ACCENT_DIM,
                },
                ..Default::default()
            })
            .into()
        } else {
            container(
                iced::widget::row![
                    text(match self.settings.language {
                        Language::Es => "Aún no hay resumen del día.",
                        Language::En => "No daily recap yet.",
                    })
                    .size(FONT_SMALL)
                    .color(TEXT_MUTED),
                    iced::widget::horizontal_space(),
                    ghost_button(
                        match self.settings.language {
                            Language::Es => "Generar ahora",
                            Language::En => "Generate now",
                        },
                        Message::GenerateRecapNow,
                    ),
                ]
                .padding(SPACE_SM as u16),
            )
            .padding(SPACE_XS as u16)
            .width(Length::Fixed(560.0))
            .style(|_| container::Style {
                background: Some(iced::Background::Color(SURFACE)),
                border: iced::Border {
                    radius: 6.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
        };

        let sessions_title = text(match self.settings.language {
            Language::Es => "Sesiones de hoy",
            Language::En => "Today's sessions",
        })
        .size(FONT_BODY)
        .color(TEXT_SECONDARY);

        let attention_disclaimer = text(match self.settings.language {
            Language::Es => "Atención = ventanas en la deny-list + ausencias detectadas por la cámara. La lista cubre redes sociales, streaming y juegos populares. Si una app o web te distrae y no la cuenta, edita rules.toml en ~/Library/Application Support/SolarFocus OS. Otros dispositivos (teléfono, tablet) no se detectan.",
            Language::En => "Focus score = denylisted windows + camera-detected absences. The list covers social, streaming, and major games. If an app or site distracts you and isn't counted, edit rules.toml under ~/Library/Application Support/SolarFocus OS. Other devices (phone, tablet) aren't detected.",
        })
        .size(FONT_TINY)
        .color(TEXT_MUTED);

        let category_totals = self
            .session_repo
            .as_ref()
            .and_then(|r| r.category_totals_last_days(7).ok())
            .unwrap_or_default();
        let category_card: Element<'_, Message> = {
            let title = text(match self.settings.language {
                Language::Es => "Por categoría · últimos 7 días",
                Language::En => "By category · last 7 days",
            })
            .size(FONT_SMALL)
            .color(TEXT_MUTED);
            let inner: Element<'_, Message> = if category_totals.is_empty() {
                text(match self.settings.language {
                    Language::Es => "Aún no hay sesiones con categoría en los últimos 7 días.",
                    Language::En => "No category-tagged sessions in the last 7 days.",
                })
                .size(FONT_SMALL)
                .color(TEXT_MUTED)
                .into()
            } else {
                let total_secs: u32 = category_totals.iter().map(|(_, s, _)| *s).sum();
                let max_secs: u32 = category_totals
                    .iter()
                    .map(|(_, s, _)| *s)
                    .max()
                    .unwrap_or(1)
                    .max(1);
                let rows: Vec<Element<'_, Message>> = category_totals
                    .iter()
                    .map(|(name, secs, count)| {
                        let mins = secs / 60;
                        let pct = if total_secs > 0 {
                            (*secs as f32 / total_secs as f32 * 100.0).round() as u32
                        } else { 0 };
                        let bar_w = (*secs as f32 / max_secs as f32 * 280.0).max(2.0);
                        let bar = iced::widget::container(
                            iced::widget::Space::with_height(Length::Fixed(8.0)),
                        )
                        .width(Length::Fixed(bar_w))
                        .height(Length::Fixed(8.0))
                        .style(|_| container::Style {
                            background: Some(iced::Background::Color(ACCENT)),
                            border: iced::Border {
                                radius: 4.0.into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        });
                        container(
                            column![
                                iced::widget::row![
                                    text(name.clone())
                                        .size(FONT_BODY)
                                        .color(TEXT_PRIMARY),
                                    iced::widget::horizontal_space(),
                                    text(format!(
                                        "{} min · {} {} · {}%",
                                        mins,
                                        count,
                                        match self.settings.language {
                                            Language::Es => if *count == 1 { "sesión" } else { "sesiones" },
                                            Language::En => if *count == 1 { "session" } else { "sessions" },
                                        },
                                        pct,
                                    ))
                                    .size(FONT_TINY)
                                    .color(TEXT_SECONDARY),
                                ],
                                bar,
                            ]
                            .spacing(SPACE_XS as u16),
                        )
                        .padding([SPACE_XS as u16, 0])
                        .into()
                    })
                    .collect();
                column(rows).spacing(SPACE_SM as u16).into()
            };
            container(column![title, inner].spacing(SPACE_SM as u16))
                .padding(SPACE_MD as u16)
                .width(Length::Fixed(560.0))
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

        let top_distractions = self
            .session_repo
            .as_ref()
            .and_then(|r| r.top_distractions_last_days(7, 6).ok())
            .unwrap_or_default();
        let distractions_card: Element<'_, Message> = {
            let title = text(match self.settings.language {
                Language::Es => "Distracciones más frecuentes · últimos 7 días",
                Language::En => "Top distractions · last 7 days",
            })
            .size(FONT_SMALL)
            .color(TEXT_MUTED);
            let inner: Element<'_, Message> = if top_distractions.is_empty() {
                text(match self.settings.language {
                    Language::Es => "Sin distracciones registradas en los últimos 7 días. Buen trabajo.",
                    Language::En => "No distractions logged in the last 7 days. Nice.",
                })
                .size(FONT_SMALL)
                .color(TEXT_MUTED)
                .into()
            } else {
                let max_count = top_distractions
                    .iter()
                    .map(|(_, c)| *c)
                    .max()
                    .unwrap_or(1)
                    .max(1);
                let rows: Vec<Element<'_, Message>> = top_distractions
                    .iter()
                    .map(|(name, count)| {
                        let bar_w = (*count as f32 / max_count as f32 * 280.0).max(2.0);
                        let bar = iced::widget::container(
                            iced::widget::Space::with_height(Length::Fixed(8.0)),
                        )
                        .width(Length::Fixed(bar_w))
                        .height(Length::Fixed(8.0))
                        .style(|_| container::Style {
                            background: Some(iced::Background::Color(DANGER)),
                            border: iced::Border {
                                radius: 4.0.into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        });
                        container(
                            column![
                                iced::widget::row![
                                    text(name.clone())
                                        .size(FONT_BODY)
                                        .color(TEXT_PRIMARY),
                                    iced::widget::horizontal_space(),
                                    text(format!(
                                        "{} {}",
                                        count,
                                        match self.settings.language {
                                            Language::Es => if *count == 1 { "vez" } else { "veces" },
                                            Language::En => if *count == 1 { "hit" } else { "hits" },
                                        },
                                    ))
                                    .size(FONT_TINY)
                                    .color(TEXT_SECONDARY),
                                ],
                                bar,
                            ]
                            .spacing(SPACE_XS as u16),
                        )
                        .padding([SPACE_XS as u16, 0])
                        .into()
                    })
                    .collect();
                column(rows).spacing(SPACE_SM as u16).into()
            };
            container(column![title, inner].spacing(SPACE_SM as u16))
                .padding(SPACE_MD as u16)
                .width(Length::Fixed(560.0))
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

        let body = column![
            text(match self.settings.language {
                Language::Es => "Estadísticas",
                Language::En => "Stats",
            })
            .size(FONT_TITLE)
            .color(TEXT_PRIMARY),
            perm_card,
            cards,
            chart_card,
            category_card,
            distractions_card,
            recap_card,
            sessions_title,
            attention_disclaimer,
            sessions_list,
        ]
        .spacing(SPACE_LG as u16)
        .padding(SPACE_XL as u16)
        .max_width(680);

        container(iced::widget::scrollable(body))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(BG)),
                ..Default::default()
            })
            .into()
    }
}
