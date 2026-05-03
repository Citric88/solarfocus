//! Vertical sidebar navigation with drawn icons. 64px wide, always visible.
//!
//! Icons are simple geometric shapes drawn via `iced::widget::canvas` so we
//! never have to fight cosmic-text's emoji fallback. Each icon = a 28×28
//! glyph rendered procedurally. Selected route gets a 3px accent bar on the
//! left edge.
//!
//! Single status pill at the bottom shows engine state (idle / focus /
//! break / distraction).

use crate::ui::palette;
use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke};
use iced::widget::{button, column, container, text, Canvas, Space};
use iced::{mouse, Color, Element, Length, Point, Rectangle, Renderer, Size, Theme};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Route {
    Focus,
    Stats,
    Coach,
    Setup,
    Help,
}

impl Default for Route {
    fn default() -> Self {
        Route::Focus
    }
}

#[derive(Debug, Clone, Copy)]
pub enum StatusPill {
    Idle,
    Focusing,
    Break,
    Distraction,
}

impl StatusPill {
    pub fn color(self) -> Color {
        match self {
            StatusPill::Idle => palette::TEXT_MUTED,
            StatusPill::Focusing => palette::ACCENT,
            StatusPill::Break => palette::ON_BREAK,
            StatusPill::Distraction => palette::DANGER,
        }
    }
    pub fn label_es(self) -> &'static str {
        match self {
            StatusPill::Idle => "Idle",
            StatusPill::Focusing => "Foco",
            StatusPill::Break => "Pausa",
            StatusPill::Distraction => "Drift",
        }
    }
}

/// Build the sidebar element. Caller provides current route + status +
/// optional download-progress percentage (0..=100) shown as a small badge
/// over the Setup icon.
pub fn view<'a, Msg: Clone + 'a>(
    current: Route,
    status: StatusPill,
    download_pct: Option<u8>,
    on_select: impl Fn(Route) -> Msg + 'a,
) -> Element<'a, Msg> {
    let on_select = std::rc::Rc::new(on_select);

    let make_btn = |route: Route, glyph: IconGlyph, label: &'static str| -> Element<'a, Msg> {
        let selected = route == current;
        let on_select = on_select.clone();
        // ENH-4: small badge over Setup icon when a download is active.
        let badge: Element<'a, Msg> = if route == Route::Setup && download_pct.is_some() {
            let p = download_pct.unwrap();
            text(format!("{p}%"))
                .size(palette::FONT_TINY)
                .color(palette::ACCENT)
                .into()
        } else {
            iced::widget::Space::with_height(Length::Fixed(0.0)).into()
        };
        button(
            column![
                Canvas::new(IconCanvas { glyph, selected })
                    .width(Length::Fixed(28.0))
                    .height(Length::Fixed(28.0)),
                text(label)
                    .size(palette::FONT_TINY)
                    .color(if selected {
                        palette::ACCENT
                    } else {
                        palette::TEXT_SECONDARY
                    }),
                badge,
            ]
            .spacing(2)
            .align_x(iced::alignment::Horizontal::Center),
        )
        .on_press_with(move || (on_select)(route))
        .padding([10, 4])
        .width(Length::Fill)
        .style(move |_, _| button::Style {
            background: Some(iced::Background::Color(if selected {
                palette::SURFACE
            } else {
                Color::TRANSPARENT
            })),
            text_color: palette::TEXT_PRIMARY,
            border: iced::Border {
                radius: 6.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
    };

    let pill: Element<'a, Msg> = container(
        column![
            Canvas::new(StatusCanvas { color: status.color() })
                .width(Length::Fixed(20.0))
                .height(Length::Fixed(20.0)),
            text(status.label_es())
                .size(palette::FONT_TINY)
                .color(palette::TEXT_SECONDARY),
        ]
        .spacing(4)
        .align_x(iced::alignment::Horizontal::Center),
    )
    .padding(8)
    .into();

    let top_pad: Element<'a, Msg> =
        Space::with_height(Length::Fixed(palette::SPACE_MD as f32)).into();
    let logo: Element<'a, Msg> = text("SF")
        .size(palette::FONT_LEAD)
        .color(palette::ACCENT)
        .into();
    let nav_gap: Element<'a, Msg> =
        Space::with_height(Length::Fixed(palette::SPACE_LG as f32)).into();
    let flex_gap: Element<'a, Msg> = Space::with_height(Length::Fill).into();
    let bot_pad: Element<'a, Msg> =
        Space::with_height(Length::Fixed(palette::SPACE_SM as f32)).into();

    container(
        column![
            top_pad,
            logo,
            nav_gap,
            make_btn(Route::Focus, IconGlyph::Focus, "Focus"),
            make_btn(Route::Stats, IconGlyph::Stats, "Stats"),
            make_btn(Route::Coach, IconGlyph::Coach, "Coach"),
            make_btn(Route::Setup, IconGlyph::Setup, "Setup"),
            make_btn(Route::Help, IconGlyph::Help, "Help"),
            flex_gap,
            pill,
            bot_pad,
        ]
        .spacing(4)
        .align_x(iced::alignment::Horizontal::Center),
    )
    .width(Length::Fixed(64.0))
    .height(Length::Fill)
    .style(|_| container::Style {
        background: Some(iced::Background::Color(palette::SIDEBAR_BG)),
        ..Default::default()
    })
    .into()
}

// --- Drawn icons via canvas ---

#[derive(Debug, Clone, Copy)]
pub enum IconGlyph {
    Focus,  // ring
    Stats,  // 3 ascending bars
    Coach,  // chat bubble
    Setup,  // gear (concentric circle + spokes)
    Help,   // circle with "?" inside
}

struct IconCanvas {
    glyph: IconGlyph,
    selected: bool,
}

impl<Msg> canvas::Program<Msg> for IconCanvas {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, Size::new(bounds.width, bounds.height));
        let color = if self.selected {
            palette::ACCENT
        } else {
            palette::TEXT_SECONDARY
        };
        let cx = bounds.width / 2.0;
        let cy = bounds.height / 2.0;

        match self.glyph {
            IconGlyph::Focus => {
                let outer = Path::circle(Point::new(cx, cy), 11.0);
                frame.stroke(
                    &outer,
                    Stroke::default().with_color(color).with_width(2.0),
                );
                let inner = Path::circle(Point::new(cx, cy), 4.0);
                frame.fill(&inner, color);
            }
            IconGlyph::Stats => {
                // Three vertical bars of increasing height.
                let bar_w = 4.0;
                let gap = 3.0;
                let max_h = 18.0;
                let base_y = cy + max_h / 2.0;
                let total_w = bar_w * 3.0 + gap * 2.0;
                let start_x = cx - total_w / 2.0;
                for (i, h) in [8.0_f32, 13.0, 18.0].iter().enumerate() {
                    let x = start_x + (i as f32) * (bar_w + gap);
                    let rect = Path::rectangle(Point::new(x, base_y - h), Size::new(bar_w, *h));
                    frame.fill(&rect, color);
                }
            }
            IconGlyph::Coach => {
                // Rounded chat bubble: rect with bottom-left tail.
                let bubble = Path::rounded_rectangle(
                    Point::new(cx - 11.0, cy - 8.0),
                    Size::new(22.0, 14.0),
                    3.0.into(),
                );
                frame.stroke(
                    &bubble,
                    Stroke::default().with_color(color).with_width(2.0),
                );
                let tail = Path::new(|p| {
                    p.move_to(Point::new(cx - 6.0, cy + 6.0));
                    p.line_to(Point::new(cx - 9.0, cy + 11.0));
                    p.line_to(Point::new(cx - 2.0, cy + 6.0));
                });
                frame.fill(&tail, color);
            }
            IconGlyph::Setup => {
                // Gear: outer ring + 4 spokes + inner dot.
                let outer = Path::circle(Point::new(cx, cy), 9.0);
                frame.stroke(
                    &outer,
                    Stroke::default().with_color(color).with_width(2.0),
                );
                for ang in [0.0_f32, 90.0, 180.0, 270.0] {
                    let r = ang.to_radians();
                    let (sx, sy) = (cx + 9.0 * r.cos(), cy + 9.0 * r.sin());
                    let (ex, ey) = (cx + 13.0 * r.cos(), cy + 13.0 * r.sin());
                    let spoke = Path::line(Point::new(sx, sy), Point::new(ex, ey));
                    frame.stroke(
                        &spoke,
                        Stroke::default().with_color(color).with_width(2.0),
                    );
                }
                let center_dot = Path::circle(Point::new(cx, cy), 2.5);
                frame.fill(&center_dot, color);
            }
            IconGlyph::Help => {
                // Circle outline + drawn "?" hook + dot.
                let outer = Path::circle(Point::new(cx, cy), 11.0);
                frame.stroke(
                    &outer,
                    Stroke::default().with_color(color).with_width(2.0),
                );
                // Hook of the question mark: small arc top, leg down.
                let hook = Path::new(|p| {
                    p.move_to(Point::new(cx - 4.0, cy - 4.0));
                    p.quadratic_curve_to(Point::new(cx, cy - 8.0), Point::new(cx + 4.0, cy - 4.0));
                    p.quadratic_curve_to(Point::new(cx + 4.0, cy), Point::new(cx, cy + 1.0));
                    p.line_to(Point::new(cx, cy + 4.0));
                });
                frame.stroke(
                    &hook,
                    Stroke::default().with_color(color).with_width(2.0),
                );
                let dot = Path::circle(Point::new(cx, cy + 7.0), 1.2);
                frame.fill(&dot, color);
            }
        }
        vec![frame.into_geometry()]
    }
}

struct StatusCanvas {
    color: Color,
}

impl<Msg> canvas::Program<Msg> for StatusCanvas {
    type State = ();
    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, Size::new(bounds.width, bounds.height));
        let cx = bounds.width / 2.0;
        let cy = bounds.height / 2.0;
        let dot = Path::circle(Point::new(cx, cy), bounds.width.min(bounds.height) / 2.0 - 2.0);
        frame.fill(&dot, self.color);
        vec![frame.into_geometry()]
    }
}

