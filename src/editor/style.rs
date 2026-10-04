use super::*;

impl Editor {
    pub fn set_arrow_double_headed(&mut self, double_headed: bool) -> bool {
        if let Some(draft) = self
            .draft
            .as_mut()
            .filter(|stroke| stroke.tool == Tool::Arrow)
        {
            draft.arrow_double_headed = double_headed;
            return true;
        }
        self.change_selected(|stroke| {
            if stroke.tool == Tool::Arrow {
                stroke.arrow_double_headed = double_headed;
            }
        })
    }

    pub fn set_magnifier_zoom(&mut self, zoom: f32) -> bool {
        if !(Magnifier::MIN_ZOOM..=Magnifier::MAX_ZOOM).contains(&zoom) {
            return false;
        }
        if let Some(magnifier) = self
            .draft
            .as_mut()
            .and_then(|stroke| stroke.magnifier.as_mut())
        {
            magnifier.zoom = zoom;
            return true;
        }
        self.change_selected(|stroke| {
            if let Some(magnifier) = &mut stroke.magnifier {
                magnifier.zoom = zoom;
            }
        })
    }

    pub fn adjust_magnifier_zoom_at(
        &mut self,
        point: Point,
        display_scale: f32,
        steps: i32,
    ) -> Option<f32> {
        if steps == 0 || self.is_dragging() {
            return None;
        }
        let index = self.hit_index(point, display_scale)?;
        let current_zoom = self.strokes[index].magnifier.as_ref()?.zoom;
        let step_scale = Magnifier::ZOOM_STEP.recip();
        let zoom = ((current_zoom * step_scale).round() + steps as f32) / step_scale;
        let zoom = zoom.clamp(Magnifier::MIN_ZOOM, Magnifier::MAX_ZOOM);
        if zoom != current_zoom {
            self.finish_style_change();
            self.selected = Some(index);
            self.set_magnifier_zoom(zoom);
        }
        Some(zoom)
    }

    pub fn preview_magnifier_zoom(&mut self, zoom: f32) {
        if !(Magnifier::MIN_ZOOM..=Magnifier::MAX_ZOOM).contains(&zoom)
            || !self
                .selected()
                .is_some_and(|stroke| stroke.magnifier.is_some())
        {
            return;
        }
        self.begin_style_change();
        self.set_magnifier_zoom(zoom);
    }

    pub fn set_redaction_style(&mut self, style: RedactionStyle) -> bool {
        if let Some(draft) = self
            .draft
            .as_mut()
            .filter(|stroke| stroke.tool.is_obscuring())
        {
            draft.tool = Tool::Redact(style);
            return true;
        }
        self.change_selected(|stroke| {
            if stroke.tool.is_obscuring() {
                stroke.tool = Tool::Redact(style);
            }
        })
    }

    pub fn set_fill_color(&mut self, color: Option<u32>) -> bool {
        if let Some(draft) = self
            .draft
            .as_mut()
            .filter(|stroke| stroke.is_filled_shape())
        {
            draft.fill_color = color;
            return true;
        }
        self.change_selected(|stroke| {
            if stroke.is_filled_shape() {
                stroke.fill_color = color;
            }
        })
    }

    pub fn set_fill_opacity(&mut self, opacity: f32) -> bool {
        if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
            return false;
        }
        if let Some(draft) = self
            .draft
            .as_mut()
            .filter(|stroke| stroke.is_filled_shape())
        {
            draft.fill_opacity = opacity;
            return true;
        }
        self.change_selected(|stroke| {
            if stroke.is_filled_shape() {
                stroke.fill_opacity = opacity;
            }
        })
    }

    pub fn set_fill_opacity_at(
        &mut self,
        point: Point,
        display_scale: f32,
        opacity: f32,
    ) -> Option<f32> {
        if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) || self.is_dragging() {
            return None;
        }
        let index = self.hit_index(point, display_scale)?;
        let stroke = &self.strokes[index];
        if !stroke.is_filled_shape() {
            return None;
        }
        if stroke.fill_opacity != opacity {
            self.finish_style_change();
            self.selected = Some(index);
            self.set_fill_opacity(opacity);
        }
        Some(opacity)
    }

    pub fn preview_fill_opacity(&mut self, opacity: f32) {
        if !opacity.is_finite()
            || !(0.0..=1.0).contains(&opacity)
            || !self.selected().is_some_and(Stroke::is_filled_shape)
        {
            return;
        }
        self.begin_style_change();
        self.set_fill_opacity(opacity);
    }

    pub fn set_effect_strength(&mut self, strength: EffectStrength) -> bool {
        if let Some(draft) = self
            .draft
            .as_mut()
            .filter(|stroke| stroke.tool.has_strength())
        {
            draft.effect_strength = strength;
            return true;
        }
        self.change_selected(|stroke| {
            if stroke.tool.has_strength() {
                stroke.effect_strength = strength;
            }
        })
    }

    pub fn set_effect_strength_at(
        &mut self,
        point: Point,
        display_scale: f32,
        strength: EffectStrength,
    ) -> Option<EffectStrength> {
        if self.is_dragging() {
            return None;
        }
        let index = self.hit_index(point, display_scale)?;
        let stroke = &self.strokes[index];
        if !stroke.tool.has_strength() {
            return None;
        }
        if stroke.effect_strength != strength {
            self.finish_style_change();
            self.selected = Some(index);
            self.set_effect_strength(strength);
        }
        Some(strength)
    }

    pub fn preview_effect_strength(&mut self, strength: EffectStrength) {
        if !self
            .selected()
            .is_some_and(|stroke| stroke.tool.has_strength())
        {
            return;
        }
        self.begin_style_change();
        self.set_effect_strength(strength);
    }

    pub fn set_selected_color(&mut self, color: u32) -> bool {
        self.change_selected(|stroke| {
            if stroke.tool.has_color() && !stroke.is_filled_shape() {
                stroke.color = color;
                stroke.brush_auto_color = false;
            }
        })
    }

    pub fn set_brush_auto_color(&mut self, color: Option<u32>) -> bool {
        let color = color.map_or(0, |color| 0xff000000 | color);
        if let Some(draft) = self
            .draft
            .as_mut()
            .filter(|stroke| stroke.tool == Tool::Brush)
        {
            draft.brush_auto_color = true;
            draft.color = color;
            return true;
        }
        self.change_selected(|stroke| {
            if stroke.tool == Tool::Brush {
                stroke.brush_auto_color = true;
                stroke.color = color;
            }
        })
    }

    pub fn set_selected_width(&mut self, width: f32) -> bool {
        if !width.is_finite() || width <= 0.0 {
            return false;
        }
        self.change_selected(|stroke| {
            if stroke.tool.has_width() && stroke.width != width {
                stroke.width = width;
                if stroke.tool.is_freehand() {
                    stroke.refresh_path_bounds();
                }
            }
        })
    }

    pub fn set_stroke_width_at(
        &mut self,
        point: Point,
        display_scale: f32,
        width: f32,
    ) -> Option<f32> {
        if !width.is_finite() || width <= 0.0 || self.is_dragging() {
            return None;
        }
        let index = self.hit_index(point, display_scale)?;
        let stroke = &self.strokes[index];
        if !stroke.tool.has_width() {
            return None;
        }
        if stroke.width != width {
            self.finish_style_change();
            self.selected = Some(index);
            self.set_selected_width(width);
        }
        Some(width)
    }

    pub fn preview_selected_width(&mut self, width: f32) {
        if !width.is_finite() || width <= 0.0 {
            return;
        }
        if !self
            .selected()
            .is_some_and(|stroke| stroke.tool.has_width())
        {
            return;
        }
        self.begin_style_change();
        self.set_selected_width(width);
    }

    pub(super) fn begin_style_change(&mut self) {
        let Some(index) = self.selected else {
            return;
        };
        if self.style_preview.is_none() {
            self.cancel();
            self.style_preview = Some((index, self.strokes[index].clone()));
        }
    }

    pub fn finish_style_change(&mut self) -> bool {
        let Some((index, before)) = self.style_preview.take() else {
            return false;
        };
        let after = self.strokes[index].clone();
        if before == after {
            return false;
        }
        self.record_edit(Edit::Transform {
            index,
            before,
            after,
        });
        true
    }

    pub(super) fn change_selected(&mut self, change: impl FnOnce(&mut Stroke)) -> bool {
        let previewing = self.style_preview.is_some();
        if !previewing {
            self.cancel();
        }
        let Some(index) = self.selected else {
            return false;
        };
        let before = self.strokes[index].clone();
        change(&mut self.strokes[index]);
        if self.strokes[index] == before {
            return false;
        }
        if previewing {
            return true;
        }
        self.record_edit(Edit::Transform {
            index,
            before,
            after: self.strokes[index].clone(),
        });
        true
    }
}
