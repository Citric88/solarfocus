//! v1.6.0 — Setup canvas tab navigation. Dispatches to the four
//! sub-tab views. Wrapped in a scrollable so all cards reach.

use iced::widget::{column, container, text};
use iced::{Element, Length};

use crate::ui::palette::*;
use crate::{App, Message, SetupTab};
use solar_focus_intelligence::Language;

impl App {
    pub fn view_setup_tabs(&self) -> Element<'_, Message> {
        let make_tab = |t: SetupTab, label: &'static str| -> Element<'_, Message> {
            let selected = t == self.setup_tab;
            iced::widget::button(
                text(label.to_string())
                    .size(FONT_BODY)
                    .color(if selected { TEXT_PRIMARY } else { TEXT_SECONDARY }),
            )
            .on_press(Message::SwitchSetupTab(t))
            .padding([8, 16])
            .style(move |_, _| iced::widget::button::Style {
                background: Some(iced::Background::Color(if selected {
                    SURFACE
                } else {
                    iced::Color::TRANSPARENT
                })),
                text_color: TEXT_PRIMARY,
                border: iced::Border {
                    radius: 6.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
        };

        let tab_bar = iced::widget::row![
            make_tab(SetupTab::General, match self.settings.language {
                Language::Es => "General",
                Language::En => "General",
            }),
            iced::widget::Space::with_width(SPACE_XS as f32),
            make_tab(SetupTab::Ai, "IA"),
            iced::widget::Space::with_width(SPACE_XS as f32),
            make_tab(SetupTab::Calibration, match self.settings.language {
                Language::Es => "Calibración",
                Language::En => "Calibration",
            }),
            iced::widget::Space::with_width(SPACE_XS as f32),
            make_tab(SetupTab::Privacy, match self.settings.language {
                Language::Es => "Privacidad",
                Language::En => "Privacy",
            }),
            iced::widget::Space::with_width(SPACE_XS as f32),
            make_tab(SetupTab::Plugins, "Plugins"),
            iced::widget::Space::with_width(SPACE_XS as f32),
            make_tab(SetupTab::About, match self.settings.language {
                Language::Es => "Acerca",
                Language::En => "About",
            }),
        ]
        .spacing(SPACE_XS as u16);

        let panel: Element<'_, Message> = match self.setup_tab {
            SetupTab::General => self.view_setup_general(),
            SetupTab::Ai => self.view_settings(),
            SetupTab::Calibration => self.view_setup_calibration(),
            SetupTab::Privacy => self.view_setup_privacy(),
            SetupTab::Plugins => self.view_setup_plugins(),
            SetupTab::About => self.view_setup_about(),
        };

        let body = column![
            text(match self.settings.language {
                Language::Es => "Ajustes",
                Language::En => "Setup",
            })
            .size(FONT_TITLE)
            .color(TEXT_PRIMARY),
            tab_bar,
            panel,
        ]
        .spacing(SPACE_LG as u16)
        .padding(SPACE_XL as u16)
        .max_width(720);

        // v1.12.2 — center horizontally so wide windows don't leave dead
        // space to the right.
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
