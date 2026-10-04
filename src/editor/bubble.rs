use super::{Bounds, Point, segment_distance};
use ui::Icon;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TailDirection {
    TopLeft,
    TopRight,
    #[default]
    BottomLeft,
    BottomRight,
}

impl TailDirection {
    pub const ALL: [Self; 4] = [
        Self::TopLeft,
        Self::TopRight,
        Self::BottomRight,
        Self::BottomLeft,
    ];

    pub fn cycle(self, reverse: bool) -> Self {
        match (self, reverse) {
            (Self::TopLeft, false) | (Self::BottomRight, true) => Self::TopRight,
            (Self::TopRight, false) | (Self::BottomLeft, true) => Self::BottomRight,
            (Self::BottomRight, false) | (Self::TopLeft, true) => Self::BottomLeft,
            (Self::BottomLeft, false) | (Self::TopRight, true) => Self::TopLeft,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::TopLeft => "Tail top left",
            Self::TopRight => "Tail top right",
            Self::BottomLeft => "Tail bottom left",
            Self::BottomRight => "Tail bottom right",
        }
    }

    pub fn number_icon(self) -> Icon {
        match self {
            Self::TopLeft => Icon::NumberTailTopLeft,
            Self::TopRight => Icon::NumberTailTopRight,
            Self::BottomLeft => Icon::NumberTailBottomLeft,
            Self::BottomRight => Icon::NumberTailBottomRight,
        }
    }

    pub fn is_left(self) -> bool {
        matches!(self, Self::TopLeft | Self::BottomLeft)
    }

    pub fn is_top(self) -> bool {
        matches!(self, Self::TopLeft | Self::TopRight)
    }

    pub fn text_icon(self) -> Icon {
        match self {
            Self::TopLeft => Icon::TextTailTopLeft,
            Self::TopRight => Icon::TextTailTopRight,
            Self::BottomLeft => Icon::TextBubble,
            Self::BottomRight => Icon::TextTailBottomRight,
        }
    }

    pub fn number_points(self, bounds: Bounds) -> [Point; 3] {
        [(0.9330127, 0.75), (0.75, 0.9330127), (1.0, 1.0)].map(|(x, y)| {
            let x = if self.is_left() { 1.0 - x } else { x };
            let y = if self.is_top() { 1.0 - y } else { y };
            Point::new(bounds.x + x * bounds.width, bounds.y + y * bounds.height)
        })
    }

    pub fn clockwise(self) -> bool {
        matches!(self, Self::TopRight | Self::BottomLeft)
    }

    pub fn number_hit_test(self, bounds: Bounds, point: Point, tolerance: f32) -> bool {
        let points = self.number_points(bounds);
        let cross =
            |a: Point, b: Point| (b.x - a.x) * (point.y - a.y) - (b.y - a.y) * (point.x - a.x);
        let sides = [
            cross(points[0], points[1]),
            cross(points[1], points[2]),
            cross(points[2], points[0]),
        ];
        sides.iter().all(|side| *side >= 0.0)
            || sides.iter().all(|side| *side <= 0.0)
            || (0..3).any(|i| segment_distance(point, points[i], points[(i + 1) % 3]) <= tolerance)
    }
}
