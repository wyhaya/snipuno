use super::*;

fn stepped_value(value: f32, min: f32, max: f32, step: f32, steps: i32) -> f32 {
    if (value >= max && steps > 0) || (value <= min && steps < 0) {
        return value;
    }
    (min + (((value - min) / step).round() + steps as f32) * step).clamp(min, max)
}

fn adjusted_stroke_width(width: f32, scale: f32, steps: i32) -> Option<f32> {
    let current = width * scale;
    let adjusted = stepped_value(
        current,
        MIN_STROKE_WIDTH,
        MAX_STROKE_WIDTH,
        STROKE_WIDTH_STEP,
        steps,
    );
    (adjusted != current).then(|| adjusted / scale)
}

impl ScreenshotEditor {
    pub(super) fn supports_scroll_adjustment(tool: Tool) -> bool {
        tool.has_width() || tool.has_strength() || tool.is_filled_shape() || tool == Tool::Magnifier
    }

    pub(super) fn adjustment_step_limit(tool: Tool) -> f32 {
        if tool == Tool::Magnifier {
            ((Magnifier::MAX_ZOOM - Magnifier::MIN_ZOOM) / Magnifier::ZOOM_STEP).ceil()
        } else if tool.is_filled_shape() {
            10.0
        } else if tool.has_strength() {
            (EffectStrength::ALL.len() - 1) as f32
        } else {
            (MAX_STROKE_WIDTH - MIN_STROKE_WIDTH) / STROKE_WIDTH_STEP
        }
    }

    pub(super) fn apply_scroll_adjustment(
        &mut self,
        index: usize,
        point: ImagePoint,
        scale: f32,
        steps: i32,
        cx: &mut Context<Self>,
    ) {
        if steps == 0 {
            return;
        }
        let stroke = &self.editor.strokes()[index];
        let tool = stroke.tool;
        if tool == Tool::Magnifier {
            if let Some(zoom) = self.editor.adjust_magnifier_zoom_at(point, scale, steps) {
                self.preferences.magnifier_zoom.set(zoom);
            }
        } else if tool.is_filled_shape() {
            let opacity = stepped_value(stroke.fill_opacity * 10.0, 0.0, 10.0, 1.0, steps) / 10.0;
            if let Some(opacity) = self.editor.set_fill_opacity_at(point, scale, opacity) {
                self.preferences.set_fill_opacity(tool, opacity);
            }
        } else if tool.has_strength() {
            let index = (stroke.effect_strength.index() as i64 + i64::from(steps))
                .clamp(0, (EffectStrength::ALL.len() - 1) as i64) as usize;
            if let Some(strength) =
                self.editor
                    .set_effect_strength_at(point, scale, EffectStrength::ALL[index])
            {
                self.preferences.effect_strength.set(strength);
            }
        } else if tool.has_width() {
            let Some(width) = adjusted_stroke_width(stroke.width, scale, steps) else {
                return;
            };
            if let Some(width) = self.editor.set_stroke_width_at(point, scale, width) {
                self.preferences.stroke_width.set(width * scale);
            }
        } else {
            return;
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::adjusted_stroke_width;

    #[test]
    fn scrolling_past_width_limits_preserves_image_width_after_viewport_resizing() {
        assert_eq!(adjusted_stroke_width(60.0, 0.49, 1), None);
        assert_eq!(adjusted_stroke_width(2.0 / 0.49, 0.49, -1), None);
    }
}
