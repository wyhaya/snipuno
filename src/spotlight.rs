use crate::editor::{Bounds, Point, Stroke, Tool};
use crate::rounded;

pub const DIM_OPACITY: f32 = 0.4;

pub fn opacity(strokes: &[Stroke]) -> Option<f32> {
    strokes
        .iter()
        .any(|stroke| {
            stroke.tool == Tool::Spotlight
                && stroke.bounds().width > 0.0
                && stroke.bounds().height > 0.0
        })
        .then_some(DIM_OPACITY)
}

pub fn mask(canvas: Bounds, regions: &[Bounds], radius: f32) -> Vec<Vec<Point>> {
    let mut pieces = vec![vec![
        Point::new(canvas.x, canvas.y),
        Point::new(canvas.x + canvas.width, canvas.y),
        Point::new(canvas.x + canvas.width, canvas.y + canvas.height),
        Point::new(canvas.x, canvas.y + canvas.height),
    ]];
    for region in regions {
        if region.width <= 0.0 || region.height <= 0.0 {
            continue;
        }
        let hole = rounded::polygon(*region, radius);
        let mut remaining = Vec::new();
        for piece in pieces {
            if disjoint(&piece, *region) {
                remaining.push(piece);
                continue;
            }
            let mut inside = piece;
            for i in 0..hole.len() {
                let (next_inside, outside) = split(&inside, hole[i], hole[(i + 1) % hole.len()]);
                if outside.len() >= 3 {
                    remaining.push(outside);
                }
                inside = next_inside;
                if inside.len() < 3 {
                    break;
                }
            }
        }
        pieces = remaining;
    }
    pieces
}

fn disjoint(polygon: &[Point], rect: Bounds) -> bool {
    polygon.iter().all(|p| p.x <= rect.x)
        || polygon.iter().all(|p| p.x >= rect.x + rect.width)
        || polygon.iter().all(|p| p.y <= rect.y)
        || polygon.iter().all(|p| p.y >= rect.y + rect.height)
}

fn split(polygon: &[Point], a: Point, b: Point) -> (Vec<Point>, Vec<Point>) {
    let mut inside = Vec::new();
    let mut outside = Vec::new();
    let Some(mut previous) = polygon.last().copied() else {
        return (inside, outside);
    };
    let side = |p: Point| (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
    let mut previous_side = side(previous);
    for &current in polygon {
        let current_side = side(current);
        if (previous_side >= 0.0) != (current_side >= 0.0) {
            let t = previous_side / (previous_side - current_side);
            let intersection = Point::new(
                previous.x + (current.x - previous.x) * t,
                previous.y + (current.y - previous.y) * t,
            );
            inside.push(intersection);
            outside.push(intersection);
        }
        if current_side >= 0.0 {
            inside.push(current);
        } else {
            outside.push(current);
        }
        previous = current;
        previous_side = current_side;
    }
    (inside, outside)
}
