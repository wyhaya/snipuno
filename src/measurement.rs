use crate::dimensions::DimensionDisplay;
use crate::editor::{Bounds, GuideOrientation, Point, Stroke, Tool};
use crate::image_processing::sample_image_color;
use crate::theme;
use image::RgbaImage;
use std::sync::Arc;

pub const COLOR: u32 = 0xffffd400;
const COLOR_TOLERANCE: u32 = 12;
const DRAG_THRESHOLD: f32 = 3.0;
const AXIS_SWITCH_RATIO: f32 = 1.1;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MeasurementMode {
    #[default]
    Measure,
    Guide,
}

impl MeasurementMode {
    pub const OPTIONS: [(Self, MeasurementAxis); 4] = [
        (Self::Measure, MeasurementAxis::Y),
        (Self::Measure, MeasurementAxis::X),
        (Self::Guide, MeasurementAxis::Y),
        (Self::Guide, MeasurementAxis::X),
    ];

    pub fn alignment_color(self) -> u32 {
        let color = match self {
            Self::Measure => COLOR,
            Self::Guide => theme::GUIDE_COLOR,
        };
        (theme::ALIGNMENT_GUIDE_ALPHA << 24) | color & 0x00ff_ffff
    }

    pub fn shortcut(self, axis: MeasurementAxis) -> &'static str {
        match (self, axis) {
            (Self::Measure, MeasurementAxis::Y) => "1",
            (Self::Measure, MeasurementAxis::X) => "2",
            (Self::Guide, MeasurementAxis::Y) => "3",
            (Self::Guide, MeasurementAxis::X) => "4",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeasurementAxis {
    X,
    Y,
}

impl MeasurementAxis {
    pub fn guide_orientation(self) -> GuideOrientation {
        match self {
            Self::X => GuideOrientation::Horizontal,
            Self::Y => GuideOrientation::Vertical,
        }
    }

    pub fn perpendicular_guide_orientation(self) -> GuideOrientation {
        match self {
            Self::X => GuideOrientation::Vertical,
            Self::Y => GuideOrientation::Horizontal,
        }
    }

    pub fn alignment_guide(self, position: Point, bounds: Bounds) -> Option<Interval> {
        if !position.x.is_finite()
            || !position.y.is_finite()
            || position.x < 0.0
            || position.y < 0.0
            || position.x > bounds.width
            || position.y > bounds.height
        {
            return None;
        }
        let (start, end) = match self {
            Self::X => (
                Point::new(position.x, 0.0),
                Point::new(position.x, bounds.height),
            ),
            Self::Y => (
                Point::new(0.0, position.y),
                Point::new(bounds.width, position.y),
            ),
        };
        Some(Interval { start, end })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Interval {
    pub start: Point,
    pub end: Point,
}

impl Interval {
    pub fn text(self, axis: MeasurementAxis, dimensions: DimensionDisplay) -> String {
        let (dx, dy) = match axis {
            MeasurementAxis::X => (self.end.x - self.start.x, 0.0),
            MeasurementAxis::Y => (0.0, self.end.y - self.start.y),
        };
        dimensions.distance(f64::from(dx), f64::from(dy))
    }

    fn contains_pixel(self, point: Point, axis: MeasurementAxis) -> bool {
        match axis {
            MeasurementAxis::X => {
                point.y + 0.5 == self.start.y && point.x >= self.start.x && point.x < self.end.x
            }
            MeasurementAxis::Y => {
                point.x + 0.5 == self.start.x && point.y >= self.start.y && point.y < self.end.y
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeasurementDrag {
    pub start: Point,
    pub end: Point,
    axis: Option<MeasurementAxis>,
}

impl MeasurementDrag {
    pub fn new(start: Point) -> Self {
        Self {
            start,
            end: start,
            axis: None,
        }
    }

    pub fn update(&mut self, end: Point, bounds: Bounds, scale: f32) {
        self.end = Point::new(
            end.x.clamp(0.0, bounds.width),
            end.y.clamp(0.0, bounds.height),
        );
        let dx = (self.end.x - self.start.x).abs();
        let dy = (self.end.y - self.start.y).abs();
        let axis = match self.axis {
            Some(MeasurementAxis::X) if dy <= dx * AXIS_SWITCH_RATIO => MeasurementAxis::X,
            Some(MeasurementAxis::Y) if dx <= dy * AXIS_SWITCH_RATIO => MeasurementAxis::Y,
            _ if dx >= dy => MeasurementAxis::X,
            _ => MeasurementAxis::Y,
        };
        self.axis = (scale.is_finite() && dx.max(dy) * scale > DRAG_THRESHOLD).then_some(axis);
    }

    pub fn interval(self) -> Option<(MeasurementAxis, Interval)> {
        let axis = self.axis?;
        let end = match axis {
            MeasurementAxis::X => Point::new(self.end.x, self.start.y),
            MeasurementAxis::Y => Point::new(self.start.x, self.end.y),
        };
        Some((
            axis,
            Interval {
                start: self.start,
                end,
            },
        ))
    }
}

pub fn measure(
    image: &RgbaImage,
    crop: Bounds,
    position: Point,
    axis: MeasurementAxis,
) -> Option<Interval> {
    if ![
        crop.x,
        crop.y,
        crop.width,
        crop.height,
        position.x,
        position.y,
    ]
    .into_iter()
    .all(f32::is_finite)
        || crop.x < 0.0
        || crop.y < 0.0
        || crop.width < 1.0
        || crop.height < 1.0
        || crop.x + crop.width > image.width() as f32
        || crop.y + crop.height > image.height() as f32
        || position.x < 0.0
        || position.y < 0.0
        || position.x >= crop.width
        || position.y >= crop.height
    {
        return None;
    }
    let x = position.x.floor();
    let y = position.y.floor();
    let color_at = |coordinate: u32| {
        let point = match axis {
            MeasurementAxis::X => Point::new(crop.x + coordinate as f32, crop.y + y),
            MeasurementAxis::Y => Point::new(crop.x + x, crop.y + coordinate as f32),
        };
        sample_image_color(image, point).unwrap()
    };
    let (seed, limit) = match axis {
        MeasurementAxis::X => (x as u32, crop.width as u32),
        MeasurementAxis::Y => (y as u32, crop.height as u32),
    };
    let seed_color = color_at(seed);
    let matches = |coordinate| {
        let color = color_at(coordinate);
        [0, 8, 16].into_iter().all(|shift| {
            ((color >> shift) & 255u32).abs_diff((seed_color >> shift) & 255) <= COLOR_TOLERANCE
        })
    };
    let mut start = seed;
    let mut end = seed + 1;
    while start > 0 && matches(start - 1) {
        start -= 1;
    }
    while end < limit && matches(end) {
        end += 1;
    }
    let (start, end) = match axis {
        MeasurementAxis::X => (
            Point::new(start as f32, y + 0.5),
            Point::new(end as f32, y + 0.5),
        ),
        MeasurementAxis::Y => (
            Point::new(x + 0.5, start as f32),
            Point::new(x + 0.5, end as f32),
        ),
    };
    Some(Interval { start, end })
}

#[derive(Default)]
pub struct MeasurementCache {
    entry: Option<CachedMeasurement>,
}

struct CachedMeasurement {
    image: Arc<RgbaImage>,
    crop: Bounds,
    position: Point,
    axis: MeasurementAxis,
    interval: Option<Interval>,
    color: Option<u32>,
}

impl MeasurementCache {
    pub fn measure(
        &mut self,
        image: &Arc<RgbaImage>,
        crop: Bounds,
        position: Point,
        axis: MeasurementAxis,
    ) -> Option<Interval> {
        let position = Point::new(position.x.floor(), position.y.floor());
        let color = sample_image_color(image, Point::new(crop.x + position.x, crop.y + position.y));
        if let Some(cached) = &self.entry
            && Arc::ptr_eq(&cached.image, image)
            && cached.crop == crop
            && cached.axis == axis
            && (cached.position == position
                || (cached.color == color
                    && cached
                        .interval
                        .is_some_and(|interval| interval.contains_pixel(position, axis))))
        {
            return cached.interval;
        }
        let interval = measure(image, crop, position, axis);
        self.entry = Some(CachedMeasurement {
            image: image.clone(),
            crop,
            position,
            axis,
            interval,
            color,
        });
        interval
    }
}

impl Stroke {
    pub fn measurement_text(&self, dimensions: DimensionDisplay) -> Option<String> {
        let Tool::Measure(axis) = self.tool else {
            return None;
        };
        Some(
            Interval {
                start: self.start,
                end: self.end,
            }
            .text(axis, dimensions),
        )
    }

    pub fn measurement_segments(&self) -> [(Point, Point); 3] {
        let cap = match self.tool {
            Tool::Measure(MeasurementAxis::X) => Point::new(0.0, 5.0 * self.width),
            _ => Point::new(5.0 * self.width, 0.0),
        };
        let tick = |point: Point| {
            (
                Point::new(point.x - cap.x, point.y - cap.y),
                Point::new(point.x + cap.x, point.y + cap.y),
            )
        };
        [(self.start, self.end), tick(self.start), tick(self.end)]
    }

    pub fn measurement_label_bounds(&self, image_size: Point, text_width: f32) -> Bounds {
        let unit = self.width;
        let width = text_width + theme::DIMENSION_PADDING * 2.0 * unit;
        let height = (theme::DIMENSION_LINE_HEIGHT + theme::DIMENSION_PADDING * 2.0) * unit;
        let gap = 8.0 * unit;
        let (x, y) = match self.tool {
            Tool::Measure(MeasurementAxis::Y) => {
                let x = if self.start.x + gap + width <= image_size.x {
                    self.start.x + gap
                } else {
                    self.start.x - gap - width
                };
                (x, self.start.y.min(self.end.y) + gap)
            }
            _ => {
                let y = if self.start.y - gap - height >= 0.0 {
                    self.start.y - gap - height
                } else {
                    self.start.y + gap
                };
                (self.start.x.min(self.end.x) + gap, y)
            }
        };
        Bounds {
            x,
            y,
            width,
            height,
        }
    }
}

#[cfg(test)]
mod tests;
