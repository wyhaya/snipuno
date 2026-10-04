use crate::{
    canvas::ImageViewport,
    dimensions::{DimensionDisplay, number},
    editor::{Bounds as ImageBounds, Point as ImagePoint, Stroke, Tool},
    theme,
};
use gpui::{
    App, BorderStyle, Bounds, Hsla, Pixels, Point, ShapedLine, SharedString, TextAlign, TextRun,
    TruncateFrom, Window, fill, outline, point, px, rgb, size,
};

pub fn paint_frame(bounds: Bounds<Pixels>, rounded: bool, window: &mut Window) {
    let mut frame = outline(bounds, rgb(theme::BRAND), BorderStyle::Solid);
    if rounded {
        frame.corner_radii = px(theme::SELECTION_RADIUS).into();
    }
    frame.border_widths = px(theme::SELECTION_BORDER_WIDTH).into();
    window.paint_quad(frame);
}

pub fn paint_handle(position: Point<Pixels>, window: &mut Window) {
    for (width, color) in [
        (
            theme::HANDLE_SIZE + theme::HANDLE_BORDER_WIDTH * 2.0,
            theme::BRAND,
        ),
        (theme::HANDLE_SIZE, theme::SURFACE),
    ] {
        let mut cap = fill(
            Bounds::new(
                position - point(px(width / 2.0), px(width / 2.0)),
                size(px(width), px(width)),
            ),
            rgb(color),
        );
        cap.corner_radii = px(width / 2.0).into();
        window.paint_quad(cap);
    }
}

pub fn paint_stroke_label(
    stroke: &Stroke,
    viewport: ImageViewport,
    dimensions: DimensionDisplay,
    window: &mut Window,
    cx: &mut App,
) {
    let text = match stroke.tool {
        Tool::Magnifier => {
            let Some(magnifier) = &stroke.magnifier else {
                return;
            };
            let zoom = format!("{:.1}", magnifier.zoom);
            format!("{}x", zoom.trim_end_matches(".0"))
        }
        Tool::Image(_) => size_label(stroke.bounds(), dimensions),
        _ => return,
    };
    let frame = stroke.selection_bounds(viewport.scale);
    paint_label(text, frame, viewport, window, cx);
}

pub fn paint_dimensions(
    bounds: ImageBounds,
    frame: ImageBounds,
    viewport: ImageViewport,
    dimensions: DimensionDisplay,
    window: &mut Window,
    cx: &mut App,
) {
    paint_label(size_label(bounds, dimensions), frame, viewport, window, cx);
}

fn size_label(bounds: ImageBounds, dimensions: DimensionDisplay) -> String {
    if bounds.height.round() == 0.0 {
        number(dimensions.width(f64::from(bounds.width)))
    } else if bounds.width.round() == 0.0 {
        number(dimensions.height(f64::from(bounds.height)))
    } else {
        dimensions.size(f64::from(bounds.width), f64::from(bounds.height))
    }
}

pub fn shape_dimension_label(
    text: SharedString,
    font_size: Pixels,
    color: Hsla,
    window: &Window,
) -> ShapedLine {
    let run = TextRun {
        len: text.len(),
        font: window.text_style().font(),
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    window
        .text_system()
        .shape_line(text, font_size, &[run], None)
}

fn paint_label(
    text: String,
    frame: ImageBounds,
    viewport: ImageViewport,
    window: &mut Window,
    cx: &mut App,
) {
    if viewport.scale <= 0.0 || !viewport.scale.is_finite() {
        return;
    }
    let frame = Bounds::from_corners(
        viewport.to_window(ImagePoint::new(frame.x, frame.y)),
        viewport.to_window(ImagePoint::new(
            frame.x + frame.width,
            frame.y + frame.height,
        )),
    );
    let available = Bounds::from_corners(
        point(px(0.0), px(theme::TOOLBAR_HEIGHT)),
        point(window.viewport_size().width, window.viewport_size().height),
    );
    paint_dimension_label(text, frame, available, window, cx);
}

pub fn paint_dimension_label(
    text: String,
    frame: Bounds<Pixels>,
    available: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    paint_selection_label(&[text], frame, available, window, cx);
}

pub fn paint_selection_label(
    text: &[String],
    frame: Bounds<Pixels>,
    available: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    if text.is_empty()
        || frame.right() < available.left()
        || frame.bottom() < available.top()
        || frame.left() > available.right()
        || frame.top() > available.bottom()
    {
        return;
    }
    let font_size = px(theme::DIMENSION_FONT_SIZE);
    let line_height = px(theme::DIMENSION_LINE_HEIGHT);
    let padding = px(theme::DIMENSION_PADDING);
    let gap = px(theme::HANDLE_SIZE / 2.0 + theme::HANDLE_BORDER_WIDTH + 1.0);
    let max_width = (available.size.width - (gap + padding) * 2.0)
        .max(px(0.0))
        .min(px(480.0));
    let mut wrapper = window
        .text_system()
        .line_wrapper(window.text_style().font(), font_size);
    let lines: Vec<_> = text
        .iter()
        .map(|text| {
            let (text, _) =
                wrapper.truncate_line(text.clone().into(), max_width, "…", &[], TruncateFrom::End);
            shape_dimension_label(text, font_size, rgb(theme::SURFACE).into(), window)
        })
        .collect();
    let width = lines
        .iter()
        .fold(px(0.0), |width, line| width.max(line.width()))
        + padding * 2.0;
    let height = line_height * lines.len() as f32 + padding * 2.0;
    let outside_x = frame
        .left()
        .max(available.left() + gap)
        .min(available.right() - width - gap);
    let outside_top = frame.top() - height - gap;
    let outside_fits_width = width + gap * 2.0 <= available.size.width;
    let origin = if outside_fits_width && outside_top >= available.top() + gap {
        point(outside_x, outside_top)
    } else {
        point(
            (frame.left().max(available.left()) + gap)
                .min((available.right() - width).max(available.left())),
            (frame.top().max(available.top()) + gap)
                .min((available.bottom() - height).max(available.top())),
        )
    };
    let mut background = fill(Bounds::new(origin, size(width, height)), rgb(theme::BRAND));
    background.corner_radii = px(theme::DIMENSION_RADIUS).into();
    window.paint_quad(background);
    for (index, line) in lines.into_iter().enumerate() {
        if let Err(error) = line.paint(
            origin + point(padding, padding + line_height * index as f32),
            line_height,
            TextAlign::Left,
            None,
            window,
            cx,
        ) {
            eprintln!("Failed to display selection label: {error}");
        }
    }
}
