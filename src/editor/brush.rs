use super::{Point, Stroke};
use std::sync::Arc;

impl Stroke {
    pub(super) fn push_path_point(&mut self, point: Point) {
        if self.points.last() == Some(&point) {
            return;
        }
        let radius = self.width / 2.0;
        if self.points.is_empty() {
            self.start = Point::new(point.x - radius, point.y - radius);
            self.end = Point::new(point.x + radius, point.y + radius);
        } else {
            self.start.x = self.start.x.min(point.x - radius);
            self.start.y = self.start.y.min(point.y - radius);
            self.end.x = self.end.x.max(point.x + radius);
            self.end.y = self.end.y.max(point.y + radius);
        }
        Arc::make_mut(&mut self.points).push(point);
    }

    pub(super) fn refresh_path_bounds(&mut self) {
        let Some(first) = self.points.first().copied() else {
            return;
        };
        let (mut minimum, mut maximum) = (first, first);
        for point in self.points.iter().skip(1) {
            minimum.x = minimum.x.min(point.x);
            minimum.y = minimum.y.min(point.y);
            maximum.x = maximum.x.max(point.x);
            maximum.y = maximum.y.max(point.y);
        }
        let radius = self.width / 2.0;
        self.start = Point::new(minimum.x - radius, minimum.y - radius);
        self.end = Point::new(maximum.x + radius, maximum.y + radius);
    }

    pub(super) fn smooth_brush(&mut self, display_scale: f32) {
        if self.points.len() < 3 || !display_scale.is_finite() || display_scale <= 0.0 {
            return;
        }
        let max_shift = (3.0 / display_scale).min(self.width / 2.0);
        let mut previous = self.points[0];
        let mut changed = false;
        for index in 1..self.points.len() - 1 {
            let current = self.points[index];
            let next = self.points[index + 1];
            let incoming = Point::new(current.x - previous.x, current.y - previous.y);
            let outgoing = Point::new(next.x - current.x, next.y - current.y);
            let start = previous;
            previous = current;
            // Keep right-angle corners and reversals, and use the original neighbors for every point.
            if incoming.x * outgoing.x + incoming.y * outgoing.y <= 0.0 {
                continue;
            }
            let before = incoming.x.hypot(incoming.y);
            let after = outgoing.x.hypot(outgoing.y);
            let fraction = before / (before + after);
            let target = Point::new(
                start.x + (next.x - start.x) * fraction,
                start.y + (next.y - start.y) * fraction,
            );
            let delta = Point::new((target.x - current.x) * 0.5, (target.y - current.y) * 0.5);
            let distance = delta.x.hypot(delta.y);
            if distance <= f32::EPSILON {
                continue;
            }
            let amount = (max_shift / distance).min(1.0);
            Arc::make_mut(&mut self.points)[index] =
                Point::new(current.x + delta.x * amount, current.y + delta.y * amount);
            changed = true;
        }
        if changed {
            self.refresh_path_bounds();
        }
    }
}
