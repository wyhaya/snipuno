use super::*;
use crate::test_support::bounds;
use image::Rgba;

#[test]
fn finds_light_and_dark_backgrounds_without_averaging_in_text() {
    for (background, ink) in [
        ([31, 36, 40, 255], [209, 213, 218, 255]),
        ([255; 4], [0, 0, 0, 255]),
    ] {
        let image = RgbaImage::from_fn(160, 80, |x, y| {
            Rgba(
                if (20..140).contains(&x) && (20..60).contains(&y) && x % 8 < 3 {
                    ink
                } else {
                    background
                },
            )
        });
        let color = (u32::from(background[0]) << 16)
            | (u32::from(background[1]) << 8)
            | u32::from(background[2]);
        assert_eq!(
            background_color(
                &image,
                Bounds {
                    x: 16.0,
                    y: 16.0,
                    width: 128.0,
                    height: 48.0
                },
                Point::default()
            ),
            Some(color)
        );
    }
}

#[test]
fn repeated_text_rows_do_not_align_with_every_sample() {
    let image = RgbaImage::from_fn(256, 128, |x, y| {
        Rgba(if x % 4 == 0 && y % 2 == 0 {
            [220, 224, 228, 255]
        } else {
            [31, 36, 40, 255]
        })
    });
    assert_eq!(
        background_color(
            &image,
            Bounds {
                x: 0.0,
                y: 0.0,
                width: 256.0,
                height: 128.0
            },
            Point::default(),
        ),
        Some(0x1f2428),
    );
}

#[test]
fn empty_invalid_and_transparent_regions_have_no_background() {
    let bounds = Bounds {
        x: 0.0,
        y: 0.0,
        width: 10.0,
        height: 10.0,
    };
    assert_eq!(
        background_color(&RgbaImage::new(0, 0), bounds, Point::default()),
        None
    );
    assert_eq!(
        background_color(&RgbaImage::new(10, 10), bounds, Point::default()),
        None
    );
    let image = RgbaImage::from_pixel(10, 10, Rgba([255; 4]));
    for invalid in [
        Bounds {
            width: 0.0,
            ..bounds
        },
        Bounds {
            x: f32::NAN,
            ..bounds
        },
        Bounds { x: 20.0, ..bounds },
    ] {
        assert_eq!(background_color(&image, invalid, Point::default()), None);
    }
    assert_eq!(
        background_color(&image, bounds, Point::default()),
        Some(0xffffff)
    );
}

#[test]
fn cached_background_follows_crop_offset_and_region_changes() {
    let source = Arc::new(RgbaImage::from_fn(80, 40, |x, _| {
        Rgba(if x < 40 { [255; 4] } else { [31, 36, 40, 255] })
    }));
    let mut cache = BackgroundCache::new(source);
    for (region, offset, expected) in [
        (
            bounds(5.0, 5.0, 20.0, 20.0),
            Point::default(),
            Some(0xffffff),
        ),
        (
            bounds(5.0, 5.0, 20.0, 20.0),
            Point::new(40.0, 0.0),
            Some(0x1f2428),
        ),
        (
            bounds(45.0, 5.0, 20.0, 20.0),
            Point::default(),
            Some(0x1f2428),
        ),
        (bounds(100.0, 0.0, 20.0, 20.0), Point::default(), None),
    ] {
        for _ in 0..2 {
            assert_eq!(cache.color(region, offset), expected);
        }
        cache.finish_frame();
    }
}

#[test]
fn brush_samples_background_near_its_start_even_on_text_after_crop() {
    let image = RgbaImage::from_fn(160, 80, |x, y| {
        Rgba(if (65..95).contains(&x) && (25..55).contains(&y) {
            if x % 6 < 2 {
                [240, 20, 20, 255]
            } else {
                [31, 36, 40, 255]
            }
        } else {
            [255; 4]
        })
    });
    let point = Point::new(10.0, 10.0);
    assert_eq!(
        brush_background_color(&image, point, 2.0, Point::default()),
        Some(0xffffff)
    );
    assert_eq!(
        brush_background_color(&image, point, 2.0, Point::new(70.0, 30.0)),
        Some(0x1f2428)
    );
}

#[test]
fn brush_background_handles_image_edges_and_unavailable_pixels() {
    let image = RgbaImage::from_pixel(20, 20, Rgba([31, 36, 40, 255]));
    assert_eq!(
        brush_background_color(&image, Point::default(), 24.0, Point::default()),
        Some(0x1f2428)
    );
    assert_eq!(
        brush_background_color(&image, Point::new(10.0, 10.0), 24.0, Point::default()),
        Some(0x1f2428)
    );
    for width in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        assert_eq!(
            brush_background_color(&image, Point::default(), width, Point::default()),
            None
        );
    }
    for image in [RgbaImage::new(0, 0), RgbaImage::new(20, 20)] {
        assert_eq!(
            brush_background_color(&image, Point::default(), 2.0, Point::default()),
            None
        );
    }
}

#[test]
fn brush_samples_around_a_solid_icon_instead_of_its_ink() {
    let image = RgbaImage::from_fn(80, 80, |x, y| {
        Rgba(if (28..52).contains(&x) && (28..52).contains(&y) {
            [0, 0, 0, 255]
        } else {
            [255; 4]
        })
    });
    assert_eq!(
        brush_background_color(&image, Point::new(40.0, 40.0), 2.0, Point::default()),
        Some(0xffffff)
    );
}
