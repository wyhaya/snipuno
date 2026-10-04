mod pixels;

pub(crate) use pixels::{captured_pixels, unpremultiply};

use crate::dimensions::PixelScale;
use crate::editor::{Bounds, EffectStrength, Point};
use gpui::RenderImage;
use image::{
    DynamicImage, Frame, ImageDecoder, ImageError, ImageReader, ImageResult, Limits, RgbaImage,
    imageops,
};
use std::{
    io::{BufRead, Seek},
    path::PathBuf,
    sync::Arc,
};

pub fn sample_image_color(image: &RgbaImage, point: Point) -> Option<u32> {
    if !point.x.is_finite()
        || !point.y.is_finite()
        || point.x < 0.0
        || point.y < 0.0
        || point.x >= image.width() as f32
        || point.y >= image.height() as f32
    {
        return None;
    }
    let [r, g, b, a] = image.get_pixel(point.x as u32, point.y as u32).0;
    let on_white =
        |channel: u8| (u32::from(channel) * u32::from(a) + 255 * (255 - u32::from(a)) + 127) / 255;
    Some(on_white(r) << 16 | on_white(g) << 8 | on_white(b))
}

pub fn sample_image_color_in_bounds(
    image: &RgbaImage,
    point: Point,
    bounds: Bounds,
) -> Option<u32> {
    if !point.x.is_finite()
        || !point.y.is_finite()
        || !bounds.x.is_finite()
        || !bounds.y.is_finite()
        || !bounds.width.is_finite()
        || !bounds.height.is_finite()
        || bounds.width <= 0.0
        || bounds.height <= 0.0
    {
        return None;
    }
    let left = bounds.x.max(0.0).floor();
    let top = bounds.y.max(0.0).floor();
    let right = (bounds.x + bounds.width).ceil().min(image.width() as f32) - 1.0;
    let bottom = (bounds.y + bounds.height).ceil().min(image.height() as f32) - 1.0;
    if left > right || top > bottom {
        return None;
    }
    sample_image_color(
        image,
        Point::new(point.x.clamp(left, right), point.y.clamp(top, bottom)),
    )
}

#[derive(Clone)]
pub struct SourceImage {
    pub rendered: Arc<RenderImage>,
    pub pixels: Arc<RgbaImage>,
    pub pixel_scale: PixelScale,
}

impl SourceImage {
    pub fn from_rgba(pixels: RgbaImage) -> Result<Self, String> {
        if pixels.width() == 0 || pixels.height() == 0 {
            return Err("Invalid image dimensions".into());
        }
        let rendered = render_image(pixels.clone());
        Ok(Self {
            rendered,
            pixels: Arc::new(pixels),
            pixel_scale: PixelScale::default(),
        })
    }
}

pub fn blurred_image(pixels: &RgbaImage, strength: EffectStrength) -> Arc<RenderImage> {
    render_image(imageops::fast_blur(pixels, strength.blur_sigma()))
}

pub fn flip_image(source: &RenderImage, horizontal: bool, vertical: bool) -> Arc<RenderImage> {
    let dimensions = source.size(0);
    let mut pixels = RgbaImage::from_raw(
        dimensions.width.0 as u32,
        dimensions.height.0 as u32,
        source.as_bytes(0).unwrap().to_vec(),
    )
    .unwrap();
    if horizontal {
        imageops::flip_horizontal_in_place(&mut pixels);
    }
    if vertical {
        imageops::flip_vertical_in_place(&mut pixels);
    }
    Arc::new(RenderImage::new([Frame::new(pixels)]))
}

pub fn load_overlay(path: PathBuf) -> Result<(Arc<RenderImage>, (u32, u32)), String> {
    let reader = ImageReader::open(path).map_err(|_| "Unable to read image file.")?;
    decode_overlay(reader)
}

pub fn decode_overlay<R: BufRead + Seek>(
    reader: ImageReader<R>,
) -> Result<(Arc<RenderImage>, (u32, u32)), String> {
    let pixels = load_pixels(reader).map_err(|error| match error {
        ImageError::Unsupported(_) => "Unsupported image format.",
        ImageError::IoError(_) => "Unable to read image.",
        _ => "Unable to load image.",
    })?;
    let dimensions = pixels.dimensions();
    if dimensions.0 == 0 || dimensions.1 == 0 {
        return Err("Invalid image dimensions".into());
    }
    Ok((render_image(pixels), dimensions))
}

fn load_pixels<R: BufRead + Seek>(reader: ImageReader<R>) -> ImageResult<RgbaImage> {
    let mut decoder = reader.with_guessed_format()?.into_decoder()?;
    let mut limits = Limits::default();
    limits.reserve(decoder.total_bytes())?;
    decoder.set_limits(limits)?;
    let orientation = decoder.orientation()?;
    let mut image = DynamicImage::from_decoder(decoder)?;
    image.apply_orientation(orientation);
    Ok(image.into_rgba8())
}

pub(crate) fn render_image(mut pixels: RgbaImage) -> Arc<RenderImage> {
    // GPUI's texture atlas expects BGRA pixels, while decoders return RGBA.
    for pixel in pixels.pixels_mut() {
        pixel.0.swap(0, 2);
    }
    Arc::new(RenderImage::new([Frame::new(pixels)]))
}

#[cfg(test)]
mod tests;
