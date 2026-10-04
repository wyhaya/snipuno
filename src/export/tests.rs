use super::{ExportImage, encode_png, save_png, text_element};
use crate::{
    editor::{
        Editor, EffectStrength, GuideOrientation, LineStyle, Point, RedactionStyle, Stroke, Tool,
    },
    erase::brush_background_color,
    image_processing::{blurred_image, render_image},
    magnifier::{Magnifier, PreviewCache},
    mosaic::Mosaic,
    test_support::{TestDir, bounds, renderer, stroke},
};
use image::{Rgba, RgbaImage, imageops};
use std::{collections::HashMap, fs, sync::Arc};

fn snapshot(source: RgbaImage, strokes: Vec<Stroke>) -> ExportImage {
    ExportImage {
        bounds: bounds(0.0, 0.0, source.width() as f32, source.height() as f32),
        source: Arc::new(source),
        strokes,
        overlays: HashMap::new(),
        blurred: HashMap::new(),
        mosaic: None,
        text: HashMap::new(),
        scale: 1.0,
    }
}

fn render(snapshot: ExportImage) -> RgbaImage {
    snapshot.render(renderer()).unwrap()
}

fn filled_shape(tool: Tool, rect: crate::editor::Bounds, color: Option<u32>) -> Stroke {
    let mut shape = stroke(tool, rect);
    shape.fill_color = color;
    shape
}

#[test]
fn cropped_export_png_preserves_pixels_and_transparency() {
    let source = RgbaImage::from_fn(80, 60, |x, y| Rgba([x as u8, y as u8, 90, 128]));
    let expected = imageops::crop_imm(&source, 12, 8, 37, 29).to_image();
    let mut image = snapshot(source, vec![]);
    image.bounds = bounds(12.0, 8.0, 37.0, 29.0);
    image.scale = 0.5;
    let output = render(image);
    assert_eq!(output, expected);
    let png = encode_png(&output).unwrap();
    assert_eq!(
        image::load_from_memory(&png).unwrap().into_rgba8(),
        expected
    );
}

#[test]
fn annotations_and_flipped_images_composite_over_the_cropped_source_at_full_resolution() {
    let source = RgbaImage::from_fn(140, 100, |x, y| Rgba([x as u8, y as u8, 90, 255]));
    let marks = vec![
        stroke(Tool::Rectangle, bounds(10.0, 10.0, 50.0, 40.0)),
        stroke(Tool::Image(7), bounds(50.0, 40.0, -20.0, -20.0)),
    ];
    let mut image = snapshot(source, marks);
    image.bounds = bounds(20.0, 10.0, 80.0, 60.0);
    image.scale = 0.5;
    image.overlays.insert(
        7,
        render_image(RgbaImage::from_fn(20, 20, |x, y| {
            Rgba([10 + x as u8 * 5, 20 + y as u8 * 5, 200, 255])
        })),
    );
    let output = render(image);
    assert_eq!(output.dimensions(), (80, 60));
    assert_eq!(*output.get_pixel(70, 55), Rgba([90, 65, 90, 255]));
    assert_eq!(*output.get_pixel(25, 10), Rgba([224, 32, 64, 255]));
    assert_eq!(*output.get_pixel(31, 21), Rgba([100, 110, 200, 255]));
    assert_eq!(*output.get_pixel(48, 38), Rgba([15, 25, 200, 255]));
}

#[test]
fn partially_visible_layers_export_as_a_crop_of_the_complete_layer() {
    let source = RgbaImage::from_fn(160, 120, |x, y| Rgba([x as u8, y as u8, 90, 255]));
    let overlay = render_image(RgbaImage::from_fn(40, 30, |x, y| {
        Rgba([20 + x as u8 * 4, 30 + y as u8 * 5, 200, 255])
    }));
    let mut layers = [
        Tool::Rectangle,
        Tool::Ellipse,
        Tool::Arrow,
        Tool::Image(7),
        Tool::Redact(RedactionStyle::Blur),
        Tool::Redact(RedactionStyle::Mosaic),
        Tool::Magnifier,
        Tool::Spotlight,
    ]
    .map(|tool| stroke(tool, bounds(0.0, 0.0, 40.0, 30.0)))
    .to_vec();
    for tool in [Tool::RectangleFill, Tool::EllipseFill] {
        for color in [None, Some(0xff123456)] {
            layers.push(filled_shape(tool, bounds(0.0, 0.0, 40.0, 30.0), color));
        }
    }
    for layer in layers {
        let tool = layer.tool;
        for (x, y) in [(20.0, 40.0), (100.0, 40.0), (60.0, 15.0), (60.0, 75.0)] {
            let mut mark = layer.clone();
            mark.start = Point::new(x, y);
            mark.end = Point::new(x + 40.0, y + 30.0);
            if tool == Tool::Magnifier {
                mark.magnifier = Some(Magnifier::default());
            }
            let mut full = snapshot(source.clone(), vec![mark]);
            full.overlays.insert(7, overlay.clone());
            let complete = render(full.clone());
            let expected = imageops::crop_imm(&complete, 40, 30, 80, 60).to_image();
            full.bounds = bounds(40.0, 30.0, 80.0, 60.0);
            full.strokes[0].start.x -= 40.0;
            full.strokes[0].start.y -= 30.0;
            full.strokes[0].end.x -= 40.0;
            full.strokes[0].end.y -= 30.0;
            let output = render(full);
            assert_eq!(output.dimensions(), (80, 60));
            for ((px, py, actual), expected) in output.enumerate_pixels().zip(expected.pixels()) {
                let tolerance = u8::from(tool == Tool::Spotlight);
                assert!(
                    actual
                        .0
                        .iter()
                        .zip(expected.0)
                        .all(|(a, b)| a.abs_diff(b) <= tolerance),
                    "{tool:?} at ({x}, {y}), pixel ({px}, {py}): {actual:?} != {expected:?}"
                );
            }
        }
    }
}

#[test]
fn fully_off_canvas_layers_leave_export_pixels_unchanged() {
    let source = RgbaImage::from_fn(80, 60, |x, y| Rgba([x as u8, y as u8, 90, 255]));
    let mut image = snapshot(
        source.clone(),
        vec![
            stroke(Tool::Rectangle, bounds(-100.0, -100.0, 40.0, 30.0)),
            stroke(Tool::Image(7), bounds(100.0, 100.0, 40.0, 30.0)),
            stroke(
                Tool::Redact(RedactionStyle::Mosaic),
                bounds(-100.0, 10.0, 40.0, 30.0),
            ),
            filled_shape(Tool::RectangleFill, bounds(100.0, 10.0, 40.0, 30.0), None),
            filled_shape(
                Tool::EllipseFill,
                bounds(100.0, 10.0, 40.0, 30.0),
                Some(0xff123456),
            ),
        ],
    );
    image.overlays.insert(
        7,
        render_image(RgbaImage::from_pixel(40, 30, Rgba([255; 4]))),
    );
    assert_eq!(render(image), source);
}

#[test]
fn blur_and_mosaic_are_rendered_even_without_preview_caches() {
    let source = RgbaImage::from_fn(120, 80, |x, _| {
        let value = if x % 12 < 6 { 0 } else { 240 };
        Rgba([value, 40, 80, 255])
    });
    let blur = stroke(
        Tool::Redact(RedactionStyle::Blur),
        bounds(5.0, 5.0, 45.0, 70.0),
    );
    let mosaic = stroke(
        Tool::Redact(RedactionStyle::Mosaic),
        bounds(60.0, 5.0, 50.0, 70.0),
    );
    let blurred = imageops::fast_blur(&source, EffectStrength::Low.blur_sigma());
    let output = render(snapshot(source.clone(), vec![blur, mosaic]));
    assert_eq!(*output.get_pixel(25, 40), *blurred.get_pixel(25, 40));
    assert_ne!(*output.get_pixel(25, 40), *source.get_pixel(25, 40));
    for x in 72..84 {
        assert_eq!(*output.get_pixel(x, 40), Rgba([120, 40, 80, 255]));
    }
    assert_eq!(*output.get_pixel(55, 40), *source.get_pixel(55, 40));
}

#[test]
fn ready_effect_caches_produce_the_same_pixels_as_uncached_export() {
    let source = RgbaImage::from_fn(120, 80, |x, y| Rgba([x as u8, y as u8, 80, 255]));
    let marks = vec![
        stroke(
            Tool::Redact(RedactionStyle::Blur),
            bounds(5.0, 5.0, 45.0, 70.0),
        ),
        stroke(
            Tool::Redact(RedactionStyle::Mosaic),
            bounds(60.0, 5.0, 50.0, 70.0),
        ),
    ];
    let mut image = snapshot(source, marks);
    let expected = render(image.clone());
    image.blurred.insert(
        EffectStrength::Low,
        blurred_image(&image.source, EffectStrength::Low),
    );
    image.mosaic = Some(Arc::new(Mosaic::new(&image.source)));
    assert_eq!(render(image), expected);
}

#[test]
fn reused_masks_keep_disjoint_effects_independent() {
    let source = RgbaImage::from_fn(240, 100, |x, y| {
        Rgba([(x * 7) as u8, (y * 11) as u8, (x + y) as u8, 255])
    });
    let mut lens = stroke(Tool::Magnifier, bounds(180.0, 30.0, 40.0, 40.0));
    lens.magnifier = Some(Magnifier::default());
    let strokes = vec![
        stroke(
            Tool::Redact(RedactionStyle::Blur),
            bounds(10.0, 20.0, 50.0, 60.0),
        ),
        stroke(
            Tool::Redact(RedactionStyle::Mosaic),
            bounds(90.0, 20.0, 50.0, 60.0),
        ),
        lens,
    ];
    let mut expected = source.clone();
    for stroke in &strokes {
        let isolated = render(snapshot(source.clone(), vec![stroke.clone()]));
        for ((expected, actual), original) in expected
            .pixels_mut()
            .zip(isolated.pixels())
            .zip(source.pixels())
        {
            if actual != original {
                *expected = *actual;
            }
        }
    }
    assert_eq!(render(snapshot(source, strokes)), expected);
}

#[test]
fn spotlight_holes_union_and_dim_the_source_once() {
    let image = snapshot(
        RgbaImage::from_pixel(140, 100, Rgba([224, 32, 64, 255])),
        vec![
            stroke(Tool::Spotlight, bounds(10.0, 10.0, 70.0, 60.0)),
            stroke(Tool::Spotlight, bounds(50.0, 30.0, 70.0, 60.0)),
        ],
    );
    let output = render(image);
    for (x, y) in [(30, 30), (60, 50), (100, 60)] {
        assert_eq!(*output.get_pixel(x, y), Rgba([224, 32, 64, 255]));
    }
    let outside = output.get_pixel(130, 50);
    assert!((outside[0] as i32 - 134).abs() <= 1);
    assert!((outside[1] as i32 - 19).abs() <= 1);
}

#[test]
fn spotlight_leaves_annotation_layers_undimmed_regardless_of_creation_order() {
    let source = RgbaImage::from_fn(140, 100, |x, _| {
        let value = if x % 12 < 6 { 0 } else { 240 };
        Rgba([value, 40, 80, 255])
    });
    let spotlight = stroke(Tool::Spotlight, bounds(100.0, 10.0, 30.0, 60.0));
    let mut layers = [
        Tool::Rectangle,
        Tool::Redact(RedactionStyle::Blur),
        Tool::Redact(RedactionStyle::Mosaic),
        Tool::Image(1),
        Tool::Magnifier,
    ]
    .map(|tool| stroke(tool, bounds(10.0, 20.0, 70.0, 60.0)))
    .to_vec();
    for tool in [Tool::RectangleFill, Tool::EllipseFill] {
        for color in [None, Some(0xff123456)] {
            layers.push(filled_shape(tool, bounds(10.0, 20.0, 70.0, 60.0), color));
        }
    }
    for mut mark in layers {
        let tool = mark.tool;
        if tool == Tool::Magnifier {
            mark.magnifier = Some(Magnifier::default());
        }
        let mut image = snapshot(source.clone(), vec![mark]);
        image.overlays.insert(
            1,
            render_image(RgbaImage::from_pixel(70, 60, Rgba([224, 32, 64, 255]))),
        );
        let expected = render(image.clone());
        let (x, y) = if tool == Tool::Rectangle {
            (45, 20)
        } else {
            (25, 40)
        };
        for index in [0, 1] {
            let mut image = image.clone();
            image.strokes.insert(index, spotlight.clone());
            let output = render(image);
            assert_eq!(
                output.get_pixel(x, y),
                expected.get_pixel(x, y),
                "{tool:?}, spotlight at index {index}"
            );
            assert_eq!(*output.get_pixel(90, 50), Rgba([144, 24, 48, 255]));
            assert_eq!(output.get_pixel(110, 50), source.get_pixel(110, 50));
        }
    }
}

#[test]
fn glass_magnifier_preview_and_export_agree_on_colors_after_cropping() {
    for alpha in [128, 255] {
        let source = RgbaImage::from_fn(140, 120, |x, y| {
            Rgba([(x * 7) as u8, (y * 11) as u8, (x + y) as u8, alpha])
        });
        let lens = bounds(20.0, 10.0, 60.0, 60.0);
        let offset = Point::new(12.0, 8.0);
        let magnifier = Magnifier::default();
        let canvas = bounds(0.0, 0.0, 100.0, 80.0);
        let (preview, placement) = PreviewCache::default()
            .image(
                &magnifier,
                lens,
                canvas,
                offset,
                &render_image(source.clone()),
                1.0,
            )
            .unwrap();
        let mut mark = stroke(Tool::Magnifier, lens);
        mark.magnifier = Some(magnifier);
        let mut image = snapshot(source, vec![mark]);
        image.bounds = bounds(offset.x, offset.y, canvas.width, canvas.height);
        let exported = render(image);
        let width = preview.size(0).width.0 as usize;
        for (index, pixel) in preview
            .as_bytes(0)
            .unwrap()
            .as_chunks::<4>()
            .0
            .iter()
            .enumerate()
        {
            if pixel[3] != 255 {
                continue;
            }
            let x = placement.x as u32 + (index % width) as u32;
            let y = placement.y as u32 + (index / width) as u32;
            let actual = exported.get_pixel(x, y).0;
            let expected = [pixel[2], pixel[1], pixel[0], pixel[3]];
            assert!(
                actual
                    .into_iter()
                    .zip(expected)
                    .all(|(a, b)| a.abs_diff(b) <= 2),
                "alpha={alpha}, pixel ({x}, {y}): {actual:?} != {expected:?}"
            );
        }
    }
}

#[test]
fn magnifier_samples_original_pixels_at_its_center_with_crop_offset_and_round_mask() {
    let source = RgbaImage::from_fn(160, 120, |x, y| Rgba([x as u8, y as u8, 100, 255]));
    for zoom in [1.5, 2.0, 2.7, 4.0] {
        let mut lens = stroke(Tool::Magnifier, bounds(20.0, 20.0, 60.0, 60.0));
        lens.magnifier = Some(Magnifier { zoom });
        let mut underlay = filled_shape(
            Tool::RectangleFill,
            bounds(25.0, 25.0, 50.0, 50.0),
            Some(0xffe02040),
        );
        underlay.fill_opacity = 0.2;
        let mut image = snapshot(source.clone(), vec![underlay, lens]);
        image.bounds = bounds(30.0, 10.0, 120.0, 100.0);
        let output = render(image);
        for (x, y) in [(50, 50), (35, 50), (65, 50), (50, 35), (50, 65)] {
            let pixel = output.get_pixel(x, y);
            let expected_x = 80.0 + (x as f32 - 50.0) / zoom;
            let expected_y = 60.0 + (y as f32 - 50.0) / zoom;
            assert!(
                (pixel[0] as f32 - expected_x).abs() <= 1.0,
                "zoom {zoom}: {pixel:?}"
            );
            assert!(
                (pixel[1] as f32 - expected_y).abs() <= 1.0,
                "zoom {zoom}: {pixel:?}"
            );
            assert_eq!(pixel[2], 100);
        }
        assert_eq!(*output.get_pixel(21, 21), *source.get_pixel(51, 31));
        assert_eq!(*output.get_pixel(79, 79), *source.get_pixel(109, 89));
    }
}

#[test]
fn filled_shapes_export_solid_and_background_colors_with_shape_masks_after_crop() {
    for tool in [Tool::RectangleFill, Tool::EllipseFill] {
        for color in [None, Some(0xff123456)] {
            let source = RgbaImage::from_fn(80, 40, |x, y| {
                Rgba(
                    if (30..50).contains(&x) && (12..20).contains(&y) && x % 4 == 0 {
                        [240, 20, 20, 255]
                    } else {
                        [31, 36, 40, 255]
                    },
                )
            });
            let mut mark = filled_shape(tool, bounds(10.0, 10.0, 20.0, 20.0), color);
            mark.color = 0xff00ff00;
            mark.width = 15.0;
            let mut image = snapshot(source.clone(), vec![mark]);
            image.bounds = bounds(20.0, 0.0, 40.0, 40.0);
            for scale in [0.5, 1.0, 2.0] {
                image.scale = scale;
                let output = render(image.clone());
                let fill = if color.is_none() {
                    [31, 36, 40, 255]
                } else {
                    [18, 52, 86, 255]
                };
                for (x, y) in [(20, 20), (15, 20), (25, 20), (20, 15), (20, 25)] {
                    assert_eq!(
                        *output.get_pixel(x, y),
                        Rgba(fill),
                        "{tool:?} {color:?} {scale}"
                    );
                }
                for (x, y) in [(8, 20), (31, 20), (20, 8), (20, 31)] {
                    assert_eq!(*output.get_pixel(x, y), *source.get_pixel(x + 20, y));
                }
                if tool == Tool::EllipseFill {
                    for (x, y) in [(11, 11), (28, 11), (11, 28), (28, 28)] {
                        assert_eq!(*output.get_pixel(x, y), *source.get_pixel(x + 20, y));
                    }
                }
            }
        }
    }
}

#[test]
fn automatic_brush_erases_along_its_path_after_crop_at_each_display_scale() {
    for background in [[255; 4], [31, 36, 40, 255]] {
        let source = RgbaImage::from_fn(100, 80, |x, y| {
            Rgba(
                if ((50..80).contains(&x) && (24..27).contains(&y))
                    || ((69..72).contains(&x) && (25..45).contains(&y))
                    || ((55..58).contains(&x) && (35..38).contains(&y))
                {
                    [240, 20, 20, 255]
                } else if x < 40 {
                    [0, 120, 200, 255]
                } else {
                    background
                },
            )
        });
        for scale in [0.5, 1.0, 2.0] {
            let mut editor = Editor::new(100.0, 80.0);
            assert!(editor.crop(bounds(40.0, 10.0, 50.0, 50.0)));
            let point = Point::new(10.0, 15.0);
            editor.begin(Tool::Brush, point, 0x4000ff00, 8.0);
            let color = brush_background_color(&source, point, 8.0, Point::new(40.0, 10.0));
            assert!(editor.set_brush_auto_color(color));
            editor.update(Point::new(30.0, 15.0), scale);
            assert!(editor.finish(Point::new(30.0, 35.0), scale));
            let mut image = snapshot(source.clone(), editor.strokes().to_vec());
            image.bounds = editor.image_bounds();
            image.scale = scale;
            let output = render(image);
            for (x, y) in [(15, 15), (25, 15), (30, 20), (30, 30)] {
                assert_eq!(*output.get_pixel(x, y), Rgba(background), "{scale}");
            }
            for (x, y) in [(16, 26), (0, 0), (40, 40)] {
                assert_eq!(
                    output.get_pixel(x, y),
                    source.get_pixel(x + 40, y + 10),
                    "{scale}"
                );
            }
        }
    }
}

#[test]
fn fill_opacity_is_applied_once_across_the_interior() {
    for tool in [Tool::RectangleFill, Tool::EllipseFill] {
        for opacity in [0.2, 0.5, 1.0] {
            let mut shape = filled_shape(tool, bounds(5.0, 5.0, 30.0, 30.0), Some(0xffe02040));
            shape.fill_opacity = opacity;
            let image = snapshot(
                RgbaImage::from_pixel(40, 40, Rgba([255; 4])),
                vec![shape.clone()],
            );
            let output = render(image);
            let center = output.get_pixel(20, 20);
            for y in 12..28 {
                for x in 12..28 {
                    assert_eq!(output.get_pixel(x, y), center);
                }
            }
            assert_eq!(*output.get_pixel(0, 0), Rgba([255; 4]));
            let transparent = render(snapshot(RgbaImage::new(40, 40), vec![shape]));
            assert_eq!(
                transparent.get_pixel(20, 20)[3],
                (opacity * 255.0).round() as u8
            );
        }
    }
}

#[test]
fn automatic_and_solid_fill_opacity_blend_original_pixels_at_each_step() {
    let source = RgbaImage::from_fn(40, 40, |x, y| {
        Rgba(if (16..24).contains(&x) && (16..24).contains(&y) {
            [80, 40, 20, 255]
        } else {
            [250, 250, 250, 255]
        })
    });
    for tool in [Tool::RectangleFill, Tool::EllipseFill] {
        for color in [None, Some(0xff0c78c8)] {
            for step in 0..=10 {
                let opacity = step as f32 / 10.0;
                let mut shape = filled_shape(tool, bounds(5.0, 5.0, 30.0, 30.0), color);
                shape.color = 0;
                shape.fill_opacity = opacity;
                let output = render(snapshot(source.clone(), vec![shape]));
                let fill = if color.is_none() {
                    [250, 250, 250]
                } else {
                    [12, 120, 200]
                };
                let alpha = (opacity * 255.0).round() / 255.0;
                for (channel, base) in [80.0, 40.0, 20.0].into_iter().enumerate() {
                    let expected =
                        (base * (1.0 - alpha) + fill[channel] as f32 * alpha).round() as u8;
                    assert!(
                        output.get_pixel(20, 20)[channel].abs_diff(expected) <= 1,
                        "{tool:?} {color:?} {step}: {:?}",
                        output.get_pixel(20, 20)
                    );
                }
                assert_eq!(output.get_pixel(20, 20)[3], 255);
                assert_eq!(output.get_pixel(0, 0), source.get_pixel(0, 0));
            }
        }
    }
}

#[test]
fn export_cache_reuses_unchanged_results_and_invalidates_each_document_input() {
    let mut original = snapshot(
        RgbaImage::from_fn(48, 40, |x, y| {
            Rgba([(x * 17) as u8, (y * 13) as u8, 80, 255])
        }),
        vec![
            stroke(Tool::Image(7), bounds(2.0, 2.0, 8.0, 8.0)),
            stroke(
                Tool::Redact(RedactionStyle::Blur),
                bounds(16.0, 8.0, 24.0, 24.0),
            ),
        ],
    );
    original.overlays.insert(
        7,
        render_image(RgbaImage::from_pixel(8, 8, Rgba([255, 0, 0, 255]))),
    );
    let first = original.clone().prepare(renderer(), None).unwrap();
    let png = first.png().unwrap();
    let repeated = original
        .clone()
        .prepare(renderer(), Some(first.clone()))
        .unwrap();
    assert!(Arc::ptr_eq(&first.pixels(), &repeated.pixels()));
    assert!(std::ptr::eq(png, repeated.png().unwrap()));

    let mut variants = Vec::new();
    let mut changed = original.clone();
    changed.source = Arc::new(RgbaImage::from_pixel(48, 40, Rgba([0, 0, 255, 255])));
    variants.push(("source", changed));
    let mut changed = original.clone();
    changed.bounds = bounds(4.0, 2.0, 40.0, 32.0);
    variants.push(("crop", changed));
    let mut changed = original.clone();
    changed.strokes[1].tool = Tool::RectangleFill;
    changed.strokes[1].fill_color = Some(0xff123456);
    variants.push(("annotation", changed));
    let mut changed = original.clone();
    changed.strokes[1].effect_strength = EffectStrength::Max;
    variants.push(("strength", changed));
    let mut changed = original.clone();
    changed.overlays.insert(
        7,
        render_image(RgbaImage::from_pixel(8, 8, Rgba([0, 255, 0, 255]))),
    );
    variants.push(("overlay", changed));
    let mut changed = original.clone();
    changed.text.insert(
        1,
        "<rect x=\"20\" y=\"12\" width=\"8\" height=\"8\" fill=\"blue\"/>".into(),
    );
    variants.push(("text payload", changed));
    let mut changed = original.clone();
    changed.scale = 2.0;
    variants.push(("scale", changed));
    for (name, image) in variants {
        let expected = render(image.clone());
        let cached = image.prepare(renderer(), Some(first.clone())).unwrap();
        assert!(!Arc::ptr_eq(&first.pixels(), &cached.pixels()), "{name}");
        assert_eq!(*cached.pixels(), expected, "{name}");
        assert_eq!(
            image::load_from_memory(cached.png().unwrap())
                .unwrap()
                .into_rgba8(),
            expected,
            "{name}"
        );
    }
}

#[test]
fn invalid_magnifiers_report_errors_instead_of_disappearing_from_export() {
    for (lens, zoom) in [
        (bounds(5.0, 5.0, 30.0, 30.0), 0.0),
        (bounds(5.0, 5.0, 30.0, 30.0), f32::NAN),
        (bounds(5.0, 5.0, 30.0, 30.0), f32::INFINITY),
        (bounds(5.0, 5.0, 0.0, 30.0), 2.0),
        (bounds(f32::INFINITY, 5.0, 30.0, 30.0), 2.0),
    ] {
        let mut mark = stroke(Tool::Magnifier, lens);
        mark.magnifier = Some(Magnifier { zoom });
        let image = snapshot(RgbaImage::new(40, 40), vec![mark]);
        assert!(
            image.prepare(renderer(), None).is_err(),
            "{lens:?}, zoom={zoom}"
        );
    }
}

#[test]
fn unavailable_image_and_incomplete_magnifier_report_errors() {
    for tool in [Tool::Image(99), Tool::Magnifier] {
        let image = snapshot(
            RgbaImage::new(40, 40),
            vec![stroke(tool, bounds(5.0, 5.0, 30.0, 30.0))],
        );
        assert!(image.prepare(renderer(), None).is_err());
    }
}

#[test]
fn save_round_trips_alpha_and_rejects_wrong_extensions_without_overwriting() {
    let dir = TestDir::new();
    let pixels = RgbaImage::from_fn(3, 2, |x, y| {
        Rgba([x as u8 * 80, y as u8 * 90, 40, (x * 100) as u8])
    });
    let cache = snapshot(pixels.clone(), vec![])
        .prepare(renderer(), None)
        .unwrap();
    assert_eq!(*cache.pixels(), pixels);
    for name in ["image.png", "image.PNG", "image"] {
        let path = dir.join(name);
        cache.save(&path).unwrap();
        assert_eq!(
            image::load_from_memory(&fs::read(path).unwrap())
                .unwrap()
                .into_rgba8(),
            pixels
        );
    }
    for name in ["image.jpg", "image.webp"] {
        let path = dir.join(name);
        fs::write(&path, b"keep this file").unwrap();
        assert!(cache.save(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"keep this file");
    }
    assert!(save_png(cache.png().unwrap(), &dir.join("missing/image.png")).is_err());
}

#[test]
fn svg_text_escapes_content_and_font_attributes() {
    let svg = text_element("中文 <>&\"'", "A\"&<B>", 20.0, 0.0, 20.0, 0xff123456, false);
    assert!(svg.contains(">中文 &lt;&gt;&amp;&quot;&apos;</text>"));
    assert!(svg.contains("font-family=\"A&quot;&amp;&lt;B&gt;\""));
    assert!(svg.contains("fill=\"#123456\""));
}

#[test]
fn vector_exports_color_the_annotation_and_preserve_unmarked_pixels() {
    for (tool, rect, painted, untouched) in [
        (
            Tool::Rectangle,
            bounds(20.0, 20.0, 60.0, 60.0),
            (50, 20),
            (50, 50),
        ),
        (
            Tool::Ellipse,
            bounds(20.0, 20.0, 60.0, 60.0),
            (50, 20),
            (20, 20),
        ),
        (
            Tool::Arrow,
            bounds(20.0, 50.0, 60.0, 0.0),
            (50, 50),
            (50, 40),
        ),
        (
            Tool::Line(LineStyle::Solid),
            bounds(20.0, 50.0, 60.0, 0.0),
            (50, 50),
            (50, 40),
        ),
        (
            Tool::Brush,
            bounds(20.0, 50.0, 60.0, 0.0),
            (50, 50),
            (50, 40),
        ),
        (
            Tool::Number(1),
            bounds(20.0, 20.0, 60.0, 60.0),
            (50, 50),
            (90, 90),
        ),
        (
            Tool::Guide(GuideOrientation::Horizontal),
            bounds(20.0, 50.5, 60.0, 0.0),
            (50, 50),
            (90, 50),
        ),
    ] {
        let mut mark = stroke(tool, rect);
        if tool == Tool::Brush {
            mark.points = Arc::new(vec![Point::new(20.0, 50.0), Point::new(80.0, 50.0)]);
        }
        let output = render(snapshot(
            RgbaImage::from_pixel(100, 100, Rgba([255; 4])),
            vec![mark],
        ));
        assert_eq!(
            *output.get_pixel(painted.0, painted.1),
            Rgba([224, 32, 64, 255]),
            "{tool:?}"
        );
        assert_eq!(
            *output.get_pixel(untouched.0, untouched.1),
            Rgba([255; 4]),
            "{tool:?}"
        );
    }
}

#[test]
fn dashed_and_wavy_exports_have_gaps_and_visible_oscillation() {
    let source = RgbaImage::from_pixel(100, 80, Rgba([255; 4]));
    let dashed = render(snapshot(
        source.clone(),
        vec![stroke(
            Tool::Line(LineStyle::Dashed),
            bounds(10.0, 40.0, 80.0, 0.0),
        )],
    ));
    assert_eq!(*dashed.get_pixel(12, 40), Rgba([224, 32, 64, 255]));
    assert_eq!(*dashed.get_pixel(22, 40), Rgba([255; 4]));
    let wavy = render(snapshot(
        source,
        vec![stroke(
            Tool::Line(LineStyle::Wavy),
            bounds(10.0, 40.0, 80.0, 0.0),
        )],
    ));
    for row in [36, 44] {
        assert!((10..90).any(|x| wavy.get_pixel(x, row)[1] < 100));
    }
    assert_eq!(*wavy.get_pixel(50, 20), Rgba([255; 4]));
}
