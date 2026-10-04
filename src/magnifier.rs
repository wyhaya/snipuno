mod glass;

pub(crate) use glass::render_lens;

use crate::{
    editor::{Bounds, Point},
    frame_cache::FrameCache,
    image_processing::render_image,
    theme,
};
use gpui::{App, ImageId, RenderImage, Window};
use std::sync::Arc;
use tiny_skia::{IntSize, Path, PathBuilder, Pixmap, Rect};

#[derive(Clone, Debug, PartialEq)]
pub struct Magnifier {
    pub zoom: f32,
}

impl Default for Magnifier {
    fn default() -> Self {
        Self { zoom: 2.0 }
    }
}

impl Magnifier {
    pub const MIN_ZOOM: f32 = 1.2;
    pub const MAX_ZOOM: f32 = 4.0;
    pub const ZOOM_STEP: f32 = 0.2;
    pub const SELECTION_WIDTH: f32 = 4.0;
    pub const SHADOW_EXTENT: f32 = 9.0;
    pub const SHADOW_OFFSET: f32 = 1.5;
    pub const SHADOW_LAYERS: [(f32, u32); 5] = [(14.0, 1), (11.0, 2), (8.0, 4), (6.0, 6), (4.0, 8)];

    pub fn image_placement(&self, lens: Bounds, offset: Point, image: Point) -> Bounds {
        let center = Point::new(lens.x + lens.width / 2.0, lens.y + lens.height / 2.0);
        Bounds {
            x: center.x - (center.x + offset.x) * self.zoom,
            y: center.y - (center.y + offset.y) * self.zoom,
            width: image.x * self.zoom,
            height: image.y * self.zoom,
        }
    }
}

fn ellipse_path(bounds: Bounds) -> Option<Path> {
    PathBuilder::from_oval(Rect::from_xywh(
        bounds.x,
        bounds.y,
        bounds.width,
        bounds.height,
    )?)
}

#[derive(Default)]
pub struct PreviewCache {
    images: FrameCache<PreviewKey, Arc<RenderImage>>,
    source: Option<(ImageId, Pixmap)>,
}

#[derive(PartialEq)]
struct PreviewKey {
    source: ImageId,
    lens: Bounds,
    offset: Point,
    zoom: f32,
    region: LensRegion,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LensRegion {
    pub(crate) bounds: Bounds,
    lens: Bounds,
    size: (u32, u32),
}

impl LensRegion {
    pub(crate) fn new(
        lens: Bounds,
        canvas: Bounds,
        scale: f32,
    ) -> Result<Option<Self>, &'static str> {
        if ![
            lens.x,
            lens.y,
            lens.width,
            lens.height,
            canvas.x,
            canvas.y,
            canvas.width,
            canvas.height,
            scale,
        ]
        .into_iter()
        .all(f32::is_finite)
            || lens.width <= 0.0
            || lens.height <= 0.0
            || canvas.width <= 0.0
            || canvas.height <= 0.0
            || scale <= 0.0
        {
            return Err("A magnifier has invalid render bounds");
        }
        let left = lens.x.max(canvas.x);
        let top = lens.y.max(canvas.y);
        let right = (lens.x + lens.width).min(canvas.x + canvas.width);
        let bottom = (lens.y + lens.height).min(canvas.y + canvas.height);
        if right <= left || bottom <= top {
            return Ok(None);
        }
        let width = (lens.width * scale).ceil();
        let height = (lens.height * scale).ceil();
        if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
            return Err("A magnifier has invalid render dimensions");
        }
        let scale_x = width / lens.width;
        let scale_y = height / lens.height;
        // Preserve the full pixel grid; padding keeps raster clipping away from filtered pixels.
        let left = (((left - lens.x) * scale_x).floor() - 2.0).max(0.0);
        let top = (((top - lens.y) * scale_y).floor() - 2.0).max(0.0);
        let right = (((right - lens.x) * scale_x).ceil() + 2.0).min(width);
        let bottom = (((bottom - lens.y) * scale_y).ceil() + 2.0).min(height);
        let size = ((right - left) as u32, (bottom - top) as u32);
        if size.0 == 0 || size.1 == 0 {
            return Ok(None);
        }
        Ok(Some(Self {
            bounds: Bounds {
                x: lens.x + left / scale_x,
                y: lens.y + top / scale_y,
                width: size.0 as f32 / scale_x,
                height: size.1 as f32 / scale_y,
            },
            lens: Bounds {
                x: -left,
                y: -top,
                width,
                height,
            },
            size,
        }))
    }
}

impl PreviewCache {
    pub fn clear(&mut self, window: &mut Window, cx: &mut App) {
        self.source = None;
        self.images.clear(|image| {
            cx.drop_image(image, Some(window));
        });
    }

    pub fn image(
        &mut self,
        magnifier: &Magnifier,
        lens: Bounds,
        canvas: Bounds,
        offset: Point,
        source: &RenderImage,
        scale: f32,
    ) -> Option<(Arc<RenderImage>, Bounds)> {
        let region = LensRegion::new(lens, canvas, scale).ok().flatten()?;
        let key = PreviewKey {
            source: source.id,
            lens,
            offset,
            zoom: magnifier.zoom,
            region,
        };
        if let Some(image) = self.images.get(&key) {
            return Some((image.clone(), region.bounds));
        }
        let dimensions = source.size(0);
        let background = [
            (theme::SURFACE >> 16) as u8,
            (theme::SURFACE >> 8) as u8,
            theme::SURFACE as u8,
            255,
        ];
        if self.source.as_ref().is_none_or(|(id, _)| *id != source.id) {
            let mut bytes = source.as_bytes(0)?.to_vec();
            for pixel in bytes.as_chunks_mut::<4>().0 {
                pixel.swap(0, 2);
                let alpha = u32::from(pixel[3]);
                if alpha < 255 {
                    for (channel, background) in pixel[..3].iter_mut().zip(background) {
                        *channel = ((u32::from(*channel) * alpha
                            + u32::from(background) * (255 - alpha)
                            + 127)
                            / 255) as u8;
                    }
                    pixel[3] = 255;
                }
            }
            self.source = Some((
                source.id,
                Pixmap::from_vec(
                    bytes,
                    IntSize::from_wh(dimensions.width.0 as u32, dimensions.height.0 as u32)?,
                )?,
            ));
        }
        let pixels = &self.source.as_ref()?.1;
        let rendered = render_lens(magnifier, lens, offset, pixels, region).ok()?;
        let image = render_image(rendered);
        self.images.insert(key, image.clone());
        Some((image, region.bounds))
    }

    pub fn finish_frame(&mut self, window: &mut Window, cx: &mut App) {
        self.images.finish_frame(|image| {
            cx.drop_image(image.clone(), Some(window));
        });
    }
}

#[cfg(test)]
mod tests;
