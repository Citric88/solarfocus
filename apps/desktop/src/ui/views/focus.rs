//! v1.6.0 — Focus canvas: hero ring + timer, deadline badge,
//! category picker, CTA, microcopy/toast, end-session link.

use iced::widget::{button, column, container, stack, text};
use iced::{Element, Length};

use crate::ui::components::chip_local;
use crate::ui::palette::*;
use crate::{App, Message, SolarFocusCore};
use solar_focus_intelligence::{ClassificationLabel, Language};

impl App {
    pub fn view_main(&self) -> Element<'_, Message> {
        let progress = self.pomodoro_engine.progress();
        let is_paused = self.pomodoro_engine.is_paused();

        let (ring_color, time_color) = match self.pomodoro_engine.state() {
            SolarFocusCore::AppState::Focusing(_) => (ACCENT, TEXT_PRIMARY),
            SolarFocusCore::AppState::Break => (ON_BREAK, ON_BREAK),
            SolarFocusCore::AppState::Completed => (ACCENT, ACCENT),
            SolarFocusCore::AppState::Idle => (TEXT_MUTED, TEXT_MUTED),
        };

        let timer_text = if matches!(
            self.pomodoro_engine.state(),
            SolarFocusCore::AppState::Idle
        ) {
            (self.pomodoro_engine.config().focus_duration as u32 / 60).to_string()
                + ":00"
        } else {
            self.pomodoro_engine.remaining_time_formatted()
        };

        let ring: Element<'_, Message> =
            iced::widget::Canvas::new(crate::ui::ring::Ring::new(progress, ring_color))
                .width(Length::Fixed(320.0))
                .height(Length::Fixed(320.0))
                .into();
        let time_label: Element<'_, Message> = container(
            column![
                text(timer_text)
                    .size(72)
                    .color(time_color)
                    .font(iced::Font::MONOSPACE),
                text(self.state_label())
                    .size(FONT_SMALL)
                    .color(TEXT_SECONDARY),
            ]
            .spacing(SPACE_XS as u16)
            .align_x(iced::alignment::Horizontal::Center),
        )
        .width(Length::Fixed(320.0))
        .height(Length::Fixed(320.0))
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into();
        let hero: Element<'_, Message> = stack![ring, time_label].into();

        let cta = self.cta_button(is_paused);

        let toast_visible = self
            .toast
            .as_ref()
            .map(|t| !t.text.trim().is_empty())
            .unwrap_or(false);
        let microcopy: Element<'_, Message> = if toast_visible {
            let t = self.toast.as_ref().unwrap();
            let toast_text_color = iced::Color { r: 0.05, g: 0.05, b: 0.05, a: 1.0 };
            // v1.13.1 — show "Falso positivo" inline whenever the last
            // classification was a distraction. One-tap path to add the
            // process to the auto-generated user exceptions plugin
            // without leaving the Focus canvas.
            let last_was_distraction = self
                .last_classification
                .as_ref()
                .map(|c| c.label == ClassificationLabel::Distraction)
                .unwrap_or(false);
            let fp_btn: Element<'_, Message> = if last_was_distraction {
                button(
                    text(match self.settings.language {
                        Language::Es => "Falso positivo",
                        Language::En => "False positive",
                    })
                    .size(FONT_SMALL)
                    .color(toast_text_color),
                )
                .on_press(Message::MarkLastDistractionAsFalsePositive)
                .padding([2, 10])
                .style(|_, _| button::Style {
                    background: Some(iced::Background::Color(iced::Color {
                        r: 0.05, g: 0.05, b: 0.05, a: 0.10,
                    })),
                    text_color: iced::Color { r: 0.05, g: 0.05, b: 0.05, a: 1.0 },
                    border: iced::Border {
                        radius: 3.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .into()
            } else {
                iced::widget::Space::with_width(0.0).into()
            };
            container(
                iced::widget::row![
                    text(t.text.clone())
                        .size(FONT_LEAD)
                        .color(toast_text_color),
                    iced::widget::horizontal_space(),
                    fp_btn,
                    iced::widget::Space::with_width(SPACE_XS as f32),
                    button(text("×").size(FONT_LEAD).color(toast_text_color))
                        .on_press(Message::DismissToast)
                        .padding([2, 10])
                        .style(|_, _| button::Style {
                            background: Some(iced::Background::Color(iced::Color {
                                r: 0.05, g: 0.05, b: 0.05, a: 0.10,
                            })),
                            text_color: iced::Color { r: 0.05, g: 0.05, b: 0.05, a: 1.0 },
                            border: iced::Border {
                                radius: 3.0.into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        }),
                ]
                .padding(SPACE_SM as u16)
                .align_y(iced::alignment::Vertical::Center),
            )
            .padding(SPACE_SM as u16)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(WARNING)),
                border: iced::Border {
                    radius: 6.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .width(Length::Fixed(420.0))
            .into()
        } else if let Some(c) = &self.last_coaching {
            text(c.clone())
                .size(FONT_BODY)
                .color(TEXT_SECONDARY)
                .into()
        } else {
            text(self.idle_microcopy())
                .size(FONT_BODY)
                .color(TEXT_MUTED)
                .into()
        };

        let end_link: Element<'_, Message> = if matches!(
            self.pomodoro_engine.state(),
            SolarFocusCore::AppState::Focusing(_) | SolarFocusCore::AppState::Break
        ) {
            button(
                text(match self.settings.language {
                    Language::Es => "Terminar sesión",
                    Language::En => "End session",
                })
                .size(FONT_SMALL)
                .color(TEXT_MUTED),
            )
            .on_press(Message::EndSession)
            .padding([4, 12])
            .style(|_, status| match status {
                button::Status::Hovered => button::Style {
                    background: Some(iced::Background::Color(SURFACE_RAISED)),
                    text_color: TEXT_PRIMARY,
                    border: iced::Border {
                        radius: 4.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                _ => button::Style {
                    background: Some(iced::Background::Color(iced::Color::TRANSPARENT)),
                    text_color: TEXT_MUTED,
                    border: iced::Border {
                        radius: 4.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            })
            .into()
        } else {
            iced::widget::Space::with_height(Length::Fixed(0.0)).into()
        };

        let category_picker: Element<'_, Message> = if matches!(
            self.pomodoro_engine.state(),
            SolarFocusCore::AppState::Idle | SolarFocusCore::AppState::Completed
        ) {
            self.category_picker()
        } else {
            iced::widget::Space::with_height(Length::Fixed(0.0)).into()
        };

        let deadline_badge: Element<'_, Message> = self.deadline_badge();

        // v1.10.0 — solid badge when Deep mode is on. Always visible so the
        // user knows their break is going to be skipped.
        let deep_badge: Element<'_, Message> = if self.settings.deep_mode_enabled {
            crate::ui::components::badge_local(
                match self.settings.language {
                    solar_focus_intelligence::Language::Es =>
                        "Modo profundo · sin descansos".to_string(),
                    solar_focus_intelligence::Language::En =>
                        "Deep mode · no breaks".to_string(),
                },
                crate::ui::components::BadgeVariant::Accent,
            )
        } else {
            iced::widget::Space::with_height(Length::Fixed(0.0)).into()
        };

        // v1.12.1 — always-visible rewards strip. Surfaces seeds + streak
        // + active category so the new v1.8/v1.9 features have a permanent
        // home in the Focus canvas, not just a 6 s toast at session end.
        let rewards_strip: Element<'_, Message> = self.rewards_strip();

        let content = column![
            hero,
            iced::widget::Space::with_height(Length::Fixed(SPACE_SM as f32)),
            deadline_badge,
            iced::widget::Space::with_height(Length::Fixed(SPACE_SM as f32)),
            deep_badge,
            iced::widget::Space::with_height(Length::Fixed(SPACE_SM as f32)),
            category_picker,
            iced::widget::Space::with_height(Length::Fixed(SPACE_SM as f32)),
            cta,
            iced::widget::Space::with_height(Length::Fixed(SPACE_MD as f32)),
            microcopy,
            iced::widget::Space::with_height(Length::Fixed(SPACE_SM as f32)),
            rewards_strip,
            end_link,
        ]
        .spacing(SPACE_MD as u16)
        .align_x(iced::alignment::Horizontal::Center);

        // v1.12.1 — wrap in scrollable so tall content (deep badge +
        // deadline + category picker + CTA + microcopy + 2-row rewards
        // strip) survives short windows instead of clipping. Same fix
        // already applied to setup_tabs and coach canvases.
        let inner = container(content)
            .width(Length::Fill)
            .center_x(Length::Fill)
            .padding(SPACE_XL as u16);

        container(iced::widget::scrollable(inner))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(BG)),
                ..Default::default()
            })
            .into()
    }

    pub(crate) fn category_picker(&self) -> Element<'_, Message> {
        let lang = self.settings.language;
        let presets: &[(&str, &str)] = &[
            ("Deep work", "Trabajo profundo"),
            ("Coding", "Código"),
            ("Reading", "Lectura"),
            ("Writing", "Escritura"),
            ("Other", "Otro"),
        ];
        let label = match lang {
            Language::Es => "Categoría de la sesión",
            Language::En => "Session category",
        };
        let current = self.settings.last_category.as_str();
        let mut chip_row = iced::widget::Row::new().spacing(SPACE_XS as u16);
        for (en, es) in presets {
            let display = match lang {
                Language::Es => *es,
                Language::En => *en,
            };
            let selected = current.eq_ignore_ascii_case(en) || current == display;
            chip_row = chip_row.push(chip_local(
                display.to_string(),
                selected,
                Message::SetCategory(en.to_string()),
            ));
        }
        let custom_input = iced::widget::text_input(
            match lang {
                Language::Es => "Otra…",
                Language::En => "Other…",
            },
            &self.custom_category_str,
        )
        .on_input(Message::SetCategoryText)
        .width(Length::Fixed(160.0))
        .padding([4, 8])
        .size(FONT_SMALL);

        container(
            column![
                text(label).size(FONT_SMALL).color(TEXT_MUTED),
                chip_row,
                custom_input,
            ]
            .spacing(SPACE_XS as u16)
            .align_x(iced::alignment::Horizontal::Center),
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
    }

    pub(crate) fn deadline_badge(&self) -> Element<'_, Message> {
        #[cfg(feature = "calendar")]
        {
            let now = chrono::Local::now();
            let live_next: Option<(String, chrono::DateTime<chrono::Local>)> =
                crate::infra::calendar::next_event(&self.calendar_events, now)
                    .map(|e| (e.title.clone(), e.start));
            let manual_next: Option<(String, chrono::DateTime<chrono::Local>)> =
                self.settings.next_deadline_at.as_deref().and_then(|s| {
                    chrono::DateTime::parse_from_rfc3339(s)
                        .ok()
                        .map(|d| {
                            (
                                self.settings.next_deadline_label.clone(),
                                d.with_timezone(&chrono::Local),
                            )
                        })
                });
            let candidate = match (live_next, manual_next) {
                (Some(a), Some(b)) => {
                    if a.1 <= b.1 { Some(a) } else { Some(b) }
                }
                (Some(a), None) => Some(a),
                (None, Some(b)) => Some(b),
                (None, None) => None,
            };
            if let Some((label, when)) = candidate {
                let delta = when - now;
                if delta.num_seconds() > 0 {
                    let mins = delta.num_minutes();
                    let h = mins / 60;
                    let m = mins % 60;
                    let pretty = if !label.is_empty() {
                        format!(
                            "{} \"{}\" {} {}h{:02}m",
                            match self.settings.language {
                                Language::Es => "Próximo:",
                                Language::En => "Next:",
                            },
                            label,
                            match self.settings.language {
                                Language::Es => "en",
                                Language::En => "in",
                            },
                            h,
                            m,
                        )
                    } else {
                        format!(
                            "{} {}h{:02}m",
                            match self.settings.language {
                                Language::Es => "Próxima reunión en",
                                Language::En => "Next meeting in",
                            },
                            h,
                            m,
                        )
                    };
                    return container(
                        text(pretty).size(FONT_SMALL).color(TEXT_SECONDARY),
                    )
                    .padding([4, 12])
                    .style(|_| container::Style {
                        background: Some(iced::Background::Color(SURFACE_RAISED)),
                        border: iced::Border {
                            radius: 6.0.into(),
                            width: 1.0,
                            color: ACCENT_DIM,
                        },
                        ..Default::default()
                    })
                    .into();
                }
            }
        }
        iced::widget::Space::with_height(Length::Fixed(0.0)).into()
    }

    pub(crate) fn state_label(&self) -> String {
        match (self.pomodoro_engine.state(), self.settings.language) {
            (SolarFocusCore::AppState::Idle, Language::Es) => "Listo para comenzar".to_string(),
            (SolarFocusCore::AppState::Idle, Language::En) => "Ready to start".to_string(),
            (SolarFocusCore::AppState::Focusing(_), Language::Es) if self.pomodoro_engine.is_paused() => "EN PAUSA".to_string(),
            (SolarFocusCore::AppState::Focusing(_), Language::En) if self.pomodoro_engine.is_paused() => "PAUSED".to_string(),
            (SolarFocusCore::AppState::Focusing(_), Language::Es) => "EN FOCO".to_string(),
            (SolarFocusCore::AppState::Focusing(_), Language::En) => "IN FOCUS".to_string(),
            (SolarFocusCore::AppState::Break, Language::Es) => "DESCANSO".to_string(),
            (SolarFocusCore::AppState::Break, Language::En) => "BREAK".to_string(),
            (SolarFocusCore::AppState::Completed, Language::Es) => "COMPLETADO".to_string(),
            (SolarFocusCore::AppState::Completed, Language::En) => "DONE".to_string(),
        }
    }

    pub(crate) fn idle_microcopy(&self) -> &'static str {
        match (self.pomodoro_engine.state(), self.settings.language) {
            (SolarFocusCore::AppState::Idle, Language::Es) => "Pomodoro de 25 minutos.",
            (SolarFocusCore::AppState::Idle, Language::En) => "25-minute pomodoro.",
            (SolarFocusCore::AppState::Break, Language::Es) => "Tomate un respiro.",
            (SolarFocusCore::AppState::Break, Language::En) => "Take a breath.",
            _ => "",
        }
    }

    pub(crate) fn cta_button(&self, is_paused: bool) -> Element<'_, Message> {
        let lang = self.settings.language;
        let (label, msg, bg) = match (self.pomodoro_engine.state(), is_paused, lang) {
            (SolarFocusCore::AppState::Idle, _, Language::Es) => ("EMPEZAR ENFOQUE", Message::StartFocus, ACCENT),
            (SolarFocusCore::AppState::Idle, _, Language::En) => ("START FOCUS", Message::StartFocus, ACCENT),
            (SolarFocusCore::AppState::Completed, _, Language::Es) => ("EMPEZAR DE NUEVO", Message::StartFocus, ACCENT),
            (SolarFocusCore::AppState::Completed, _, Language::En) => ("START AGAIN", Message::StartFocus, ACCENT),
            (SolarFocusCore::AppState::Focusing(_), true, Language::Es) => ("REANUDAR", Message::Resume, ACCENT),
            (SolarFocusCore::AppState::Focusing(_), true, Language::En) => ("RESUME", Message::Resume, ACCENT),
            (SolarFocusCore::AppState::Focusing(_), false, Language::Es) => ("PAUSAR", Message::Pause, ACCENT_DIM),
            (SolarFocusCore::AppState::Focusing(_), false, Language::En) => ("PAUSE", Message::Pause, ACCENT_DIM),
            (SolarFocusCore::AppState::Break, true, Language::Es) => ("REANUDAR", Message::Resume, ACCENT),
            (SolarFocusCore::AppState::Break, true, Language::En) => ("RESUME", Message::Resume, ACCENT),
            (SolarFocusCore::AppState::Break, false, Language::Es) => ("VOLVER AL FOCO", Message::StartFocus, ACCENT),
            (SolarFocusCore::AppState::Break, false, Language::En) => ("BACK TO FOCUS", Message::StartFocus, ACCENT),
        };
        button(
            text(label)
                .size(FONT_LEAD)
                .color(BG),
        )
        .on_press(msg)
        .padding([14, 32])
        .style(move |_, status| {
            let bg = match status {
                button::Status::Hovered => SURFACE_RAISED,
                _ => bg,
            };
            button::Style {
                background: Some(iced::Background::Color(bg)),
                text_color: TEXT_PRIMARY,
                border: iced::Border {
                    radius: 8.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
        .into()
    }

    /// v1.12.1 — always-visible rewards strip on the Focus canvas.
    /// Two-row stack inside a max_width(640) container so the badges
    /// never overflow even at the 800x600 minimum window. iced 0.13
    /// has no flex-wrap, so the split is hand-crafted.
    fn rewards_strip(&self) -> Element<'_, Message> {
        use crate::ui::components::{badge_local, BadgeVariant};
        use solar_focus_intelligence::Language;
        let lang = self.settings.language;

        let seeds_today = self
            .session_repo
            .as_ref()
            .and_then(|r| r.seeds_today().ok())
            .unwrap_or(0);
        let streak = self.pomodoro_engine.sessions_completed();
        let category = self.settings.last_category.clone();

        let total_label = match lang {
            Language::Es => format!("Semillas: {}", self.seeds_total_cache),
            Language::En => format!("Seeds: {}", self.seeds_total_cache),
        };
        let today_label = match lang {
            Language::Es => format!("Hoy +{seeds_today}"),
            Language::En => format!("Today +{seeds_today}"),
        };
        let sessions_label = match lang {
            Language::Es => format!("Sesiones hoy: {}", self.sessions_today),
            Language::En => format!("Sessions today: {}", self.sessions_today),
        };
        let streak_label = match lang {
            Language::Es => format!("Racha: {streak}"),
            Language::En => format!("Streak: {streak}"),
        };
        let category_label = match lang {
            Language::Es => format!("Categoría: {category}"),
            Language::En => format!("Category: {category}"),
        };

        let row_primary = iced::widget::row![
            badge_local(total_label, BadgeVariant::Accent),
            iced::widget::Space::with_width(SPACE_XS as f32),
            badge_local(today_label, BadgeVariant::Muted),
            iced::widget::Space::with_width(SPACE_XS as f32),
            badge_local(sessions_label, BadgeVariant::Muted),
        ]
        .align_y(iced::alignment::Vertical::Center);

        let row_context = iced::widget::row![
            badge_local(streak_label, BadgeVariant::Muted),
            iced::widget::Space::with_width(SPACE_XS as f32),
            badge_local(category_label, BadgeVariant::Muted),
        ]
        .align_y(iced::alignment::Vertical::Center);

        container(
            column![row_primary, row_context]
                .spacing(SPACE_XS as u16)
                .align_x(iced::alignment::Horizontal::Center),
        )
        .max_width(640)
        .into()
    }
}
