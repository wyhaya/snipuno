use super::{LensRegion, Magnifier, ellipse_path};
use crate::{
    editor::{Bounds, Point},
    theme,
};
use image::RgbaImage;
use tiny_skia::{Color, FillRule, FilterQuality, Mask, Pixmap, PixmapPaint, PixmapRef, Transform};

// Normalized ellipse radii: 0 is the center and 1 is the lens boundary.
const EDGE_START: f32 = 0.80;
const EDGE_TRANSITION_WIDTH: f32 = 0.20;
const REFRACTION_STRENGTH: f32 = 0.085;

// Width limits are in source-image pixels, before preview scaling.
const DISPERSION_RADIUS_RATIO: f32 = 0.004;
const DISPERSION_MIN: f32 = 0.25;
const DISPERSION_MAX: f32 = 2.0;
const RIM_WIDTH_RADIUS_RATIO: f32 = 0.004;
const RIM_WIDTH_MIN: f32 = 0.65;
const RIM_WIDTH_MAX: f32 = 1.5;

// Unit vector pointing toward the light; negative X/Y lights the upper-left rim.
const LIGHT_DIRECTION: [f32; 2] = [-0.6, -0.8];
const SHADE_STRENGTH: f32 = 0.10;
const SHADE_AMBIENT: f32 = 0.4;
const SHADE_DIRECTIONAL: f32 = 0.6;

const REFLECTION_STRENGTH: f32 = 0.85;
const EDGE_REFLECTION_STRENGTH: f32 = 0.24;
const EDGE_REFLECTION_AMBIENT: f32 = 0.2;
const EDGE_REFLECTION_DIRECTIONAL: f32 = 0.8;
const EDGE_REFLECTION_FALLOFF: i32 = 3;
const RIM_REFLECTION_STRENGTH: f32 = 0.65;
const RIM_REFLECTION_AMBIENT: f32 = 0.25;
const RIM_REFLECTION_DIRECTIONAL: f32 = 0.75;
const RIM_REFLECTION_SHARPNESS: i32 = 2;

const WARMTH_STRENGTH: f32 = 0.08;
const HIGHLIGHT_RGB: [f32; 3] = [255.0, 255.0, 255.0];
const WARM_RGB: [f32; 3] = [255.0, 246.0, 235.0];

/// Returns straight-alpha RGBA pixels; each adapter handles its required pixel format.
pub(crate) fn render_lens(
    magnifier: &Magnifier,
    lens: Bounds,
    offset: Point,
    source: &Pixmap,
    region: LensRegion,
) -> Result<RgbaImage, &'static str> {
    if !magnifier.zoom.is_finite()
        || magnifier.zoom <= 0.0
        || !offset.x.is_finite()
        || !offset.y.is_finite()
    {
        return Err("A magnifier has an invalid zoom or source offset");
    }
    let (width, height) = region.size;
    let mut rendered = Pixmap::new(width, height).ok_or("Unable to allocate magnifier pixels")?;
    let mut mask = Mask::new(width, height).ok_or("Unable to allocate magnifier mask")?;
    mask.fill_path(
        &ellipse_path(region.lens).ok_or("A magnifier has an invalid lens shape")?,
        FillRule::Winding,
        true,
        Transform::identity(),
    );
    let background = [
        ((theme::SURFACE >> 16) & 255) as f32,
        ((theme::SURFACE >> 8) & 255) as f32,
        (theme::SURFACE & 255) as f32,
    ];
    rendered.fill(Color::from_rgba8(
        background[0] as u8,
        background[1] as u8,
        background[2] as u8,
        255,
    ));
    let full = magnifier.image_placement(
        lens,
        offset,
        Point::new(source.width() as f32, source.height() as f32),
    );
    let scale_x = region.lens.width / lens.width;
    let scale_y = region.lens.height / lens.height;
    rendered.draw_pixmap(
        0,
        0,
        source.as_ref(),
        &PixmapPaint {
            quality: FilterQuality::Bilinear,
            ..Default::default()
        },
        Transform::from_row(
            magnifier.zoom * scale_x,
            0.0,
            0.0,
            magnifier.zoom * scale_y,
            (full.x - lens.x) * scale_x + region.lens.x,
            (full.y - lens.y) * scale_y + region.lens.y,
        ),
        None,
    );
    let radius = Point::new(lens.width * 0.5, lens.height * 0.5);
    let short_radius = radius.x.min(radius.y);
    let center = Point::new(lens.x + radius.x + offset.x, lens.y + radius.y + offset.y);
    let dispersion = (short_radius * DISPERSION_RADIUS_RATIO).clamp(DISPERSION_MIN, DISPERSION_MAX)
        / magnifier.zoom;
    let rim_width = (short_radius * RIM_WIDTH_RADIUS_RATIO).clamp(RIM_WIDTH_MIN, RIM_WIDTH_MAX);
    let source = source.as_ref();
    let mut bytes = rendered.take();
    for (index, (pixel, coverage)) in bytes
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(mask.data())
        .enumerate()
    {
        if *coverage == 0 {
            pixel.fill(0);
            continue;
        }
        let dx = (index % width as usize) as f32 + 0.5 - region.lens.x - region.lens.width * 0.5;
        let dy = (index / width as usize) as f32 + 0.5 - region.lens.y - region.lens.height * 0.5;
        let nx = dx / (region.lens.width * 0.5);
        let ny = dy / (region.lens.height * 0.5);
        let distance_squared = nx * nx + ny * ny;
        if distance_squared > EDGE_START * EDGE_START {
            let distance = distance_squared.sqrt();
            let edge = ((distance - EDGE_START) / EDGE_TRANSITION_WIDTH).clamp(0.0, 1.0);
            let edge = edge * edge * (3.0 - 2.0 * edge);
            let bend = 1.0 - REFRACTION_STRENGTH * edge * edge;
            let x = center.x + dx / scale_x / magnifier.zoom * bend;
            let y = center.y + dy / scale_y / magnifier.zoom * bend;
            let shift = dispersion * edge * edge;
            let refracted = [
                sample(source, x + nx * shift, y + ny * shift, 0, background[0]),
                sample(source, x, y, 1, background[1]),
                sample(source, x - nx * shift, y - ny * shift, 2, background[2]),
            ];
            let light = ((LIGHT_DIRECTION[0] * nx + LIGHT_DIRECTION[1] * ny) / distance).max(0.0);
            let opposite =
                ((-LIGHT_DIRECTION[0] * nx - LIGHT_DIRECTION[1] * ny) / distance).max(0.0);
            let rim = ((1.0 - distance) * short_radius / rim_width).max(0.0);
            let rim = (-rim * rim).exp();
            let shade =
                SHADE_STRENGTH * edge * edge * (SHADE_AMBIENT + SHADE_DIRECTIONAL * opposite);
            let reflection = REFLECTION_STRENGTH
                * (EDGE_REFLECTION_STRENGTH
                    * edge.powi(EDGE_REFLECTION_FALLOFF)
                    * (EDGE_REFLECTION_AMBIENT + EDGE_REFLECTION_DIRECTIONAL * light * light)
                    + RIM_REFLECTION_STRENGTH
                        * rim
                        * (RIM_REFLECTION_AMBIENT
                            + RIM_REFLECTION_DIRECTIONAL * light.powi(RIM_REFLECTION_SHARPNESS)));
            let warmth = WARMTH_STRENGTH * edge * edge * opposite;
            for (channel, ((value, highlight), warm)) in pixel[..3]
                .iter_mut()
                .zip(refracted.into_iter().zip(HIGHLIGHT_RGB).zip(WARM_RGB))
            {
                let value = value * (1.0 - shade);
                let value = value + (highlight - value) * reflection;
                *channel = (value + (warm - value) * warmth).round().clamp(0.0, 255.0) as u8;
            }
        }
        pixel[3] = *coverage;
    }
    RgbaImage::from_raw(width, height, bytes).ok_or("A magnifier has invalid pixel data")
}

fn sample(source: PixmapRef<'_>, x: f32, y: f32, channel: usize, background: f32) -> f32 {
    let x = x - 0.5;
    let y = y - 0.5;
    let left = x.floor();
    let top = y.floor();
    let tx = x - left;
    let ty = y - top;
    let mut color = 0.0;
    for (x, y, weight) in [
        (left, top, (1.0 - tx) * (1.0 - ty)),
        (left + 1.0, top, tx * (1.0 - ty)),
        (left, top + 1.0, (1.0 - tx) * ty),
        (left + 1.0, top + 1.0, tx * ty),
    ] {
        let sample =
            if x >= 0.0 && y >= 0.0 && x < source.width() as f32 && y < source.height() as f32 {
                let index = ((y as usize * source.width() as usize) + x as usize) * 4;
                let pixel = &source.data()[index..index + 4];
                let transparency = 1.0 - pixel[3] as f32 / 255.0;
                pixel[channel] as f32 + background * transparency
            } else {
                background
            };
        color += sample * weight;
    }
    color
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::IntSize;

    #[test]
    fn refraction_sampling_interpolates_premultiplied_pixels_over_the_background() {
        let source = Pixmap::from_vec(
            vec![50, 25, 0, 128, 0, 100, 200, 255],
            IntSize::from_wh(2, 1).unwrap(),
        )
        .unwrap();
        for (channel, expected) in [88.5, 126.0, 163.5].into_iter().enumerate() {
            let actual = sample(source.as_ref(), 1.0, 0.5, channel, 255.0);
            assert!((actual - expected).abs() < 0.001);
            assert_eq!(sample(source.as_ref(), -2.0, 0.5, channel, 255.0), 255.0);
        }
    }
}
