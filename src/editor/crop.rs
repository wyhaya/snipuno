use super::{Bounds, Edit, Editor, Point};

impl Editor {
    pub fn image_bounds(&self) -> Bounds {
        self.source_bounds
    }

    pub fn crop(&mut self, bounds: Bounds) -> bool {
        let right = bounds.x + bounds.width;
        let bottom = bounds.y + bounds.height;
        if bounds.width == 0.0
            || bounds.height == 0.0
            || ![bounds.x, bounds.y, right, bottom]
                .into_iter()
                .all(f32::is_finite)
        {
            return false;
        }
        let left = bounds.x.min(right).round().clamp(0.0, self.image_width);
        let top = bounds.y.min(bottom).round().clamp(0.0, self.image_height);
        let right = bounds.x.max(right).round().clamp(0.0, self.image_width);
        let bottom = bounds.y.max(bottom).round().clamp(0.0, self.image_height);
        if right - left < 1.0 || bottom - top < 1.0 {
            return false;
        }
        let before = self.source_bounds;
        let after = Bounds {
            x: before.x + left,
            y: before.y + top,
            width: right - left,
            height: bottom - top,
        };
        if before == after {
            return false;
        }
        self.deselect();
        self.apply_crop(after);
        self.record_edit(Edit::Crop { before, after });
        true
    }

    pub(super) fn apply_crop(&mut self, bounds: Bounds) {
        let delta = Point::new(
            self.source_bounds.x - bounds.x,
            self.source_bounds.y - bounds.y,
        );
        for stroke in &mut self.strokes {
            stroke.translate(delta);
        }
        self.source_bounds = bounds;
        self.image_width = bounds.width;
        self.image_height = bounds.height;
    }
}
