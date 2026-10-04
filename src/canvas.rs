//! Shared image layout and native GPUI annotation painting.

use crate::crop::CropSelection;
use crate::dimensions::DimensionDisplay;
use crate::editor::{
    Bounds as ImageBounds, LineStyle, Point as ImagePoint, RedactionStyle, Stroke, TextAnnotation,
    Tool,
};
use crate::magnifier::{Magnifier, PreviewCache};
use crate::mosaic::Mosaic;
use crate::theme::{self, ANNOTATION_RADIUS as CORNER_RADIUS, CANVAS_PADDING, HANDLE_SIZE};
use crate::{rounded, selection, spotlight};
use gpui::{
    App, Bounds, ContentMask, Corners, FontWeight, Hsla, Path, PathBuilder, PathStyle, Pixels,
    Point, RenderImage, ShapedLine, SharedString, TextAlign, TextRun, Window, WrappedLine, fill,
    point, px, rgb, rgba, size,
};
use gpui_base::Theme;
use lyon_path::{LineCap, LineJoin};
use std::slice;

#[derive(Clone, Copy, Debug)]
pub struct ImageViewport {
    /// Image bounds in window logical pixels, including the centering offset.
    pub bounds: Bounds<Pixels>,
    /// Window logical pixels per original image pixel.
    pub scale: f32,
}

impl ImageViewport {
    /// Fits an image inside the available canvas, with at most one display pixel
    /// per original image pixel.
    pub fn fit(
        image_width: f32,
        image_height: f32,
        available: Bounds<Pixels>,
        scale_factor: f32,
    ) -> Self {
        Self::fit_with_padding(
            image_width,
            image_height,
            available,
            CANVAS_PADDING,
            scale_factor.recip(),
        )
    }

    pub fn aligned(mut self, available: Bounds<Pixels>, alignment: Point<f32>) -> Self {
        let offset = |available: Pixels, image: Pixels, alignment: f32| {
            let room = (f32::from(available - image) - CANVAS_PADDING * 2.0).max(0.0);
            px(room * (alignment.clamp(0.0, 1.0) - 0.5))
        };
        self.bounds.origin.x += offset(available.size.width, self.bounds.size.width, alignment.x);
        self.bounds.origin.y += offset(available.size.height, self.bounds.size.height, alignment.y);
        self
    }

    fn fit_with_padding(
        image_width: f32,
        image_height: f32,
        available: Bounds<Pixels>,
        padding: f32,
        max_scale: f32,
    ) -> Self {
        let available_width = f32::from(available.size.width).max(0.0);
        let available_height = f32::from(available.size.height).max(0.0);
        let valid_image = image_width.is_finite()
            && image_height.is_finite()
            && image_width > 0.0
            && image_height > 0.0;
        let scale = if valid_image {
            ((available_width - padding * 2.0).max(0.0) / image_width)
                .min((available_height - padding * 2.0).max(0.0) / image_height)
                .min(max_scale)
        } else {
            0.0
        };
        let width = if valid_image {
            image_width * scale
        } else {
            0.0
        };
        let height = if valid_image {
            image_height * scale
        } else {
            0.0
        };
        Self {
            bounds: Bounds {
                origin: point(
                    available.origin.x + px((available_width - width) / 2.0),
                    available.origin.y + px((available_height - height) / 2.0),
                ),
                size: size(px(width), px(height)),
            },
            scale,
        }
    }

    pub fn contains(&self, position: Point<Pixels>) -> bool {
        self.scale > 0.0 && self.bounds.contains(&position)
    }

    /// Converts a window pointer position into original image coordinates.
    /// Out-of-image positions are preserved so Editor can clamp a dragged end.
    pub fn to_image(self, position: Point<Pixels>) -> ImagePoint {
        if self.scale <= 0.0 || !self.scale.is_finite() {
            return ImagePoint::default();
        }
        ImagePoint::new(
            f32::from(position.x - self.bounds.origin.x) / self.scale,
            f32::from(position.y - self.bounds.origin.y) / self.scale,
        )
    }

    pub fn to_window(self, position: ImagePoint) -> Point<Pixels> {
        point(
            self.bounds.origin.x + px(position.x * self.scale),
            self.bounds.origin.y + px(position.y * self.scale),
        )
    }

    pub fn source_bounds(&self, crop: ImageBounds, dimensions: (u32, u32)) -> Bounds<Pixels> {
        Bounds {
            origin: self.to_window(ImagePoint::new(-crop.x, -crop.y)),
            size: size(
                px(dimensions.0 as f32 * self.scale),
                px(dimensions.1 as f32 * self.scale),
            ),
        }
    }
}

/// Paints an annotation during GPUI's canvas paint phase. The content mask
/// clips the stroke and arrowhead to the image, including drags at its edges.
pub fn paint_stroke(
    stroke: &Stroke,
    viewport: ImageViewport,
    dimensions: DimensionDisplay,
    window: &mut Window,
    mosaic: &Mosaic,
    source_offset: ImagePoint,
    cx: &mut App,
) {
    if let Some(text) = &stroke.text {
        if let Ok(lines) = layout_text(text, stroke.color, viewport.scale, window, cx) {
            let offset = text.content_offset();
            let mut origin = viewport.to_window(ImagePoint::new(
                stroke.start.x + offset.x,
                stroke.start.y + offset.y,
            ));
            let line_height = px(text.line_height() * viewport.scale);
            window.with_content_mask(
                Some(ContentMask {
                    bounds: viewport.bounds,
                }),
                |window| {
                    if let Some(path) = text_bubble_path(
                        text,
                        window_bounds(stroke.bounds(), viewport),
                        viewport.scale,
                    ) {
                        window.paint_path(path, rgba(stroke.color.rotate_left(8)));
                    }
                    for line in lines {
                        let _ = line.paint(origin, line_height, TextAlign::Left, None, window, cx);
                        origin.y += line.size(line_height).height;
                    }
                },
            );
        }
        return;
    }
    if stroke.tool == Tool::Redact(RedactionStyle::Mosaic) {
        let bounds = stroke.bounds();
        let radius = CORNER_RADIUS / viewport.scale;
        let outline = rounded::polygon(bounds, radius);
        for (start, end, color) in mosaic.tiles_at(stroke, source_offset) {
            let tile = ImageBounds {
                x: start.x,
                y: start.y,
                width: end.x - start.x,
                height: end.y - start.y,
            };
            let corners = [
                start,
                ImagePoint::new(end.x, start.y),
                end,
                ImagePoint::new(start.x, end.y),
            ];
            if corners
                .iter()
                .all(|p| rounded::contains(bounds, *p, radius))
            {
                window.paint_quad(fill(window_bounds(tile, viewport), rgba(color)));
            } else {
                let points = rounded::clip_to_rect(&outline, tile);
                if points.len() >= 3 {
                    let mut path = PathBuilder::fill();
                    path.move_to(viewport.to_window(points[0]));
                    for point in points.iter().skip(1) {
                        path.line_to(viewport.to_window(*point));
                    }
                    path.close();
                    if let Ok(path) = path.build() {
                        window.paint_path(path, rgba(color));
                    }
                }
            }
        }
        return;
    }
    if stroke.tool == Tool::Brush {
        paint_brush(
            stroke,
            viewport,
            window,
            stroke.width * viewport.scale,
            rgba(stroke.color.rotate_left(8)).into(),
        );
        return;
    }
    let Some(path) = stroke_path(stroke, viewport) else {
        return;
    };
    // Editor stores AARRGGBB; GPUI's rgba() takes RRGGBBAA.
    let mut color = rgba(stroke.paint_color().rotate_left(8));
    color.a = stroke.paint_opacity();
    if stroke.tool == Tool::Spotlight {
        color = rgb(theme::DIMMING);
        color.a = spotlight::DIM_OPACITY;
    }
    window.with_content_mask(
        Some(ContentMask {
            bounds: viewport.bounds,
        }),
        |window| {
            window.paint_path(path, color);
            if let Tool::Number(number) = stroke.tool {
                paint_number(stroke, number, viewport, window, cx);
            }
            if let Some(bounds) = stroke.measurement_label {
                let mut background = fill(
                    window_bounds(bounds, viewport),
                    rgba(stroke.color.rotate_left(8)),
                );
                background.corner_radii =
                    px(theme::DIMENSION_RADIUS * stroke.width * viewport.scale).into();
                window.paint_quad(background);
                if let Some(label) = layout_measurement(stroke, viewport.scale, dimensions, window)
                {
                    let _ = label.line.paint(
                        viewport.to_window(label.origin),
                        px(label.line_height * viewport.scale),
                        TextAlign::Left,
                        None,
                        window,
                        cx,
                    );
                }
            }
        },
    );
}

fn paint_brush(
    stroke: &Stroke,
    viewport: ImageViewport,
    window: &mut Window,
    width: f32,
    color: Hsla,
) {
    if !viewport.scale.is_finite() || viewport.scale <= 0.0 {
        return;
    }
    let Some(path) = stroke_path_with_width(stroke, viewport, width) else {
        return;
    };
    window.with_content_mask(
        Some(ContentMask {
            bounds: viewport.bounds,
        }),
        |window| window.paint_path(path, color),
    );
}

pub(super) fn window_bounds(bounds: ImageBounds, viewport: ImageViewport) -> Bounds<Pixels> {
    Bounds {
        origin: viewport.to_window(ImagePoint::new(bounds.x, bounds.y)),
        size: size(
            px(bounds.width * viewport.scale),
            px(bounds.height * viewport.scale),
        ),
    }
}

pub fn paint_crop(
    crop: &CropSelection,
    viewport: ImageViewport,
    dimensions: DimensionDisplay,
    window: &mut Window,
    cx: &mut App,
) {
    if viewport.scale <= 0.0 || !viewport.scale.is_finite() {
        return;
    }
    let rect = window_bounds(crop.bounds(), viewport);
    window.paint_quad(fill(rect, rgba(0x00000088)));
    let mut grid_color = rgb(0xffffff);
    grid_color.a = 0.2;
    for fraction in [1.0 / 3.0, 2.0 / 3.0] {
        window.paint_quad(fill(
            Bounds::new(
                point(rect.origin.x + rect.size.width * fraction, rect.origin.y),
                size(px(1.0), rect.size.height),
            ),
            grid_color,
        ));
        window.paint_quad(fill(
            Bounds::new(
                point(rect.origin.x, rect.origin.y + rect.size.height * fraction),
                size(rect.size.width, px(1.0)),
            ),
            grid_color,
        ));
    }
    let frame = crop.selection_bounds(viewport.scale);
    selection::paint_frame(window_bounds(frame, viewport), true, window);
    for (_, handle) in crop.selection_handles(viewport.scale) {
        selection::paint_handle(viewport.to_window(handle), window);
    }
    selection::paint_dimensions(crop.bounds(), frame, viewport, dimensions, window, cx);
}

pub fn rounded_corners(bounds: Bounds<Pixels>) -> Corners<Pixels> {
    px(CORNER_RADIUS
        .min(f32::from(bounds.size.width) / 2.0)
        .min(f32::from(bounds.size.height) / 2.0))
    .into()
}

pub fn paint_spotlights(strokes: &[Stroke], viewport: ImageViewport, window: &mut Window) {
    let Some(opacity) = spotlight::opacity(strokes) else {
        return;
    };
    if let Some(path) = spotlight_path(strokes, viewport) {
        let mut color = rgb(theme::DIMMING);
        color.a = opacity;
        window.paint_path(path, color);
    }
}

pub(super) fn spotlight_path(strokes: &[Stroke], viewport: ImageViewport) -> Option<Path<Pixels>> {
    if !viewport.scale.is_finite() || viewport.scale <= 0.0 {
        return None;
    }
    let regions: Vec<_> = strokes
        .iter()
        .filter(|stroke| stroke.tool == Tool::Spotlight)
        .map(Stroke::bounds)
        .filter(|bounds| bounds.width > 0.0 && bounds.height > 0.0)
        .collect();
    if regions.is_empty() {
        return None;
    }
    let canvas = ImageBounds {
        x: 0.0,
        y: 0.0,
        width: f32::from(viewport.bounds.size.width) / viewport.scale,
        height: f32::from(viewport.bounds.size.height) / viewport.scale,
    };
    let mut path = PathBuilder::fill();
    for polygon in spotlight::mask(canvas, &regions, CORNER_RADIUS / viewport.scale) {
        path.move_to(viewport.to_window(polygon[0]));
        for point in &polygon[1..] {
            path.line_to(viewport.to_window(*point));
        }
        path.close();
    }
    path.build().ok()
}

pub fn paint_magnifier(
    stroke: &Stroke,
    viewport: ImageViewport,
    image: &RenderImage,
    source_offset: ImagePoint,
    previews: &mut PreviewCache,
    window: &mut Window,
) {
    let Some(magnifier) = &stroke.magnifier else {
        return;
    };
    let lens = stroke.bounds();
    if lens.width <= 0.0 || lens.height <= 0.0 || viewport.scale <= 0.0 {
        return;
    }
    for (path, color) in magnifier_shadow_paths(lens, viewport) {
        window.paint_path(path, rgba(color.rotate_left(8)));
    }
    let canvas = ImageBounds {
        x: 0.0,
        y: 0.0,
        width: f32::from(viewport.bounds.size.width) / viewport.scale,
        height: f32::from(viewport.bounds.size.height) / viewport.scale,
    };
    if let Some((preview, bounds)) = previews.image(
        magnifier,
        lens,
        canvas,
        source_offset,
        image,
        viewport.scale * window.scale_factor(),
    ) {
        let bounds = window_bounds(bounds, viewport);
        if let Err(error) =
            window.paint_image(bounds, bounds, Corners::default(), preview, 0, false)
        {
            eprintln!("Failed to display magnifier: {error}");
        }
    }
}

pub(super) fn magnifier_shadow_paths(
    mut bounds: ImageBounds,
    viewport: ImageViewport,
) -> impl Iterator<Item = (Path<Pixels>, u32)> {
    bounds.y += Magnifier::SHADOW_OFFSET / viewport.scale;
    Magnifier::SHADOW_LAYERS
        .into_iter()
        .filter_map(move |(width, alpha)| {
            ellipse_path(bounds, viewport, Some(width)).map(|path| (path, alpha << 24))
        })
}

pub(super) fn layout_text(
    text: &TextAnnotation,
    color: u32,
    scale: f32,
    window: &Window,
    cx: &App,
) -> Result<Vec<WrappedLine>, String> {
    let mut font = window.text_style().font();
    font.family = Theme::global(cx).tokens.typography.sans;
    let run = TextRun {
        len: text.content.len(),
        font,
        color: rgba(text.foreground_color(color).rotate_left(8)).into(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    window
        .text_system()
        .shape_text(
            text.content.clone().into(),
            px(text.font_size * scale),
            &[run],
            Some(px(text.content_width() * scale)),
            None,
        )
        .map(|lines| lines.into_iter().collect())
        .map_err(|error| error.to_string())
}

pub fn measure_text(
    text: &TextAnnotation,
    scale: f32,
    window: &Window,
    cx: &App,
) -> Option<ImagePoint> {
    let lines = layout_text(text, 0xff000000 | theme::TEXT, scale, window, cx).ok()?;
    let line_height = px(text.line_height() * scale);
    let width = lines
        .iter()
        .map(|line| f32::from(line.size(line_height).width))
        .fold(0.0, f32::max);
    let height: f32 = lines
        .iter()
        .map(|line| f32::from(line.size(line_height).height))
        .sum();
    let padding = text.padding();
    Some(ImagePoint::new(
        (width / scale).min(text.content_width()) + padding.x * 2.0 + text.tail_width(),
        height / scale + padding.y * 2.0,
    ))
}

pub(super) struct MeasurementLabelLayout {
    pub line: ShapedLine,
    pub origin: ImagePoint,
    pub font_size: f32,
    pub line_height: f32,
}

pub(super) fn prepare_measurement_label(
    stroke: &mut Stroke,
    dimensions: DimensionDisplay,
    image_size: ImagePoint,
    window: &Window,
) {
    if stroke.measurement_label.is_none() {
        return;
    }
    let Some(text) = stroke.measurement_text(dimensions) else {
        return;
    };
    let line = selection::shape_dimension_label(
        text.into(),
        px(theme::DIMENSION_FONT_SIZE * stroke.width),
        rgb(theme::TEXT).into(),
        window,
    );
    // TODO: After cropping, switching label sides can move a hidden measurement
    // label into the visible image. Preserve crop-relative placement instead.
    stroke.measurement_label =
        Some(stroke.measurement_label_bounds(image_size, f32::from(line.width())));
}

pub(super) fn layout_measurement(
    stroke: &Stroke,
    scale: f32,
    dimensions: DimensionDisplay,
    window: &Window,
) -> Option<MeasurementLabelLayout> {
    let bounds = stroke.measurement_label?;
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    let padding = theme::DIMENSION_PADDING * stroke.width;
    let line_height = theme::DIMENSION_LINE_HEIGHT * stroke.width;
    let font_size = theme::DIMENSION_FONT_SIZE * stroke.width;
    let line = selection::shape_dimension_label(
        stroke.measurement_text(dimensions)?.into(),
        px(font_size * scale),
        rgb(theme::TEXT).into(),
        window,
    );
    Some(MeasurementLabelLayout {
        line,
        origin: ImagePoint::new(bounds.x + padding, bounds.y + padding),
        font_size,
        line_height,
    })
}

pub(super) fn text_bubble_path(
    text: &TextAnnotation,
    bounds: Bounds<Pixels>,
    scale: f32,
) -> Option<Path<Pixels>> {
    if !text.bubble
        || !scale.is_finite()
        || scale <= 0.0
        || bounds.size.width <= px(0.0)
        || bounds.size.height <= px(0.0)
    {
        return None;
    }
    let tail = px(text.tail_width() * scale).min(bounds.size.width / 4.0);
    let radius = px(text.corner_radius() * scale)
        .min((bounds.size.width - tail) / 2.0)
        .min(bounds.size.height / 2.0);
    let left = bounds.origin.x + tail;
    let top = bounds.origin.y;
    let right = bounds.origin.x + bounds.size.width;
    let bottom = bounds.origin.y + bounds.size.height;
    let radii = point(radius, radius);
    let position = |x, y| {
        let x = if text.tail.is_left() {
            x
        } else {
            bounds.origin.x * 2.0 + bounds.size.width - x
        };
        let y = if text.tail.is_top() {
            bounds.origin.y * 2.0 + bounds.size.height - y
        } else {
            y
        };
        point(x, y)
    };
    let mut path = PathBuilder::fill();
    path.move_to(position(left + radius, top));
    path.line_to(position(right - radius, top));
    path.arc_to(
        radii,
        px(0.0),
        false,
        text.tail.clockwise(),
        position(right, top + radius),
    );
    path.line_to(position(right, bottom - radius));
    path.arc_to(
        radii,
        px(0.0),
        false,
        text.tail.clockwise(),
        position(right - radius, bottom),
    );
    path.line_to(position(left + radius, bottom));
    path.cubic_bezier_to(
        position(left + radius * 0.3, bottom - radius * 0.2),
        position(left + radius * 0.6, bottom),
        position(left + radius * 0.45, bottom - radius * 0.1),
    );
    path.cubic_bezier_to(
        position(bounds.origin.x, bottom),
        position(left + radius * 0.15, bottom - radius * 0.3),
        position(bounds.origin.x + tail * 0.6, bottom),
    );
    path.cubic_bezier_to(
        position(left, bottom - radius),
        position(bounds.origin.x + tail * 0.85, bottom - radius * 0.16),
        position(left, bottom - radius * 0.45),
    );
    path.line_to(position(left, top + radius));
    path.arc_to(
        radii,
        px(0.0),
        false,
        text.tail.clockwise(),
        position(left + radius, top),
    );
    path.close();
    path.build().ok()
}

fn rectangle_path(
    bounds: ImageBounds,
    viewport: ImageViewport,
    width: Option<f32>,
) -> Option<Path<Pixels>> {
    if bounds.width <= 0.0 || bounds.height <= 0.0 {
        return None;
    }
    let mut path = match width {
        Some(width) => PathBuilder::stroke(px(width)),
        None => PathBuilder::fill(),
    };
    let position = |x, y| viewport.to_window(ImagePoint::new(x, y));
    let r = (CORNER_RADIUS / viewport.scale)
        .min(bounds.width / 2.0)
        .min(bounds.height / 2.0);
    let (left, top, right, bottom) = (
        bounds.x,
        bounds.y,
        bounds.x + bounds.width,
        bounds.y + bounds.height,
    );
    let radii = point(px(r * viewport.scale), px(r * viewport.scale));
    path.move_to(position(left + r, top));
    path.line_to(position(right - r, top));
    path.arc_to(radii, px(0.0), false, true, position(right, top + r));
    path.line_to(position(right, bottom - r));
    path.arc_to(radii, px(0.0), false, true, position(right - r, bottom));
    path.line_to(position(left + r, bottom));
    path.arc_to(radii, px(0.0), false, true, position(left, bottom - r));
    path.line_to(position(left, top + r));
    path.arc_to(radii, px(0.0), false, true, position(left + r, top));
    path.close();
    path.build().ok()
}

fn ellipse_path(
    bounds: ImageBounds,
    viewport: ImageViewport,
    width: Option<f32>,
) -> Option<Path<Pixels>> {
    let middle_y = bounds.y + bounds.height / 2.0;
    let left = viewport.to_window(ImagePoint::new(bounds.x, middle_y));
    let right = viewport.to_window(ImagePoint::new(bounds.x + bounds.width, middle_y));
    let radii = point(
        px(bounds.width * viewport.scale / 2.0),
        px(bounds.height * viewport.scale / 2.0),
    );
    let mut path = match width {
        Some(width) => PathBuilder::stroke(px(width)),
        None => PathBuilder::fill(),
    };
    path.move_to(left);
    path.arc_to(radii, px(0.0), false, true, right);
    path.arc_to(radii, px(0.0), false, true, left);
    path.close();
    path.build().ok()
}

fn number_path(
    stroke: &Stroke,
    viewport: ImageViewport,
    width: Option<f32>,
) -> Option<Path<Pixels>> {
    let bounds = stroke.bounds();
    let [a, b, tip] = stroke.number_tail.number_points(bounds);
    let mut path = match width {
        Some(width) => PathBuilder::stroke(px(width)),
        None => PathBuilder::fill(),
    };
    path.move_to(viewport.to_window(a));
    path.arc_to(
        point(
            px(bounds.width * viewport.scale / 2.0),
            px(bounds.height * viewport.scale / 2.0),
        ),
        px(0.0),
        true,
        stroke.number_tail.clockwise(),
        viewport.to_window(b),
    );
    path.line_to(viewport.to_window(tip));
    path.close();
    path.build().ok()
}

fn paint_number(
    stroke: &Stroke,
    number: u32,
    viewport: ImageViewport,
    window: &mut Window,
    cx: &mut App,
) {
    let bounds = stroke.bounds();
    let diameter = bounds.width.min(bounds.height) * viewport.scale;
    if diameter <= 0.0 {
        return;
    }
    let text: SharedString = number.to_string().into();
    let mut font = window.text_style().font();
    font.weight = FontWeight::BOLD;
    font.family = Theme::global(cx).tokens.typography.mono;
    let color = rgb(theme::contrasting_text(stroke.color));
    let run = TextRun {
        len: text.len(),
        font,
        color: color.into(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let mut font_size = px(diameter * 0.55);
    let mut line =
        window
            .text_system()
            .shape_line(text.clone(), font_size, slice::from_ref(&run), None);
    let max_width = px(diameter * 0.76);
    if line.width() > max_width {
        font_size *= max_width / line.width();
        line = window
            .text_system()
            .shape_line(text, font_size, &[run], None);
    }
    let center = viewport.to_window(ImagePoint::new(
        bounds.x + bounds.width / 2.0,
        bounds.y + bounds.height / 2.0,
    ));
    let origin = center - point(line.width() / 2.0, font_size / 2.0);
    if let Err(error) = line.paint(origin, font_size, TextAlign::Left, None, window, cx) {
        eprintln!("Failed to display number: {error}");
    }
}

pub fn paint_hover(stroke: &Stroke, viewport: ImageViewport, window: &mut Window) {
    if !viewport.scale.is_finite() || viewport.scale <= 0.0 {
        return;
    }
    window.with_content_mask(
        Some(ContentMask {
            bounds: viewport.bounds,
        }),
        |window| {
            if matches!(
                stroke.tool,
                Tool::Arrow | Tool::Line(_) | Tool::Brush | Tool::Measure(_)
            ) {
                if let Some(path) = stroke_path_with_width(
                    stroke,
                    viewport,
                    stroke.width * viewport.scale + theme::HOVER_STROKE_EXPANSION,
                ) {
                    window.paint_path(path, theme::brand(theme::HOVER_STROKE_OPACITY));
                }
                return;
            }
            if matches!(stroke.tool, Tool::Number(_)) {
                if let Some(path) = number_path(stroke, viewport, None) {
                    window.paint_path(path, theme::brand(theme::HOVER_FILL_OPACITY));
                }
                if let Some(path) = number_path(stroke, viewport, Some(theme::HOVER_BORDER_WIDTH)) {
                    window.paint_path(path, theme::brand(theme::HOVER_BORDER_OPACITY));
                }
                return;
            }
            if matches!(stroke.tool, Tool::Guide(_)) {
                if let Some(path) = stroke_path_with_width(
                    stroke,
                    viewport,
                    stroke.display_width(viewport.scale) + theme::HOVER_STROKE_EXPANSION,
                ) {
                    window.paint_path(path, theme::brand(theme::HOVER_STROKE_OPACITY));
                }
                return;
            }
            let shape_path = if matches!(
                stroke.tool,
                Tool::Ellipse | Tool::EllipseFill | Tool::Magnifier
            ) {
                ellipse_path
            } else {
                rectangle_path
            };
            let bounds = stroke.bounds();
            if bounds.width <= 0.0 || bounds.height <= 0.0 {
                return;
            }
            if let Some(path) = shape_path(bounds, viewport, None) {
                window.paint_path(path, theme::brand(theme::HOVER_FILL_OPACITY));
            }
            if let Some(path) = shape_path(bounds, viewport, Some(theme::HOVER_BORDER_WIDTH)) {
                window.paint_path(path, theme::brand(theme::HOVER_BORDER_OPACITY));
            }
        },
    );
}

pub fn paint_selection(stroke: &Stroke, viewport: ImageViewport, window: &mut Window) {
    if !viewport.scale.is_finite() || viewport.scale <= 0.0 {
        return;
    }
    if stroke.tool == Tool::Brush {
        let width = stroke.width * viewport.scale;
        paint_brush(
            stroke,
            viewport,
            window,
            width + theme::SELECTION_BORDER_WIDTH * 2.0,
            theme::brand(1.0),
        );
        paint_brush(
            stroke,
            viewport,
            window,
            width,
            rgba(stroke.color.rotate_left(8)).into(),
        );
        return;
    }
    let original = stroke.bounds();
    let bounds = stroke.selection_bounds(viewport.scale);
    let margin = px(stroke.selection_padding(viewport.scale) * viewport.scale
        + HANDLE_SIZE / 2.0
        + theme::HANDLE_BORDER_WIDTH);
    let image = viewport.bounds;
    let left = if original.x < 0.0 {
        image.left()
    } else {
        image.left() - margin
    };
    let top = if original.y < 0.0 {
        image.top()
    } else {
        image.top() - margin
    };
    let right = if viewport
        .to_window(ImagePoint::new(original.x + original.width, 0.0))
        .x
        > image.right()
    {
        image.right()
    } else {
        image.right() + margin
    };
    let bottom = if viewport
        .to_window(ImagePoint::new(0.0, original.y + original.height))
        .y
        > image.bottom()
    {
        image.bottom()
    } else {
        image.bottom() + margin
    };
    window.with_content_mask(
        Some(ContentMask {
            bounds: Bounds::new(point(left, top), size(right - left, bottom - top)),
        }),
        |window| {
            if stroke.tool == Tool::Line(LineStyle::Dashed) {
                let mut selection = stroke.clone();
                selection.tool = Tool::Line(LineStyle::Solid);
                if let Some(path) =
                    stroke_path_with_width(&selection, viewport, theme::SELECTION_BORDER_WIDTH)
                {
                    window.paint_path(path, theme::brand(1.0));
                }
            } else if matches!(stroke.tool, Tool::Line(_) | Tool::Measure(_)) {
                if let Some(path) = stroke_path_with_width(
                    stroke,
                    viewport,
                    stroke.width * viewport.scale + theme::SELECTION_BORDER_WIDTH * 2.0,
                ) {
                    window.paint_path(path, theme::brand(1.0));
                }
                if let Some(path) = stroke_path(stroke, viewport) {
                    window.paint_path(path, rgba(stroke.color.rotate_left(8)));
                }
            } else if stroke.tool != Tool::Arrow {
                selection::paint_frame(
                    window_bounds(bounds, viewport),
                    !matches!(stroke.tool, Tool::Guide(_)),
                    window,
                );
            }
            for ((_, original), (_, position)) in stroke
                .resize_handles()
                .into_iter()
                .zip(stroke.selection_handles(viewport.scale))
            {
                if !viewport.bounds.contains(&viewport.to_window(original)) {
                    continue;
                }
                selection::paint_handle(viewport.to_window(position), window);
            }
        },
    );
}

pub(super) fn stroke_path(stroke: &Stroke, viewport: ImageViewport) -> Option<Path<Pixels>> {
    stroke_path_with_width(stroke, viewport, stroke.display_width(viewport.scale))
}

fn stroke_path_with_width(
    stroke: &Stroke,
    viewport: ImageViewport,
    width: f32,
) -> Option<Path<Pixels>> {
    if !viewport.scale.is_finite() || viewport.scale <= 0.0 {
        return None;
    }
    if stroke.tool == Tool::Spotlight {
        return spotlight_path(slice::from_ref(stroke), viewport);
    }
    let bounds = stroke.bounds();
    if stroke.tool == Tool::Text {
        return text_bubble_path(
            stroke.text.as_ref()?,
            window_bounds(bounds, viewport),
            viewport.scale,
        );
    }
    if match stroke.tool {
        Tool::Rectangle
        | Tool::RectangleFill
        | Tool::Magnifier
        | Tool::Ellipse
        | Tool::EllipseFill
        | Tool::Number(_) => bounds.width <= 0.0 || bounds.height <= 0.0,
        Tool::Arrow | Tool::Line(_) | Tool::Guide(_) | Tool::Measure(_) => {
            (stroke.end.x - stroke.start.x).hypot(stroke.end.y - stroke.start.y) <= f32::EPSILON
        }
        Tool::Brush => stroke.points.len() < 2,
        Tool::Redact(RedactionStyle::Blur)
        | Tool::Redact(RedactionStyle::Mosaic)
        | Tool::Image(_)
        | Tool::Text
        | Tool::Spotlight => true,
    } {
        return None;
    }
    let mut path = PathBuilder::stroke(px(width));
    if matches!(stroke.tool, Tool::Arrow | Tool::Line(_) | Tool::Brush)
        && let PathStyle::Stroke(options) = &mut path.style
    {
        *options = options
            .with_line_cap(LineCap::Round)
            .with_line_join(LineJoin::Round);
    }
    match stroke.tool {
        Tool::Measure(_) => {
            for (start, end) in stroke.measurement_segments() {
                path.move_to(viewport.to_window(start));
                path.line_to(viewport.to_window(end));
            }
        }
        Tool::Line(_) => {
            let mut previous = None;
            for (start, end) in stroke.line_segments() {
                if previous != Some(start) {
                    path.move_to(viewport.to_window(start));
                }
                path.line_to(viewport.to_window(end));
                previous = Some(end);
            }
        }
        Tool::Guide(_) => {
            path.move_to(viewport.to_window(stroke.start));
            path.line_to(viewport.to_window(stroke.end));
        }
        Tool::Brush => {
            path.move_to(viewport.to_window(stroke.points[0]));
            for point in &stroke.points[1..] {
                path.line_to(viewport.to_window(*point));
            }
        }
        Tool::Redact(RedactionStyle::Blur)
        | Tool::Redact(RedactionStyle::Mosaic)
        | Tool::Image(_)
        | Tool::Text
        | Tool::Spotlight => return None,
        Tool::Rectangle | Tool::RectangleFill => {
            return rectangle_path(
                bounds,
                viewport,
                (stroke.tool == Tool::Rectangle).then_some(width),
            );
        }
        Tool::Number(_) => return number_path(stroke, viewport, None),
        Tool::Ellipse | Tool::EllipseFill | Tool::Magnifier => {
            return ellipse_path(
                bounds,
                viewport,
                (!stroke.is_filled_shape()).then_some(width),
            );
        }
        Tool::Arrow => {
            let arrow = stroke.arrow_geometry();
            path.move_to(viewport.to_window(arrow.start));
            path.curve_to(
                viewport.to_window(arrow.tip),
                viewport.to_window(arrow.control),
            );
            for head in stroke.arrow_heads() {
                path.move_to(viewport.to_window(head.wing_a));
                path.line_to(viewport.to_window(head.tip));
                path.line_to(viewport.to_window(head.wing_b));
            }
        }
    }
    path.build().ok()
}
