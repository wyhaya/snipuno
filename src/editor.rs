//! Annotation geometry lives in image pixels, independently of the window size.

use crate::dimensions::PixelGrid;
use crate::magnifier::Magnifier;
use crate::theme::{self, HANDLE_HIT_RADIUS};
use std::sync::Arc;

mod brush;
mod bubble;
mod crop;
mod geometry;
mod history;
mod line;
mod measurement;
mod stamps;
mod stroke;
mod style;
mod transform;

pub use crate::measurement::MeasurementAxis;

pub use bubble::TailDirection;
use geometry::midpoint;
pub use geometry::{Bounds, Point, ResizeHandle, segment_distance};
use history::Edit;
pub use line::LineStyle;
use line::snap_line_endpoint;
pub use stroke::{EffectStrength, GuideOrientation, RedactionStyle, Stroke, TextAnnotation, Tool};

const MIN_DISPLAY_DRAG: f32 = 3.0;

#[derive(Clone, Debug)]
struct TransformDraft {
    index: usize,
    origin: Point,
    before: Stroke,
    handle: Option<ResizeHandle>,
    display_scale: f32,
}

#[derive(Clone, Debug)]
pub struct Editor {
    grid: PixelGrid,
    original_bounds: Bounds,
    source_bounds: Bounds,
    image_width: f32,
    image_height: f32,
    strokes: Vec<Stroke>,
    draft: Option<Stroke>,
    transforming: Option<TransformDraft>,
    style_preview: Option<(usize, Stroke)>,
    selected: Option<usize>,
    past: Vec<Edit>,
    future: Vec<Edit>,
    history_revision: u64,
}

impl Editor {
    pub fn new(image_width: f32, image_height: f32) -> Self {
        assert!(image_width.is_finite() && image_width > 0.0);
        assert!(image_height.is_finite() && image_height > 0.0);
        let original_bounds = Bounds {
            x: 0.0,
            y: 0.0,
            width: image_width,
            height: image_height,
        };
        Self {
            grid: PixelGrid::default(),
            original_bounds,
            source_bounds: original_bounds,
            image_width,
            image_height,
            strokes: Vec::new(),
            draft: None,
            transforming: None,
            style_preview: None,
            selected: None,
            past: Vec::new(),
            future: Vec::new(),
            history_revision: 0,
        }
    }

    pub fn set_grid(&mut self, grid: PixelGrid) {
        self.cancel();
        self.grid = grid;
    }

    pub fn strokes(&self) -> &[Stroke] {
        &self.strokes
    }

    pub fn draft(&self) -> Option<&Stroke> {
        self.draft.as_ref()
    }

    pub fn selected(&self) -> Option<&Stroke> {
        self.selected.and_then(|index| self.strokes.get(index))
    }

    pub fn deselect(&mut self) {
        self.finish_style_change();
        self.cancel();
        self.selected = None;
    }

    pub fn is_dragging(&self) -> bool {
        self.draft.is_some() || self.transforming.is_some()
    }

    pub fn annotation_count(&self) -> usize {
        self.strokes.len()
    }

    pub fn begin(&mut self, tool: Tool, point: Point, color: u32, width: f32) {
        if matches!(
            tool,
            Tool::Image(_) | Tool::Number(_) | Tool::Guide(_) | Tool::Measure(_) | Tool::Text
        ) {
            return;
        }
        self.deselect();
        let point = self.clamp(point);
        self.draft = Some(Stroke {
            tool,
            start: point,
            end: point,
            color,
            brush_auto_color: false,
            points: Arc::default(),
            arrow_control: None,
            arrow_double_headed: false,
            fill_color: None,
            fill_opacity: 1.0,
            text: None,
            measurement_label: None,
            magnifier: (tool == Tool::Magnifier).then(Magnifier::default),
            number_tail: TailDirection::default(),
            effect_strength: EffectStrength::default(),
            width: if width.is_finite() && width > 0.0 {
                width
            } else {
                1.0
            },
        });
        if tool.is_freehand() {
            let draft = self.draft.as_mut().unwrap();
            draft.push_path_point(point);
        }
    }

    pub fn update(&mut self, point: Point, display_scale: f32) {
        self.update_constrained(point, display_scale, false);
    }

    /// Constrains rectangles to squares and ellipses to circles while Shift is
    /// held. The shared side length is limited by the available image area.
    pub fn update_constrained(&mut self, point: Point, display_scale: f32, constrained: bool) {
        if self.transforming.is_some() {
            if self.active_resize_handle().is_some() {
                self.update_resize(point, constrained);
            } else {
                self.update_move(point);
            }
            return;
        }
        let mut point = self.clamp(point);
        if let Some(draft) = self.draft.as_ref()
            && matches!(draft.tool, Tool::Line(_))
        {
            point = snap_line_endpoint(draft.start, point, display_scale);
        }
        if let Some(draft) = self.draft.as_ref()
            && constrained
            && matches!(
                draft.tool,
                Tool::Rectangle
                    | Tool::RectangleFill
                    | Tool::Spotlight
                    | Tool::Magnifier
                    | Tool::Ellipse
                    | Tool::EllipseFill
                    | Tool::Redact(_)
            )
        {
            let start = draft.start;
            let dx = point.x - start.x;
            let dy = point.y - start.y;
            let sx = if dx < 0.0 { -1.0 } else { 1.0 };
            let sy = if dy < 0.0 { -1.0 } else { 1.0 };
            let room_x = if sx < 0.0 {
                start.x
            } else {
                self.image_width - start.x
            };
            let room_y = if sy < 0.0 {
                start.y
            } else {
                self.image_height - start.y
            };
            let side = dx.abs().max(dy.abs()).min(room_x).min(room_y);
            point = self.clamp(Point::new(start.x + sx * side, start.y + sy * side));
        }
        if let Some(draft) = self.draft.as_mut() {
            if draft.tool.is_freehand() {
                draft.push_path_point(point);
            } else {
                draft.end = point;
            }
        }
    }

    /// Commits a drag if it covers at least three screen pixels. Area shapes
    /// require both dimensions to reach that size. A rejected drag keeps redo.
    pub fn finish(&mut self, point: Point, display_scale: f32) -> bool {
        self.finish_constrained(point, display_scale, false)
    }

    pub fn finish_constrained(
        &mut self,
        point: Point,
        display_scale: f32,
        constrained: bool,
    ) -> bool {
        self.update_constrained(point, display_scale, constrained);
        if let Some(transforming) = self.transforming.take() {
            let after = self.strokes[transforming.index].clone();
            if after == transforming.before {
                return false;
            }
            self.record_edit(Edit::Transform {
                index: transforming.index,
                before: transforming.before,
                after,
            });
            return true;
        }
        let Some(mut stroke) = self.draft.take() else {
            return false;
        };
        if !stroke.is_large_enough(display_scale) {
            return false;
        }
        if stroke.tool == Tool::Brush {
            stroke.smooth_brush(display_scale);
        }
        self.strokes.push(stroke.clone());
        self.selected = Some(self.strokes.len() - 1);
        self.record_edit(Edit::Add(stroke));
        true
    }

    pub fn cancel(&mut self) -> bool {
        let canceled = self.draft.take().is_some();
        if let Some((index, before)) = self.style_preview.take() {
            self.strokes[index] = before;
            return true;
        }
        if let Some(transforming) = self.transforming.take() {
            self.strokes[transforming.index] = transforming.before;
            return true;
        }
        canceled
    }

    fn clamp(&self, point: Point) -> Point {
        point.snapped_within(
            self.grid,
            Bounds {
                x: 0.0,
                y: 0.0,
                width: self.image_width,
                height: self.image_height,
            },
        )
    }
}

#[cfg(test)]
mod tests;
