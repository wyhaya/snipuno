use crate::dimensions::PixelGrid;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn snapped(self, grid: PixelGrid) -> Self {
        Self::new(
            grid.snap_x(f64::from(self.x)) as f32,
            grid.snap_y(f64::from(self.y)) as f32,
        )
    }

    pub fn snapped_within(self, grid: PixelGrid, bounds: Bounds) -> Self {
        Self::new(
            grid.clamp_x(
                f64::from(self.x),
                f64::from(bounds.x),
                f64::from(bounds.x + bounds.width),
            ) as f32,
            grid.clamp_y(
                f64::from(self.y),
                f64::from(bounds.y),
                f64::from(bounds.y + bounds.height),
            ) as f32,
        )
    }
}

pub fn segment_distance(point: Point, start: Point, end: Point) -> f32 {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let length = dx * dx + dy * dy;
    let t = if length > 0.0 {
        (((point.x - start.x) * dx + (point.y - start.y) * dy) / length).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (point.x - start.x - t * dx).hypot(point.y - start.y - t * dy)
}

pub(super) fn midpoint(a: Point, b: Point) -> Point {
    Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0)
}

pub(super) fn quadratic_hit(
    point: Point,
    start: Point,
    control: Point,
    end: Point,
    tolerance: f32,
) -> bool {
    const MAX_DEPTH: usize = 12;
    let mut segments = [(start, control, end, 0); MAX_DEPTH + 1];
    let mut length = 1;
    while length > 0 {
        length -= 1;
        let (start, control, end, depth) = segments[length];
        if point.x < start.x.min(control.x).min(end.x) - tolerance
            || point.x > start.x.max(control.x).max(end.x) + tolerance
            || point.y < start.y.min(control.y).min(end.y) - tolerance
            || point.y > start.y.max(control.y).max(end.y) + tolerance
        {
            continue;
        }
        if depth >= MAX_DEPTH
            || segment_distance(control, start, end) <= (tolerance * 0.25).max(0.001)
        {
            if segment_distance(point, start, end) <= tolerance {
                return true;
            }
        } else {
            let a = midpoint(start, control);
            let b = midpoint(control, end);
            let middle = midpoint(a, b);
            segments[length] = (start, a, middle, depth + 1);
            segments[length + 1] = (middle, b, end, depth + 1);
            length += 2;
        }
    }
    false
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizeHandle {
    TopLeft,
    TopRight,
    BottomRight,
    BottomLeft,
    Top,
    Right,
    Bottom,
    Left,
    ArrowStart,
    ArrowEnd,
    ArrowMiddle,
}

impl ResizeHandle {
    pub(crate) fn axes(self) -> (i8, i8) {
        match self {
            Self::TopLeft => (-1, -1),
            Self::TopRight => (1, -1),
            Self::BottomRight => (1, 1),
            Self::BottomLeft => (-1, 1),
            Self::Top => (0, -1),
            Self::Right => (1, 0),
            Self::Bottom => (0, 1),
            Self::Left => (-1, 0),
            Self::ArrowStart | Self::ArrowEnd | Self::ArrowMiddle => (0, 0),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Bounds {
    pub fn snapped(self, grid: PixelGrid) -> Self {
        Self {
            x: grid.snap_x(f64::from(self.x)) as f32,
            y: grid.snap_y(f64::from(self.y)) as f32,
            width: grid.snap_x(f64::from(self.width)) as f32,
            height: grid.snap_y(f64::from(self.height)) as f32,
        }
    }

    pub fn snapped_within(self, grid: PixelGrid, canvas: Bounds) -> Self {
        let width = grid.clamp_x(f64::from(self.width), 0.0, f64::from(canvas.width)) as f32;
        let height = grid.clamp_y(f64::from(self.height), 0.0, f64::from(canvas.height)) as f32;
        let origin = Point::new(self.x, self.y).snapped_within(
            grid,
            Bounds {
                width: canvas.width - width,
                height: canvas.height - height,
                ..canvas
            },
        );
        Self {
            x: origin.x,
            y: origin.y,
            width,
            height,
        }
    }

    pub fn resize_edge_at(
        self,
        point: Point,
        radius: f32,
        canvas: Point,
        padding: f32,
    ) -> Option<ResizeHandle> {
        let left = self.x;
        let top = self.y;
        let right = left + self.width;
        let bottom = top + self.height;
        [
            (
                ResizeHandle::Top,
                Point::new(left, top),
                Point::new(right, top),
            ),
            (
                ResizeHandle::Right,
                Point::new(right, top),
                Point::new(right, bottom),
            ),
            (
                ResizeHandle::Bottom,
                Point::new(left, bottom),
                Point::new(right, bottom),
            ),
            (
                ResizeHandle::Left,
                Point::new(left, top),
                Point::new(left, bottom),
            ),
        ]
        .into_iter()
        .filter_map(|(handle, original_start, original_end)| {
            let mut start = Point::new(original_start.x.max(0.0), original_start.y.max(0.0));
            let mut end = Point::new(original_end.x.min(canvas.x), original_end.y.min(canvas.y));
            if start.x > end.x || start.y > end.y {
                return None;
            }
            let (x, y) = handle.axes();
            if x == 0 {
                if start.x == original_start.x {
                    start.x -= padding;
                }
                if end.x == original_end.x {
                    end.x += padding;
                }
                start.y += y as f32 * padding;
                end.y += y as f32 * padding;
            } else {
                if start.y == original_start.y {
                    start.y -= padding;
                }
                if end.y == original_end.y {
                    end.y += padding;
                }
                start.x += x as f32 * padding;
                end.x += x as f32 * padding;
            }
            let distance = segment_distance(point, start, end);
            (distance <= radius).then_some((handle, distance))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(handle, _)| handle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curve_hits_follow_the_curve_including_degenerate_segments() {
        let start = Point::new(0.0, 0.0);
        let control = Point::new(50.0, 100.0);
        let end = Point::new(100.0, 0.0);
        for (point, expected) in [
            (start, true),
            (end, true),
            (Point::new(50.0, 50.0), true),
            (Point::new(25.0, 37.5), true),
            (Point::new(50.0, 0.0), false),
            (Point::new(50.0, 52.0), false),
            (Point::new(-2.0, 0.0), false),
        ] {
            assert_eq!(
                quadratic_hit(point, start, control, end, 1.0),
                expected,
                "{point:?}"
            );
        }
        assert!(quadratic_hit(
            Point::new(1.0, 0.0),
            start,
            start,
            start,
            1.0
        ));
        assert!(!quadratic_hit(
            Point::new(1.1, 0.0),
            start,
            start,
            start,
            1.0
        ));
    }
}
