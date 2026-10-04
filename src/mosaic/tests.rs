use super::*;
use crate::{
    editor::{RedactionStyle, Tool},
    test_support::{bounds, stroke},
};
use image::Rgba;

#[test]
fn every_strength_matches_direct_pixel_averages_and_covers_partial_blocks_once() {
    for (width, height) in [(1, 1), (3, 7), (25, 13), (101, 97)] {
        let image = RgbaImage::from_fn(width, height, |x, y| {
            Rgba([
                (x * 17) as u8,
                (y * 13) as u8,
                (x * y) as u8,
                (x * 7 + y * 11) as u8,
            ])
        });
        let mosaic = Mosaic::new(&image);
        for strength in EffectStrength::ALL {
            let mut mark = stroke(
                Tool::Redact(RedactionStyle::Mosaic),
                bounds(0.0, 0.0, width as f32, height as f32),
            );
            mark.effect_strength = strength;
            let mut covered = vec![0; (width * height) as usize];
            let tiles: Vec<_> = mosaic.tiles_at(&mark, Point::default()).collect();
            let block = strength.block_size();
            assert_eq!(
                tiles.len(),
                (width.div_ceil(block) * height.div_ceil(block)) as usize
            );
            for (start, end, color) in tiles {
                let mut sum = [0u32; 4];
                let mut count = 0;
                for y in start.y as u32..end.y as u32 {
                    for x in start.x as u32..end.x as u32 {
                        covered[(y * width + x) as usize] += 1;
                        for (sum, channel) in sum.iter_mut().zip(image.get_pixel(x, y).0) {
                            *sum += u32::from(channel);
                        }
                        count += 1;
                    }
                }
                assert_eq!(
                    color.to_be_bytes(),
                    sum.map(|s| (s / count) as u8),
                    "{width}×{height} {strength:?}"
                );
                assert!(end.x - start.x <= strength.block_size() as f32);
                assert!(end.y - start.y <= strength.block_size() as f32);
            }
            assert!(covered.iter().all(|count| *count == 1), "{strength:?}");
        }
    }
}

#[test]
fn cropped_mosaic_keeps_source_colors_and_exact_local_coverage() {
    let image = RgbaImage::from_fn(100, 80, |x, y| Rgba([x as u8, y as u8, 80, 255]));
    let mosaic = Mosaic::new(&image);
    let mark = stroke(
        Tool::Redact(RedactionStyle::Mosaic),
        bounds(29.0, 37.0, 50.0, 26.0),
    );
    let mut local = mark.clone();
    local.start = Point::new(9.0, 7.0);
    local.end = Point::new(59.0, 33.0);
    let expected: Vec<_> = mosaic
        .tiles_at(&mark, Point::default())
        .map(|(a, b, color)| {
            (
                Point::new(a.x - 20.0, a.y - 30.0),
                Point::new(b.x - 20.0, b.y - 30.0),
                color,
            )
        })
        .collect();
    let actual: Vec<_> = mosaic.tiles_at(&local, Point::new(20.0, 30.0)).collect();
    assert_eq!(actual, expected);
    assert_eq!(
        actual
            .iter()
            .map(|(a, b, _)| (b.x - a.x) * (b.y - a.y))
            .sum::<f32>(),
        50.0 * 26.0
    );
    assert!(
        actual
            .iter()
            .all(|(a, b, _)| a.x >= 9.0 && a.y >= 7.0 && b.x <= 59.0 && b.y <= 33.0)
    );
}

#[test]
fn empty_images_and_nonintersecting_regions_produce_no_tiles() {
    let mark = stroke(
        Tool::Redact(RedactionStyle::Mosaic),
        bounds(10.0, 10.0, 10.0, 10.0),
    );
    for (width, height, offset) in [
        (0, 0, Point::default()),
        (0, 20, Point::default()),
        (20, 0, Point::default()),
        (20, 20, Point::new(20.0, 0.0)),
        (20, 20, Point::new(-30.0, 0.0)),
    ] {
        assert_eq!(
            Mosaic::new(&RgbaImage::new(width, height))
                .tiles_at(&mark, offset)
                .count(),
            0
        );
    }
}
