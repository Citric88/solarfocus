//! v1.6.0 — First-run onboarding wizard. Three pages (Welcome,
//! Profile, Download) with a dot-progress indicator + nav row.
//! Delegates to model_download_panel (still in main.rs) for the
//! download step.

use iced::widget::{column, container, text};
use iced::{Element, Length};

use crate::ui::palette::*;
use crate::{App, Message, WizardStep};
use solar_focus_intelligence::Language;

impl App {
    pub fn view_wizard(&self) -> Element<'_, Message> {
        let progress_dot = |active: bool| {
            text(if active { "●" } else { "○" })
                .size(FONT_LEAD)
                .color(if active { ACCENT } else { TEXT_MUTED })
        };
        let dots = iced::widget::row![
            progress_dot(self.wizard_step >= WizardStep::Welcome),
            iced::widget::Space::with_width(SPACE_SM as f32),
            progress_dot(self.wizard_step >= WizardStep::Profile),
            iced::widget::Space::with_width(SPACE_SM as f32),
            progress_dot(self.wizard_step >= WizardStep::Download),
        ];

        let body: Element<'_, Message> = match self.wizard_step {
            WizardStep::Welcome => self.wizard_welcome(),
            WizardStep::Profile => self.wizard_profile(),
            WizardStep::Download => self.wizard_download(),
            WizardStep::Done => iced::widget::Space::with_height(Length::Fixed(0.0)).into(),
        };

        let nav = iced::widget::row![
            iced::widget::button(text(match self.settings.language {
                Language::Es => "Atrás",
                Language::En => "Back",
            }))
            .on_press(Message::WizardBack)
            .padding([8, 18])
            .style(|_, _| iced::widget::button::Style {
                background: Some(iced::Background::Color(SURFACE_RAISED)),
                text_color: TEXT_PRIMARY,
                border: iced::Border {
                    radius: 6.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
            iced::widget::horizontal_space(),
            iced::widget::button(text(match (self.wizard_step, self.settings.language) {
                (WizardStep::Download, Language::Es) => "Empezar",
                (WizardStep::Download, Language::En) => "Get started",
                (_, Language::Es) => "Siguiente",
                (_, Language::En) => "Next",
            }))
            .on_press(if self.wizard_step == WizardStep::Download {
                Message::WizardFinish
            } else {
                Message::WizardNext
            })
            .padding([8, 22])
            .style(|_, _| iced::widget::button::Style {
                background: Some(iced::Background::Color(ACCENT)),
                text_color: BG,
                border: iced::Border {
                    radius: 6.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
        ];

        container(
            column![
                dots,
                iced::widget::Space::with_height(SPACE_LG as f32),
                body,
                iced::widget::Space::with_height(SPACE_XL as f32),
                nav,
            ]
            .padding(SPACE_XL as u16)
            .spacing(SPACE_MD as u16)
            .max_width(560),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(BG)),
            ..Default::default()
        })
        .into()
    }

    fn wizard_welcome(&self) -> Element<'_, Message> {
        column![
            text("SolarFocus OS").size(FONT_TITLE).color(TEXT_PRIMARY),
            text(match self.settings.language {
                Language::Es =>
                    "Productividad enfocada con IA local. Tu actividad nunca sale del equipo.",
                Language::En =>
                    "Focused productivity with local AI. Your activity never leaves your machine.",
            })
            .size(FONT_BODY)
            .color(TEXT_SECONDARY),
            iced::widget::Space::with_height(SPACE_MD as f32),
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
        .spacing(SPACE_SM as u16)
        .into()
    }

    fn wizard_profile(&self) -> Element<'_, Message> {
        use crate::infra::settings::RamMode;
        column![
            text(match self.settings.language {
                Language::Es => "Elige tu perfil de RAM",
                Language::En => "Pick your RAM profile",
            })
            .size(FONT_TITLE)
            .color(TEXT_PRIMARY),
            text(match self.settings.language {
                Language::Es => "Lo puedes cambiar después en Setup → General.",
                Language::En => "You can change this later in Setup → General.",
            })
            .size(FONT_SMALL)
            .color(TEXT_SECONDARY),
            iced::widget::Space::with_height(SPACE_MD as f32),
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
        .spacing(SPACE_SM as u16)
        .into()
    }

    fn wizard_download(&self) -> Element<'_, Message> {
        use crate::infra::settings::RamMode;

        let title = text(match self.settings.language {
            Language::Es => "Descarga del modelo IA",
            Language::En => "AI model download",
        })
        .size(FONT_TITLE)
        .color(TEXT_PRIMARY);

        if self.settings.ram_mode != RamMode::Full {
            return column![
                title,
                text(match self.settings.language {
                    Language::Es =>
                        "Tu perfil actual no necesita descargar el modelo IA. Listo para empezar.",
                    Language::En =>
                        "Your current profile doesn't need the AI model download. Ready to start.",
                })
                .size(FONT_BODY)
                .color(TEXT_SECONDARY),
            ]
            .spacing(SPACE_MD as u16)
            .into();
        }

        column![title, self.model_download_panel()]
            .spacing(SPACE_MD as u16)
            .into()
    }
}
