use super::{Editor, Point, Stroke, Tool};
use std::f32::consts::TAU;

const AXIS_SNAP_DISTANCE: f32 = 6.0;
const AXIS_SNAP_SLOPE: f32 = 0.1;

pub(super) fn snap_line_endpoint(start: Point, end: Point, display_scale: f32) -> Point {
    let dx = (end.x - start.x).abs();
    let dy = (end.y - start.y).abs();
    if dy * display_scale <= AXIS_SNAP_DISTANCE && dy <= dx * AXIS_SNAP_SLOPE {
        Point::new(end.x, start.y)
    } else if dx * display_scale <= AXIS_SNAP_DISTANCE && dx <= dy * AXIS_SNAP_SLOPE {
        Point::new(start.x, end.y)
    } else {
        end
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LineStyle {
    #[default]
    Solid,
    Dashed,
    Wavy,
}

impl Stroke {
    pub(super) fn wave_amplitude(&self) -> f32 {
        self.width * 1.5
    }

    pub fn line_segments(&self) -> impl Iterator<Item = (Point, Point)> + '_ {
        let delta = Point::new(self.end.x - self.start.x, self.end.y - self.start.y);
        let length = delta.x.hypot(delta.y);
        let unit = if length > f32::EPSILON {
            Point::new(delta.x / length, delta.y / length)
        } else {
            Point::default()
        };
        let dash = self.width * 3.0;
        let period = dash * 1.75;
        let cycles = (length / (self.wave_amplitude() * 6.0)).round().max(1.0);
        let count = if length <= f32::EPSILON {
            0
        } else {
            match self.tool {
                Tool::Line(LineStyle::Solid) => 1,
                Tool::Line(LineStyle::Dashed) => (length / period).ceil() as usize,
                Tool::Line(LineStyle::Wavy) => (cycles as usize) * 24,
                _ => 0,
            }
        };
        let position = move |distance: f32, offset: f32| {
            Point::new(
                self.start.x + unit.x * distance - unit.y * offset,
                self.start.y + unit.y * distance + unit.x * offset,
            )
        };
        (0..count).map(move |index| match self.tool {
            Tool::Line(LineStyle::Dashed) => {
                let dash = length / (count as f32 + (count - 1) as f32 * 0.75);
                let start = index as f32 * dash * 1.75;
                let end = if index + 1 == count {
                    self.end
                } else {
                    position(start + dash, 0.0)
                };
                (position(start, 0.0), end)
            }
            Tool::Line(LineStyle::Wavy) => {
                let sample = |index: usize| {
                    if index == 0 {
                        return self.start;
                    }
                    if index == count {
                        return self.end;
                    }
                    let t = index as f32 / count as f32;
                    position(length * t, (t * cycles * TAU).sin() * self.wave_amplitude())
                };
                (sample(index), sample(index + 1))
            }
            _ => (self.start, self.end),
        })
    }
}

impl Editor {
    pub fn set_selected_line_style(&mut self, style: LineStyle) -> bool {
        self.change_selected(|stroke| {
            if matches!(stroke.tool, Tool::Line(_)) {
                stroke.tool = Tool::Line(style);
            }
        })
    }
}
