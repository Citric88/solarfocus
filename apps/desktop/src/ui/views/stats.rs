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

        // v1.12.1 Wave 2 — fresh seed pulls for cards + new charts.
        let seeds_today_count: u32 = self
            .session_repo
            .as_ref()
            .and_then(|r| r.seeds_today().ok())
            .unwrap_or(0);
        let seeds_week: Vec<(String, u32)> = self
            .session_repo
            .as_ref()
            .and_then(|r| r.seeds_last_days(7).ok())
            .unwrap_or_default();
        let seeds_recent: Vec<(String, String, u32)> = self
            .session_repo
            .as_ref()
            .and_then(|r| r.recent_seeds(10).ok())
            .unwrap_or_default();

        // v1.12.2 Wave 4 — dashboard analytics pulls.
        let avg_attention_today: Option<u8> = self
            .session_repo
            .as_ref()
            .and_then(|r| r.average_attention_last_days(1).ok().flatten());
        let avg_attention_7d: Option<u8> = self
            .session_repo
            .as_ref()
            .and_then(|r| r.average_attention_last_days(7).ok().flatten());
        let focus_by_hour: Vec<(String, u32)> = self
            .session_repo
            .as_ref()
            .and_then(|r| r.focus_minutes_by_hour(30).ok())
            .unwrap_or_default();
        let seeds_kind_breakdown: Vec<(String, u32)> = self
            .session_repo
            .as_ref()
            .and_then(|r| r.seeds_by_kind().ok())
            .unwrap_or_default();
        let longest_streak: u32 = self
            .session_repo
            .as_ref()
            .and_then(|r| r.longest_valid_streak().ok())
            .unwrap_or(0);
        let longest_session: Option<(String, u32, String)> = self
            .session_repo
            .as_ref()
            .and_then(|r| r.longest_session().ok().flatten());
        let perfect_days: u32 = self
            .session_repo
            .as_ref()
            .and_then(|r| r.perfect_days_this_month().ok())
            .unwrap_or(0);
        let validity_7d: (u32, u32) = self
            .session_repo
            .as_ref()
            .and_then(|r| r.validity_last_days(7).ok())
            .unwrap_or((0, 0));

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

        // v1.12.1 — split each KPI card into a named binding so the
        // body can stack them in a 2-row layout. Each card is the same
        // 200 px-wide container.
        let card_today = card(
            match self.settings.language {
                Language::Es => "HOY",
                Language::En => "TODAY",
            },
            format!("{}", self.sessions_today),
            &format!("{} min de foco", today_secs / 60),
        );
        let card_distractions = card(
            match self.settings.language {
                Language::Es => "DISTRACCIONES",
                Language::En => "DISTRACTIONS",
            },
            format!("{}", self.distractions_today),
            match self.settings.language {
                Language::Es => "hoy",
                Language::En => "today",
            },
        );
        let card_week = card(
            match self.settings.language {
                Language::Es => "ESTA SEMANA",
                Language::En => "THIS WEEK",
            },
            format!("{} min", week_secs / 60),
            &format!("{} sesiones de coaching", up + down),
        );
        let card_lifetime = card(
            match self.settings.language {
                Language::Es => "TOTAL",
                Language::En => "ALL-TIME",
            },
            format!("{}", lifetime_n),
            &format!("{} h totales", lifetime_secs / 3600),
        );
        let card_seeds = card(
            match self.settings.language {
                Language::Es => "SEMILLAS",
                Language::En => "SEEDS",
            },
            format!("{}", self.seeds_total_cache),
            &match self.settings.language {
                Language::Es => format!("Hoy: +{}", seeds_today_count),
                Language::En => format!("Today: +{}", seeds_today_count),
            },
        );

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

        // v1.12.1 Wave 2 — weekly seeds chart. Same WeeklyChart widget,
        // ACCENT_DIM color so the eye reads it as secondary to focus
        // minutes above.
        let seed_chart_bars: Vec<(String, u32)> = seeds_week
            .iter()
            .map(|(d, n)| {
                let parsed = chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok();
                let label = parsed
                    .map(|d| weekday_short(d.weekday()))
                    .unwrap_or("?".to_string());
                (label, *n)
            })
            .collect();
        let seed_chart_widget = {
            let mut wc = crate::ui::chart::WeeklyChart::new(seed_chart_bars);
            wc.bar_color = ACCENT_DIM;
            wc
        };
        let seed_chart_canvas: Element<'_, Message> = iced::widget::Canvas::new(seed_chart_widget)
            .width(Length::Fixed(560.0))
            .height(Length::Fixed(160.0))
            .into();
        let seed_chart_card = container(
            column![
                text(match self.settings.language {
                    Language::Es => "Semillas por día · últimos 7 días",
                    Language::En => "Seeds per day · last 7 days",
                })
                .size(FONT_SMALL)
                .color(TEXT_MUTED),
                seed_chart_canvas,
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

        // v1.12.1 Wave 2 — seed ledger panel. Last 10 harvest events
        // with localized kind labels.
        let seed_ledger_card: Element<'_, Message> = {
            let title = text(match self.settings.language {
                Language::Es => "Cosechas recientes",
                Language::En => "Recent harvests",
            })
            .size(FONT_SMALL)
            .color(TEXT_MUTED);

            let inner: Element<'_, Message> = if seeds_recent.is_empty() {
                text(match self.settings.language {
                    Language::Es => "Aún no has cosechado semillas. Completa una sesión válida (Atención ≥ umbral) para empezar.",
                    Language::En => "No seeds harvested yet. Complete a valid session (Focus ≥ threshold) to begin.",
                })
                .size(FONT_SMALL)
                .color(TEXT_MUTED)
                .into()
            } else {
                let lang = self.settings.language;
                let rows: Vec<Element<'_, Message>> = seeds_recent
                    .iter()
                    .map(|(when, kind, amount)| {
                        let kind_label: String = match (kind.as_str(), lang) {
                            ("session", Language::Es) => "Sesión completa".to_string(),
                            ("session", Language::En) => "Session".to_string(),
                            ("attention_bonus", Language::Es) => "Bonus de atención".to_string(),
                            ("attention_bonus", Language::En) => "Attention bonus".to_string(),
                            ("streak_bonus", Language::Es) => "Racha de 4".to_string(),
                            ("streak_bonus", Language::En) => "Streak of 4".to_string(),
                            ("plugin_bonus", Language::Es) => "Bonus de plugin".to_string(),
                            ("plugin_bonus", Language::En) => "Plugin bonus".to_string(),
                            (other, _) => other.to_string(),
                        };
                        container(
                            iced::widget::row![
                                text(format!("+{amount}")).size(FONT_SMALL).color(ACCENT),
                                iced::widget::Space::with_width(SPACE_SM as f32),
                                badge_local(kind_label, BadgeVariant::Accent),
                                iced::widget::horizontal_space(),
                                text(when.chars().take(16).collect::<String>())
                                    .size(FONT_TINY)
                                    .color(TEXT_MUTED),
                            ]
                            .padding(SPACE_XS as u16)
                            .align_y(iced::alignment::Vertical::Center),
                        )
                        .padding(SPACE_XS as u16)
                        .into()
                    })
                    .collect();
                column(rows).spacing(SPACE_XS as u16).into()
            };

            container(column![title, inner].spacing(SPACE_SM as u16))
                .padding(SPACE_MD as u16)
                .width(Length::Fixed(560.0))
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

        // v1.12.1 Wave 2 — streak ring beside the cards. Shows progress
        // toward the next streak-of-4 bonus (sessions_completed % 4 / 4).
        let streak_ring: Element<'_, Message> = {
            let streak = self.pomodoro_engine.sessions_completed();
            let progress = ((streak % 4) as f32) / 4.0;
            let display = if progress == 0.0 && streak > 0 {
                1.0 // just hit 4 → show full
            } else {
                progress
            };
            let ring = crate::ui::ring::Ring {
                progress: display,
                color: ACCENT,
                track_color: SURFACE_RAISED,
                thickness: 8.0,
            };
            let canvas: Element<'_, Message> = iced::widget::Canvas::new(ring)
                .width(Length::Fixed(86.0))
                .height(Length::Fixed(86.0))
                .into();
            container(
                column![
                    text(match self.settings.language {
                        Language::Es => "RACHA",
                        Language::En => "STREAK",
                    })
                    .size(FONT_SMALL)
                    .color(TEXT_MUTED),
                    canvas,
                    text(format!("{} / 4", streak % 4))
                        .size(FONT_TINY)
                        .color(TEXT_SECONDARY),
                ]
                .spacing(SPACE_XS as u16)
                .align_x(iced::alignment::Horizontal::Center),
            )
            .padding(SPACE_MD as u16)
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

        // v1.12.1 — split the previous flat cards row into two rows so
        // 6 elements fit at the 800 px minimum window. Same approach as
        // the Focus rewards strip: deterministic layout, no flex-wrap.
        // Row 1: HOY · DISTRACCIONES · ESTA SEMANA
        // Row 2: TOTAL · SEMILLAS · streak_ring
        let cards_row1 = iced::widget::row![
            card_today,
            iced::widget::Space::with_width(SPACE_MD as f32),
            card_distractions,
            iced::widget::Space::with_width(SPACE_MD as f32),
            card_week,
        ];
        let cards_row2 = iced::widget::row![
            card_lifetime,
            iced::widget::Space::with_width(SPACE_MD as f32),
            card_seeds,
            iced::widget::Space::with_width(SPACE_MD as f32),
            streak_ring,
        ]
        .align_y(iced::alignment::Vertical::Center);
        let cards_with_ring = column![cards_row1, cards_row2].spacing(SPACE_MD as u16);

        // ===== v1.12.2 Wave 4 — dashboard analytics =====

        // Card 1: Atención promedio (hoy + 7d).
        let attention_card: Element<'_, Message> = {
            let avg_today_str = match avg_attention_today {
                Some(n) => format!("{n}%"),
                None => "—".to_string(),
            };
            let avg_7d_str = match avg_attention_7d {
                Some(n) => format!("{n}%"),
                None => "—".to_string(),
            };
            let valid_pct = if validity_7d.1 > 0 {
                (validity_7d.0 * 100) / validity_7d.1
            } else {
                0
            };
            let valid_label = match self.settings.language {
                Language::Es => format!(
                    "{}/{} válidas ({}%)",
                    validity_7d.0, validity_7d.1, valid_pct
                ),
                Language::En => format!(
                    "{}/{} valid ({}%)",
                    validity_7d.0, validity_7d.1, valid_pct
                ),
            };
            let metric =
                |label: &str, value: String, sub: String| -> Element<'_, Message> {
                    container(
                        column![
                            text(label.to_string())
                                .size(FONT_SMALL)
                                .color(TEXT_MUTED),
                            text(value).size(FONT_TITLE).color(TEXT_PRIMARY),
                            text(sub).size(FONT_TINY).color(TEXT_SECONDARY),
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
            container(
                column![
                    text(match self.settings.language {
                        Language::Es => "Atención y validez",
                        Language::En => "Focus and validity",
                    })
                    .size(FONT_SMALL)
                    .color(TEXT_MUTED),
                    iced::widget::row![
                        metric(
                            match self.settings.language {
                                Language::Es => "ATENCIÓN HOY",
                                Language::En => "FOCUS TODAY",
                            },
                            avg_today_str,
                            match self.settings.language {
                                Language::Es => "promedio de sesiones".to_string(),
                                Language::En => "session average".to_string(),
                            },
                        ),
                        iced::widget::Space::with_width(SPACE_MD as f32),
                        metric(
                            match self.settings.language {
                                Language::Es => "ATENCIÓN 7D",
                                Language::En => "FOCUS 7D",
                            },
                            avg_7d_str,
                            valid_label,
                        ),
                    ],
                ]
                .spacing(SPACE_SM as u16),
            )
            .padding(SPACE_MD as u16)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(BG)),
                border: iced::Border {
                    radius: 8.0.into(),
                    width: 1.0,
                    color: ACCENT_DIM,
                },
                ..Default::default()
            })
            .into()
        };

        // Card 2: Distribución horaria (focus minutes by hour of day, last 30d).
        let hourly_card: Element<'_, Message> = {
            let bars: Vec<(String, u32)> = focus_by_hour.clone();
            let widget = crate::ui::chart::WeeklyChart::new(bars);
            let canvas: Element<'_, Message> = iced::widget::Canvas::new(widget)
                .width(Length::Fixed(720.0))
                .height(Length::Fixed(160.0))
                .into();
            let total_min: u32 = focus_by_hour.iter().map(|(_, n)| *n).sum();
            let best_hour: Option<&(String, u32)> = focus_by_hour
                .iter()
                .filter(|(_, n)| *n > 0)
                .max_by_key(|(_, n)| *n);
            let summary = match (best_hour, self.settings.language) {
                (Some((h, n)), Language::Es) => {
                    format!("Mejor hora: {h}h · {n} min · total 30 días: {total_min} min")
                }
                (Some((h, n)), Language::En) => {
                    format!("Best hour: {h}h · {n} min · 30-day total: {total_min} min")
                }
                (None, Language::Es) => "Sin datos suficientes todavía.".to_string(),
                (None, Language::En) => "Not enough data yet.".to_string(),
            };
            container(
                column![
                    text(match self.settings.language {
                        Language::Es => "¿Cuándo te concentras mejor? · últimos 30 días",
                        Language::En => "When do you focus best? · last 30 days",
                    })
                    .size(FONT_SMALL)
                    .color(TEXT_MUTED),
                    canvas,
                    text(summary).size(FONT_TINY).color(TEXT_SECONDARY),
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
            })
            .into()
        };

        // Card 3: Origen de semillas (kind breakdown).
        let seeds_origin_card: Element<'_, Message> = {
            let total: u32 = seeds_kind_breakdown.iter().map(|(_, n)| *n).sum();
            let inner: Element<'_, Message> = if seeds_kind_breakdown.is_empty() || total == 0 {
                text(match self.settings.language {
                    Language::Es => "Aún no has cosechado semillas.",
                    Language::En => "No seeds harvested yet.",
                })
                .size(FONT_SMALL)
                .color(TEXT_MUTED)
                .into()
            } else {
                let lang = self.settings.language;
                let max_count = seeds_kind_breakdown.iter().map(|(_, n)| *n).max().unwrap_or(1);
                let rows: Vec<Element<'_, Message>> = seeds_kind_breakdown
                    .iter()
                    .map(|(kind, n)| {
                        let label: String = match (kind.as_str(), lang) {
                            ("session", Language::Es) => "Sesión completa".to_string(),
                            ("session", Language::En) => "Session".to_string(),
                            ("attention_bonus", Language::Es) => "Bonus de atención".to_string(),
                            ("attention_bonus", Language::En) => "Attention bonus".to_string(),
                            ("streak_bonus", Language::Es) => "Racha de 4".to_string(),
                            ("streak_bonus", Language::En) => "Streak of 4".to_string(),
                            ("plugin_bonus", Language::Es) => "Bonus de plugin".to_string(),
                            ("plugin_bonus", Language::En) => "Plugin bonus".to_string(),
                            (other, _) => other.to_string(),
                        };
                        let pct = (*n * 100) / total.max(1);
                        let bar_w = ((*n as f32 / max_count as f32) * 360.0).max(2.0);
                        let bar = container(iced::widget::Space::with_width(Length::Fixed(bar_w)))
                            .height(Length::Fixed(6.0))
                            .style(|_| container::Style {
                                background: Some(iced::Background::Color(ACCENT)),
                                border: iced::Border {
                                    radius: 3.0.into(),
                                    ..Default::default()
                                },
                                ..Default::default()
                            });
                        column![
                            iced::widget::row![
                                text(label).size(FONT_SMALL).color(TEXT_PRIMARY),
                                iced::widget::horizontal_space(),
                                text(format!("{n} · {pct}%"))
                                    .size(FONT_SMALL)
                                    .color(TEXT_SECONDARY),
                            ],
                            bar,
                        ]
                        .spacing(2)
                        .into()
                    })
                    .collect();
                column(rows).spacing(SPACE_SM as u16).into()
            };
            container(
                column![
                    text(match self.settings.language {
                        Language::Es => "Origen de tus semillas",
                        Language::En => "Where your seeds come from",
                    })
                    .size(FONT_SMALL)
                    .color(TEXT_MUTED),
                    inner,
                ]
                .spacing(SPACE_SM as u16),
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
        };

        // Card 4: Logros — best streak + longest session + perfect days.
        let achievements_card: Element<'_, Message> = {
            let achievement = |label: &str, value: String, sub: String| -> Element<'_, Message> {
                container(
                    column![
                        text(label.to_string())
                            .size(FONT_SMALL)
                            .color(TEXT_MUTED),
                        text(value).size(FONT_LEAD).color(TEXT_PRIMARY),
                        text(sub).size(FONT_TINY).color(TEXT_SECONDARY),
                    ]
                    .spacing(2),
                )
                .padding(SPACE_MD as u16)
                .width(Length::Fixed(180.0))
                .style(|_| container::Style {
                    background: Some(iced::Background::Color(SURFACE_RAISED)),
                    border: iced::Border {
                        radius: 8.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .into()
            };
            let longest_session_str = match &longest_session {
                Some((start, secs, cat)) => {
                    let date = chrono::DateTime::parse_from_rfc3339(start)
                        .ok()
                        .map(|d| {
                            d.with_timezone(&chrono::Local)
                                .format("%Y-%m-%d")
                                .to_string()
                        })
                        .unwrap_or_else(|| "?".to_string());
                    (format!("{} min", secs / 60), format!("{cat} · {date}"))
                }
                None => ("—".to_string(), "".to_string()),
            };
            container(
                column![
                    text(match self.settings.language {
                        Language::Es => "Logros",
                        Language::En => "Achievements",
                    })
                    .size(FONT_SMALL)
                    .color(TEXT_MUTED),
                    iced::widget::row![
                        achievement(
                            match self.settings.language {
                                Language::Es => "MEJOR RACHA",
                                Language::En => "BEST STREAK",
                            },
                            format!("{longest_streak}"),
                            match self.settings.language {
                                Language::Es => "sesiones consecutivas".to_string(),
                                Language::En => "consecutive sessions".to_string(),
                            },
                        ),
                        iced::widget::Space::with_width(SPACE_MD as f32),
                        achievement(
                            match self.settings.language {
                                Language::Es => "SESIÓN MÁS LARGA",
                                Language::En => "LONGEST SESSION",
                            },
                            longest_session_str.0,
                            longest_session_str.1,
                        ),
                        iced::widget::Space::with_width(SPACE_MD as f32),
                        achievement(
                            match self.settings.language {
                                Language::Es => "DÍAS PERFECTOS",
                                Language::En => "PERFECT DAYS",
                            },
                            format!("{perfect_days}"),
                            match self.settings.language {
                                Language::Es => "este mes · sin distracciones".to_string(),
                                Language::En => "this month · zero distractions".to_string(),
                            },
                        ),
                    ],
                ]
                .spacing(SPACE_SM as u16),
            )
            .padding(SPACE_MD as u16)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(BG)),
                border: iced::Border {
                    radius: 8.0.into(),
                    width: 1.0,
                    color: ACCENT_DIM,
                },
                ..Default::default()
            })
            .into()
        };

        // Card 5: Coach feedback strip (single line, muted).
        let coach_feedback_strip: Element<'_, Message> = {
            let (up, down) = self.feedback_counts_cache;
            let total = up + down;
            if total == 0 {
                iced::widget::Space::with_height(0.0).into()
            } else {
                let pct = (up * 100) / total.max(1);
                let txt = match self.settings.language {
                    Language::Es => format!(
                        "Coach IA · {pct}% útil ({up} útiles / {down} no útiles · {total} calificaciones)"
                    ),
                    Language::En => format!(
                        "AI coach · {pct}% helpful ({up} helpful / {down} not / {total} ratings)"
                    ),
                };
                container(text(txt).size(FONT_SMALL).color(TEXT_MUTED))
                    .padding(SPACE_XS as u16)
                    .into()
            }
        };

        let body = column![
            text(match self.settings.language {
                Language::Es => "Estadísticas",
                Language::En => "Stats",
            })
            .size(FONT_TITLE)
            .color(TEXT_PRIMARY),
            perm_card,
            cards_with_ring,
            attention_card,
            achievements_card,
            chart_card,
            hourly_card,
            seed_chart_card,
            seeds_origin_card,
            seed_ledger_card,
            category_card,
            distractions_card,
            recap_card,
            sessions_title,
            attention_disclaimer,
            sessions_list,
            coach_feedback_strip,
        ]
        .spacing(SPACE_LG as u16)
        .padding(SPACE_XL as u16)
        .max_width(900);

        // v1.12.2 — center the body horizontally so wide windows don't
        // leave dead space to the right of the scroll bar. The
        // max_width(900) cap keeps lines readable on ultrawide displays.
        let centered = container(body)
            .width(Length::Fill)
            .center_x(Length::Fill);

        container(iced::widget::scrollable(centered))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(BG)),
                ..Default::default()
            })
            .into()
    }
}
