//! Drawn progress ring used behind the hero timer. A circular arc that
//! grows from 12 o'clock clockwise as `progress` (0.0..=1.0) increases.

use crate::ui::palette;
use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke};
use iced::{mouse, Color, Point, Rectangle, Renderer, Size, Theme};

pub struct Ring {
    pub progress: f32,
    pub color: Color,
    pub track_color: Color,
    pub thickness: f32,
}

impl Ring {
    pub fn new(progress: f32, color: Color) -> Self {
        Self {
            progress: progress.clamp(0.0, 1.0),
            color,
            track_color: palette::SURFACE_RAISED,
            thickness: 8.0,
        }
    }
}

impl<Msg> canvas::Program<Msg> for Ring {
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
        let r = (bounds.width.min(bounds.height) / 2.0) - self.thickness;

        // Track (full circle, dim).
        let track = Path::circle(Point::new(cx, cy), r);
        frame.stroke(
            &track,
            Stroke::default()
                .with_color(self.track_color)
                .with_width(self.thickness),
        );

        if self.progress > 0.0 {
            // Filled arc from -90° (top) clockwise.
            let start = -std::f32::consts::FRAC_PI_2;
            let end = start + self.progress * std::f32::consts::TAU;
            let arc = Path::new(|p| {
                p.arc(canvas::path::Arc {
                    center: Point::new(cx, cy),
                    radius: r,
                    start_angle: iced::Radians(start),
                    end_angle: iced::Radians(end),
                });
            });
            frame.stroke(
                &arc,
                Stroke::default()
                    .with_color(self.color)
                    .with_width(self.thickness)
                    .with_line_cap(canvas::LineCap::Round),
            );
        }

        vec![frame.into_geometry()]
    }
}
