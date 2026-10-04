use crate::dimensions::PixelGrid;
use crate::editor::{Bounds, Point, ResizeHandle};
use crate::theme::{HANDLE_HIT_RADIUS, SELECTION_BORDER_WIDTH};

#[derive(Clone)]
pub struct CropSelection {
    bounds: Bounds,
    canvas: Point,
    drag: Option<Drag>,
    grid: PixelGrid,
}

#[derive(Clone, Copy)]
struct Drag {
    start: Point,
    before: Bounds,
    kind: DragKind,
}

#[derive(Clone, Copy)]
enum DragKind {
    Draw,
    Move,
    Resize(ResizeHandle),
}

impl CropSelection {
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            bounds: Bounds {
                x: 0.0,
                y: 0.0,
                width,
                height,
            },
            canvas: Point::new(width, height),
            drag: None,
            grid: PixelGrid::default(),
        }
    }

    pub fn set_grid(&mut self, grid: PixelGrid) {
        self.cancel_drag();
        self.grid = grid;
    }

    pub fn bounds(&self) -> Bounds {
        self.bounds
    }

    pub fn can_apply(&self) -> bool {
        self.drag.is_none()
            && self.bounds.width >= 1.0
            && self.bounds.height >= 1.0
            && (self.bounds.width < self.canvas.x || self.bounds.height < self.canvas.y)
    }

    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    pub fn handles(&self) -> [(ResizeHandle, Point); 8] {
        let Bounds {
            x,
            y,
            width,
            height,
        } = self.bounds;
        [
            (ResizeHandle::TopLeft, Point::new(x, y)),
            (ResizeHandle::TopRight, Point::new(x + width, y)),
            (ResizeHandle::BottomRight, Point::new(x + width, y + height)),
            (ResizeHandle::BottomLeft, Point::new(x, y + height)),
            (ResizeHandle::Top, Point::new(x + width / 2.0, y)),
            (ResizeHandle::Right, Point::new(x + width, y + height / 2.0)),
            (
                ResizeHandle::Bottom,
                Point::new(x + width / 2.0, y + height),
            ),
            (ResizeHandle::Left, Point::new(x, y + height / 2.0)),
        ]
    }

    pub fn selection_bounds(&self, scale: f32) -> Bounds {
        let padding = SELECTION_BORDER_WIDTH / scale;
        Bounds {
            x: self.bounds.x - padding,
            y: self.bounds.y - padding,
            width: self.bounds.width + 2.0 * padding,
            height: self.bounds.height + 2.0 * padding,
        }
    }

    pub fn selection_handles(&self, scale: f32) -> [(ResizeHandle, Point); 8] {
        let padding = SELECTION_BORDER_WIDTH / scale;
        self.handles().map(|(handle, position)| {
            let (x, y) = handle.axes();
            (
                handle,
                Point::new(
                    position.x + x as f32 * padding,
                    position.y + y as f32 * padding,
                ),
            )
        })
    }

    pub fn handle_at(&self, point: Point, scale: f32) -> Option<ResizeHandle> {
        if scale <= 0.0 || !scale.is_finite() {
            return None;
        }
        self.selection_handles(scale)
            .into_iter()
            .find_map(|(handle, position)| {
                ((position.x - point.x).hypot(position.y - point.y) * scale <= HANDLE_HIT_RADIUS)
                    .then_some(handle)
            })
            .or_else(|| {
                self.bounds.resize_edge_at(
                    point,
                    HANDLE_HIT_RADIUS / scale,
                    self.canvas,
                    SELECTION_BORDER_WIDTH / scale,
                )
            })
    }

    pub fn contains(&self, point: Point) -> bool {
        point.x >= self.bounds.x
            && point.y >= self.bounds.y
            && point.x <= self.bounds.x + self.bounds.width
            && point.y <= self.bounds.y + self.bounds.height
    }

    pub fn begin(&mut self, point: Point, scale: f32) {
        let kind = if let Some(handle) = self.handle_at(point, scale) {
            DragKind::Resize(handle)
        } else if self.can_apply() && self.contains(point) {
            DragKind::Move
        } else {
            DragKind::Draw
        };
        let start = if matches!(kind, DragKind::Draw) {
            self.clamp(point)
        } else {
            point
        };
        self.drag = Some(Drag {
            start,
            before: self.bounds,
            kind,
        });
        if matches!(kind, DragKind::Draw) {
            self.bounds = Bounds {
                x: start.x,
                y: start.y,
                width: 0.0,
                height: 0.0,
            };
        }
    }

    pub fn update(&mut self, point: Point, constrained: bool) {
        let Some(drag) = self.drag else {
            return;
        };
        let point = if matches!(drag.kind, DragKind::Draw) {
            self.clamp(point)
        } else {
            point
        };
        let delta = Point::new(point.x - drag.start.x, point.y - drag.start.y).snapped(self.grid);
        if !matches!(drag.kind, DragKind::Draw) && delta == Point::default() {
            self.bounds = drag.before;
            return;
        }
        match drag.kind {
            DragKind::Move => {
                self.bounds = drag.before;
                self.translate(delta);
            }
            DragKind::Draw => {
                let mut end = point;
                if constrained {
                    let dx = end.x - drag.start.x;
                    let dy = end.y - drag.start.y;
                    let sx = if dx < 0.0 { -1.0 } else { 1.0 };
                    let sy = if dy < 0.0 { -1.0 } else { 1.0 };
                    let room_x = if sx < 0.0 {
                        drag.start.x
                    } else {
                        self.canvas.x - drag.start.x
                    };
                    let room_y = if sy < 0.0 {
                        drag.start.y
                    } else {
                        self.canvas.y - drag.start.y
                    };
                    let side = dx.abs().max(dy.abs()).min(room_x).min(room_y);
                    end = self.clamp(Point::new(
                        drag.start.x + side * sx,
                        drag.start.y + side * sy,
                    ));
                }
                self.bounds = between(drag.start, end);
            }
            DragKind::Resize(handle) => {
                let b = drag.before;
                let dx = delta.x;
                let dy = delta.y;
                if constrained {
                    self.resize_proportionally(b, handle, Point::new(dx, dy));
                    return;
                }
                let (mut left, mut top, mut right, mut bottom) =
                    (b.x, b.y, b.x + b.width, b.y + b.height);
                match handle {
                    ResizeHandle::TopLeft => {
                        left += dx;
                        top += dy;
                    }
                    ResizeHandle::TopRight => {
                        right += dx;
                        top += dy;
                    }
                    ResizeHandle::BottomLeft => {
                        left += dx;
                        bottom += dy;
                    }
                    ResizeHandle::BottomRight => {
                        right += dx;
                        bottom += dy;
                    }
                    ResizeHandle::Top => top += dy,
                    ResizeHandle::Right => right += dx,
                    ResizeHandle::Bottom => bottom += dy,
                    ResizeHandle::Left => left += dx,
                    _ => return,
                }
                self.bounds = between(
                    self.clamp(Point::new(left, top)),
                    self.clamp(Point::new(right, bottom)),
                );
            }
        }
    }

    fn resize_proportionally(&mut self, before: Bounds, handle: ResizeHandle, delta: Point) {
        let (x, y) = handle.axes();
        let (x, y) = (x as f32, y as f32);
        let anchor = Point::new(
            before.x + before.width * (1.0 - x) / 2.0,
            before.y + before.height * (1.0 - y) / 2.0,
        );
        let sx = 1.0 + x * delta.x / before.width;
        let sy = 1.0 + y * delta.y / before.height;
        let factor = if x == 0.0 {
            sy.abs()
        } else if y == 0.0 {
            sx.abs()
        } else {
            sx.abs().max(sy.abs())
        };
        let direction_x = x * if sx < 0.0 { -1.0 } else { 1.0 };
        let direction_y = y * if sy < 0.0 { -1.0 } else { 1.0 };
        let room = |anchor: f32, limit: f32, direction: f32| {
            if direction < 0.0 {
                anchor
            } else if direction > 0.0 {
                limit - anchor
            } else {
                anchor.min(limit - anchor) * 2.0
            }
        };
        let maximum = (room(anchor.x, self.canvas.x, direction_x) / before.width)
            .min(room(anchor.y, self.canvas.y, direction_y) / before.height);
        let factor = factor
            .max((1.0 / before.width).max(1.0 / before.height))
            .min(maximum);
        let width = before.width * factor;
        let height = before.height * factor;
        let left = anchor.x
            + if direction_x < 0.0 {
                -width
            } else if direction_x == 0.0 {
                -width / 2.0
            } else {
                0.0
            };
        let top = anchor.y
            + if direction_y < 0.0 {
                -height
            } else if direction_y == 0.0 {
                -height / 2.0
            } else {
                0.0
            };
        self.bounds = Bounds {
            x: left,
            y: top,
            width,
            height,
        }
        .snapped_within(
            self.grid,
            Bounds {
                x: 0.0,
                y: 0.0,
                width: self.canvas.x,
                height: self.canvas.y,
            },
        );
    }

    pub fn finish(&mut self, point: Point, scale: f32, square: bool) {
        self.update(point, square);
        let Some(drag) = self.drag.take() else {
            return;
        };
        if self.bounds.width < 1.0
            || self.bounds.height < 1.0
            || (matches!(drag.kind, DragKind::Draw)
                && (self.bounds.width * scale < 3.0 || self.bounds.height * scale < 3.0))
        {
            self.bounds = drag.before;
        }
    }

    pub fn cancel_drag(&mut self) -> bool {
        if let Some(drag) = self.drag.take() {
            self.bounds = drag.before;
            true
        } else {
            false
        }
    }

    pub fn nudge(&mut self, delta: Point) {
        if self.drag.is_none() {
            self.translate(delta);
        }
    }

    fn translate(&mut self, delta: Point) {
        self.bounds.x = self.grid.clamp_x(
            f64::from(self.bounds.x + delta.x),
            0.0,
            f64::from(self.canvas.x - self.bounds.width),
        ) as f32;
        self.bounds.y = self.grid.clamp_y(
            f64::from(self.bounds.y + delta.y),
            0.0,
            f64::from(self.canvas.y - self.bounds.height),
        ) as f32;
    }

    fn clamp(&self, point: Point) -> Point {
        point.snapped_within(
            self.grid,
            Bounds {
                x: 0.0,
                y: 0.0,
                width: self.canvas.x,
                height: self.canvas.y,
            },
        )
    }
}

fn between(a: Point, b: Point) -> Bounds {
    Bounds {
        x: a.x.min(b.x),
        y: a.y.min(b.y),
        width: (b.x - a.x).abs(),
        height: (b.y - a.y).abs(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::bounds;

    #[test]
    fn crop_draw_snaps_reverse_drags_and_rejects_tiny_clicks() {
        let mut crop = CropSelection::new(100.0, 80.0);
        let original = crop.bounds();
        crop.begin(Point::new(50.0, 40.0), 1.0);
        crop.finish(Point::new(51.0, 41.0), 1.0, false);
        assert_eq!(crop.bounds(), original);
        assert!(!crop.can_apply());
        crop.begin(Point::new(70.4, 60.1), 1.0);
        crop.finish(Point::new(-20.0, 10.2), 1.0, false);
        assert_eq!(crop.bounds(), bounds(0.0, 10.0, 70.0, 50.0));
        assert!(crop.can_apply());
    }

    #[test]
    fn crop_move_clamps_to_canvas_and_cancel_restores_selection() {
        let mut crop = CropSelection::new(100.0, 80.0);
        crop.begin(Point::new(20.0, 10.0), 1.0);
        crop.finish(Point::new(80.0, 60.0), 1.0, false);
        let before = crop.bounds();
        crop.begin(Point::new(45.0, 35.0), 1.0);
        crop.update(Point::new(120.0, 120.0), false);
        assert_eq!(crop.bounds(), bounds(40.0, 30.0, 60.0, 50.0));
        assert!(crop.cancel_drag());
        assert_eq!(crop.bounds(), before);
        crop.nudge(Point::new(-100.0, -100.0));
        assert_eq!(crop.bounds(), bounds(0.0, 0.0, 60.0, 50.0));
    }

    #[test]
    fn crop_resize_crosses_the_anchor_and_shift_can_be_released() {
        let mut crop = CropSelection::new(400.0, 300.0);
        crop.begin(Point::new(100.0, 80.0), 1.0);
        crop.finish(Point::new(220.0, 140.0), 1.0, false);
        let before = crop.bounds();
        crop.begin(Point::new(220.0, 140.0), 1.0);
        crop.update(Point::new(260.0, 150.0), true);
        assert_eq!(crop.bounds(), bounds(100.0, 80.0, 160.0, 80.0));
        crop.update(Point::new(260.0, 150.0), false);
        assert_eq!(crop.bounds(), bounds(100.0, 80.0, 160.0, 70.0));
        assert!(crop.cancel_drag());
        assert_eq!(crop.bounds(), before);
        crop.begin(Point::new(100.0, 80.0), 1.0);
        crop.finish(Point::new(280.0, 180.0), 1.0, true);
        assert_eq!(crop.bounds(), bounds(220.0, 140.0, 80.0, 40.0));
    }
}
