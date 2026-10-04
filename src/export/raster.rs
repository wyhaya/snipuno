use crate::{
    canvas::{ImageViewport, stroke_path},
    editor::{Bounds as ImageBounds, Stroke},
    image_processing::unpremultiply,
};
use gpui::{DevicePixels, Pixels, RenderImage, SvgRenderer, SvgSize, size};
use image::RgbaImage;
use tiny_skia::{
    FillRule, FilterQuality, IntSize, Mask, Paint, Path as RasterPath,
    PathBuilder as RasterPathBuilder, Pixmap, PixmapPaint, Rect, Transform,
};

pub(super) struct RasterImage {
    pub(super) pixels: Pixmap,
}

impl RasterImage {
    pub(super) fn image(&mut self, source: &Pixmap, bounds: ImageBounds, mask: Option<&Mask>) {
        self.pixels.draw_pixmap(
            0,
            0,
            source.as_ref(),
            &PixmapPaint {
                quality: FilterQuality::Bicubic,
                ..Default::default()
            },
            Transform::from_row(
                bounds.width / source.width() as f32,
                0.0,
                0.0,
                bounds.height / source.height() as f32,
                bounds.x,
                bounds.y,
            ),
            mask,
        );
    }

    pub(super) fn mask<'a>(
        &self,
        cache: &'a mut Option<Mask>,
        path: Option<&RasterPath>,
    ) -> Result<&'a Mask, String> {
        if cache.is_none() {
            *cache = Some(
                Mask::new(self.pixels.width(), self.pixels.height())
                    .ok_or("Unable to allocate image mask")?,
            );
        }
        let mask = cache.as_mut().unwrap();
        mask.clear();
        if let Some(path) = path {
            mask.fill_path(path, FillRule::Winding, true, Transform::identity());
        }
        Ok(mask)
    }

    pub(super) fn rect(
        &mut self,
        bounds: ImageBounds,
        radius: f32,
        color: u32,
        mask: Option<&Mask>,
    ) {
        if let Some(path) = rounded_path(bounds, radius) {
            self.pixels.fill_path(
                &path,
                &paint(color, (color >> 24) as f32 / 255.0),
                FillRule::Winding,
                Transform::identity(),
                mask,
            );
        }
    }

    pub(super) fn stroke(&mut self, stroke: &Stroke, viewport: ImageViewport) {
        if let Some(path) = stroke_path(stroke, viewport) {
            let color = stroke.paint_color();
            self.path(path, viewport.scale, color, stroke.paint_opacity());
        }
    }

    pub(super) fn path(&mut self, path: gpui::Path<Pixels>, scale: f32, color: u32, opacity: f32) {
        let mut builder = RasterPathBuilder::new();
        // A single winding fill unions the canvas triangles without seams or repeated blending.
        for triangle in path.vertices.as_chunks::<3>().0.iter() {
            let [a, mut b, mut c] = std::array::from_fn(|index| {
                let p = triangle[index].xy_position;
                (f32::from(p.x) / scale, f32::from(p.y) / scale)
            });
            if (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0) < 0.0 {
                std::mem::swap(&mut b, &mut c);
            }
            builder.move_to(a.0, a.1);
            builder.line_to(b.0, b.1);
            builder.line_to(c.0, c.1);
            builder.close();
        }
        if let Some(path) = builder.finish() {
            self.pixels.fill_path(
                &path,
                &paint(color, opacity),
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    }

    pub(super) fn text(
        &mut self,
        text: &str,
        bounds: ImageBounds,
        renderer: &SvgRenderer,
    ) -> Result<(), String> {
        // Keep clipping outside the image from changing antialiasing on its edge pixels.
        let left = (bounds.x.floor() - 1.0).max(-1.0);
        let top = (bounds.y.floor() - 1.0).max(-1.0);
        let width = ((bounds.x + bounds.width + 1.0)
            .ceil()
            .min(self.pixels.width() as f32 + 1.0)
            - left) as u32;
        let height = ((bounds.y + bounds.height + 1.0)
            .ceil()
            .min(self.pixels.height() as f32 + 1.0)
            - top) as u32;
        if width == 0 || height == 0 {
            return Ok(());
        }
        let svg = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"{left} {top} {width} {height}\">{text}</svg>"
        );
        let parsed = renderer
            .parse_svg(svg.as_bytes())
            .map_err(|error| format!("Unable to export text: {error}"))?;
        let rendered = renderer
            .render_parsed(
                &parsed,
                SvgSize::ExactSize(size(
                    DevicePixels(width as i32),
                    DevicePixels(height as i32),
                )),
            )
            .map_err(|error| format!("Unable to export text: {error}"))?;
        let text = render_pixmap(&rendered)?;
        self.pixels.draw_pixmap(
            left as i32,
            top as i32,
            text.as_ref(),
            &PixmapPaint::default(),
            Transform::identity(),
            None,
        );
        Ok(())
    }

    pub(super) fn into_rgba(self) -> RgbaImage {
        let (width, height) = (self.pixels.width(), self.pixels.height());
        let mut bytes = self.pixels.take();
        unpremultiply(&mut bytes);
        RgbaImage::from_raw(width, height, bytes).unwrap()
    }
}

pub(super) fn paint(color: u32, opacity: f32) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color_rgba8(
        (color >> 16) as u8,
        (color >> 8) as u8,
        color as u8,
        (opacity * 255.0).round() as u8,
    );
    paint
}

pub(super) fn rounded_path(bounds: ImageBounds, radius: f32) -> Option<RasterPath> {
    let rect = Rect::from_xywh(bounds.x, bounds.y, bounds.width, bounds.height)?;
    let radius = radius.min(bounds.width / 2.0).min(bounds.height / 2.0);
    if radius <= 0.0 {
        return Some(RasterPathBuilder::from_rect(rect));
    }
    let (left, top, right, bottom) = (rect.left(), rect.top(), rect.right(), rect.bottom());
    let c = radius * 0.5522848;
    let mut path = RasterPathBuilder::new();
    path.move_to(left + radius, top);
    path.line_to(right - radius, top);
    path.cubic_to(
        right - radius + c,
        top,
        right,
        top + radius - c,
        right,
        top + radius,
    );
    path.line_to(right, bottom - radius);
    path.cubic_to(
        right,
        bottom - radius + c,
        right - radius + c,
        bottom,
        right - radius,
        bottom,
    );
    path.line_to(left + radius, bottom);
    path.cubic_to(
        left + radius - c,
        bottom,
        left,
        bottom - radius + c,
        left,
        bottom - radius,
    );
    path.line_to(left, top + radius);
    path.cubic_to(
        left,
        top + radius - c,
        left + radius - c,
        top,
        left + radius,
        top,
    );
    path.close();
    path.finish()
}

pub(super) fn crop_source(source: &RgbaImage, bounds: ImageBounds) -> RgbaImage {
    let (x, y, width, height) = (
        bounds.x as usize,
        bounds.y as usize,
        bounds.width as usize,
        bounds.height as usize,
    );
    let stride = source.width() as usize * 4;
    let mut bytes = Vec::with_capacity(width * height * 4);
    for row in y..y + height {
        let start = row * stride + x * 4;
        bytes.extend_from_slice(&source.as_raw()[start..start + width * 4]);
    }
    RgbaImage::from_raw(width as u32, height as u32, bytes).unwrap()
}

pub(super) fn rgba_pixmap(pixels: RgbaImage) -> Result<Pixmap, String> {
    let (width, height) = pixels.dimensions();
    pixmap(pixels.into_raw(), width, height, false)
}

pub(super) fn render_pixmap(image: &RenderImage) -> Result<Pixmap, String> {
    let dimensions = image.size(0);
    let bytes = image
        .as_bytes(0)
        .ok_or("The rendered image has no pixels")?
        .to_vec();
    pixmap(
        bytes,
        dimensions.width.0 as u32,
        dimensions.height.0 as u32,
        true,
    )
}

fn pixmap(mut bytes: Vec<u8>, width: u32, height: u32, bgra: bool) -> Result<Pixmap, String> {
    for pixel in bytes.as_chunks_mut::<4>().0 {
        if bgra {
            pixel.swap(0, 2);
        }
        let alpha = u32::from(pixel[3]);
        if alpha < 255 {
            for channel in &mut pixel[..3] {
                *channel = ((u32::from(*channel) * alpha + 127) / 255) as u8;
            }
        }
    }
    IntSize::from_wh(width, height)
        .and_then(|size| Pixmap::from_vec(bytes, size))
        .ok_or_else(|| "The rendered image has invalid dimensions".into())
}
