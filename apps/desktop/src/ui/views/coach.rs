//! v1.6.0 — Coach canvas: model badge, last coach message with
//! Útil / No útil thumbs, scrolling feedback history, clear button.

use iced::widget::{column, container, text};
use iced::{Element, Length};

use crate::ui::components::{badge_local, ghost_button, BadgeVariant};
use crate::ui::palette::*;
use crate::{App, Message};
use solar_focus_intelligence::Language;

impl App {
    pub fn view_coach_placeholder(&self) -> Element<'_, Message> {
        use solar_focus_intelligence::prompts::looks_coherent;

        let lang = self.settings.language;
        let last = self.last_coaching.clone().unwrap_or_else(|| match lang {
            Language::Es => "(Aún no hay mensajes del coach)".to_string(),
            Language::En => "(No coach messages yet)".to_string(),
        });
        let model_badge = if cfg!(feature = "llm") && self.coach.is_ready() {
            format!(
                "{} · {:?}",
                match lang {
                    Language::Es => "Modelo activo",
                    Language::En => "Active model",
                },
                self.settings.model_choice
            )
        } else {
            match lang {
                Language::Es => "Coach básico (sin LLM cargado)".to_string(),
                Language::En => "Basic coach (no LLM loaded)".to_string(),
            }
        };

        let title = text(match lang {
            Language::Es => "Coach IA",
            Language::En => "AI Coach",
        })
        .size(FONT_TITLE)
        .color(TEXT_PRIMARY);
        let subtitle = text(match lang {
            Language::Es =>
                "Aquí ves el último mensaje del coach y puedes calificarlo. \
                 Tu feedback ajusta los próximos.",
            Language::En =>
                "Here you see the latest coach message and rate it. \
                 Your feedback shapes the next ones.",
        })
        .size(FONT_BODY)
        .color(TEXT_SECONDARY);

        let helpful_label = match lang {
            Language::Es => "Útil",
            Language::En => "Helpful",
        };
        let not_helpful_label = match lang {
            Language::Es => "No útil",
            Language::En => "Not helpful",
        };
        let live = container(
            column![
                text(model_badge).size(FONT_SMALL).color(TEXT_MUTED),
                text(last.clone()).size(FONT_LEAD).color(TEXT_PRIMARY),
                iced::widget::row![
                    iced::widget::horizontal_space(),
                    ghost_button(helpful_label, Message::ThumbsUp),
                    iced::widget::Space::with_width(SPACE_XS as f32),
                    ghost_button(not_helpful_label, Message::ThumbsDown),
                ],
            ]
            .spacing(SPACE_SM as u16),
        )
        .padding(SPACE_LG as u16)
        .width(Length::Fixed(640.0))
        .style(|_| container::Style {
            background: Some(iced::Background::Color(SURFACE)),
            border: iced::Border {
                radius: 10.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });

        let recent: Vec<(String, i32, String)> = self
            .session_repo
            .as_ref()
            .and_then(|r| r.recent_feedback(20).ok())
            .unwrap_or_default()
            .into_iter()
            .filter(|(_, _, msg)| looks_coherent(msg, lang))
            .collect();
        let has_any_feedback_at_all = self.feedback_counts_cache.0 > 0
            || self.feedback_counts_cache.1 > 0;

        let history_title = text(match lang {
            Language::Es => "Historial de feedback",
            Language::En => "Feedback history",
        })
        .size(FONT_BODY)
        .color(TEXT_SECONDARY);

        let history_items: Vec<Element<'_, Message>> = if recent.is_empty() {
            vec![text(match lang {
                Language::Es => "(Aún no has dejado feedback.)",
                Language::En => "(No feedback yet.)",
            })
            .size(FONT_SMALL)
            .color(TEXT_MUTED)
            .into()]
        } else {
            recent
                .into_iter()
                .map(|(when, rating, msg)| {
                    let (rating_label, rating_variant) = if rating > 0 {
                        (
                            match lang {
                                Language::Es => "Útil",
                                Language::En => "Helpful",
                            },
                            BadgeVariant::Accent,
                        )
                    } else {
                        (
                            match lang {
                                Language::Es => "No útil",
                                Language::En => "Not helpful",
                            },
                            BadgeVariant::Danger,
                        )
                    };
                    container(
                        iced::widget::row![
                            badge_local(rating_label.to_string(), rating_variant),
                            iced::widget::Space::with_width(SPACE_SM as f32),
                            column![
                                text(msg).size(FONT_SMALL).color(TEXT_PRIMARY),
                                text(when).size(FONT_TINY).color(TEXT_MUTED),
                            ]
                            .spacing(2),
                        ]
                        .padding(SPACE_SM as u16)
                        .align_y(iced::alignment::Vertical::Center),
                    )
                    .padding(SPACE_XS as u16)
                    .width(Length::Fixed(640.0))
                    .style(|_| container::Style {
                        background: Some(iced::Background::Color(SURFACE)),
                        border: iced::Border {
                            radius: 6.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                    .into()
                })
                .collect()
        };

        let history_col = column(history_items).spacing(SPACE_XS as u16);

        let clear_button: Element<'_, Message> = if has_any_feedback_at_all {
            iced::widget::row![
                iced::widget::horizontal_space(),
                ghost_button(
                    match lang {
                        Language::Es => "Limpiar historial",
                        Language::En => "Clear history",
                    },
                    Message::ClearFeedbackHistory,
                ),
            ]
            .into()
        } else {
            iced::widget::Space::with_height(Length::Fixed(0.0)).into()
        };

        // v1.9.0 — solarpunk garden. Hero counter + 7-day chart + ledger
        // of last 10 seed events. Pure pull from the seeds table; no
        // expensive recompute.
        let garden = self.view_garden();

        let body = column![
            title,
            subtitle,
            garden,
            live,
            history_title,
            history_col,
            clear_button,
        ]
        .spacing(SPACE_LG as u16)
        .padding(SPACE_XL as u16)
        .max_width(720);

        iced::widget::scrollable(
            container(body)
                .width(Length::Fill)
                .center_x(Length::Fill)
                .style(|_| container::Style {
                    background: Some(iced::Background::Color(BG)),
                    ..Default::default()
                }),
        )
        .height(Length::Fill)
        .into()
    }

    /// v1.9.0 — solarpunk garden. Hero counter + bilingual subtitle +
    /// 7-day mini-chart + ledger.
    fn view_garden(&self) -> Element<'_, Message> {
        use crate::ui::components::{badge_local, BadgeVariant};
        use solar_focus_intelligence::Language;

        let lang = self.settings.language;
        let total = self.seeds_total_cache;

        let today = self
            .session_repo
            .as_ref()
            .and_then(|r| r.seeds_today().ok())
            .unwrap_or(0);
        let last7: Vec<(String, u32)> = self
            .session_repo
            .as_ref()
            .and_then(|r| r.seeds_last_days(7).ok())
            .unwrap_or_default();
        let week_total: u32 = last7.iter().map(|(_, n)| n).sum();

        // v1.12.1 — solid colored circle drawn as a Container instead of
        // a unicode glyph (cosmic-text falls back to a stack of bars for
        // anything outside the bundled font's coverage). 80x80 ACCENT
        // disc with rounded corners = visually a seed without depending
        // on font glyph support.
        let hero_disc: Element<'_, Message> = container(
            iced::widget::Space::with_height(Length::Fixed(0.0)),
        )
        .width(Length::Fixed(80.0))
        .height(Length::Fixed(80.0))
        .style(|_| container::Style {
            background: Some(iced::Background::Color(ACCENT)),
            border: iced::Border {
                radius: 40.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into();

        let header = iced::widget::row![
            hero_disc,
            iced::widget::Space::with_width(SPACE_MD as f32),
            column![
                text(format!("{}", total))
                    .size(FONT_HERO)
                    .color(TEXT_PRIMARY),
                text(match lang {
                    Language::Es => "Semillas cultivadas",
                    Language::En => "Seeds grown",
                })
                .size(FONT_SMALL)
                .color(TEXT_SECONDARY),
            ]
            .spacing(2),
            iced::widget::horizontal_space(),
            column![
                badge_local(
                    match lang {
                        Language::Es => format!("Hoy +{today}"),
                        Language::En => format!("Today +{today}"),
                    },
                    BadgeVariant::Accent,
                ),
                iced::widget::Space::with_height(SPACE_XS as f32),
                badge_local(
                    match lang {
                        Language::Es => format!("Semana +{week_total}"),
                        Language::En => format!("Week +{week_total}"),
                    },
                    BadgeVariant::Muted,
                ),
            ]
            .spacing(2),
        ]
        .align_y(iced::alignment::Vertical::Center);

        // Mini bar chart for last 7 days. Normalised to the day with the
        // most seeds; empty days render at minimum height for the row to
        // stay visually present.
        let max_day: u32 = last7.iter().map(|(_, n)| *n).max().unwrap_or(1).max(1);
        let bars: Vec<Element<'_, Message>> = last7
            .iter()
            .map(|(day, n)| {
                let pct = (*n as f32) / (max_day as f32);
                let height_px = 6.0_f32 + pct * 60.0_f32;
                let bar = container(iced::widget::Space::with_height(Length::Fixed(height_px)))
                    .width(Length::Fixed(28.0))
                    .style(|_| container::Style {
                        background: Some(iced::Background::Color(ACCENT)),
                        border: iced::Border {
                            radius: 4.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    });
                // v1.12.1 — weekday short label (M/T/W/...) so the
                // 7-day garden chart reads consistently with Stats.
                let day_label = chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d")
                    .ok()
                    .map(|d| {
                        use chrono::Datelike;
                        crate::app::helpers::weekday_short(d.weekday())
                    })
                    .unwrap_or_else(|| "?".to_string());
                column![
                    text(format!("{n}")).size(FONT_TINY).color(TEXT_MUTED),
                    bar,
                    text(day_label).size(FONT_TINY).color(TEXT_MUTED),
                ]
                .spacing(4)
                .align_x(iced::alignment::Horizontal::Center)
                .into()
            })
            .collect();
        let chart_row: Element<'_, Message> = if bars.is_empty() {
            text(match lang {
                Language::Es => "Aún no has cosechado semillas. Completa una sesión válida (Atención ≥ 60%) para empezar tu jardín.",
                Language::En => "No seeds harvested yet. Complete a valid session (Focus ≥ 60%) to start your garden.",
            })
            .size(FONT_SMALL)
            .color(TEXT_MUTED)
            .into()
        } else {
            iced::widget::row(bars)
                .spacing(SPACE_SM as u16)
                .align_y(iced::alignment::Vertical::Bottom)
                .into()
        };

        // Ledger: last 10 events.
        let recent = self
            .session_repo
            .as_ref()
            .and_then(|r| r.recent_seeds(10).ok())
            .unwrap_or_default();
        let ledger_items: Vec<Element<'_, Message>> = if recent.is_empty() {
            Vec::new()
        } else {
            recent
                .into_iter()
                .map(|(when, kind, amount)| {
                    let kind_label = match (kind.as_str(), lang) {
                        ("session", Language::Es) => "Sesión completa",
                        ("session", Language::En) => "Session",
                        ("attention_bonus", Language::Es) => "Bonus de atención",
                        ("attention_bonus", Language::En) => "Attention bonus",
                        ("streak_bonus", Language::Es) => "Racha de 4",
                        ("streak_bonus", Language::En) => "Streak of 4",
                        ("plugin_bonus", Language::Es) => "Bonus de plugin",
                        ("plugin_bonus", Language::En) => "Plugin bonus",
                        (_, _) => "+",
                    };
                    container(
                        iced::widget::row![
                            text(format!("+{amount}"))
                                .size(FONT_SMALL)
                                .color(ACCENT),
                            iced::widget::Space::with_width(SPACE_SM as f32),
                            text(kind_label.to_string())
                                .size(FONT_SMALL)
                                .color(TEXT_PRIMARY),
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
                .collect()
        };
        let ledger: Element<'_, Message> = if ledger_items.is_empty() {
            iced::widget::Space::with_height(Length::Fixed(0.0)).into()
        } else {
            column![
                text(match lang {
                    Language::Es => "Cosechas recientes",
                    Language::En => "Recent harvests",
                })
                .size(FONT_SMALL)
                .color(TEXT_MUTED),
                column(ledger_items).spacing(2),
            ]
            .spacing(SPACE_XS as u16)
            .into()
        };

        container(
            column![
                header,
                iced::widget::Space::with_height(SPACE_SM as f32),
                chart_row,
                iced::widget::Space::with_height(SPACE_XS as f32),
                ledger,
            ]
            .spacing(SPACE_SM as u16),
        )
        .padding(SPACE_LG as u16)
        .width(Length::Fixed(640.0))
        .style(|_| container::Style {
            background: Some(iced::Background::Color(SURFACE)),
            border: iced::Border {
                radius: 10.0.into(),
                width: 1.0,
                color: ACCENT_DIM,
            },
            ..Default::default()
        })
        .into()
    }
}
