//! v1.6.0 — Reusable UI component primitives extracted from main.rs.
//!
//! Each helper takes a `crate::Message` and returns an `Element` so it
//! plugs straight into any `view_*` function. Visual tokens come from
//! `ui::palette`; no inline color literals.

use crate::Message;
use iced::widget::{button, column, container, text};
use iced::{Color, Element};

use super::palette::*;

/// Reusable card wrapper for Setup tabs (and anywhere a section needs
/// a labelled SURFACE-coloured frame). Shows a muted label header
/// above the body content with a 1 px ACCENT_DIM border separating
/// the card from the canvas BG so chips inside don't bleed visually.
pub fn settings_card_local<'a>(label: &'a str, body: Element<'a, Message>) -> Element<'a, Message> {
    container(
        column![
            text(label.to_string())
                .size(FONT_SMALL)
                .color(TEXT_MUTED),
            body,
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
    .into()
}

/// Pill-style chip used for toggle buttons across Setup tabs.
/// Selected highlights in ACCENT; unselected uses SURFACE_RAISED with
/// a 1.5 px ACCENT_DIM border so the silhouette is visible against
/// the card's SURFACE background.
pub fn chip_local<'a>(label: String, selected: bool, msg: Message) -> Element<'a, Message> {
    iced::widget::button(
        text(label)
            .size(FONT_SMALL)
            .color(if selected { BG } else { TEXT_PRIMARY }),
    )
    .on_press(msg)
    .padding([6, 14])
    .style(move |_, status| {
        let hovered = matches!(status, iced::widget::button::Status::Hovered);
        iced::widget::button::Style {
            background: Some(iced::Background::Color(if selected {
                ACCENT
            } else if hovered {
                Color {
                    r: SURFACE_RAISED.r + 0.04,
                    g: SURFACE_RAISED.g + 0.04,
                    b: SURFACE_RAISED.b + 0.04,
                    a: 1.0,
                }
            } else {
                SURFACE_RAISED
            })),
            text_color: if selected { BG } else { TEXT_PRIMARY },
            border: iced::Border {
                radius: 6.0.into(),
                width: if selected { 0.0 } else { 1.5 },
                color: if selected { ACCENT } else { ACCENT_DIM },
            },
            ..Default::default()
        }
    })
    .into()
}

/// Pure visual label, not interactive. Replaces the inline-styled
/// "small bordered text container" patterns scattered across Stats /
/// Coach / Setup. Pick a variant for the semantic colour; the surface
/// is always SURFACE_RAISED with a 1 px border in the variant colour.
#[derive(Clone, Copy)]
#[allow(dead_code)] // Some variants used only by future call sites.
pub enum BadgeVariant {
    Accent,
    Muted,
    Warning,
    Danger,
}

pub fn badge_local<'a>(label: String, variant: BadgeVariant) -> Element<'a, Message> {
    let color = match variant {
        BadgeVariant::Accent => ACCENT,
        BadgeVariant::Muted => TEXT_MUTED,
        BadgeVariant::Warning => WARNING,
        BadgeVariant::Danger => DANGER,
    };
    container(text(label).size(FONT_TINY).color(color))
        .padding([2, 8])
        .style(move |_| container::Style {
            background: Some(iced::Background::Color(SURFACE_RAISED)),
            border: iced::Border {
                radius: 4.0.into(),
                width: 1.0,
                color,
            },
            ..Default::default()
        })
        .into()
}

/// Destructive action button. DANGER border + DANGER text on
/// transparent background; hover fills with low-alpha DANGER. Used in
/// Privacy "Borrar todos los datos", Coach "Limpiar historial",
/// Setup AI "Eliminar modelo" — anywhere a click is irreversible.
pub fn destructive_button<'a>(label: String, msg: Message) -> Element<'a, Message> {
    iced::widget::button(text(label).size(FONT_SMALL).color(DANGER))
        .on_press(msg)
        .padding([6, 14])
        .style(move |_, status| {
            let hovered = matches!(status, iced::widget::button::Status::Hovered);
            iced::widget::button::Style {
                background: Some(iced::Background::Color(if hovered {
                    Color { r: DANGER.r, g: DANGER.g, b: DANGER.b, a: 0.12 }
                } else {
                    Color::TRANSPARENT
                })),
                text_color: DANGER,
                border: iced::Border {
                    radius: 6.0.into(),
                    width: 1.5,
                    color: DANGER,
                },
                ..Default::default()
            }
        })
        .into()
}

/// Subdued text-only button: transparent background, hover lifts to
/// SURFACE_RAISED with TEXT_PRIMARY. Used for inline secondary actions
/// (Cancel, Regenerate, Re-check, etc).
pub fn ghost_button(label: &str, msg: Message) -> Element<'_, Message> {
    iced::widget::button(text(label.to_string()).size(FONT_SMALL).color(TEXT_SECONDARY))
        .on_press(msg)
        .padding([4, 10])
        .style(|_, status| match status {
            iced::widget::button::Status::Hovered => iced::widget::button::Style {
                background: Some(iced::Background::Color(SURFACE_RAISED)),
                text_color: TEXT_PRIMARY,
                border: iced::Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            },
            _ => iced::widget::button::Style {
                background: Some(iced::Background::Color(iced::Color::TRANSPARENT)),
                text_color: TEXT_SECONDARY,
                border: iced::Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            },
        })
        .into()
}

#[allow(dead_code)]
pub fn primary_button(label: &str, msg: Message) -> Element<'_, Message> {
    button(text(label.to_string()).size(18))
        .on_press(msg)
        .padding([10, 24])
        .style(|_, _| button::Style {
            background: Some(iced::Background::Color(Color::from_rgb(0.2, 0.6, 0.3))),
            text_color: Color::WHITE,
            border: iced::Border {
                radius: 6.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}

#[allow(dead_code)]
pub fn secondary_button(label: &str, msg: Message) -> Element<'_, Message> {
    button(text(label.to_string()).size(18))
        .on_press(msg)
        .padding([10, 24])
        .style(|_, _| button::Style {
            background: Some(iced::Background::Color(Color::from_rgb(0.30, 0.50, 0.40))),
            text_color: Color::WHITE,
            border: iced::Border {
                radius: 6.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}
