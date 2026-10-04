use super::*;

impl Editor {
    pub fn hit_index(&self, point: Point, display_scale: f32) -> Option<usize> {
        if !display_scale.is_finite()
            || display_scale <= 0.0
            || !point.x.is_finite()
            || !point.y.is_finite()
        {
            return None;
        }
        self.strokes.iter().rposition(|stroke| {
            let bounds = stroke.paint_bounds(display_scale);
            bounds.x < self.image_width
                && bounds.y < self.image_height
                && bounds.x + bounds.width > 0.0
                && bounds.y + bounds.height > 0.0
                && stroke.hit_test(point, 5.0 / display_scale, display_scale)
        })
    }

    pub fn hovered_layer(&self, point: Point, display_scale: f32) -> Option<&Stroke> {
        if self.resize_handle_at(point, display_scale).is_some() {
            return self.selected();
        }
        self.hit_index(point, display_scale)
            .and_then(|index| self.strokes.get(index))
    }

    pub fn resize_handle_at(&self, point: Point, display_scale: f32) -> Option<ResizeHandle> {
        if !display_scale.is_finite() || display_scale <= 0.0 {
            return None;
        }
        let radius = HANDLE_HIT_RADIUS / display_scale;
        let stroke = self.selected()?;
        let handles = stroke.selection_handles(display_scale);
        let padding = stroke.selection_padding(display_scale);
        let handle = handles
            .iter()
            .copied()
            .filter(|(handle, position)| {
                let (x, y) = handle.axes();
                let original = Point::new(
                    position.x - x as f32 * padding,
                    position.y - y as f32 * padding,
                );
                original.x >= 0.0
                    && original.x <= self.image_width
                    && original.y >= 0.0
                    && original.y <= self.image_height
            })
            .filter_map(|(handle, position)| {
                let distance = segment_distance(point, position, position);
                (distance <= radius).then_some((handle, distance))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(handle, _)| handle);
        handle.or_else(|| {
            if handles.is_empty() || matches!(stroke.tool, Tool::Arrow | Tool::Line(_)) {
                return None;
            }
            stroke.bounds().resize_edge_at(
                point,
                radius,
                Point::new(self.image_width, self.image_height),
                padding,
            )
        })
    }

    pub fn active_resize_handle(&self) -> Option<ResizeHandle> {
        let draft = self.transforming.as_ref()?;
        let handle = draft.handle?;
        if matches!(
            handle,
            ResizeHandle::ArrowStart | ResizeHandle::ArrowEnd | ResizeHandle::ArrowMiddle
        ) {
            return Some(handle);
        }
        let stroke = &self.strokes[draft.index];
        let before = &draft.before;
        let (mut x, mut y) = handle.axes();
        if (stroke.end.x < stroke.start.x) != (before.end.x < before.start.x) {
            x = -x;
        }
        if (stroke.end.y < stroke.start.y) != (before.end.y < before.start.y) {
            y = -y;
        }
        stroke
            .resize_handles()
            .into_iter()
            .find(|(handle, _)| handle.axes() == (x, y))
            .map(|(handle, _)| handle)
    }

    pub fn begin_selection(&mut self, point: Point, display_scale: f32) {
        self.finish_style_change();
        self.cancel();
        if let Some(handle) = self.resize_handle_at(point, display_scale)
            && self.selected.is_some()
        {
            let index = self.raise_selected().unwrap();
            self.transforming = Some(TransformDraft {
                index,
                origin: point,
                before: self.strokes[index].clone(),
                handle: Some(handle),
                display_scale,
            });
        } else {
            self.begin_move(point, display_scale);
        }
    }

    pub fn begin_move(&mut self, point: Point, display_scale: f32) {
        self.cancel();
        self.selected = None;
        if !display_scale.is_finite() || display_scale <= 0.0 {
            return;
        }
        self.selected = self.hit_index(point, display_scale);
        if let Some(index) = self.raise_selected() {
            self.transforming = Some(TransformDraft {
                index,
                origin: point,
                before: self.strokes[index].clone(),
                handle: None,
                display_scale,
            });
        }
    }

    pub(super) fn raise_selected(&mut self) -> Option<usize> {
        let index = self.selected?;
        let top = self.strokes.len() - 1;
        if index != top {
            let stroke = self.strokes.remove(index);
            self.strokes.push(stroke);
            self.selected = Some(top);
            self.record_edit(Edit::Raise { index });
        }
        Some(top)
    }

    pub(super) fn update_move(&mut self, point: Point) {
        let Some(transforming) = self.transforming.as_ref() else {
            return;
        };
        if !point.x.is_finite() || !point.y.is_finite() {
            return;
        }
        let shape = Self::translated(
            &transforming.before,
            Point::new(
                point.x - transforming.origin.x,
                point.y - transforming.origin.y,
            ),
            self.grid,
        );
        self.strokes[transforming.index] = shape;
    }

    pub(super) fn translated(stroke: &Stroke, delta: Point, grid: PixelGrid) -> Stroke {
        let mut translated = stroke.clone();
        translated.translate(delta.snapped(grid));
        translated
    }

    pub fn nudge_selected(&mut self, delta: Point) -> bool {
        if self.is_dragging() || !delta.x.is_finite() || !delta.y.is_finite() {
            return false;
        }
        let grid = self.grid;
        self.change_selected(|stroke| {
            *stroke = Self::translated(stroke, delta, grid);
        })
    }

    pub(super) fn update_resize(&mut self, point: Point, constrained: bool) {
        let Some(draft) = self.transforming.as_ref() else {
            return;
        };
        let Some(handle) = draft.handle else {
            return;
        };
        if !point.x.is_finite() || !point.y.is_finite() {
            return;
        }
        let mut stroke = draft.before.clone();
        let constrained = constrained || matches!(stroke.tool, Tool::Number(_) | Tool::Text);
        let delta =
            Point::new(point.x - draft.origin.x, point.y - draft.origin.y).snapped(self.grid);
        if delta == Point::default() {
            self.strokes[draft.index] = stroke;
            return;
        }
        let dx = delta.x;
        let dy = delta.y;
        if handle == ResizeHandle::ArrowMiddle {
            let middle = stroke.arrow_point(0.5);
            let chord_middle = midpoint(stroke.start, stroke.end);
            let control = Point::new(
                2.0 * (middle.x + dx) - chord_middle.x,
                2.0 * (middle.y + dy) - chord_middle.y,
            );
            stroke.arrow_control =
                (segment_distance(control, chord_middle, chord_middle) > 0.001).then_some(control);
            self.strokes[draft.index] = stroke;
            return;
        }
        if matches!(handle, ResizeHandle::ArrowStart | ResizeHandle::ArrowEnd) {
            let (original, anchor) = if handle == ResizeHandle::ArrowStart {
                (stroke.start, stroke.end)
            } else {
                (stroke.end, stroke.start)
            };
            let mut moved = Point::new(original.x + dx, original.y + dy);
            if constrained {
                let vx = original.x - anchor.x;
                let vy = original.y - anchor.y;
                let length = vx.hypot(vy);
                if length > f32::EPSILON {
                    let factor =
                        ((moved.x - anchor.x) * vx + (moved.y - anchor.y) * vy) / (length * length);
                    let sign = if factor < 0.0 { -1.0 } else { 1.0 };
                    let factor = factor
                        .abs()
                        .max(MIN_DISPLAY_DRAG / draft.display_scale / length)
                        * sign;
                    moved = Point::new(anchor.x + vx * factor, anchor.y + vy * factor);
                }
            }
            let mut moved = moved.snapped(self.grid);
            if matches!(stroke.tool, Tool::Line(_)) && !constrained {
                moved = snap_line_endpoint(anchor, moved, draft.display_scale);
            }
            if handle == ResizeHandle::ArrowStart {
                stroke.start = moved;
            } else {
                stroke.end = moved;
            }
            if stroke.is_large_enough(draft.display_scale) {
                if let Some(control) = draft.before.arrow_control {
                    let before = &draft.before;
                    let vx = before.end.x - before.start.x;
                    let vy = before.end.y - before.start.y;
                    let length_squared = vx * vx + vy * vy;
                    if length_squared > f32::EPSILON {
                        let cx = control.x - before.start.x;
                        let cy = control.y - before.start.y;
                        let along = (cx * vx + cy * vy) / length_squared;
                        let across = (cy * vx - cx * vy) / length_squared;
                        let dx = stroke.end.x - stroke.start.x;
                        let dy = stroke.end.y - stroke.start.y;
                        stroke.arrow_control = Some(Point::new(
                            stroke.start.x + along * dx - across * dy,
                            stroke.start.y + along * dy + across * dx,
                        ));
                    }
                }
                self.strokes[draft.index] = stroke;
            }
            return;
        }
        let bounds = stroke.bounds();
        let (x, y) = handle.axes();
        let anchor = Point::new(
            bounds.x + bounds.width * (1.0 - x as f32) / 2.0,
            bounds.y + bounds.height * (1.0 - y as f32) / 2.0,
        );
        let min = MIN_DISPLAY_DRAG / draft.display_scale;
        let mut sx = if x == 0 {
            1.0
        } else {
            1.0 + x as f32 * dx / bounds.width
        };
        let mut sy = if y == 0 {
            1.0
        } else {
            1.0 + y as f32 * dy / bounds.height
        };
        if stroke.tool == Tool::Text {
            sx = sx.max(0.0);
            sy = sy.max(0.0);
        }
        let sign_x = if sx < 0.0 { -1.0 } else { 1.0 };
        let sign_y = if sy < 0.0 { -1.0 } else { 1.0 };
        let min_x = (min / bounds.width).min(1.0);
        let min_y = (min / bounds.height).min(1.0);
        if constrained {
            let factor = if x == 0 {
                sy.abs()
            } else if y == 0 {
                sx.abs()
            } else {
                sx.abs().max(sy.abs())
            };
            let factor = factor.max(min_x.max(min_y));
            sx = factor * sign_x;
            sy = factor * sign_y;
        } else {
            if x != 0 {
                sx = sx.abs().max(min_x) * sign_x;
            }
            if y != 0 {
                sy = sy.abs().max(min_y) * sign_y;
            }
        }
        stroke.start = Point::new(
            anchor.x + (stroke.start.x - anchor.x) * sx,
            anchor.y + (stroke.start.y - anchor.y) * sy,
        );
        stroke.end = Point::new(
            anchor.x + (stroke.end.x - anchor.x) * sx,
            anchor.y + (stroke.end.y - anchor.y) * sy,
        );
        let reverse_x = stroke.end.x < stroke.start.x;
        let reverse_y = stroke.end.y < stroke.start.y;
        let snapped = stroke.bounds().snapped(self.grid);
        stroke.start = Point::new(
            snapped.x + if reverse_x { snapped.width } else { 0.0 },
            snapped.y + if reverse_y { snapped.height } else { 0.0 },
        );
        stroke.end = Point::new(
            snapped.x + if reverse_x { 0.0 } else { snapped.width },
            snapped.y + if reverse_y { 0.0 } else { snapped.height },
        );
        if let Some(text) = stroke.text.as_mut() {
            let factor = snapped.width / bounds.width;
            text.font_size *= factor;
            text.wrap_width *= factor;
        }
        if stroke.is_large_enough(draft.display_scale * 1.001) {
            self.strokes[draft.index] = stroke;
        }
    }
}
