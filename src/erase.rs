use crate::editor::{Bounds, Point};
use crate::frame_cache::FrameCache;
use image::RgbaImage;
use std::{collections::HashMap, sync::Arc};

pub struct BackgroundCache {
    source: Arc<RgbaImage>,
    colors: FrameCache<(Bounds, Point), Option<u32>>,
}

impl BackgroundCache {
    pub fn new(source: Arc<RgbaImage>) -> Self {
        Self {
            source,
            colors: FrameCache::default(),
        }
    }

    pub fn color(&mut self, bounds: Bounds, offset: Point) -> Option<u32> {
        let key = (bounds, offset);
        if let Some(color) = self.colors.get(&key) {
            return *color;
        }
        let color = background_color(&self.source, bounds, offset);
        self.colors.insert(key, color);
        color
    }

    pub fn finish_frame(&mut self) {
        self.colors.finish_frame(|_| {});
    }
}

pub fn brush_background_color(
    image: &RgbaImage,
    point: Point,
    width: f32,
    offset: Point,
) -> Option<u32> {
    if !width.is_finite() || width <= 0.0 {
        return None;
    }
    let radius = (width / 2.0).max(12.0);
    let bounds = Bounds {
        x: point.x - radius,
        y: point.y - radius,
        width: radius * 2.0,
        height: radius * 2.0,
    };
    sample_background_color(image, bounds, offset, true)
        .or_else(|| background_color(image, bounds, offset))
}

pub fn background_color(image: &RgbaImage, bounds: Bounds, offset: Point) -> Option<u32> {
    sample_background_color(image, bounds, offset, false)
}

fn sample_background_color(
    image: &RgbaImage,
    bounds: Bounds,
    offset: Point,
    border_only: bool,
) -> Option<u32> {
    let origin_x = bounds.x + offset.x;
    let origin_y = bounds.y + offset.y;
    if ![origin_x, origin_y, bounds.width, bounds.height]
        .into_iter()
        .all(f32::is_finite)
        || bounds.width <= 0.0
        || bounds.height <= 0.0
    {
        return None;
    }
    let left = (origin_x.floor() - 3.0).clamp(0.0, image.width() as f32) as u32;
    let top = (origin_y.floor() - 3.0).clamp(0.0, image.height() as f32) as u32;
    let right = ((origin_x + bounds.width).ceil() + 3.0).clamp(0.0, image.width() as f32) as u32;
    let bottom = ((origin_y + bounds.height).ceil() + 3.0).clamp(0.0, image.height() as f32) as u32;
    let step_x = (right - left).div_ceil(64).max(1);
    let step_y = (bottom - top).div_ceil(64).max(1);
    // Jitter each cell deterministically to avoid aligning with repeated text rows.
    let mut seed = 0x9e3779b9_u32;
    let mut sample_offset = |size| {
        if size == 1 {
            return 0;
        }
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed % size
    };
    let mut colors = HashMap::new();
    let mut most = 0;
    let mut background = None;
    for y in (top..bottom).step_by(step_y as usize) {
        for x in (left..right).step_by(step_x as usize) {
            let sample_x = x + sample_offset(step_x.min(right - x));
            let sample_y = y + sample_offset(step_y.min(bottom - y));
            if border_only
                && (origin_x..origin_x + bounds.width).contains(&(sample_x as f32))
                && (origin_y..origin_y + bounds.height).contains(&(sample_y as f32))
            {
                continue;
            }
            let [r, g, b, a] = image.get_pixel(sample_x, sample_y).0;
            if a == 0 {
                continue;
            }
            let color = (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b);
            let count = colors.entry(color).or_insert(0);
            *count += 1;
            if *count > most {
                most = *count;
                background = Some(color);
            }
        }
    }
    background
}

#[cfg(test)]
mod tests;
