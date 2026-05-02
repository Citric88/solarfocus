//! Simple bar chart drawn via canvas. 7 bars, evenly spaced, with day
//! labels (M/T/W/T/F/S/S) along the bottom.

use crate::ui::palette;
use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke, Text};
use iced::{mouse, Color, Point, Rectangle, Renderer, Size, Theme};

pub struct WeeklyChart {
    /// (label, value); label is one char (M/T/W/...).
    pub bars: Vec<(String, u32)>,
    pub bar_color: Color,
}

impl WeeklyChart {
    pub fn new(bars: Vec<(String, u32)>) -> Self {
        Self {
            bars,
            bar_color: palette::ACCENT,
        }
    }
}

impl<Msg> canvas::Program<Msg> for WeeklyChart {
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

        let n = self.bars.len().max(1) as f32;
        let label_h = 20.0;
        let chart_h = bounds.height - label_h;
        let max_val = self.bars.iter().map(|(_, v)| *v).max().unwrap_or(1).max(1) as f32;

        let bar_w = (bounds.width / n) * 0.55;
        let gap = (bounds.width / n) - bar_w;

        // Baseline.
        let baseline = Path::line(
            Point::new(0.0, chart_h),
            Point::new(bounds.width, chart_h),
        );
        frame.stroke(
            &baseline,
            Stroke::default()
                .with_color(palette::TEXT_MUTED)
                .with_width(1.0),
        );

        for (i, (label, val)) in self.bars.iter().enumerate() {
            let h = (*val as f32 / max_val) * (chart_h - 8.0);
            let x = (i as f32) * (bar_w + gap) + gap / 2.0;
            let y = chart_h - h;
            let rect = Path::rounded_rectangle(
                Point::new(x, y),
                Size::new(bar_w, h),
                3.0.into(),
            );
            frame.fill(&rect, self.bar_color);

            let label_text = Text {
                content: label.clone(),
                position: Point::new(x + bar_w / 2.0, chart_h + 4.0),
                color: palette::TEXT_MUTED,
                size: iced::Pixels(12.0),
                font: iced::Font::DEFAULT,
                horizontal_alignment: iced::alignment::Horizontal::Center,
                vertical_alignment: iced::alignment::Vertical::Top,
                ..Default::default()
            };
            frame.fill_text(label_text);
        }
        vec![frame.into_geometry()]
    }
}
