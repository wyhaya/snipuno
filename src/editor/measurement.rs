use super::{Editor, EffectStrength, Point, Stroke, TailDirection, Tool};
use crate::measurement::{self, Interval, MeasurementAxis};
use std::sync::Arc;

impl Editor {
    pub fn measurement_preview(
        &self,
        interval: Interval,
        axis: MeasurementAxis,
        scale: f32,
        label_text_width: f32,
    ) -> Option<Stroke> {
        let aligned = match axis {
            MeasurementAxis::X => interval.start.y == interval.end.y,
            MeasurementAxis::Y => interval.start.x == interval.end.x,
        };
        if !scale.is_finite()
            || scale <= 0.0
            || !label_text_width.is_finite()
            || label_text_width < 0.0
            || !aligned
        {
            return None;
        }
        let mut stroke = Stroke {
            tool: Tool::Measure(axis),
            start: interval.start,
            end: interval.end,
            color: measurement::COLOR,
            brush_auto_color: false,
            width: 1.0 / scale,
            measurement_label: None,
            points: Arc::default(),
            effect_strength: EffectStrength::default(),
            arrow_control: None,
            arrow_double_headed: false,
            fill_color: None,
            fill_opacity: 1.0,
            text: None,
            magnifier: None,
            number_tail: TailDirection::default(),
        };
        stroke.measurement_label = Some(stroke.measurement_label_bounds(
            Point::new(self.image_width, self.image_height),
            label_text_width,
        ));
        Some(stroke)
    }

    pub fn add_measurement(&mut self, stroke: Stroke) {
        if !matches!(stroke.tool, Tool::Measure(_)) || stroke.measurement_label.is_none() {
            return;
        }
        self.add_stamp(stroke);
        self.selected = None;
    }
}
