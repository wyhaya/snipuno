mod raster;
mod text;
#[cfg(test)]
use text::text_element;
pub(crate) use text::text_svg;

use self::raster::{RasterImage, crop_source, render_pixmap, rgba_pixmap, rounded_path};
use crate::{
    canvas::{ImageViewport, magnifier_shadow_paths, spotlight_path, stroke_path},
    editor::{
        Bounds as ImageBounds, EffectStrength, Point as ImagePoint, RedactionStyle, Stroke, Tool,
    },
    erase::background_color,
    magnifier::{LensRegion, render_lens},
    mosaic::Mosaic,
    spotlight, theme,
};
use gpui::{Bounds, ImageId, RenderImage, SvgRenderer, point, px, size};
use image::{
    ExtendedColorType, ImageEncoder, RgbaImage,
    codecs::png::{CompressionType, FilterType, PngEncoder},
    imageops,
};
use std::{
    collections::{HashMap, hash_map::Entry},
    fs,
    path::Path,
    sync::{Arc, OnceLock},
};

#[cfg(test)]
mod tests;

#[derive(Clone)]
pub(crate) struct ExportCache {
    key: Arc<ExportKey>,
    image: Arc<RgbaImage>,
    png: Arc<OnceLock<Result<Vec<u8>, String>>>,
}

impl ExportCache {
    pub fn save(&self, path: &Path) -> Result<(), String> {
        save_png(self.png()?, path)
    }

    pub fn pixels(&self) -> Arc<RgbaImage> {
        self.image.clone()
    }

    fn png(&self) -> Result<&[u8], String> {
        self.png
            .get_or_init(|| encode_png(&self.image))
            .as_deref()
            .map_err(Clone::clone)
    }
}

struct ExportKey {
    source: Arc<RgbaImage>,
    bounds: ImageBounds,
    strokes: Vec<Stroke>,
    overlays: HashMap<u64, ImageId>,
    text: HashMap<usize, String>,
    scale: f32,
}

impl ExportKey {
    fn matches(&self, other: &ExportImage, overlays: &HashMap<u64, ImageId>) -> bool {
        Arc::ptr_eq(&self.source, &other.source)
            && self.bounds == other.bounds
            && self.strokes == other.strokes
            && self.overlays == *overlays
            && self.text == other.text
            && self.scale == other.scale
    }
}

#[derive(Clone)]
pub(crate) struct ExportImage {
    pub source: Arc<RgbaImage>,
    pub bounds: ImageBounds,
    pub strokes: Vec<Stroke>,
    pub overlays: HashMap<u64, Arc<RenderImage>>,
    pub blurred: HashMap<EffectStrength, Arc<RenderImage>>,
    pub mosaic: Option<Arc<Mosaic>>,
    pub text: HashMap<usize, String>,
    pub scale: f32,
}

impl ExportImage {
    pub fn prepare(
        self,
        renderer: SvgRenderer,
        cached: Option<ExportCache>,
    ) -> Result<ExportCache, String> {
        let overlays = self
            .strokes
            .iter()
            .filter_map(|stroke| {
                if let Tool::Image(id) = stroke.tool {
                    self.overlays.get(&id).map(|image| (id, image.id))
                } else {
                    None
                }
            })
            .collect();
        if let Some(cached) = cached.filter(|cached| cached.key.matches(&self, &overlays)) {
            return Ok(cached);
        }
        let pixels = self.render(renderer)?;
        let image = Arc::new(pixels);
        let key = Arc::new(ExportKey {
            source: self.source,
            bounds: self.bounds,
            strokes: self.strokes,
            overlays,
            text: self.text,
            scale: self.scale,
        });
        Ok(ExportCache {
            key,
            image,
            png: Arc::default(),
        })
    }

    fn render(&self, renderer: SvgRenderer) -> Result<RgbaImage, String> {
        let pixels = crop_source(&self.source, self.bounds);
        if self.strokes.is_empty() {
            return Ok(pixels);
        }
        let viewport = ImageViewport {
            bounds: Bounds::new(
                point(px(0.0), px(0.0)),
                size(
                    px(self.bounds.width * self.scale),
                    px(self.bounds.height * self.scale),
                ),
            ),
            scale: self.scale,
        };
        let offset = ImagePoint::new(self.bounds.x, self.bounds.y);
        let source_bounds = ImageBounds {
            x: -offset.x,
            y: -offset.y,
            width: self.source.width() as f32,
            height: self.source.height() as f32,
        };
        let radius = theme::ANNOTATION_RADIUS / self.scale;
        let mut output = RasterImage {
            pixels: rgba_pixmap(pixels)?,
        };
        if let Some(path) = spotlight_path(&self.strokes, viewport) {
            output.path(path, self.scale, theme::DIMMING, spotlight::DIM_OPACITY);
        }
        let mut source = None;
        let mut mosaic = self.mosaic.clone();
        let mut blurs = HashMap::new();
        let mut overlays = HashMap::new();
        let mut mask_cache = None;
        for (index, stroke) in self.strokes.iter().enumerate() {
            let bounds = stroke.bounds();
            match stroke.tool {
                Tool::RectangleFill | Tool::EllipseFill if stroke.fill_color.is_none() => {
                    if let Some(color) = background_color(&self.source, bounds, offset)
                        && let Some(path) = stroke_path(stroke, viewport)
                    {
                        output.path(path, self.scale, color, stroke.paint_opacity());
                    }
                }
                Tool::Redact(RedactionStyle::Blur) => {
                    let image = match blurs.entry(stroke.effect_strength) {
                        Entry::Occupied(entry) => entry.into_mut(),
                        Entry::Vacant(entry) => {
                            let pixels =
                                if let Some(image) = self.blurred.get(&stroke.effect_strength) {
                                    render_pixmap(image)?
                                } else {
                                    rgba_pixmap(imageops::fast_blur(
                                        &self.source,
                                        stroke.effect_strength.blur_sigma(),
                                    ))?
                                };
                            entry.insert(pixels)
                        }
                    };
                    let mask =
                        output.mask(&mut mask_cache, rounded_path(bounds, radius).as_ref())?;
                    output.image(image, source_bounds, Some(mask));
                }
                Tool::Redact(RedactionStyle::Mosaic) => {
                    let mosaic = mosaic.get_or_insert_with(|| Arc::new(Mosaic::new(&self.source)));
                    let mask =
                        output.mask(&mut mask_cache, rounded_path(bounds, radius).as_ref())?;
                    for (start, end, color) in mosaic.tiles_at(stroke, offset) {
                        output.rect(
                            ImageBounds {
                                x: start.x,
                                y: start.y,
                                width: end.x - start.x,
                                height: end.y - start.y,
                            },
                            0.0,
                            color.rotate_right(8),
                            Some(mask),
                        );
                    }
                }
                Tool::Image(id) => {
                    let image = match overlays.entry(id) {
                        Entry::Occupied(entry) => entry.into_mut(),
                        Entry::Vacant(entry) => {
                            let image = self
                                .overlays
                                .get(&id)
                                .ok_or("An image layer is unavailable")?;
                            entry.insert(render_pixmap(image)?)
                        }
                    };
                    output.image(
                        image,
                        ImageBounds {
                            x: stroke.start.x,
                            y: stroke.start.y,
                            width: stroke.end.x - stroke.start.x,
                            height: stroke.end.y - stroke.start.y,
                        },
                        None,
                    );
                }
                Tool::Magnifier => {
                    let magnifier = stroke
                        .magnifier
                        .as_ref()
                        .ok_or("A magnifier is incomplete")?;
                    let region = LensRegion::new(
                        bounds,
                        ImageBounds {
                            x: 0.0,
                            y: 0.0,
                            width: self.bounds.width,
                            height: self.bounds.height,
                        },
                        1.0,
                    )?;
                    for (path, color) in magnifier_shadow_paths(bounds, viewport) {
                        output.path(path, viewport.scale, color, (color >> 24) as f32 / 255.0);
                    }
                    if let Some(region) = region {
                        if source.is_none() {
                            source = Some(rgba_pixmap(self.source.as_ref().clone())?);
                        }
                        let pixels = render_lens(
                            magnifier,
                            bounds,
                            offset,
                            source.as_ref().unwrap(),
                            region,
                        )?;
                        output.image(&rgba_pixmap(pixels)?, region.bounds, None);
                    }
                }
                Tool::Measure(_) => {
                    output.stroke(stroke, viewport);
                    if let Some(label) = stroke.measurement_label {
                        output.rect(
                            label,
                            theme::DIMENSION_RADIUS * stroke.width,
                            stroke.color,
                            None,
                        );
                    }
                }
                Tool::Spotlight => {}
                _ => output.stroke(stroke, viewport),
            }
            if let Some(text) = self.text.get(&index) {
                output.text(text, stroke.measurement_label.unwrap_or(bounds), &renderer)?;
            }
        }
        Ok(output.into_rgba())
    }
}

fn encode_png(pixels: &RgbaImage) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    PngEncoder::new_with_quality(&mut bytes, CompressionType::Fast, FilterType::Sub)
        .write_image(
            pixels.as_raw(),
            pixels.width(),
            pixels.height(),
            ExtendedColorType::Rgba8,
        )
        .map_err(|error| format!("Unable to encode PNG: {error}"))?;
    Ok(bytes)
}

fn save_png(bytes: &[u8], path: &Path) -> Result<(), String> {
    if path
        .extension()
        .is_some_and(|extension| !extension.eq_ignore_ascii_case("png"))
    {
        return Err("Save the image with a .png extension".into());
    }
    fs::write(path, bytes).map_err(|error| format!("Unable to save {}: {error}", path.display()))
}
