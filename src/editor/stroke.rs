use super::geometry::{midpoint, quadratic_hit, segment_distance};
use super::{
    Bounds, LineStyle, MIN_DISPLAY_DRAG, MeasurementAxis, Point, ResizeHandle, TailDirection,
};
use crate::{
    magnifier::Magnifier,
    theme::{self, SELECTION_BORDER_WIDTH},
};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuideOrientation {
    Horizontal,
    Vertical,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextAnnotation {
    pub content: Arc<str>,
    pub font_size: f32,
    pub wrap_width: f32,
    pub bubble: bool,
    pub tail: TailDirection,
}

impl TextAnnotation {
    pub fn cycle_bubble_style(
        style: Option<TailDirection>,
        reverse: bool,
    ) -> Option<TailDirection> {
        match style {
            None if reverse => Some(TailDirection::BottomLeft),
            None => Some(TailDirection::TopLeft),
            Some(TailDirection::TopLeft) if reverse => None,
            Some(TailDirection::BottomLeft) if !reverse => None,
            Some(tail) => Some(tail.cycle(reverse)),
        }
    }

    pub fn padding(&self) -> Point {
        if self.bubble {
            Point::new(self.font_size * 0.65, self.font_size * 0.35)
        } else {
            Point::default()
        }
    }

    pub fn line_height(&self) -> f32 {
        self.font_size
    }

    pub fn placement_origin(&self, position: Point) -> Point {
        let offset = self.content_offset();
        Point::new(position.x - offset.x, position.y - offset.y)
    }

    pub fn content_width(&self) -> f32 {
        (self.wrap_width - self.padding().x * 2.0 - self.tail_width()).max(self.font_size)
    }

    pub fn content_offset(&self) -> Point {
        let padding = self.padding();
        Point::new(
            padding.x
                + if self.tail.is_left() {
                    self.tail_width()
                } else {
                    0.0
                },
            padding.y,
        )
    }

    pub fn tail_width(&self) -> f32 {
        if self.bubble {
            self.font_size * 0.35
        } else {
            0.0
        }
    }

    pub fn corner_radius(&self) -> f32 {
        if self.bubble {
            self.font_size * 0.85
        } else {
            0.0
        }
    }

    pub fn foreground_color(&self, color: u32) -> u32 {
        if !self.bubble {
            return color;
        }
        0xff000000 | theme::contrasting_text(color)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RedactionStyle {
    #[default]
    Blur,
    Mosaic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Rectangle,
    RectangleFill,
    Ellipse,
    EllipseFill,
    Arrow,
    Line(LineStyle),
    Brush,
    Redact(RedactionStyle),
    Spotlight,
    Magnifier,
    Text,
    Image(u64),
    Number(u32),
    Guide(GuideOrientation),
    Measure(MeasurementAxis),
}

impl Tool {
    pub fn is_shape(self) -> bool {
        matches!(
            self,
            Self::Rectangle | Self::RectangleFill | Self::Ellipse | Self::EllipseFill
        )
    }

    pub fn is_filled_shape(self) -> bool {
        matches!(self, Self::RectangleFill | Self::EllipseFill)
    }

    pub fn has_auto_color(self) -> bool {
        self.is_filled_shape() || self == Self::Brush
    }

    pub fn family(self) -> Self {
        match self {
            Self::RectangleFill => Self::Rectangle,
            Self::EllipseFill => Self::Ellipse,
            Self::Number(_) => Self::Number(0),
            Self::Guide(_) => Self::Measure(MeasurementAxis::Y),
            Self::Measure(_) => Self::Measure(MeasurementAxis::Y),
            Self::Line(_) => Self::Line(LineStyle::Solid),
            Self::Redact(_) => Self::Redact(RedactionStyle::default()),
            _ => self,
        }
    }

    pub fn is_obscuring(self) -> bool {
        matches!(self, Self::Redact(_))
    }

    pub fn has_strength(self) -> bool {
        self.is_obscuring()
    }

    pub fn has_width(self) -> bool {
        matches!(
            self,
            Self::Rectangle | Self::Ellipse | Self::Arrow | Self::Line(_) | Self::Brush
        )
    }

    pub fn has_color(self) -> bool {
        matches!(
            self,
            Self::Rectangle
                | Self::RectangleFill
                | Self::Ellipse
                | Self::EllipseFill
                | Self::Arrow
                | Self::Line(_)
                | Self::Brush
                | Self::Text
                | Self::Number(_)
        )
    }

    pub fn has_style_cycle(self) -> bool {
        matches!(
            self,
            Self::Arrow
                | Self::Line(_)
                | Self::Redact(_)
                | Self::Text
                | Self::Number(_)
                | Self::Measure(_)
                | Self::Guide(_)
        )
    }

    pub fn is_stamp(self) -> bool {
        matches!(self, Self::Number(_))
    }

    pub fn is_freehand(self) -> bool {
        matches!(self, Self::Brush)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum EffectStrength {
    Low,
    Medium,
    #[default]
    High,
    Max,
}

impl EffectStrength {
    pub const ALL: [Self; 4] = [Self::Low, Self::Medium, Self::High, Self::Max];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
            Self::Max => "Max",
        }
    }

    pub fn blur_sigma(self) -> f32 {
        [8.0, 16.0, 28.0, 44.0][self.index()]
    }

    pub fn block_size(self) -> u32 {
        [12, 20, 32, 48][self.index()]
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArrowGeometry {
    pub start: Point,
    pub tip: Point,
    pub control: Point,
    pub wing_a: Point,
    pub wing_b: Point,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stroke {
    pub tool: Tool,
    pub start: Point,
    pub end: Point,
    /// Color packed as 0xAARRGGBB.
    pub color: u32,
    pub brush_auto_color: bool,
    /// Width in original image pixels.
    pub width: f32,
    pub points: Arc<Vec<Point>>,
    pub effect_strength: EffectStrength,
    pub arrow_control: Option<Point>,
    pub arrow_double_headed: bool,
    pub fill_color: Option<u32>,
    pub fill_opacity: f32,
    pub text: Option<TextAnnotation>,
    pub magnifier: Option<Magnifier>,
    pub measurement_label: Option<Bounds>,
    pub number_tail: TailDirection,
}

impl Stroke {
    pub fn is_filled_shape(&self) -> bool {
        self.tool.is_filled_shape()
    }

    pub fn paint_color(&self) -> u32 {
        if self.is_filled_shape() {
            self.fill_color.unwrap_or(self.color)
        } else {
            self.color
        }
    }

    pub fn paint_opacity(&self) -> f32 {
        if self.is_filled_shape() {
            self.fill_color
                .map_or(1.0, |color| (color >> 24) as f32 / 255.0)
                * self.fill_opacity
        } else {
            (self.color >> 24) as f32 / 255.0
        }
    }

    pub fn display_width(&self, display_scale: f32) -> f32 {
        if self.is_filled_shape() {
            0.0
        } else if matches!(self.tool, Tool::Guide(_)) {
            1.0
        } else if self.tool == Tool::Magnifier {
            Magnifier::SELECTION_WIDTH
        } else {
            self.width * display_scale
        }
    }

    pub fn selection_padding(&self, display_scale: f32) -> f32 {
        if !display_scale.is_finite()
            || display_scale <= 0.0
            || matches!(self.tool, Tool::Arrow | Tool::Brush | Tool::Line(_))
        {
            return 0.0;
        }
        let stroke_radius = if matches!(
            self.tool,
            Tool::Rectangle | Tool::Ellipse | Tool::Magnifier | Tool::Guide(_)
        ) {
            self.display_width(display_scale) / display_scale / 2.0
        } else {
            0.0
        };
        stroke_radius + SELECTION_BORDER_WIDTH / display_scale
    }

    pub fn selection_bounds(&self, display_scale: f32) -> Bounds {
        let bounds = self.bounds();
        let padding = self.selection_padding(display_scale);
        Bounds {
            x: bounds.x - padding,
            y: bounds.y - padding,
            width: bounds.width + padding * 2.0,
            height: bounds.height + padding * 2.0,
        }
    }

    pub fn selection_handles(&self, display_scale: f32) -> Vec<(ResizeHandle, Point)> {
        let padding = self.selection_padding(display_scale);
        self.resize_handles()
            .into_iter()
            .map(|(handle, mut position)| {
                let (x, y) = handle.axes();
                position.x += x as f32 * padding;
                position.y += y as f32 * padding;
                (handle, position)
            })
            .collect()
    }

    pub(super) fn translate(&mut self, delta: Point) {
        if delta == Point::default() {
            return;
        }
        let translate = |point: &mut Point| {
            point.x += delta.x;
            point.y += delta.y;
        };
        translate(&mut self.start);
        translate(&mut self.end);
        if let Some(label) = &mut self.measurement_label {
            label.x += delta.x;
            label.y += delta.y;
        }
        if let Some(control) = &mut self.arrow_control {
            translate(control);
        }
        if !self.points.is_empty() {
            for point in Arc::make_mut(&mut self.points) {
                translate(point);
            }
        }
    }

    pub fn bounds(&self) -> Bounds {
        let mut left = self.start.x.min(self.end.x);
        let mut top = self.start.y.min(self.end.y);
        let mut right = self.start.x.max(self.end.x);
        let mut bottom = self.start.y.max(self.end.y);
        if matches!(self.tool, Tool::Measure(_)) {
            for (a, b) in self.measurement_segments() {
                left = left.min(a.x).min(b.x);
                top = top.min(a.y).min(b.y);
                right = right.max(a.x).max(b.x);
                bottom = bottom.max(a.y).max(b.y);
            }
        }
        if self.tool == Tool::Line(LineStyle::Wavy) {
            let amplitude = self.wave_amplitude();
            left -= amplitude;
            top -= amplitude;
            right += amplitude;
            bottom += amplitude;
        }
        if self.tool == Tool::Arrow
            && let Some(control) = self.arrow_control
        {
            for (start, control, end) in [
                (self.start.x, control.x, self.end.x),
                (self.start.y, control.y, self.end.y),
            ] {
                let denominator = start - 2.0 * control + end;
                if denominator.abs() > f32::EPSILON {
                    let t = (start - control) / denominator;
                    if t > 0.0 && t < 1.0 {
                        let point = self.arrow_point(t);
                        left = left.min(point.x);
                        top = top.min(point.y);
                        right = right.max(point.x);
                        bottom = bottom.max(point.y);
                    }
                }
            }
        }
        Bounds {
            x: left,
            y: top,
            width: right - left,
            height: bottom - top,
        }
    }

    pub fn paint_bounds(&self, display_scale: f32) -> Bounds {
        let bounds = self.bounds();
        let mut left = bounds.x;
        let mut top = bounds.y;
        let mut right = bounds.x + bounds.width;
        let mut bottom = bounds.y + bounds.height;
        if self.tool == Tool::Arrow {
            for point in self
                .arrow_heads()
                .flat_map(|arrow| [arrow.wing_a, arrow.wing_b])
            {
                left = left.min(point.x);
                top = top.min(point.y);
                right = right.max(point.x);
                bottom = bottom.max(point.y);
            }
        }
        let padding = match self.tool {
            Tool::Rectangle
            | Tool::Ellipse
            | Tool::Arrow
            | Tool::Line(_)
            | Tool::Guide(_)
            | Tool::Measure(_) => self.display_width(display_scale) / display_scale / 2.0,
            Tool::Magnifier => Magnifier::SHADOW_EXTENT / display_scale,
            _ => 0.0,
        };
        Bounds {
            x: left - padding,
            y: top - padding,
            width: right - left + padding * 2.0,
            height: bottom - top + padding * 2.0,
        }
    }

    pub fn arrow_point(&self, t: f32) -> Point {
        let control = self
            .arrow_control
            .unwrap_or_else(|| midpoint(self.start, self.end));
        let u = 1.0 - t;
        Point::new(
            u * u * self.start.x + 2.0 * u * t * control.x + t * t * self.end.x,
            u * u * self.start.y + 2.0 * u * t * control.y + t * t * self.end.y,
        )
    }

    /// The arrowhead follows the curve tangent and shrinks for short arrows.
    pub fn arrow_geometry(&self) -> ArrowGeometry {
        self.arrow_geometry_at(self.start, self.end)
    }

    pub fn arrow_heads(&self) -> impl Iterator<Item = ArrowGeometry> + '_ {
        std::iter::once(self.arrow_geometry()).chain(
            self.arrow_double_headed
                .then(|| self.arrow_geometry_at(self.end, self.start)),
        )
    }

    fn arrow_geometry_at(&self, start: Point, end: Point) -> ArrowGeometry {
        let control = self.arrow_control.unwrap_or_else(|| midpoint(start, end));
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let length = dx.hypot(dy);
        if length <= f32::EPSILON {
            return ArrowGeometry {
                start,
                tip: end,
                control,
                wing_a: end,
                wing_b: end,
            };
        }
        let head_length = (self.width * 5.0).max(17.0).min(length * 0.45);
        let half_head_width = head_length * 0.6;
        let tangent = Point::new(end.x - control.x, end.y - control.y);
        let tangent_length = tangent.x.hypot(tangent.y);
        let (ux, uy) = if tangent_length > f32::EPSILON {
            (tangent.x / tangent_length, tangent.y / tangent_length)
        } else {
            (dx / length, dy / length)
        };
        let base = Point::new(end.x - ux * head_length, end.y - uy * head_length);
        ArrowGeometry {
            start,
            tip: end,
            control,
            wing_a: Point::new(base.x - uy * half_head_width, base.y + ux * half_head_width),
            wing_b: Point::new(base.x + uy * half_head_width, base.y - ux * half_head_width),
        }
    }

    pub fn resize_handles(&self) -> Vec<(ResizeHandle, Point)> {
        if self.tool.is_freehand() || matches!(self.tool, Tool::Guide(_) | Tool::Measure(_)) {
            return Vec::new();
        }
        if self.tool == Tool::Arrow {
            return vec![
                (ResizeHandle::ArrowStart, self.start),
                (ResizeHandle::ArrowEnd, self.end),
                (ResizeHandle::ArrowMiddle, self.arrow_point(0.5)),
            ];
        }
        if matches!(self.tool, Tool::Line(_)) {
            return vec![
                (ResizeHandle::ArrowStart, self.start),
                (ResizeHandle::ArrowEnd, self.end),
            ];
        }
        let bounds = self.bounds();
        [
            ResizeHandle::TopLeft,
            ResizeHandle::TopRight,
            ResizeHandle::BottomRight,
            ResizeHandle::BottomLeft,
            ResizeHandle::Top,
            ResizeHandle::Right,
            ResizeHandle::Bottom,
            ResizeHandle::Left,
        ]
        .into_iter()
        .map(|handle| {
            let (x, y) = handle.axes();
            (
                handle,
                Point::new(
                    bounds.x + bounds.width * (x as f32 + 1.0) / 2.0,
                    bounds.y + bounds.height * (y as f32 + 1.0) / 2.0,
                ),
            )
        })
        .collect()
    }

    pub(super) fn hit_test(&self, point: Point, tolerance: f32, display_scale: f32) -> bool {
        let bounds = self.bounds();
        let tolerance = tolerance + self.display_width(display_scale) / display_scale / 2.0;
        if matches!(self.tool, Tool::Number(_))
            && self.number_tail.number_hit_test(bounds, point, tolerance)
        {
            return true;
        }
        match self.tool {
            Tool::Measure(_) => self
                .measurement_segments()
                .into_iter()
                .any(|(a, b)| segment_distance(point, a, b) <= tolerance),
            Tool::Brush => {
                if point.x < bounds.x - tolerance
                    || point.x > bounds.x + bounds.width + tolerance
                    || point.y < bounds.y - tolerance
                    || point.y > bounds.y + bounds.height + tolerance
                {
                    return false;
                }
                if let [only] = self.points.as_slice() {
                    segment_distance(point, *only, *only) <= tolerance
                } else {
                    self.points
                        .windows(2)
                        .any(|segment| segment_distance(point, segment[0], segment[1]) <= tolerance)
                }
            }
            Tool::Redact(_)
            | Tool::Rectangle
            | Tool::RectangleFill
            | Tool::Spotlight
            | Tool::Text
            | Tool::Image(_) => {
                point.x >= bounds.x - tolerance
                    && point.x <= bounds.x + bounds.width + tolerance
                    && point.y >= bounds.y - tolerance
                    && point.y <= bounds.y + bounds.height + tolerance
            }
            Tool::Ellipse | Tool::EllipseFill | Tool::Number(_) | Tool::Magnifier => {
                let rx = bounds.width / 2.0 + tolerance;
                let ry = bounds.height / 2.0 + tolerance;
                ((point.x - bounds.x - bounds.width / 2.0) / rx).powi(2)
                    + ((point.y - bounds.y - bounds.height / 2.0) / ry).powi(2)
                    <= 1.0
            }
            Tool::Guide(_) | Tool::Line(LineStyle::Solid | LineStyle::Dashed) => {
                segment_distance(point, self.start, self.end) <= tolerance
            }
            Tool::Line(LineStyle::Wavy) => {
                point.x >= bounds.x - tolerance
                    && point.x <= bounds.x + bounds.width + tolerance
                    && point.y >= bounds.y - tolerance
                    && point.y <= bounds.y + bounds.height + tolerance
                    && self
                        .line_segments()
                        .any(|(start, end)| segment_distance(point, start, end) <= tolerance)
            }
            Tool::Arrow => {
                let arrow = self.arrow_geometry();
                quadratic_hit(point, arrow.start, arrow.control, arrow.tip, tolerance)
                    || self.arrow_heads().any(|head| {
                        segment_distance(point, head.wing_a, head.tip) <= tolerance
                            || segment_distance(point, head.wing_b, head.tip) <= tolerance
                    })
            }
        }
    }

    pub(crate) fn is_large_enough(&self, display_scale: f32) -> bool {
        if !display_scale.is_finite() || display_scale <= 0.0 {
            return false;
        }
        let bounds = self.bounds();
        match self.tool {
            Tool::Rectangle
            | Tool::RectangleFill
            | Tool::Spotlight
            | Tool::Magnifier
            | Tool::Text
            | Tool::Ellipse
            | Tool::EllipseFill
            | Tool::Image(_)
            | Tool::Number(_)
            | Tool::Redact(_) => {
                bounds.width * display_scale >= MIN_DISPLAY_DRAG
                    && bounds.height * display_scale >= MIN_DISPLAY_DRAG
            }
            Tool::Measure(_) => false,
            Tool::Arrow | Tool::Line(_) | Tool::Guide(_) => {
                (self.end.x - self.start.x).hypot(self.end.y - self.start.y) * display_scale
                    >= MIN_DISPLAY_DRAG
            }
            Tool::Brush => {
                (bounds.width - self.width)
                    .max(0.0)
                    .hypot((bounds.height - self.width).max(0.0))
                    * display_scale
                    >= MIN_DISPLAY_DRAG
            }
        }
    }
}
