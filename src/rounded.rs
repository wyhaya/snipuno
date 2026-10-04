use crate::editor::{Bounds, Point};

pub fn contains(bounds: Bounds, point: Point, radius: f32) -> bool {
    if point.x < bounds.x
        || point.x > bounds.x + bounds.width
        || point.y < bounds.y
        || point.y > bounds.y + bounds.height
    {
        return false;
    }
    let radius = radius
        .min(bounds.width / 2.0)
        .min(bounds.height / 2.0)
        .max(0.0);
    let x = point
        .x
        .clamp(bounds.x + radius, bounds.x + bounds.width - radius);
    let y = point
        .y
        .clamp(bounds.y + radius, bounds.y + bounds.height - radius);
    (point.x - x).hypot(point.y - y) <= radius + 0.0001
}

pub fn polygon(bounds: Bounds, radius: f32) -> Vec<Point> {
    let r = radius
        .min(bounds.width / 2.0)
        .min(bounds.height / 2.0)
        .max(0.0);
    let mut points = Vec::with_capacity(68);
    for (cx, cy, angle) in [
        (bounds.x + bounds.width - r, bounds.y + r, -90.0_f32),
        (
            bounds.x + bounds.width - r,
            bounds.y + bounds.height - r,
            0.0,
        ),
        (bounds.x + r, bounds.y + bounds.height - r, 90.0),
        (bounds.x + r, bounds.y + r, 180.0),
    ] {
        for step in 0..=16 {
            let angle = (angle + step as f32 * 90.0 / 16.0).to_radians();
            points.push(Point::new(cx + r * angle.cos(), cy + r * angle.sin()));
        }
    }
    points
}

pub fn clip_to_rect(polygon: &[Point], rect: Bounds) -> Vec<Point> {
    let mut points = polygon.to_vec();
    for (horizontal, edge, keep_greater) in [
        (true, rect.x, true),
        (true, rect.x + rect.width, false),
        (false, rect.y, true),
        (false, rect.y + rect.height, false),
    ] {
        let input = std::mem::take(&mut points);
        if input.is_empty() {
            break;
        }
        let coordinate = |p: Point| if horizontal { p.x } else { p.y };
        let inside = |p: Point| {
            if keep_greater {
                coordinate(p) >= edge
            } else {
                coordinate(p) <= edge
            }
        };
        let mut previous = *input.last().unwrap();
        for current in input {
            if inside(previous) != inside(current) {
                let t =
                    (edge - coordinate(previous)) / (coordinate(current) - coordinate(previous));
                points.push(Point::new(
                    previous.x + t * (current.x - previous.x),
                    previous.y + t * (current.y - previous.y),
                ));
            }
            if inside(current) {
                points.push(current);
            }
            previous = current;
        }
    }
    points
}
