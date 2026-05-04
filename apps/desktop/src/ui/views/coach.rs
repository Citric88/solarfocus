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

        let body = column![
            title,
            subtitle,
            live,
            history_title,
            history_col,
            clear_button,
        ]
        .spacing(SPACE_LG as u16)
        .padding(SPACE_XL as u16)
        .max_width(720);

        container(body)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(BG)),
                ..Default::default()
            })
            .into()
    }
}
