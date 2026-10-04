use super::*;
use crate::image_processing::sample_image_color_in_bounds;
use std::time::{Duration, Instant};

const SCROLL_PIXELS_PER_STEP: f32 = 24.0;

impl ScreenshotEditor {
    pub(super) fn canvas_position(&self, position: Point<Pixels>) -> Option<ImagePoint> {
        let panel = self.options_panel.borrow();
        if panel.is_dragging() || panel.contains(position) {
            return None;
        }
        self.viewport
            .get()
            .filter(|viewport| viewport.contains(position))
            .map(|viewport| viewport.to_image(position))
    }

    pub(super) fn clear_hover(&mut self, cx: &mut Context<Self>) {
        self.adjustment_scroll = None;
        let hovered = self.hover_position.take().is_some();
        let handle = self.hovered_handle.take().is_some();
        let preview = self.preview_position.take().is_some();
        if hovered || handle || preview {
            cx.notify();
        }
    }

    pub(super) fn pointer_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.adjustment_scroll = None;
        if self.options_panel.borrow().contains(event.position) {
            return;
        }
        if matches!(self.tool, Some(Tool::Measure(_))) && self.crop.is_none() {
            if event.button == MouseButton::Left
                && event.click_count == 1
                && let Some(viewport) = self.viewport.get().filter(|v| v.contains(event.position))
            {
                self.hover_position = Some(viewport.to_image(event.position));
                self.measurement_drag = self.hover_position.map(MeasurementDrag::new);
                self.focus.focus(window, cx);
                cx.stop_propagation();
                cx.notify();
            }
            return;
        }
        let Some(viewport) = self.viewport.get() else {
            return;
        };
        let point = viewport.to_image(event.position);
        if let Some(crop) = self.crop.as_mut() {
            if viewport.contains(event.position) || crop.handle_at(point, viewport.scale).is_some()
            {
                crop.begin(point, viewport.scale);
                self.focus.focus(window, cx);
                cx.notify();
            }
            return;
        }
        let over_handle = self.text_edit.is_none()
            && self
                .editor
                .resize_handle_at(point, viewport.scale)
                .is_some();
        if !viewport.contains(event.position) && !over_handle {
            return;
        }
        let was_editing_text = self.text_edit.is_some();
        let continue_text = was_editing_text && self.tool == Some(Tool::Text);
        self.finish_text(true, window, cx);
        cx.stop_propagation();
        self.focus.focus(window, cx);
        self.hover_position = Some(point);
        if was_editing_text {
            if continue_text {
                self.tool = Some(Tool::Text);
            } else {
                cx.notify();
                return;
            }
        }
        if event.click_count == 2
            && (self.tool.is_none()
                || self.editor.selected().is_some()
                || self.tool == Some(Tool::Text))
        {
            self.editor.begin_selection(point, viewport.scale);
            self.editor.cancel();
            if self.editor.selected().is_some_and(|s| s.tool == Tool::Text) {
                self.start_text(point, true, window, cx);
                return;
            }
        }
        if over_handle {
            self.editor.begin_selection(point, viewport.scale);
            cx.notify();
            return;
        }
        if self.edits_at(point, viewport.scale) {
            self.editor.begin_selection(point, viewport.scale);
        } else if self.tool == Some(Tool::Text) {
            self.start_text(point, false, window, cx);
        } else if self.tool.is_some_and(Tool::is_stamp) {
            self.preview_position = Some(point);
            self.place_stamp(point);
        } else if let Some(tool) = self.tool {
            let color = self.preferences.color.get();
            let width = if tool.has_width() {
                self.image_stroke_width()
            } else {
                3.0 / viewport.scale
            };
            self.editor.begin(tool, point, color, width);
            if tool == Tool::Brush && self.preferences.brush_auto_color.get() {
                self.apply_brush_auto_color();
            }
            if tool.is_filled_shape() {
                self.editor
                    .set_fill_color(self.preferences.fill_color(tool));
                self.editor
                    .set_fill_opacity(self.preferences.fill_opacity(tool));
            }
            if tool.has_strength() {
                self.editor
                    .set_effect_strength(self.preferences.effect_strength.get());
            }
            if tool == Tool::Magnifier {
                self.editor
                    .set_magnifier_zoom(self.preferences.magnifier_zoom.get());
            }
            if tool == Tool::Arrow {
                self.editor
                    .set_arrow_double_headed(self.preferences.arrow_double_headed.get());
            }
        } else {
            self.editor.begin_selection(point, viewport.scale);
        }
        cx.notify();
    }

    pub(super) fn pointer_scroll(
        &mut self,
        event: &ScrollWheelEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.touch_phase != TouchPhase::Moved {
            self.adjustment_scroll = None;
        }
        if matches!(event.touch_phase, TouchPhase::Ended | TouchPhase::Cancelled)
            || self.crop.is_some()
            || self.text_edit.is_some()
            || self.editor.is_dragging()
            || self.measurement_drag.is_some()
            || self.toolbar_drag_pending
        {
            self.adjustment_scroll = None;
            return;
        }
        let target = self.canvas_position(event.position).and_then(|point| {
            let viewport = self.viewport.get()?;
            let index = self.editor.hit_index(point, viewport.scale)?;
            let tool = self.editor.strokes()[index].tool;
            Self::supports_scroll_adjustment(tool).then_some((point, viewport.scale, index, tool))
        });
        let Some((point, scale, index, tool)) = target else {
            self.adjustment_scroll = None;
            return;
        };
        let delta = event.delta.pixel_delta(px(SCROLL_PIXELS_PER_STEP));
        let horizontal = f32::from(delta.x);
        let vertical = f32::from(delta.y);
        if !vertical.is_finite() || !horizontal.is_finite() || vertical.abs() <= horizontal.abs() {
            self.adjustment_scroll = None;
            return;
        }
        cx.stop_propagation();
        let now = Instant::now();
        let delta = vertical / SCROLL_PIXELS_PER_STEP;
        let remainder = self
            .adjustment_scroll
            .filter(|(target, remainder, last)| {
                *target == index
                    && now.duration_since(*last) <= Duration::from_millis(250)
                    && remainder.signum() == delta.signum()
            })
            .map_or(0.0, |(_, remainder, _)| remainder);
        let max_steps = Self::adjustment_step_limit(tool);
        let accumulated = (remainder + delta).clamp(-max_steps, max_steps);
        let steps = accumulated.trunc() as i32;
        self.adjustment_scroll = Some((index, accumulated - steps as f32, now));
        self.apply_scroll_adjustment(index, point, scale, steps, cx);
    }

    pub(super) fn pointer_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let color = self.pixel_color_at(event.position);
        self.sampling_position = color.map(|_| event.position);
        if let Some(color) = color
            && self.sampled_color != color
        {
            self.sampled_color = color;
            cx.notify();
        }
        if self.move_options_panel(event, cx) {
            return;
        }
        if self.move_toolbar(event, window, cx) {
            return;
        }
        if let Some(crop) = self.crop.as_mut() {
            if let Some(viewport) = self.viewport.get() {
                let point = viewport.to_image(event.position);
                if crop.is_dragging() {
                    if event.dragging() {
                        crop.update(point, event.modifiers.shift);
                    } else {
                        crop.cancel_drag();
                    }
                    cx.notify();
                }
                let handle = crop.handle_at(point, viewport.scale);
                let hover = viewport.contains(event.position).then_some(point);
                if self.hovered_handle != handle || self.hover_position != hover {
                    self.hovered_handle = handle;
                    self.hover_position = hover;
                    cx.notify();
                }
            }
            return;
        }
        let position = self.canvas_position(event.position);
        let measuring = matches!(self.tool, Some(Tool::Measure(_)));
        if self.hover_position != position
            && (measuring || self.active_measurement_option().is_some())
        {
            cx.notify();
        }
        if measuring {
            if !event.dragging() && self.measurement_drag.take().is_some() {
                cx.notify();
            }
            if let Some(drag) = self.measurement_drag.as_mut() {
                if let Some(viewport) = self.viewport.get() {
                    let previous = *drag;
                    drag.update(
                        viewport.to_image(event.position),
                        self.editor.image_bounds(),
                        viewport.scale,
                    );
                    self.hover_position = position;
                    self.hovered_handle = None;
                    if *drag != previous {
                        cx.notify();
                    }
                }
                return;
            }
            self.hover_position = position;
            self.hovered_handle = None;
            return;
        }
        if let Some(viewport) = self.viewport.get() {
            let old = self
                .hover_position
                .and_then(|p| self.editor.hit_index(p, viewport.scale));
            let new = position.and_then(|p| self.editor.hit_index(p, viewport.scale));
            if old != new {
                cx.notify();
            }
        }
        self.hover_position = position;
        if self.tool.is_some_and(Tool::is_stamp) && self.preview_position != position {
            self.preview_position = position;
            cx.notify();
        }
        if !self.editor.is_dragging() {
            let handle = self.viewport.get().and_then(|viewport| {
                self.editor
                    .resize_handle_at(viewport.to_image(event.position), viewport.scale)
            });
            if self.hovered_handle != handle {
                self.hovered_handle = handle;
                cx.notify();
            }
            return;
        }
        if !event.dragging() {
            self.editor.cancel();
        } else if let Some(viewport) = self.viewport.get() {
            let point = viewport.to_image(event.position);
            if event.modifiers.shift {
                self.editor.update_constrained(point, viewport.scale, true);
            } else {
                self.editor.update(point, viewport.scale);
            }
        }
        cx.notify();
    }

    pub(super) fn pointer_up(
        &mut self,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button == MouseButton::Left && self.options_panel.borrow_mut().end_drag() {
            cx.notify();
            return;
        }
        self.toolbar_drag_pending = false;
        if event.button == MouseButton::Left
            && let Some(mut drag) = self.measurement_drag.take()
        {
            if let Some(viewport) = self.viewport.get() {
                drag.update(
                    viewport.to_image(event.position),
                    self.editor.image_bounds(),
                    viewport.scale,
                );
                if !matches!(self.tool, Some(Tool::Measure(_))) {
                    return;
                }
                if drag.interval().is_some() {
                    self.measurement_drag = Some(drag);
                    self.place_measurement(window);
                    self.measurement_drag = None;
                } else {
                    self.hover_position = Some(drag.start);
                    self.place_measurement(window);
                }
                self.hover_position = self.canvas_position(event.position);
                cx.notify();
            }
            return;
        }
        if let Some(crop) = self.crop.as_mut() {
            if event.button == MouseButton::Left
                && crop.is_dragging()
                && let Some(viewport) = self.viewport.get()
            {
                crop.finish(
                    viewport.to_image(event.position),
                    viewport.scale,
                    event.modifiers.shift,
                );
                cx.notify();
            }
            return;
        }
        if event.button != MouseButton::Left || !self.editor.is_dragging() {
            return;
        }
        if let Some(viewport) = self.viewport.get() {
            let point = viewport.to_image(event.position);
            let was_drawing = self.editor.draft().is_some();
            let committed = if event.modifiers.shift {
                self.editor.finish_constrained(point, viewport.scale, true)
            } else {
                self.editor.finish(point, viewport.scale)
            };
            if was_drawing && !committed && self.tool != Some(Tool::Brush) {
                self.tool = None;
                self.preview_position = None;
                self.hovered_handle = None;
                self.editor.deselect();
            }
        }
        cx.notify();
    }

    pub(super) fn key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.text_edit.is_some() {
            return;
        }
        if ColorFormat::handle_key_down(event, cx) {
            return;
        }
        let key = event.keystroke.key.to_lowercase();
        let modifiers = event.keystroke.modifiers;
        let primary = if cfg!(target_os = "macos") {
            modifiers.platform
        } else {
            modifiers.control
        };
        if self.crop.is_some() && !primary && !modifiers.alt {
            match key.as_str() {
                "enter" => self.apply_crop(window, cx),
                "up" | "down" | "left" | "right" => {
                    let step = if modifiers.shift { 10.0 } else { 1.0 };
                    let delta = match key.as_str() {
                        "up" => ImagePoint::new(0.0, -step),
                        "down" => ImagePoint::new(0.0, step),
                        "left" => ImagePoint::new(-step, 0.0),
                        _ => ImagePoint::new(step, 0.0),
                    };
                    let grid = self.dimension_display().grid();
                    let delta = ImagePoint::new(delta.x * grid.x as f32, delta.y * grid.y as f32);
                    self.crop.as_mut().unwrap().nudge(delta);
                    cx.notify();
                }
                _ => {}
            }
            if matches!(key.as_str(), "enter" | "up" | "down" | "left" | "right") {
                cx.stop_propagation();
                return;
            }
        }
        if self.option_key_down(event, window, cx) {
            cx.stop_propagation();
            cx.notify();
            return;
        }
        let handled = if matches!(self.tool, Some(Tool::Measure(_)))
            && key == "enter"
            && self.measurement_drag.is_none()
            && !modifiers.platform
            && !modifiers.control
            && !modifiers.alt
            && self.focus.is_focused(window)
        {
            if !event.is_held {
                self.place_measurement(window);
            }
            true
        } else if key == "enter"
            && !modifiers.platform
            && !modifiers.control
            && !modifiers.alt
            && !modifiers.shift
            && self.focus.is_focused(window)
            && matches!(self.tool, Some(Tool::Number(_)))
        {
            if !event.is_held {
                let position = self
                    .preview_position
                    .unwrap_or_else(|| self.canvas_center());
                if self.editor.selected().is_none() || self.stamp_preview().is_some() {
                    self.preview_position = Some(position);
                    self.place_stamp(position);
                }
            }
            true
        } else if key == "enter"
            && self.focus.is_focused(window)
            && self.tool == Some(Tool::Text)
            && self.editor.selected().is_none()
            && !modifiers.platform
            && !modifiers.control
            && !modifiers.alt
        {
            self.start_text(self.canvas_center(), false, window, cx);
            true
        } else if key == "enter"
            && self.focus.is_focused(window)
            && !modifiers.platform
            && !modifiers.control
            && !modifiers.alt
            && self.editor.selected().is_some_and(|s| s.tool == Tool::Text)
        {
            self.start_text(self.canvas_center(), true, window, cx);
            true
        } else if matches!(key.as_str(), "delete" | "backspace")
            && !modifiers.platform
            && !modifiers.control
            && !modifiers.alt
        {
            let deleted = self.editor.delete_selected();
            if deleted {
                self.hovered_handle = None;
            }
            deleted
        } else if matches!(key.as_str(), "up" | "down" | "left" | "right")
            && !modifiers.platform
            && !modifiers.control
            && !modifiers.alt
            && (self.editor.selected().is_some() || self.tool.is_some_and(Tool::is_stamp))
            && self.focus.is_focused(window)
            && !self.editor.is_dragging()
        {
            // macOS can redispatch arrow keys with KeyDownEvent::is_held reset to false.
            let repeated = !self.pressed_nudge_keys.insert(key.clone());
            let step = if modifiers.shift {
                20.0
            } else if repeated {
                10.0
            } else {
                1.0
            };
            let delta = match key.as_str() {
                "up" => ImagePoint::new(0.0, -step),
                "down" => ImagePoint::new(0.0, step),
                "left" => ImagePoint::new(-step, 0.0),
                _ => ImagePoint::new(step, 0.0),
            };
            let grid = self.dimension_display().grid();
            let delta = ImagePoint::new(delta.x * grid.x as f32, delta.y * grid.y as f32);
            if self.editor.selected().is_none() && self.tool.is_some_and(Tool::is_stamp) {
                let position = self
                    .preview_position
                    .unwrap_or_else(|| self.canvas_center());
                self.preview_position = Some(ImagePoint::new(
                    (position.x + delta.x).clamp(0.0, self.editor.image_bounds().width),
                    (position.y + delta.y).clamp(0.0, self.editor.image_bounds().height),
                ));
            } else {
                self.editor.nudge_selected(delta);
            }
            self.hovered_handle = None;
            true
        } else if !modifiers.platform && !modifiers.control && !modifiers.alt {
            match key.as_str() {
                "1" | "2" | "3" | "4" if !modifiers.shift && self.focus.is_focused(window) => {
                    if !event.is_held {
                        let index = key.as_bytes()[0] as usize - b'1' as usize;
                        self.choose_measurement_option(MeasurementMode::OPTIONS[index], window, cx);
                    }
                    true
                }
                "c" => {
                    self.start_crop(window, cx);
                    true
                }
                "v" => {
                    self.choose_tool(None, window, cx);
                    true
                }
                "r" => {
                    self.choose_tool(
                        Some(self.preferences.shape_tool(Tool::Rectangle)),
                        window,
                        cx,
                    );
                    true
                }
                "o" => {
                    self.choose_tool(Some(self.preferences.shape_tool(Tool::Ellipse)), window, cx);
                    true
                }
                "b" => {
                    self.choose_tool(Some(Tool::Brush), window, cx);
                    true
                }
                "s" => {
                    self.choose_tool(
                        Some(Tool::Line(self.preferences.line_style.get())),
                        window,
                        cx,
                    );
                    true
                }
                "l" => {
                    self.choose_tool(Some(Tool::Magnifier), window, cx);
                    true
                }
                "h" => {
                    self.choose_tool(Some(Tool::Spotlight), window, cx);
                    true
                }
                "t" => {
                    self.choose_tool(Some(Tool::Text), window, cx);
                    true
                }
                "n" => {
                    self.choose_tool(Some(Tool::Number(0)), window, cx);
                    true
                }
                "e" => {
                    self.choose_tool(
                        Some(Tool::Redact(self.preferences.redaction_style.get())),
                        window,
                        cx,
                    );
                    true
                }
                "a" => {
                    self.choose_tool(Some(Tool::Arrow), window, cx);
                    true
                }
                _ => false,
            }
        } else {
            false
        };
        if handled {
            cx.stop_propagation();
            cx.notify();
        }
    }

    pub(super) fn pixel_color_at(&self, position: Point<Pixels>) -> Option<u32> {
        let point = self.canvas_position(position)?;
        let source = &self.source.pixels;
        let crop = self.editor.image_bounds();
        sample_image_color_in_bounds(
            source,
            ImagePoint::new(point.x + crop.x, point.y + crop.y),
            crop,
        )
    }

    pub(super) fn copy_pixel_color(&mut self, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(
            ColorFormat::current(cx).format(self.sampled_color),
        ));
        self.color_copy_feedback = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(1200))
                .await;
            let _ = this.update(cx, |this, cx| {
                this.color_copy_feedback = None;
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn move_toolbar(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &App,
    ) -> bool {
        if std::mem::take(&mut self.toolbar_drag_pending) && event.dragging() {
            desktop::start_window_move(window, cx);
            return true;
        }
        false
    }
}
