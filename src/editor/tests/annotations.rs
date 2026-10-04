use super::*;
use crate::{
    dimensions::{DimensionMode, PixelScale},
    measurement::Interval,
};

#[test]
fn numbers_follow_existing_labels_across_delete_undo_and_overflow() {
    let mut editor = Editor::new(100.0, 80.0);
    for number in 1..=3 {
        assert!(editor.add_number(Point::new(50.0, 40.0), 20.0, 0, TailDirection::default()));
        assert_eq!(editor.selected().unwrap().tool, Tool::Number(number));
    }
    assert!(editor.delete_selected());
    assert_eq!(editor.next_number(), Some(3));
    assert!(editor.undo());
    assert_eq!(editor.next_number(), Some(4));
    editor.strokes.last_mut().unwrap().tool = Tool::Number(u32::MAX);
    assert_eq!(editor.next_number(), None);
    assert!(!editor.add_number(Point::default(), 20.0, 0, TailDirection::default()));
}

#[test]
fn image_placement_limits_initial_size_and_clamps_position() {
    for (width, height, center, expected) in [
        (
            60.0,
            40.0,
            Point::new(50.0, 40.0),
            bounds(20.0, 20.0, 60.0, 40.0),
        ),
        (
            90.0,
            60.0,
            Point::new(50.0, 40.0),
            bounds(10.0, 13.0, 80.0, 160.0 / 3.0),
        ),
        (
            100.0,
            80.0,
            Point::new(50.0, 40.0),
            bounds(10.0, 8.0, 80.0, 64.0),
        ),
        (
            35.0,
            70.0,
            Point::new(50.0, 40.0),
            bounds(34.0, 8.0, 32.0, 64.0),
        ),
        (
            200.0,
            100.0,
            Point::new(50.0, 40.0),
            bounds(10.0, 20.0, 80.0, 40.0),
        ),
        (
            8.0,
            8.0,
            Point::new(30.0, 20.0),
            bounds(18.0, 8.0, 24.0, 24.0),
        ),
        (
            20.0,
            10.0,
            Point::new(-10.0, 100.0),
            bounds(0.0, 56.0, 48.0, 24.0),
        ),
    ] {
        let mut editor = Editor::new(100.0, 80.0);
        assert!(editor.add_image_at(7, width, height, center));
        let placed = editor.selected().unwrap().bounds();
        assert_eq!((placed.x, placed.y), (expected.x, expected.y));
        assert!((placed.width - expected.width).abs() < 0.0001, "{placed:?}");
        assert!(
            (placed.height - expected.height).abs() < 0.0001,
            "{placed:?}"
        );
    }
    for (width, height) in [
        (0.0, 10.0),
        (10.0, -1.0),
        (f32::NAN, 10.0),
        (10.0, f32::INFINITY),
    ] {
        assert!(!Editor::new(100.0, 80.0).add_image(7, width, height));
    }
}

#[test]
fn image_placement_prioritizes_canvas_limit_when_minimum_conflicts() {
    for (canvas_width, canvas_height, width, height, expected_width, expected_height) in [
        (100.0, 80.0, 1000.0, 100.0, 80.0, 8.0),
        (100.0, 80.0, 10.0, 100.0, 6.4, 64.0),
        (20.0, 20.0, 8.0, 8.0, 16.0, 16.0),
        (20.0, 100.0, 8.0, 4.0, 16.0, 8.0),
        (100.0, 20.0, 4.0, 8.0, 8.0, 16.0),
    ] {
        let mut editor = Editor::new(canvas_width, canvas_height);
        assert!(editor.add_image(7, width, height));
        let placed = editor.selected().unwrap().bounds();
        assert!((placed.width - expected_width).abs() < 0.0001, "{placed:?}");
        assert!(
            (placed.height - expected_height).abs() < 0.0001,
            "{placed:?}"
        );
    }
}

#[test]
fn image_placement_preserves_aspect_ratio_across_pixel_grids() {
    for grid in [
        PixelGrid::default(),
        PixelGrid::new(1.25, 1.5),
        PixelGrid::new(2.0, 2.0),
    ] {
        for (canvas_width, canvas_height, width, height, expected_width, expected_height) in [
            (101.0, 81.0, 101.0, 81.0, 80.8, 64.8),
            (100.0, 80.0, 8.0, 8.0, 24.0, 24.0),
            (100.0, 80.0, 8.0, 4.0, 48.0, 24.0),
            (100.0, 80.0, 1.0, 1000.0, 0.064, 64.0),
            (1.0, 1.0, 1000.0, 1.0, 0.8, 0.0008),
        ] {
            let mut editor = Editor::new(canvas_width, canvas_height);
            editor.set_grid(grid);
            assert!(editor.add_image(7, width, height));
            let placed = editor.selected().unwrap().bounds();
            assert!((placed.width - expected_width).abs() < 0.0001, "{placed:?}");
            assert!(
                (placed.height - expected_height).abs() < 0.0001,
                "{placed:?}"
            );
            assert!((placed.width / placed.height / (width / height) - 1.0).abs() < 0.0001);
            assert!(placed.width <= canvas_width * 0.8 + 0.0001);
            assert!(placed.height <= canvas_height * 0.8 + 0.0001);
        }
    }
}

#[test]
fn text_edits_keep_content_and_position_and_reject_invalid_input() {
    let mut editor = selected_editor(Tool::Text);
    let before = editor.strokes().to_vec();
    let mut changed = text("Second line\n中文 & <text>");
    changed.bubble = true;
    changed.tail = TailDirection::TopRight;
    assert!(editor.set_selected_text(changed.clone(), 0xff123456, Point::new(500.0, 100.0)));
    assert_eq!(editor.selected().unwrap().start, before[0].start);
    assert_eq!(editor.selected().unwrap().text, Some(changed.clone()));
    assert_reversible(&mut editor, &before);
    let valid = editor.strokes().to_vec();
    for invalid in [
        text(" \n "),
        TextAnnotation {
            font_size: 0.0,
            ..changed.clone()
        },
        TextAnnotation {
            wrap_width: f32::NAN,
            ..changed.clone()
        },
    ] {
        assert!(!editor.set_selected_text(invalid, 0, Point::new(80.0, 40.0)));
        assert_eq!(editor.strokes(), valid);
    }
    assert!(!editor.set_selected_text(changed, 0, Point::new(f32::INFINITY, 40.0)));
    assert_eq!(editor.strokes(), valid);
}

#[test]
fn redaction_changes_keep_region_and_are_individually_undoable() {
    let mut editor = selected_editor(Tool::Redact(RedactionStyle::Blur));
    for style in [RedactionStyle::Mosaic, RedactionStyle::Blur] {
        let before = editor.strokes().to_vec();
        assert!(editor.set_redaction_style(style));
        assert_eq!(editor.selected().unwrap().bounds(), before[0].bounds());
        assert_eq!(editor.selected().unwrap().tool, Tool::Redact(style));
        assert_reversible(&mut editor, &before);
    }
}

#[test]
fn fill_color_changes_preserve_shape_and_are_undoable() {
    for tool in [Tool::RectangleFill, Tool::EllipseFill] {
        let mut editor = selected_editor(tool);
        let shape = editor.selected().unwrap().clone();
        assert!(shape.is_filled_shape());
        assert_eq!(shape.fill_color, None);
        for color in [Some(0xff123456), None] {
            let before = editor.strokes().to_vec();
            assert!(editor.set_fill_color(color));
            assert_reversible(&mut editor, &before);
            assert!(!editor.selected().unwrap().tool.has_width());
            assert!(!editor.set_selected_width(9.0));
            assert!(!editor.set_selected_color(0xffabcdef));
            let changed = editor.selected().unwrap();
            assert_eq!(changed.tool, shape.tool);
            assert_eq!(changed.color, shape.color);
            assert_eq!(changed.width, shape.width);
            assert_eq!(changed.bounds(), shape.bounds());
            assert_eq!(changed.fill_color, color);
        }
    }
    for tool in [Tool::Rectangle, Tool::Ellipse, Tool::Arrow, Tool::Brush] {
        let mut editor = selected_editor(tool);
        let before = editor.strokes().to_vec();
        assert!(!editor.set_fill_color(Some(0xff123456)));
        assert_eq!(editor.strokes(), before);
    }
}

#[test]
fn filled_shapes_default_to_auto_and_preserve_constrained_size_and_move() {
    for tool in [Tool::RectangleFill, Tool::EllipseFill] {
        let mut editor = Editor::new(100.0, 100.0);
        editor.begin(tool, Point::new(10.0, 10.0), 0xffabcdef, 5.0);
        assert_eq!(editor.draft().unwrap().fill_color, None);
        assert_eq!(editor.draft().unwrap().fill_opacity, 1.0);
        assert!(editor.finish_constrained(Point::new(50.0, 35.0), 1.0, true));
        let filled = editor.selected().unwrap().clone();
        assert_eq!(filled.bounds(), bounds(10.0, 10.0, 40.0, 40.0));
        assert_eq!(filled.paint_bounds(1.0), filled.bounds());
        editor.begin_move(Point::new(30.0, 30.0), 1.0);
        assert!(editor.is_dragging());
        assert!(editor.finish(Point::new(40.0, 40.0), 1.0));
        assert_eq!(
            editor.selected().unwrap().bounds(),
            bounds(20.0, 20.0, 40.0, 40.0)
        );
        assert_eq!(editor.selected().unwrap().fill_color, None);
        assert!(editor.undo());
        assert_eq!(editor.selected().unwrap(), &filled);
    }
}

#[test]
fn fill_opacity_edits_validate_values_and_preserve_other_properties() {
    for tool in [Tool::RectangleFill, Tool::EllipseFill] {
        let mut editor = selected_editor(tool);
        let original = editor.selected().unwrap().clone();
        for opacity in [0.2, 0.5, 0.0, 1.0] {
            let before = editor.strokes().to_vec();
            assert!(editor.set_fill_opacity(opacity));
            let mut expected = original.clone();
            expected.fill_opacity = opacity;
            assert_eq!(editor.selected().unwrap(), &expected);
            assert_reversible(&mut editor, &before);
        }
        let before = editor.strokes().to_vec();
        for opacity in [-0.1, 1.1, f32::NAN, f32::INFINITY] {
            assert!(!editor.set_fill_opacity(opacity));
            editor.preview_fill_opacity(opacity);
            assert_eq!(editor.strokes(), before);
        }
    }
    for tool in [Tool::Rectangle, Tool::Ellipse, Tool::Arrow, Tool::Brush] {
        let mut editor = selected_editor(tool);
        let before = editor.strokes().to_vec();
        assert!(!editor.set_fill_opacity(0.5));
        editor.preview_fill_opacity(0.5);
        assert_eq!(editor.strokes(), before);
    }
}

#[test]
fn style_edits_change_only_supported_properties() {
    for tool in [
        Tool::Rectangle,
        Tool::Ellipse,
        Tool::Arrow,
        Tool::Brush,
        Tool::Text,
    ] {
        let mut editor = selected_editor(tool);
        let before = editor.strokes().to_vec();
        assert!(editor.set_selected_color(0xff123456));
        let mut expected = before[0].clone();
        expected.color = 0xff123456;
        assert_eq!(editor.strokes(), [expected]);
        assert_reversible(&mut editor, &before);
    }
    for tool in [
        Tool::Redact(RedactionStyle::Blur),
        Tool::Redact(RedactionStyle::Mosaic),
        Tool::Spotlight,
        Tool::Magnifier,
        Tool::Image(7),
    ] {
        let mut editor = selected_editor(tool);
        let before = editor.strokes().to_vec();
        assert!(!editor.set_selected_color(0xff123456));
        assert!(!editor.set_selected_width(9.0));
        assert_eq!(editor.strokes(), before);
    }
}

#[test]
fn brush_auto_color_stays_opaque_and_fixed_and_color_changes_are_undoable() {
    let mut editor = Editor::new(100.0, 80.0);
    editor.begin(Tool::Brush, Point::new(20.0, 20.0), 0x40123456, 3.0);
    assert!(!editor.draft().unwrap().brush_auto_color);
    assert!(editor.set_brush_auto_color(Some(0x1f2428)));
    for point in [Point::new(40.0, 20.0), Point::new(40.0, 40.0)] {
        editor.update(point, 1.0);
        assert_eq!(editor.draft().unwrap().paint_color(), 0xff1f2428);
        assert_eq!(editor.draft().unwrap().paint_opacity(), 1.0);
    }
    assert!(editor.finish(Point::new(40.0, 40.0), 1.0));
    let before = editor.strokes().to_vec();
    assert!(editor.set_selected_width(12.0));
    assert_eq!(editor.selected().unwrap().color, 0xff1f2428);
    assert_reversible(&mut editor, &before);
    let before = editor.strokes().to_vec();
    assert!(editor.set_selected_color(0xff1f2428));
    assert!(!editor.selected().unwrap().brush_auto_color);
    assert_reversible(&mut editor, &before);
    let before = editor.strokes().to_vec();
    assert!(editor.set_brush_auto_color(Some(0xffffff)));
    assert!(editor.selected().unwrap().brush_auto_color);
    assert_reversible(&mut editor, &before);
    assert!(editor.set_brush_auto_color(None));
    assert_eq!(editor.selected().unwrap().paint_opacity(), 0.0);
    let mut rectangle = selected_editor(Tool::Rectangle);
    let before = rectangle.strokes().to_vec();
    assert!(!rectangle.set_brush_auto_color(Some(0xffffff)));
    assert_eq!(rectangle.strokes(), before);
}

#[test]
fn measurement_length_survives_move_crop_and_unit_changes() {
    let mut editor = Editor::new(400.0, 300.0);
    let interval = Interval {
        start: Point::new(100.0, 80.0),
        end: Point::new(220.0, 80.0),
    };
    let mark = editor
        .measurement_preview(interval, MeasurementAxis::X, 1.0, 30.0)
        .unwrap();
    editor.add_measurement(mark);
    let before = editor.strokes().to_vec();
    editor.begin_move(Point::new(160.0, 80.0), 1.0);
    assert!(editor.finish(Point::new(170.0, 100.0), 1.0));
    assert_reversible(&mut editor, &before);
    assert!(editor.crop(bounds(10.0, 20.0, 300.0, 200.0)));
    let mark = &editor.strokes()[0];
    assert_eq!((mark.start, mark.end), (interval.start, interval.end));
    for (mode, expected) in [
        (DimensionMode::Logical, "60"),
        (DimensionMode::Original, "120"),
    ] {
        assert_eq!(
            mark.measurement_text(PixelScale::new(2.0, 2.0).display(mode))
                .as_deref(),
            Some(expected)
        );
    }
}

#[test]
fn guides_place_full_or_partial_intervals_without_adding_preview_history() {
    let mut editor = Editor::new(100.0, 80.0);
    let full = editor
        .guide_preview(GuideOrientation::Horizontal, Point::new(30.0, 40.0), 1.0)
        .unwrap();
    assert_eq!(
        (full.start, full.end),
        (Point::new(0.0, 40.0), Point::new(100.0, 40.0))
    );
    let interval = Interval {
        start: Point::new(30.0, 20.0),
        end: Point::new(30.0, 60.0),
    };
    let short = editor
        .guide_segment_preview(GuideOrientation::Vertical, interval, 1.0)
        .unwrap();
    assert_eq!((short.start, short.end), (interval.start, interval.end));
    assert!(!editor.can_undo());
    assert!(
        editor
            .guide_segment_preview(GuideOrientation::Horizontal, interval, 1.0)
            .is_none()
    );
    assert!(editor.add_guide_stroke(short.clone()));
    assert_eq!(editor.strokes(), [short]);
    assert!(editor.undo());
    assert!(editor.strokes().is_empty());
}

#[test]
fn number_placement_clamps_diameter_and_rejects_invalid_values() {
    let mut editor = Editor::new(100.0, 80.0);
    assert!(editor.add_number(Point::new(-20.0, 100.0), 200.0, 0, TailDirection::TopRight));
    assert_eq!(
        editor.selected().unwrap().bounds(),
        bounds(0.0, 0.0, 80.0, 80.0)
    );
    let before = editor.strokes().to_vec();
    for diameter in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        assert!(!editor.add_number(Point::default(), diameter, 0, TailDirection::default()));
    }
    assert!(!editor.add_number(Point::new(f32::NAN, 0.0), 20.0, 0, TailDirection::default()));
    assert_eq!(editor.strokes(), before);
    assert_eq!(editor.next_number(), Some(2));
}

#[test]
fn magnifier_zoom_accepts_limits_and_rejects_invalid_values_without_edits() {
    let mut editor = selected_editor(Tool::Magnifier);
    for zoom in [1.2, 4.0] {
        let before = editor.strokes().to_vec();
        assert!(editor.set_magnifier_zoom(zoom));
        assert_eq!(
            editor.selected().unwrap().magnifier.as_ref().unwrap().zoom,
            zoom
        );
        assert_reversible(&mut editor, &before);
    }
    let before = editor.strokes().to_vec();
    for zoom in [0.0, 1.19, 4.01, f32::NAN, f32::INFINITY] {
        assert!(!editor.set_magnifier_zoom(zoom));
        assert_eq!(editor.strokes(), before);
    }
}

#[test]
fn magnifier_scroll_adjustment_selects_without_raising_and_is_undoable() {
    let mut editor = selected_editor(Tool::Magnifier);
    draw(
        &mut editor,
        Tool::RectangleFill,
        Point::new(250.0, 20.0),
        Point::new(300.0, 70.0),
    );
    let before = editor.strokes().to_vec();
    let revision = editor.history_revision();
    assert_eq!(editor.selected, Some(1));
    assert_eq!(
        editor.adjust_magnifier_zoom_at(Point::new(160.0, 160.0), 1.0, 3),
        Some(2.6)
    );
    assert_eq!(editor.selected, Some(0));
    assert_eq!(editor.strokes().len(), before.len());
    assert_eq!(editor.strokes()[0].tool, Tool::Magnifier);
    assert_eq!(editor.strokes()[0].bounds(), before[0].bounds());
    assert_eq!(editor.strokes()[1], before[1]);
    assert_eq!(editor.history_revision(), revision + 1);
    assert_reversible(&mut editor, &before);
    assert_eq!(editor.selected, Some(0));
}

#[test]
fn magnifier_scroll_boundaries_preserve_selection_history_and_redo() {
    for (zoom, steps) in [(Magnifier::MIN_ZOOM, -1), (Magnifier::MAX_ZOOM, 1)] {
        let mut editor = selected_editor(Tool::Magnifier);
        assert!(editor.set_magnifier_zoom(zoom));
        draw(
            &mut editor,
            Tool::Rectangle,
            Point::new(250.0, 20.0),
            Point::new(300.0, 70.0),
        );
        assert!(editor.set_selected_width(5.0));
        let after = editor.strokes().to_vec();
        assert!(editor.undo());
        let before = editor.strokes().to_vec();
        let revision = editor.history_revision();
        assert_eq!(editor.selected, Some(1));
        assert!(editor.can_redo());
        assert_eq!(
            editor.adjust_magnifier_zoom_at(Point::new(160.0, 160.0), 1.0, steps),
            Some(zoom)
        );
        assert_eq!(editor.selected, Some(1));
        assert_eq!(editor.strokes(), before);
        assert_eq!(editor.history_revision(), revision);
        assert!(editor.redo());
        assert_eq!(editor.strokes(), after);
    }
}

#[test]
fn magnifier_scroll_adjustment_quantizes_and_clamps_multiple_steps() {
    let mut editor = selected_editor(Tool::Magnifier);
    assert!(editor.set_magnifier_zoom(2.01));
    let point = Point::new(160.0, 160.0);
    for (steps, zoom) in [
        (1, 2.2),
        (-1, 2.0),
        (1, 2.2),
        (i32::MIN, Magnifier::MIN_ZOOM),
        (i32::MAX, Magnifier::MAX_ZOOM),
    ] {
        assert_eq!(
            editor.adjust_magnifier_zoom_at(point, 1.0, steps),
            Some(zoom)
        );
        assert_eq!(
            editor.selected().unwrap().magnifier.as_ref().unwrap().zoom,
            zoom
        );
    }
}

#[test]
fn magnifier_scroll_adjustment_rejects_covered_lenses_and_invalid_targets() {
    let mut editor = selected_editor(Tool::Magnifier);
    let before = editor.strokes().to_vec();
    let revision = editor.history_revision();
    for (point, scale, steps) in [
        (Point::new(160.0, 160.0), 1.0, 0),
        (Point::new(20.0, 20.0), 1.0, 1),
        (Point::new(f32::NAN, 160.0), 1.0, 1),
        (Point::new(160.0, 160.0), 0.0, 1),
        (Point::new(160.0, 160.0), f32::INFINITY, 1),
    ] {
        assert_eq!(editor.adjust_magnifier_zoom_at(point, scale, steps), None);
        assert_eq!(editor.strokes(), before);
        assert_eq!(editor.selected, Some(0));
        assert_eq!(editor.history_revision(), revision);
    }
    draw(
        &mut editor,
        Tool::RectangleFill,
        Point::new(140.0, 140.0),
        Point::new(180.0, 180.0),
    );
    let before = editor.strokes().to_vec();
    let revision = editor.history_revision();
    assert_eq!(
        editor.adjust_magnifier_zoom_at(Point::new(160.0, 160.0), 1.0, 1),
        None
    );
    assert_eq!(editor.strokes(), before);
    assert_eq!(editor.selected, Some(1));
    assert_eq!(editor.history_revision(), revision);
}

#[test]
fn magnifier_scroll_adjustment_rejects_drawing_moving_and_resizing() {
    let mut editor = selected_editor(Tool::Magnifier);
    let point = Point::new(160.0, 160.0);
    editor.begin(Tool::Brush, Point::new(20.0, 20.0), 0, 3.0);
    let draft = editor.draft().cloned();
    let before = editor.strokes().to_vec();
    let revision = editor.history_revision();
    assert_eq!(editor.adjust_magnifier_zoom_at(point, 1.0, 1), None);
    assert_eq!(editor.draft(), draft.as_ref());
    assert_eq!(editor.strokes(), before);
    assert_eq!(editor.history_revision(), revision);
    editor.cancel();
    editor.begin_move(point, 1.0);
    assert!(editor.is_dragging());
    assert_eq!(editor.adjust_magnifier_zoom_at(point, 1.0, 1), None);
    assert!(editor.is_dragging());
    assert_eq!(editor.strokes(), before);
    assert_eq!(editor.history_revision(), revision);
    editor.cancel();
    let handle = handle_position(&editor, ResizeHandle::TopRight);
    editor.begin_selection(handle, 1.0);
    assert!(editor.active_resize_handle().is_some());
    assert_eq!(editor.adjust_magnifier_zoom_at(point, 1.0, 1), None);
    assert!(editor.active_resize_handle().is_some());
    assert_eq!(editor.strokes(), before);
    assert_eq!(editor.history_revision(), revision);
}

#[test]
fn magnifier_scroll_adjustment_commits_previous_style_preview_first() {
    let mut editor = selected_editor(Tool::Magnifier);
    draw(
        &mut editor,
        Tool::Rectangle,
        Point::new(250.0, 20.0),
        Point::new(300.0, 70.0),
    );
    let before = editor.strokes().to_vec();
    editor.preview_selected_width(9.0);
    assert_eq!(
        editor.adjust_magnifier_zoom_at(Point::new(160.0, 160.0), 1.0, 1),
        Some(2.2)
    );
    assert!(editor.undo());
    assert_eq!(editor.strokes()[0], before[0]);
    assert_eq!(editor.strokes()[1].width, 9.0);
    assert!(editor.undo());
    assert_eq!(editor.strokes(), before);
}

#[test]
fn hovered_width_changes_select_without_raising_and_refresh_brush_bounds() {
    for tool in [
        Tool::Rectangle,
        Tool::Ellipse,
        Tool::Arrow,
        Tool::Line(LineStyle::Solid),
        Tool::Line(LineStyle::Dashed),
        Tool::Line(LineStyle::Wavy),
        Tool::Brush,
    ] {
        let mut editor = selected_editor(tool);
        draw(
            &mut editor,
            Tool::RectangleFill,
            Point::new(250.0, 20.0),
            Point::new(300.0, 70.0),
        );
        let before = editor.strokes().to_vec();
        let point = if matches!(tool, Tool::Rectangle | Tool::Ellipse) {
            Point::new(100.0, 160.0)
        } else {
            Point::new(100.0, 120.0)
        };
        assert_eq!(editor.set_stroke_width_at(point, 2.0, 6.0), Some(6.0));
        assert_eq!(editor.selected, Some(0));
        let mut expected = before[0].clone();
        expected.width = 6.0;
        if tool.is_freehand() {
            expected.refresh_path_bounds();
        }
        assert_eq!(editor.strokes(), [expected, before[1].clone()]);
        assert_reversible(&mut editor, &before);
    }
}

#[test]
fn hovered_strength_and_opacity_changes_preserve_other_properties_and_are_undoable() {
    for tool in [
        Tool::Redact(RedactionStyle::Blur),
        Tool::Redact(RedactionStyle::Mosaic),
        Tool::RectangleFill,
        Tool::EllipseFill,
    ] {
        let mut editor = selected_editor(tool);
        draw(
            &mut editor,
            Tool::Rectangle,
            Point::new(250.0, 20.0),
            Point::new(300.0, 70.0),
        );
        let before = editor.strokes().to_vec();
        let point = Point::new(160.0, 160.0);
        let mut expected = before[0].clone();
        if tool.has_strength() {
            assert_eq!(
                editor.set_effect_strength_at(point, 1.0, EffectStrength::Low),
                Some(EffectStrength::Low)
            );
            expected.effect_strength = EffectStrength::Low;
        } else {
            assert_eq!(editor.set_fill_opacity_at(point, 1.0, 0.5), Some(0.5));
            expected.fill_opacity = 0.5;
        }
        assert_eq!(editor.selected, Some(0));
        assert_eq!(editor.strokes(), [expected, before[1].clone()]);
        assert_reversible(&mut editor, &before);
    }
}

#[test]
fn hovered_style_no_ops_preserve_selection_preview_history_and_redo() {
    for tool in [
        Tool::Brush,
        Tool::Redact(RedactionStyle::Blur),
        Tool::RectangleFill,
    ] {
        let mut editor = selected_editor(tool);
        draw(
            &mut editor,
            Tool::Rectangle,
            Point::new(250.0, 20.0),
            Point::new(300.0, 70.0),
        );
        assert!(editor.set_selected_width(5.0));
        let after = editor.strokes().to_vec();
        assert!(editor.undo());
        let before = editor.strokes().to_vec();
        let revision = editor.history_revision();
        editor.preview_selected_width(9.0);
        let preview = editor.strokes().to_vec();
        let point = Point::new(160.0, 160.0);
        match tool {
            Tool::Brush => {
                assert_eq!(editor.set_stroke_width_at(point, 1.0, 3.0), Some(3.0));
            }
            Tool::Redact(_) => {
                assert_eq!(
                    editor.set_effect_strength_at(point, 1.0, EffectStrength::High),
                    Some(EffectStrength::High)
                );
            }
            _ => {
                assert_eq!(editor.set_fill_opacity_at(point, 1.0, 1.0), Some(1.0));
            }
        }
        assert_eq!(editor.selected, Some(1));
        assert_eq!(editor.strokes(), preview);
        assert!(editor.style_preview.is_some());
        assert_eq!(editor.history_revision(), revision);
        assert!(editor.can_redo());
        editor.cancel();
        assert_eq!(editor.strokes(), before);
        assert!(editor.redo());
        assert_eq!(editor.strokes(), after);
    }
}

#[test]
fn hovered_style_changes_reject_invalid_values_and_unsupported_tools() {
    let point = Point::new(160.0, 160.0);
    let mut brush = selected_editor(Tool::Brush);
    let before = brush.strokes().to_vec();
    for width in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        assert_eq!(brush.set_stroke_width_at(point, 1.0, width), None);
        assert_eq!(brush.strokes(), before);
    }
    let mut filled = selected_editor(Tool::RectangleFill);
    let before = filled.strokes().to_vec();
    for opacity in [-0.1, 1.1, f32::NAN, f32::INFINITY] {
        assert_eq!(filled.set_fill_opacity_at(point, 1.0, opacity), None);
        assert_eq!(filled.strokes(), before);
    }
    let mut unsupported = selected_editor(Tool::Magnifier);
    let before = unsupported.strokes().to_vec();
    let revision = unsupported.history_revision();
    assert_eq!(unsupported.set_stroke_width_at(point, 1.0, 9.0), None);
    assert_eq!(
        unsupported.set_effect_strength_at(point, 1.0, EffectStrength::Low),
        None
    );
    assert_eq!(unsupported.set_fill_opacity_at(point, 1.0, 0.5), None);
    assert_eq!(unsupported.strokes(), before);
    assert_eq!(unsupported.history_revision(), revision);
}

#[test]
fn hovered_style_changes_reject_covered_targets_invalid_hits_and_active_drags() {
    for tool in [
        Tool::Brush,
        Tool::Line(LineStyle::Solid),
        Tool::Redact(RedactionStyle::Blur),
        Tool::RectangleFill,
    ] {
        let mut editor = selected_editor(tool);
        let point = Point::new(160.0, 160.0);
        let before = editor.strokes().to_vec();
        let revision = editor.history_revision();
        for (point, scale) in [
            (Point::new(20.0, 20.0), 1.0),
            (Point::new(f32::NAN, 160.0), 1.0),
            (point, 0.0),
            (point, f32::INFINITY),
        ] {
            assert!(!apply_hovered_style(&mut editor, tool, point, scale));
            assert_eq!(editor.strokes(), before);
            assert_eq!(editor.history_revision(), revision);
        }
        editor.begin(Tool::Brush, Point::new(20.0, 20.0), 0, 3.0);
        let draft = editor.draft().cloned();
        assert!(!apply_hovered_style(&mut editor, tool, point, 1.0));
        assert_eq!(editor.draft(), draft.as_ref());
        editor.cancel();
        editor.begin_move(point, 1.0);
        assert!(editor.is_dragging());
        assert!(!apply_hovered_style(&mut editor, tool, point, 1.0));
        assert!(editor.is_dragging());
        editor.cancel();
        if let Some((_, handle)) = editor
            .selected()
            .unwrap()
            .selection_handles(1.0)
            .first()
            .copied()
        {
            editor.begin_selection(handle, 1.0);
            assert!(editor.active_resize_handle().is_some());
            assert!(!apply_hovered_style(&mut editor, tool, point, 1.0));
            assert!(editor.active_resize_handle().is_some());
        } else {
            assert!(tool.is_freehand());
        }
        assert_eq!(editor.strokes(), before);
        assert_eq!(editor.history_revision(), revision);
        editor.cancel();
        draw(
            &mut editor,
            Tool::Magnifier,
            Point::new(120.0, 120.0),
            Point::new(200.0, 200.0),
        );
        let before = editor.strokes().to_vec();
        let revision = editor.history_revision();
        assert!(!apply_hovered_style(&mut editor, tool, point, 1.0));
        assert_eq!(editor.strokes(), before);
        assert_eq!(editor.selected, Some(1));
        assert_eq!(editor.history_revision(), revision);
    }
}

#[test]
fn hovered_style_changes_commit_previous_preview_before_recording_edits() {
    for tool in [
        Tool::Brush,
        Tool::Redact(RedactionStyle::Blur),
        Tool::RectangleFill,
    ] {
        let mut editor = selected_editor(tool);
        draw(
            &mut editor,
            Tool::Rectangle,
            Point::new(250.0, 20.0),
            Point::new(300.0, 70.0),
        );
        let before = editor.strokes().to_vec();
        editor.preview_selected_width(9.0);
        assert!(apply_hovered_style(
            &mut editor,
            tool,
            Point::new(160.0, 160.0),
            1.0,
        ));
        assert!(editor.undo());
        assert_eq!(editor.strokes()[0], before[0]);
        assert_eq!(editor.strokes()[1].width, 9.0);
        assert!(editor.undo());
        assert_eq!(editor.strokes(), before);
    }
}

fn apply_hovered_style(editor: &mut Editor, tool: Tool, point: Point, scale: f32) -> bool {
    match tool {
        tool if tool.has_width() => editor.set_stroke_width_at(point, scale, 9.0).is_some(),
        Tool::Redact(_) => editor
            .set_effect_strength_at(point, scale, EffectStrength::Low)
            .is_some(),
        _ => editor.set_fill_opacity_at(point, scale, 0.5).is_some(),
    }
}
