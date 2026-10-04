use super::*;
use crate::{erase::brush_background_color, measurement::Interval, selection};

impl ScreenshotEditor {
    pub(super) fn choose_tool(
        &mut self,
        tool: Option<Tool>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.finish_text(true, window, cx);
        self.measurement_drag = None;
        self.crop = None;
        self.editor.deselect();
        self.adjustment_scroll = None;
        self.tool = self.preferences.select_tool(tool);
        self.hovered_handle = None;
        self.hover_position = self.canvas_position(window.mouse_position());
        self.preview_position = if self.tool.is_some_and(Tool::is_stamp) {
            self.hover_position
        } else {
            None
        };
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn canvas_center(&self) -> ImagePoint {
        let bounds = self.editor.image_bounds();
        ImagePoint::new(bounds.width / 2.0, bounds.height / 2.0)
    }

    pub(super) fn edits_at(&self, point: ImagePoint, scale: f32) -> bool {
        self.tool.is_none()
            || ((self.tool == Some(Tool::Text) || self.editor.selected().is_some())
                && self.editor.hovered_layer(point, scale).is_some())
    }

    pub(super) fn measurement_interval(&self) -> Option<(MeasurementAxis, Interval)> {
        let Tool::Measure(axis) = self.tool? else {
            return None;
        };
        if self.crop.is_some() || self.text_edit.is_some() {
            return None;
        }
        let interval = self.measurement_cache.borrow_mut().measure(
            &self.source.pixels,
            self.editor.image_bounds(),
            self.hover_position?,
            axis,
        )?;
        Some((axis, interval))
    }

    pub(super) fn measurement_preview(&self, window: &Window) -> Option<Stroke> {
        let Tool::Measure(axis) = self.tool? else {
            return None;
        };
        if self.crop.is_some() || self.text_edit.is_some() {
            return None;
        }
        let scale = self.viewport.get()?.scale;
        let drag_interval = self.measurement_drag.and_then(MeasurementDrag::interval);
        if self.preferences.measurement_mode.get() == MeasurementMode::Guide {
            if let Some((axis, interval)) = drag_interval {
                return self.editor.guide_segment_preview(
                    axis.guide_orientation(),
                    interval,
                    scale,
                );
            }
            let position = self.hover_position?;
            let bounds = self.editor.image_bounds();
            if position.x < 0.0
                || position.y < 0.0
                || position.x >= bounds.width
                || position.y >= bounds.height
            {
                return None;
            }
            return self
                .editor
                .guide_preview(axis.guide_orientation(), position, scale);
        }
        if !scale.is_finite() || scale <= 0.0 {
            return None;
        }
        let (axis, interval) = drag_interval.or_else(|| self.measurement_interval())?;
        let line = selection::shape_dimension_label(
            interval.text(axis, self.dimension_display()).into(),
            px(theme::DIMENSION_FONT_SIZE),
            rgb(theme::TEXT).into(),
            window,
        );
        self.editor
            .measurement_preview(interval, axis, scale, f32::from(line.width()) / scale)
    }

    pub(super) fn measurement_alignment_preview(&self) -> Option<Stroke> {
        let Tool::Measure(_) = self.tool? else {
            return None;
        };
        if self.crop.is_some() || self.text_edit.is_some() {
            return None;
        }
        let scale = self.viewport.get()?.scale;
        let drag = self.measurement_drag?;
        let (axis, _) = drag.interval()?;
        let interval = axis.alignment_guide(drag.end, self.editor.image_bounds())?;
        let mut stroke = self.editor.guide_segment_preview(
            axis.perpendicular_guide_orientation(),
            interval,
            scale,
        )?;
        stroke.tool = Tool::Line(LineStyle::Dashed);
        stroke.color = self.preferences.measurement_mode.get().alignment_color();
        Some(stroke)
    }

    pub(super) fn place_measurement(&mut self, window: &Window) {
        if let Some(stroke) = self.measurement_preview(window) {
            if matches!(stroke.tool, Tool::Guide(_)) {
                self.editor.add_guide_stroke(stroke);
                self.editor.deselect();
            } else {
                self.editor.add_measurement(stroke);
            }
        }
    }

    pub(super) fn stamp_preview(&self) -> Option<Stroke> {
        let tool = self.tool?;
        let position = self.preview_position?;
        let viewport = self.viewport.get()?;
        if self.editor.is_dragging() || self.edits_at(position, viewport.scale) {
            return None;
        }
        match tool {
            Tool::Number(_) => self.editor.number_preview(
                position,
                DEFAULT_NUMBER_SIZE / viewport.scale,
                self.preferences.color.get(),
                self.preferences.number_tail.get(),
            ),
            _ => None,
        }
    }

    pub(super) fn place_stamp(&mut self, point: ImagePoint) {
        let Some(viewport) = self.viewport.get() else {
            return;
        };
        if matches!(self.tool, Some(Tool::Number(_))) {
            self.editor.add_number(
                point,
                DEFAULT_NUMBER_SIZE / viewport.scale,
                self.preferences.color.get(),
                self.preferences.number_tail.get(),
            );
        }
    }

    pub(super) fn active_tool(&self) -> Option<Tool> {
        if self.text_edit.is_some() {
            return Some(Tool::Text);
        }
        self.editor
            .selected()
            .map(|stroke| stroke.tool)
            .or(self.tool)
    }

    pub(super) fn active_width(&self) -> f32 {
        let scale = self.viewport.get().map_or(1.0, |viewport| viewport.scale);
        let width = self
            .editor
            .selected()
            .map_or(self.preferences.stroke_width.get(), |stroke| {
                stroke.display_width(scale)
            });
        (MIN_STROKE_WIDTH
            + ((width - MIN_STROKE_WIDTH) / STROKE_WIDTH_STEP).round() * STROKE_WIDTH_STEP)
            .clamp(MIN_STROKE_WIDTH, MAX_STROKE_WIDTH)
    }

    pub(super) fn image_stroke_width(&self) -> f32 {
        self.viewport
            .get()
            .filter(|viewport| viewport.scale.is_finite() && viewport.scale > 0.0)
            .map_or(self.preferences.stroke_width.get(), |viewport| {
                self.preferences.stroke_width.get() / viewport.scale
            })
    }

    pub(super) fn apply_brush_auto_color(&mut self) {
        let Some(stroke) = self.editor.draft().or_else(|| self.editor.selected()) else {
            return;
        };
        let Some(point) = stroke.points.first().copied() else {
            return;
        };
        let bounds = self.editor.image_bounds();
        let color = brush_background_color(
            &self.source.pixels,
            point,
            stroke.width,
            ImagePoint::new(bounds.x, bounds.y),
        );
        self.editor.set_brush_auto_color(color);
    }

    pub(super) fn set_fill_color(
        &mut self,
        color: Option<u32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tool) = self.active_tool().filter(|tool| tool.is_filled_shape()) else {
            return;
        };
        self.preferences.set_fill_color(tool, color);
        self.editor.set_fill_color(color);
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn active_fill_opacity(&self, tool: Tool) -> f32 {
        self.editor
            .selected()
            .map_or(self.preferences.fill_opacity(tool), |stroke| {
                stroke.fill_opacity
            })
    }

    pub(super) fn undo(&mut self) {
        self.crop = None;
        self.reset_pointer();
        self.editor.undo();
    }

    pub(super) fn redo(&mut self) {
        self.crop = None;
        self.reset_pointer();
        self.editor.redo();
    }
}
