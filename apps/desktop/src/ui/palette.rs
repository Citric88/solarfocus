#![allow(dead_code)] // Some tokens are wired in later UI commits.

//! Solarpunk-dark palette + spacing tokens. Every color in the app must
//! come from here — no inline `Color::from_rgb` literals in views.

use iced::Color;

// ------- Surfaces --------
pub const BG: Color = Color::from_rgb(0x0E as f32 / 255.0, 0x16 as f32 / 255.0, 0x12 as f32 / 255.0);
pub const SURFACE: Color =
    Color::from_rgb(0x14 as f32 / 255.0, 0x20 as f32 / 255.0, 0x1B as f32 / 255.0);
pub const SURFACE_RAISED: Color =
    Color::from_rgb(0x1A as f32 / 255.0, 0x2A as f32 / 255.0, 0x23 as f32 / 255.0);
pub const SIDEBAR_BG: Color =
    Color::from_rgb(0x0A as f32 / 255.0, 0x12 as f32 / 255.0, 0x0E as f32 / 255.0);

// ------- Accents --------
pub const ACCENT: Color =
    Color::from_rgb(0x4F as f32 / 255.0, 0xAE as f32 / 255.0, 0x7E as f32 / 255.0);
pub const ACCENT_DIM: Color =
    Color::from_rgb(0x2C as f32 / 255.0, 0x5F as f32 / 255.0, 0x46 as f32 / 255.0);

// ------- Semantic --------
pub const WARNING: Color =
    Color::from_rgb(0xE0 as f32 / 255.0, 0xB4 as f32 / 255.0, 0x3F as f32 / 255.0);
pub const DANGER: Color =
    Color::from_rgb(0xC4 as f32 / 255.0, 0x5E as f32 / 255.0, 0x52 as f32 / 255.0);
pub const ON_BREAK: Color =
    Color::from_rgb(0x6E as f32 / 255.0, 0xA8 as f32 / 255.0, 0xD8 as f32 / 255.0);

// ------- Text --------
pub const TEXT_PRIMARY: Color =
    Color::from_rgb(0xE8 as f32 / 255.0, 0xF0 as f32 / 255.0, 0xEC as f32 / 255.0);
pub const TEXT_SECONDARY: Color =
    Color::from_rgb(0x94 as f32 / 255.0, 0xA8 as f32 / 255.0, 0xA0 as f32 / 255.0);
pub const TEXT_MUTED: Color =
    Color::from_rgb(0x5E as f32 / 255.0, 0x6F as f32 / 255.0, 0x69 as f32 / 255.0);

// ------- Spacing scale --------
pub const SPACE_XS: u16 = 4;
pub const SPACE_SM: u16 = 8;
pub const SPACE_MD: u16 = 16;
pub const SPACE_LG: u16 = 24;
pub const SPACE_XL: u16 = 40;

// ------- Type scale (px) --------
pub const FONT_TINY: u16 = 11;
pub const FONT_SMALL: u16 = 13;
pub const FONT_BODY: u16 = 15;
pub const FONT_LEAD: u16 = 18;
pub const FONT_TITLE: u16 = 28;
pub const FONT_HERO: u16 = 96; // timer
