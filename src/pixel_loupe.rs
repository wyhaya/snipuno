use crate::{selection::paint_dimension_label, theme};
use gpui::{
    App, BorderStyle, Bounds, Corners, Pixels, Point, Window, fill, outline, point, px, rgb, size,
};

pub fn paint(
    pointer: Point<Pixels>,
    available: Bounds<Pixels>,
    label: String,
    pixel_at: impl FnMut(i32, i32) -> u32,
    window: &mut Window,
    cx: &mut App,
) {
    const EXTENT: f32 = 110.0;
    let origin = magnifier_origin(pointer, available, EXTENT);
    let frame = Bounds::new(origin, size(px(EXTENT), px(EXTENT)));
    paint_in_bounds(frame, pixel_at, window);
    paint_dimension_label(label, frame, available, window, cx);
}

pub fn paint_in_bounds(
    frame: Bounds<Pixels>,
    mut pixel_at: impl FnMut(i32, i32) -> u32,
    window: &mut Window,
) {
    const CELL: f32 = 10.0;
    const RADIUS: f32 = theme::DIMENSION_RADIUS;
    let columns = (f32::from(frame.size.width) / CELL).ceil() as i32 | 1;
    let rows = (f32::from(frame.size.height) / CELL).ceil() as i32 | 1;
    let x = f32::from(frame.center().x) - columns as f32 * CELL / 2.0;
    let y = f32::from(frame.center().y) - rows as f32 * CELL / 2.0;
    for row in 0..rows {
        for col in 0..columns {
            let color = pixel_at(col - columns / 2, row - rows / 2);
            let mut cell = fill(
                Bounds::new(
                    point(px(x + col as f32 * CELL), px(y + row as f32 * CELL)),
                    size(px(CELL), px(CELL)),
                )
                .intersect(&frame),
                rgb(color),
            );
            cell.corner_radii = Corners {
                top_left: px(if row == 0 && col == 0 { RADIUS } else { 0.0 }),
                top_right: px(if row == 0 && col == columns - 1 {
                    RADIUS
                } else {
                    0.0
                }),
                bottom_left: px(if row == rows - 1 && col == 0 {
                    RADIUS
                } else {
                    0.0
                }),
                bottom_right: px(if row == rows - 1 && col == columns - 1 {
                    RADIUS
                } else {
                    0.0
                }),
            };
            window.paint_quad(cell);
        }
    }
    let selected = Bounds::new(
        point(
            px(x + (columns / 2) as f32 * CELL),
            px(y + (rows / 2) as f32 * CELL),
        ),
        size(px(CELL), px(CELL)),
    );
    let mut frame_border = outline(frame, rgb(theme::BRAND), BorderStyle::Solid);
    frame_border.corner_radii = px(RADIUS).into();
    frame_border.border_widths = px(1.0).into();
    window.paint_quad(frame_border);
    let mut selected_border = outline(selected, rgb(theme::BRAND), BorderStyle::Solid);
    selected_border.border_widths = px(1.0).into();
    window.paint_quad(selected_border);
}

fn magnifier_origin(
    pointer: Point<Pixels>,
    available: Bounds<Pixels>,
    extent: f32,
) -> Point<Pixels> {
    const GAP: f32 = 20.0;
    const MARGIN: f32 = 4.0;
    const LABEL_SPACE: f32 = 36.0;
    let left = f32::from(available.left()) + MARGIN;
    let right = f32::from(available.right()) - MARGIN;
    let top = f32::from(available.top()) + MARGIN;
    let bottom = f32::from(available.bottom()) - MARGIN;
    let pointer_x = f32::from(pointer.x);
    let pointer_y = f32::from(pointer.y);
    let x = if pointer_x + GAP + extent <= right {
        pointer_x + GAP
    } else {
        pointer_x - GAP - extent
    };
    let y = if pointer_y + GAP + extent + LABEL_SPACE <= bottom {
        pointer_y + GAP
    } else {
        pointer_y - GAP - extent
    };
    let max_y = (bottom - extent).max(top);
    let min_y = (top + LABEL_SPACE).min(max_y);
    point(
        px(x.clamp(left, (right - extent).max(left))),
        px(y.clamp(min_y, max_y)),
    )
}
