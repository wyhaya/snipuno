use super::*;
use crate::test_support::{TestDir, animated_gif, bounds};
use image::{
    ExtendedColorType, ImageEncoder, ImageFormat, Rgb, RgbImage, Rgba, codecs::jpeg::JpegEncoder,
};
use std::fs;

#[test]
fn pixel_sampling_uses_image_coordinates_and_composites_alpha_on_white() {
    let pixels = RgbaImage::from_fn(2, 2, |x, y| match (x, y) {
        (0, 0) => Rgba([10, 20, 30, 255]),
        (1, 0) => Rgba([255, 0, 0, 128]),
        _ => Rgba([0, 0, 0, 0]),
    });
    assert_eq!(
        sample_image_color(&pixels, Point::new(0.9, 0.1)),
        Some(0x0a141e)
    );
    assert_eq!(
        sample_image_color(&pixels, Point::new(1.9, 0.1)),
        Some(0xff7f7f)
    );
    assert_eq!(
        sample_image_color(&pixels, Point::new(1.0, 1.0)),
        Some(0xffffff)
    );
    for point in [
        Point::new(-0.1, 0.0),
        Point::new(2.0, 0.0),
        Point::new(0.0, 2.0),
        Point::new(f32::NAN, 0.0),
    ] {
        assert_eq!(sample_image_color(&pixels, point), None);
    }
}

#[test]
fn blur_softens_edges_preserves_dimensions_and_converts_channels() {
    let pixels = RgbaImage::from_fn(81, 21, |x, _| {
        Rgba([if x < 40 { 0 } else { 200 }, 40, 80, 255])
    });
    let original = pixels.clone();
    let blurred = blurred_image(&pixels, EffectStrength::Low);
    assert_eq!(blurred.size(0).width.0, 81);
    assert_eq!(blurred.size(0).height.0, 21);
    let bytes = blurred.as_bytes(0).unwrap();
    let red = |x: usize| bytes[(10 * 81 + x) * 4 + 2];
    assert!(red(39) > 0 && red(39) < 200);
    assert!(red(40) > red(39) && red(40) < 200);
    assert!(red(0) < red(39) && red(80) > red(40));
    assert!(
        bytes
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| p[0] == 80 && p[1] == 40 && p[3] == 255)
    );
    assert_eq!(pixels, original);
}

#[test]
fn image_mirroring_reverses_pixels_on_each_axis() {
    let pixels = RgbaImage::from_fn(3, 2, |x, y| Rgba([(x + y * 3) as u8, 0, 0, 255]));
    let source = RenderImage::new(vec![Frame::new(pixels)]);
    for (horizontal, vertical, expected) in [
        (false, false, vec![0, 1, 2, 3, 4, 5]),
        (true, false, vec![2, 1, 0, 5, 4, 3]),
        (false, true, vec![3, 4, 5, 0, 1, 2]),
        (true, true, vec![5, 4, 3, 2, 1, 0]),
    ] {
        let flipped = flip_image(&source, horizontal, vertical);
        let values: Vec<_> = flipped
            .as_bytes(0)
            .unwrap()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| p[0])
            .collect();
        assert_eq!(values, expected);
        assert_eq!(
            flip_image(&flipped, horizontal, vertical).as_bytes(0),
            source.as_bytes(0)
        );
    }
}

#[test]
fn jpeg_import_applies_exif_orientation_to_pixels_and_dimensions() {
    let source = RgbImage::from_fn(24, 16, |x, y| Rgb([x as u8 * 8, y as u8 * 12, 80]));
    let mut bytes = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(&mut bytes, 100);
    encoder
        .set_exif_metadata(vec![
            b'I', b'I', 42, 0, 8, 0, 0, 0, 1, 0, 0x12, 0x01, 3, 0, 1, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0,
            0,
        ])
        .unwrap();
    encoder
        .write_image(source.as_raw(), 24, 16, ExtendedColorType::Rgb8)
        .unwrap();
    let expected = image::load_from_memory(&bytes)
        .unwrap()
        .rotate90()
        .into_rgba8();
    let dir = TestDir::new();
    let path = dir.join("oriented.jpg");
    fs::write(&path, bytes).unwrap();
    let result = load_overlay(path.clone());
    let (rendered, dimensions) = result.unwrap();
    assert_eq!(dimensions, (16, 24));
    for (actual, expected) in rendered
        .as_bytes(0)
        .unwrap()
        .as_chunks::<4>()
        .0
        .iter()
        .zip(expected.pixels())
    {
        let [r, g, b, a] = expected.0;
        assert_eq!(*actual, [b, g, r, a]);
    }
}

#[test]
fn cropped_sampling_clamps_to_visible_pixels_and_rejects_invalid_regions() {
    let image = RgbaImage::from_fn(4, 4, |x, y| Rgba([x as u8, y as u8, 0, 255]));
    for (crop, point, expected) in [
        (
            bounds(1.0, 1.0, 2.0, 2.0),
            Point::new(-10.0, 10.0),
            Some(0x010200),
        ),
        (
            bounds(1.5, 1.5, 2.0, 2.0),
            Point::new(10.0, 10.0),
            Some(0x030300),
        ),
        (
            bounds(1.5, 1.5, 2.0, 2.0),
            Point::new(-10.0, -10.0),
            Some(0x010100),
        ),
        (
            bounds(3.0, 3.0, 1.0, 1.0),
            Point::new(10.0, -10.0),
            Some(0x030300),
        ),
        (bounds(1.0, 1.0, 0.0, 2.0), Point::default(), None),
        (bounds(4.0, 0.0, 2.0, 2.0), Point::default(), None),
        (bounds(f32::NAN, 0.0, 2.0, 2.0), Point::default(), None),
        (
            bounds(0.0, 0.0, 2.0, 2.0),
            Point::new(f32::INFINITY, 0.0),
            None,
        ),
    ] {
        assert_eq!(
            sample_image_color_in_bounds(&image, point, crop),
            expected,
            "{crop:?} {point:?}"
        );
    }
    assert_eq!(
        sample_image_color_in_bounds(
            &RgbaImage::new(0, 0),
            Point::default(),
            bounds(0.0, 0.0, 1.0, 1.0)
        ),
        None
    );
}

#[test]
fn import_uses_file_contents_and_preserves_channels_and_transparency() {
    let dir = TestDir::new();
    let pixels = RgbaImage::from_fn(3, 2, |x, y| {
        Rgba([20 + x as u8 * 20, 40 + y as u8 * 20, 80, 128])
    });
    for (name, format) in [
        ("image.png", ImageFormat::Png),
        ("image.webp", ImageFormat::WebP),
        ("image.jpg", ImageFormat::Jpeg),
        ("image.bmp", ImageFormat::Bmp),
        ("image.ico", ImageFormat::Ico),
        ("unknown.data", ImageFormat::Png),
    ] {
        let path = dir.join(name);
        let image = DynamicImage::ImageRgba8(pixels.clone());
        if format == ImageFormat::Jpeg {
            image.to_rgb8().save_with_format(&path, format).unwrap();
        } else {
            image.save_with_format(&path, format).unwrap();
        }
        let (actual, dimensions) = load_overlay(path.clone()).unwrap();
        assert_eq!(dimensions, (3, 2));
        let expected = image::load_from_memory(&fs::read(path).unwrap())
            .unwrap()
            .into_rgba8();
        let bgra: Vec<_> = expected
            .pixels()
            .flat_map(|p| [p[2], p[1], p[0], p[3]])
            .collect();
        assert_eq!(actual.as_bytes(0).unwrap(), bgra, "{name}");
        if format != ImageFormat::Jpeg {
            assert_eq!(expected, pixels);
        }
    }
    let path = dir.join("broken.png");
    fs::write(&path, b"not an image").unwrap();
    assert!(load_overlay(path).is_err());
    assert!(load_overlay(dir.join("missing.png")).is_err());
    assert!(SourceImage::from_rgba(RgbaImage::new(0, 1)).is_err());
    assert!(SourceImage::from_rgba(RgbaImage::new(1, 0)).is_err());
}

#[test]
fn gif_import_uses_only_the_first_frame_and_preserves_transparency() {
    let dir = TestDir::new();
    let path = dir.join("animated.gif");
    fs::write(&path, animated_gif()).unwrap();
    let (image, dimensions) = load_overlay(path).unwrap();
    assert_eq!(dimensions, (3, 2));
    assert_eq!(image.frame_count(), 1);
    assert_eq!(
        image.as_bytes(0).unwrap(),
        [0, 0, 0, 0, 0, 0, 255, 255, 0, 0, 255, 255].repeat(2)
    );
}
