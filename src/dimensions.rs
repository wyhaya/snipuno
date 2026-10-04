use gpui::{App, Global};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DimensionMode {
    #[default]
    Logical,
    Original,
}

impl Global for DimensionMode {}

impl DimensionMode {
    pub fn current(cx: &App) -> Self {
        cx.try_global::<Self>().copied().unwrap_or_default()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PixelScale {
    x: f64,
    y: f64,
}

impl Default for PixelScale {
    fn default() -> Self {
        Self::new(1.0, 1.0)
    }
}

impl PixelScale {
    pub fn new(x: f64, y: f64) -> Self {
        let valid = |value: f64| {
            if value.is_finite() && value > 0.0 {
                value
            } else {
                1.0
            }
        };
        Self {
            x: valid(x),
            y: valid(y),
        }
    }

    pub fn display(self, mode: DimensionMode) -> DimensionDisplay {
        DimensionDisplay(match mode {
            DimensionMode::Logical => self,
            DimensionMode::Original => Self::default(),
        })
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DimensionDisplay(PixelScale);

impl DimensionDisplay {
    pub fn grid(self) -> PixelGrid {
        PixelGrid::new(self.0.x, self.0.y)
    }

    pub fn width(self, pixels: f64) -> f64 {
        pixels / self.0.x
    }

    pub fn height(self, pixels: f64) -> f64 {
        pixels / self.0.y
    }

    pub fn size(self, width: f64, height: f64) -> String {
        format!(
            "{} × {}",
            number(self.width(width)),
            number(self.height(height))
        )
    }

    pub fn distance(self, dx: f64, dy: f64) -> String {
        number(self.width(dx).hypot(self.height(dy)))
    }
}

pub fn number(value: f64) -> String {
    let value = value.round();
    if value == 0.0 {
        "0".into()
    } else {
        format!("{value:.0}")
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PixelGrid {
    pub x: f64,
    pub y: f64,
}

impl Default for PixelGrid {
    fn default() -> Self {
        Self::new(1.0, 1.0)
    }
}

impl PixelGrid {
    pub fn new(x: f64, y: f64) -> Self {
        let scale = PixelScale::new(x, y);
        Self {
            x: scale.x,
            y: scale.y,
        }
    }

    pub fn snap_x(self, value: f64) -> f64 {
        (value / self.x).round() * self.x
    }

    pub fn snap_y(self, value: f64) -> f64 {
        (value / self.y).round() * self.y
    }

    pub fn clamp_x(self, value: f64, min: f64, max: f64) -> f64 {
        Self::clamp(value, min, max, self.x)
    }

    pub fn clamp_y(self, value: f64, min: f64, max: f64) -> f64 {
        Self::clamp(value, min, max, self.y)
    }

    fn clamp(value: f64, min: f64, max: f64, step: f64) -> f64 {
        let first = (min / step).ceil();
        let last = (max / step).floor();
        let value = if value.is_nan() { 0.0 } else { value };
        if first <= last {
            (value / step).round().clamp(first, last) * step
        } else {
            value.clamp(min, max)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dimensions_convert_each_axis_before_rounding_and_measuring() {
        let source = PixelScale::new(2.0, 1.5);
        let logical = source.display(DimensionMode::Logical);
        assert_eq!(logical.size(200.0, 100.0), "100 × 67");
        assert_eq!(logical.distance(-6.0, 6.0), "5");
        assert_eq!(
            source.display(DimensionMode::Original).size(200.0, 100.0),
            "200 × 100"
        );
        for (value, expected) in [
            (-0.1, "0"),
            (0.49, "0"),
            (0.5, "1"),
            (1.49, "1"),
            (1.5, "2"),
        ] {
            assert_eq!(number(value), expected);
        }
    }

    #[test]
    fn grid_snaps_to_valid_points_and_handles_subpixel_ranges() {
        let grid = PixelGrid::new(1.5, 2.0);
        for (value, expected) in [(0.74, 0.0), (0.75, 1.5), (-0.75, -1.5)] {
            assert_eq!(grid.snap_x(value), expected);
        }
        assert_eq!(grid.snap_y(1.0), 2.0);
        for (value, min, max, expected) in [
            (-100.0, 0.2, 4.0, 1.5),
            (100.0, 0.2, 4.0, 3.0),
            (0.0, 0.2, 0.8, 0.2),
            (2.0, 0.2, 0.8, 0.8),
            (f64::NAN, 0.0, 4.0, 0.0),
            (f64::INFINITY, 0.0, 4.0, 3.0),
        ] {
            assert_eq!(grid.clamp_x(value, min, max), expected);
        }
    }

    #[test]
    fn invalid_density_falls_back_per_axis() {
        for value in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert_eq!(
                PixelScale::new(value, 2.0)
                    .display(DimensionMode::Logical)
                    .size(10.0, 10.0),
                "10 × 5"
            );
            assert_eq!(PixelGrid::new(2.0, value), PixelGrid::new(2.0, 1.0));
        }
    }
}
