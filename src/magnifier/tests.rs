use super::*;
use crate::{
    editor::{Editor, ResizeHandle, Tool},
    image_processing::render_image,
    test_support::bounds,
};
use image::{Rgba, RgbaImage};

fn source(width: u32, height: u32) -> Arc<RenderImage> {
    render_image(RgbaImage::from_fn(width, height, |x, y| {
        Rgba([(x * 7) as u8, (y * 11) as u8, (x + y) as u8, 180])
    }))
}

#[test]
fn oversized_magnifier_allocates_only_the_visible_preview() {
    let mut editor = Editor::new(800.0, 600.0);
    editor.begin(Tool::Magnifier, Point::new(200.0, 80.0), 0, 3.0);
    assert!(editor.finish(Point::new(220.0, 480.0), 0.5));
    let grab = editor
        .selected()
        .unwrap()
        .selection_handles(0.5)
        .into_iter()
        .find(|(handle, _)| *handle == ResizeHandle::Right)
        .unwrap()
        .1;
    editor.begin_selection(grab, 0.5);
    assert!(editor.finish_constrained(Point::new(grab.x + 1200.0, grab.y), 0.5, true));
    let lens = editor.selected().unwrap().bounds();
    assert_eq!((lens.width, lens.height), (1220.0, 24400.0));

    let (image, region) = PreviewCache::default()
        .image(
            &Magnifier::default(),
            lens,
            bounds(0.0, 0.0, 800.0, 600.0),
            Point::default(),
            &source(800, 600),
            1.0,
        )
        .unwrap();
    assert_eq!(region, bounds(200.0, -2.0, 602.0, 604.0));
    assert_eq!(image.size(0).width.0, 602);
    assert_eq!(image.size(0).height.0, 604);
    assert_eq!(image.as_bytes(0).unwrap().len(), 602 * 604 * 4);
}

#[test]
fn off_canvas_magnifiers_skip_source_conversion_and_preview_allocation() {
    let mut cache = PreviewCache::default();
    let source = source(80, 60);
    for lens in [
        bounds(-100.0, 10.0, 40.0, 30.0),
        bounds(80.0, 10.0, 40.0, 30.0),
        bounds(20.0, -100.0, 40.0, 30.0),
        bounds(20.0, 60.0, 40.0, 30.0),
    ] {
        assert!(
            cache
                .image(
                    &Magnifier::default(),
                    lens,
                    bounds(0.0, 0.0, 80.0, 60.0),
                    Point::default(),
                    &source,
                    1.0,
                )
                .is_none()
        );
        assert!(cache.source.is_none());
    }
}

#[test]
fn clipped_previews_preserve_sampling_and_ellipse_coverage() {
    let source = source(160, 120);
    let canvas = bounds(30.0, 20.0, 80.0, 60.0);
    let offset = Point::new(11.0, 7.0);
    for lens in [
        bounds(10.0, 30.0, 40.0, 30.0),
        bounds(90.0, 30.0, 40.0, 30.0),
        bounds(50.0, 5.0, 40.0, 30.0),
        bounds(50.0, 65.0, 40.0, 30.0),
        bounds(10.25, 5.5, 140.5, 100.25),
    ] {
        for scale in [0.65, 1.0, 1.75] {
            for zoom in [1.2, 2.7] {
                let magnifier = Magnifier { zoom };
                let mut cache = PreviewCache::default();
                let (full, _) = cache
                    .image(&magnifier, lens, lens, offset, &source, scale)
                    .unwrap();
                let (clipped, region) = cache
                    .image(&magnifier, lens, canvas, offset, &source, scale)
                    .unwrap();
                let full_size = full.size(0);
                let clipped_size = clipped.size(0);
                let left =
                    ((region.x - lens.x) / lens.width * full_size.width.0 as f32).round() as usize;
                let top = ((region.y - lens.y) / lens.height * full_size.height.0 as f32).round()
                    as usize;
                let full_bytes = full.as_bytes(0).unwrap();
                let clipped_bytes = clipped.as_bytes(0).unwrap();
                for y in 0..clipped_size.height.0 as usize {
                    for x in 0..clipped_size.width.0 as usize {
                        let position = Point::new(
                            lens.x
                                + (left as f32 + x as f32 + 0.5) * lens.width
                                    / full_size.width.0 as f32,
                            lens.y
                                + (top as f32 + y as f32 + 0.5) * lens.height
                                    / full_size.height.0 as f32,
                        );
                        if position.x < canvas.x
                            || position.y < canvas.y
                            || position.x >= canvas.x + canvas.width
                            || position.y >= canvas.y + canvas.height
                        {
                            continue;
                        }
                        let expected = ((top + y) * full_size.width.0 as usize + left + x) * 4;
                        let actual = (y * clipped_size.width.0 as usize + x) * 4;
                        let actual = &clipped_bytes[actual..actual + 4];
                        let expected = &full_bytes[expected..expected + 4];
                        // tiny-skia's 4×4 coverage grid can differ by one sample after clipping.
                        assert!(
                            actual[3].abs_diff(expected[3]) <= 16,
                            "lens={lens:?} scale={scale} zoom={zoom} pixel=({x}, {y}): {actual:?} != {expected:?}"
                        );
                        if actual[3] > 0 && expected[3] > 0 {
                            assert!(
                                actual[..3]
                                    .iter()
                                    .zip(&expected[..3])
                                    .all(|(a, b)| a.abs_diff(*b) <= 1),
                                "lens={lens:?} scale={scale} zoom={zoom} pixel=({x}, {y}): {actual:?} != {expected:?}"
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn preview_cache_tracks_visible_region_even_when_texture_size_is_unchanged() {
    let mut cache = PreviewCache::default();
    let source = source(100, 100);
    let magnifier = Magnifier::default();
    let lens = bounds(0.0, 0.0, 100.0, 100.0);
    let first_canvas = bounds(20.0, 20.0, 20.0, 20.0);
    let (first, first_bounds) = cache
        .image(
            &magnifier,
            lens,
            first_canvas,
            Point::default(),
            &source,
            1.0,
        )
        .unwrap();
    let (reused, _) = cache
        .image(
            &magnifier,
            lens,
            first_canvas,
            Point::default(),
            &source,
            1.0,
        )
        .unwrap();
    assert!(Arc::ptr_eq(&first, &reused));
    let (changed, changed_bounds) = cache
        .image(
            &magnifier,
            lens,
            bounds(30.0, 20.0, 20.0, 20.0),
            Point::default(),
            &source,
            1.0,
        )
        .unwrap();
    assert_eq!(first.size(0), changed.size(0));
    assert_ne!(first_bounds, changed_bounds);
    assert!(!Arc::ptr_eq(&first, &changed));
    assert_ne!(first.as_bytes(0), changed.as_bytes(0));
}
