use super::*;

const MIN_IMAGE_SIZE: f64 = 24.0;
const MAX_IMAGE_CANVAS_RATIO: f64 = 0.8;

impl Editor {
    pub fn next_number(&self) -> Option<u32> {
        self.strokes
            .iter()
            .filter_map(|stroke| match stroke.tool {
                Tool::Number(number) => Some(number),
                _ => None,
            })
            .max()
            .unwrap_or(0)
            .checked_add(1)
    }

    pub fn number_preview(
        &self,
        center: Point,
        diameter: f32,
        color: u32,
        tail: TailDirection,
    ) -> Option<Stroke> {
        if !diameter.is_finite()
            || diameter <= 0.0
            || !center.x.is_finite()
            || !center.y.is_finite()
        {
            return None;
        }
        let number = self.next_number()?;
        let (start, end) = self.number_bounds(center, diameter);
        Some(Stroke {
            tool: Tool::Number(number),
            start,
            end,
            color,
            brush_auto_color: false,
            width: 0.0,
            points: Arc::default(),
            arrow_control: None,
            arrow_double_headed: false,
            fill_color: None,
            fill_opacity: 1.0,
            text: None,
            measurement_label: None,
            magnifier: None,
            number_tail: tail,
            effect_strength: EffectStrength::default(),
        })
    }

    pub fn add_number(
        &mut self,
        center: Point,
        diameter: f32,
        color: u32,
        tail: TailDirection,
    ) -> bool {
        let Some(stroke) = self.number_preview(center, diameter, color, tail) else {
            return false;
        };
        self.add_stamp(stroke);
        true
    }

    pub fn guide_preview(
        &self,
        orientation: GuideOrientation,
        position: Point,
        display_scale: f32,
    ) -> Option<Stroke> {
        if !position.x.is_finite()
            || !position.y.is_finite()
            || !display_scale.is_finite()
            || display_scale <= 0.0
        {
            return None;
        }
        let position = self.clamp(position);
        let (start, end) = match orientation {
            GuideOrientation::Horizontal => (
                Point::new(0.0, position.y),
                Point::new(self.image_width, position.y),
            ),
            GuideOrientation::Vertical => (
                Point::new(position.x, 0.0),
                Point::new(position.x, self.image_height),
            ),
        };
        Some(Stroke {
            tool: Tool::Guide(orientation),
            start,
            end,
            color: theme::GUIDE_COLOR,
            brush_auto_color: false,
            width: 1.0 / display_scale,
            points: Arc::default(),
            arrow_control: None,
            arrow_double_headed: false,
            fill_color: None,
            fill_opacity: 1.0,
            text: None,
            measurement_label: None,
            magnifier: None,
            number_tail: TailDirection::default(),
            effect_strength: EffectStrength::default(),
        })
    }

    pub fn guide_segment_preview(
        &self,
        orientation: GuideOrientation,
        interval: crate::measurement::Interval,
        display_scale: f32,
    ) -> Option<Stroke> {
        let aligned = match orientation {
            GuideOrientation::Horizontal => interval.start.y == interval.end.y,
            GuideOrientation::Vertical => interval.start.x == interval.end.x,
        };
        if !aligned || !interval.end.x.is_finite() || !interval.end.y.is_finite() {
            return None;
        }
        let mut stroke = self.guide_preview(orientation, interval.start, display_scale)?;
        stroke.start = self.clamp(interval.start);
        stroke.end = self.clamp(interval.end);
        (stroke.start != stroke.end).then_some(stroke)
    }

    pub fn add_guide_stroke(&mut self, stroke: Stroke) -> bool {
        let aligned = match stroke.tool {
            Tool::Guide(GuideOrientation::Horizontal) => stroke.start.y == stroke.end.y,
            Tool::Guide(GuideOrientation::Vertical) => stroke.start.x == stroke.end.x,
            _ => return false,
        };
        if !aligned || stroke.start == stroke.end {
            return false;
        }
        self.add_stamp(stroke);
        true
    }

    pub fn text_preview(
        &self,
        position: Point,
        text: TextAnnotation,
        color: u32,
        measured: Point,
    ) -> Option<Stroke> {
        self.text_stroke(position, text, color, measured)
    }

    pub fn selected_text_preview(
        &self,
        text: TextAnnotation,
        color: u32,
        measured: Point,
    ) -> Option<Stroke> {
        let stroke = self.selected().filter(|stroke| stroke.tool == Tool::Text)?;
        self.text_preview(stroke.start, text, color, measured)
    }

    pub(super) fn text_stroke(
        &self,
        position: Point,
        text: TextAnnotation,
        color: u32,
        measured: Point,
    ) -> Option<Stroke> {
        if text.content.trim().is_empty()
            || !text.font_size.is_finite()
            || text.font_size <= 0.0
            || !text.wrap_width.is_finite()
            || text.wrap_width <= 0.0
            || !position.x.is_finite()
            || !position.y.is_finite()
            || !measured.x.is_finite()
            || !measured.y.is_finite()
            || measured.x <= 0.0
            || measured.y <= 0.0
            || !(position.x + measured.x).is_finite()
            || !(position.y + measured.y).is_finite()
        {
            return None;
        }
        let measured = Point::new(
            ((f64::from(measured.x) / self.grid.x).ceil() * self.grid.x) as f32,
            ((f64::from(measured.y) / self.grid.y).ceil() * self.grid.y) as f32,
        );
        Some(Stroke {
            tool: Tool::Text,
            start: position,
            end: Point::new(position.x + measured.x, position.y + measured.y),
            color,
            brush_auto_color: false,
            width: 0.0,
            points: Arc::default(),
            effect_strength: EffectStrength::default(),
            arrow_control: None,
            arrow_double_headed: false,
            fill_color: None,
            fill_opacity: 1.0,
            text: Some(text),
            measurement_label: None,
            magnifier: None,
            number_tail: TailDirection::default(),
        })
    }

    pub fn add_text(
        &mut self,
        position: Point,
        text: TextAnnotation,
        color: u32,
        measured: Point,
    ) -> bool {
        let Some(stroke) = self.text_preview(position, text, color, measured) else {
            return false;
        };
        self.add_stamp(stroke);
        true
    }

    pub fn set_selected_text(&mut self, text: TextAnnotation, color: u32, measured: Point) -> bool {
        let Some(changed) = self.selected_text_preview(text, color, measured) else {
            return false;
        };
        self.change_selected(|stroke| *stroke = changed)
    }

    pub(super) fn add_stamp(&mut self, stroke: Stroke) {
        self.deselect();
        self.strokes.push(stroke.clone());
        self.record_edit(Edit::Add(stroke));
        self.selected = Some(self.strokes.len() - 1);
    }

    pub(super) fn number_bounds(&self, center: Point, diameter: f32) -> (Point, Point) {
        let diameter = self.grid.clamp_x(
            f64::from(diameter),
            self.grid
                .x
                .min(f64::from(self.image_width.min(self.image_height))),
            f64::from(self.image_width.min(self.image_height)),
        ) as f32;
        let start = Point::new(
            self.grid.clamp_x(
                f64::from(center.x - diameter / 2.0),
                0.0,
                f64::from(self.image_width - diameter),
            ) as f32,
            self.grid.clamp_y(
                f64::from(center.y - diameter / 2.0),
                0.0,
                f64::from(self.image_height - diameter),
            ) as f32,
        );
        (start, Point::new(start.x + diameter, start.y + diameter))
    }

    pub fn set_number_tail(&mut self, tail: TailDirection) -> bool {
        self.change_selected(|stroke| {
            if matches!(stroke.tool, Tool::Number(_)) {
                stroke.number_tail = tail;
            }
        })
    }

    pub fn add_image(&mut self, id: u64, width: f32, height: f32) -> bool {
        self.add_image_at(
            id,
            width,
            height,
            Point::new(self.image_width / 2.0, self.image_height / 2.0),
        )
    }

    pub fn add_image_at(&mut self, id: u64, width: f32, height: f32, center: Point) -> bool {
        if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
            return false;
        }
        let width = f64::from(width);
        let height = f64::from(height);
        let max_width = f64::from(self.image_width) * MAX_IMAGE_CANVAS_RATIO;
        let max_height = f64::from(self.image_height) * MAX_IMAGE_CANVAS_RATIO;
        let min_scale = MIN_IMAGE_SIZE / width.min(height);
        let max_scale = (max_width / width).min(max_height / height);
        let scale = min_scale.max(1.0).min(max_scale);
        let width = (width * scale) as f32;
        let height = (height * scale) as f32;
        let center = self.clamp(center);
        let start = Point::new(
            self.grid.clamp_x(
                f64::from(center.x - width / 2.0),
                0.0,
                f64::from(self.image_width - width),
            ) as f32,
            self.grid.clamp_y(
                f64::from(center.y - height / 2.0),
                0.0,
                f64::from(self.image_height - height),
            ) as f32,
        );
        let stroke = Stroke {
            tool: Tool::Image(id),
            start,
            end: Point::new(start.x + width, start.y + height),
            color: 0,
            brush_auto_color: false,
            width: 0.0,
            points: Arc::default(),
            arrow_control: None,
            arrow_double_headed: false,
            fill_color: None,
            fill_opacity: 1.0,
            text: None,
            measurement_label: None,
            magnifier: None,
            number_tail: TailDirection::default(),
            effect_strength: EffectStrength::default(),
        };
        self.add_stamp(stroke);
        true
    }
}
