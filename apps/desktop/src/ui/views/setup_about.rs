//! v1.6.0 — Setup → Acerca de tab. Static identity card.

use iced::widget::{column, text};
use iced::Element;

use crate::ui::components::settings_card_local;
use crate::ui::palette::*;
use crate::{App, Message};
use solar_focus_intelligence::Language;

impl App {
    pub fn view_setup_about(&self) -> Element<'_, Message> {
        let body: Element<'_, Message> = column![
            text("SolarFocus OS").size(FONT_TITLE).color(TEXT_PRIMARY),
            text("v1.6.0").size(FONT_BODY).color(TEXT_SECONDARY),
            text(match self.settings.language {
                Language::Es =>
                    "Productividad enfocada con IA local. Privacidad por diseño.",
                Language::En =>
                    "Focused productivity with local AI. Privacy by design.",
            })
            .size(FONT_SMALL)
            .color(TEXT_SECONDARY),
            iced::widget::Space::with_height(SPACE_LG as f32),
            text("Apache-2.0 / MIT").size(FONT_TINY).color(TEXT_MUTED),
            text("github.com/Citric88/solarfocus")
                .size(FONT_TINY)
                .color(TEXT_MUTED),
        ]
        .spacing(SPACE_SM as u16)
        .into();
        settings_card_local(
            match self.settings.language {
                Language::Es => "Acerca de",
                Language::En => "About",
            },
            body,
        )
    }
}
