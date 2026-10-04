use super::*;

#[test]
fn selection_raises_only_the_hit_layer_and_can_be_undone() {
    let mut editor = Editor::new(200.0, 160.0);
    draw(
        &mut editor,
        Tool::Rectangle,
        Point::new(20.0, 20.0),
        Point::new(100.0, 100.0),
    );
    draw(
        &mut editor,
        Tool::Ellipse,
        Point::new(60.0, 60.0),
        Point::new(140.0, 140.0),
    );
    let before = editor.strokes().to_vec();
    assert_eq!(editor.hit_index(Point::new(100.0, 60.0), 1.0), Some(1));
    editor.begin_move(Point::new(20.0, 50.0), 1.0);
    assert!(!editor.finish(Point::new(20.0, 50.0), 1.0));
    assert_eq!(editor.strokes(), [before[1].clone(), before[0].clone()]);
    assert_reversible(&mut editor, &before);
    let revision = editor.history_revision();
    editor.begin_move(Point::new(20.0, 50.0), 1.0);
    assert!(!editor.finish(Point::new(20.0, 50.0), 1.0));
    assert_eq!(editor.history_revision(), revision);
}

#[test]
fn moving_layers_crosses_every_canvas_edge_without_changing_their_shape() {
    for tool in [
        Tool::Rectangle,
        Tool::Ellipse,
        Tool::Arrow,
        Tool::Brush,
        Tool::Image(7),
        Tool::Text,
        Tool::Number(1),
        Tool::Magnifier,
        Tool::Spotlight,
        Tool::Line(LineStyle::Solid),
        Tool::Line(LineStyle::Dashed),
        Tool::Line(LineStyle::Wavy),
        Tool::Redact(RedactionStyle::Blur),
        Tool::Redact(RedactionStyle::Mosaic),
        Tool::Guide(GuideOrientation::Horizontal),
        Tool::Guide(GuideOrientation::Vertical),
        Tool::Measure(MeasurementAxis::X),
        Tool::Measure(MeasurementAxis::Y),
    ] {
        let mut editor = selected_editor(tool);
        let before = editor.strokes().to_vec();
        let original = &before[0];
        let size = original.bounds();
        for delta in [
            Point::new(-1000.0, 0.0),
            Point::new(1000.0, 0.0),
            Point::new(0.0, -1000.0),
            Point::new(0.0, 1000.0),
        ] {
            assert!(editor.nudge_selected(delta));
            let moved = editor.selected().unwrap();
            assert_eq!(
                moved.bounds(),
                bounds(size.x + delta.x, size.y + delta.y, size.width, size.height),
                "{tool:?} {delta:?}"
            );
            for (actual, original) in [moved.start, moved.end]
                .iter()
                .chain(moved.points.iter())
                .zip(
                    [original.start, original.end]
                        .iter()
                        .chain(original.points.iter()),
                )
            {
                assert_eq!(
                    *actual,
                    Point::new(original.x + delta.x, original.y + delta.y)
                );
            }
            assert_eq!(moved.text, original.text);
            if let (Some(actual), Some(original)) =
                (moved.measurement_label, original.measurement_label)
            {
                assert_eq!(actual.x, original.x + delta.x);
                assert_eq!(actual.y, original.y + delta.y);
            }
            assert_reversible(&mut editor, &before);
            assert!(editor.nudge_selected(Point::new(-delta.x, -delta.y)));
            assert_eq!(editor.strokes(), before);
        }
        assert!(!editor.nudge_selected(Point::new(f32::NAN, 0.0)));
        editor.deselect();
        assert!(!editor.nudge_selected(Point::new(-10.0, 0.0)));
    }
}

#[test]
fn dragging_a_layer_outside_and_back_preserves_grab_offset_and_can_be_canceled() {
    for tool in [Tool::Rectangle, Tool::Image(7)] {
        let mut editor = selected_editor(tool);
        editor.set_grid(PixelGrid::new(2.0, 2.0));
        let before = editor.strokes().to_vec();
        let grab = Point::new(110.0, 130.0);
        editor.begin_move(grab, 1.0);
        editor.update(Point::new(-40.0, -20.0), 1.0);
        assert_eq!(
            editor.selected().unwrap().bounds(),
            bounds(-50.0, -30.0, 120.0, 80.0)
        );
        assert_eq!(editor.hit_index(Point::new(20.0, 20.0), 1.0), Some(0));
        editor.update(Point::new(-1000.0, -1000.0), 1.0);
        assert_eq!(editor.hit_index(Point::new(20.0, 20.0), 1.0), None);
        editor.update(grab, 1.0);
        assert_eq!(editor.strokes(), before);
        assert!(editor.finish(Point::new(-40.0, -20.0), 1.0));
        assert_reversible(&mut editor, &before);
        let partially_visible = editor.strokes().to_vec();
        editor.begin_move(Point::new(20.0, 20.0), 1.0);
        editor.update(Point::new(500.0, 500.0), 1.0);
        assert!(editor.cancel());
        assert_eq!(editor.strokes(), partially_visible);
    }
}

#[test]
fn resizing_layers_can_extend_past_the_canvas_with_a_fixed_anchor() {
    for tool in [
        Tool::Rectangle,
        Tool::Ellipse,
        Tool::Image(7),
        Tool::Text,
        Tool::Number(1),
        Tool::Magnifier,
    ] {
        let mut editor = selected_editor(tool);
        let before = editor.strokes().to_vec();
        let original = before[0].bounds();
        let grab = handle_position(&editor, ResizeHandle::TopLeft);
        editor.begin_selection(grab, 1.0);
        assert!(editor.finish(
            Point::new(
                grab.x - original.width * 2.0,
                grab.y - original.height * 2.0
            ),
            1.0
        ));
        assert_eq!(
            editor.selected().unwrap().bounds(),
            bounds(
                original.x - original.width * 2.0,
                original.y - original.height * 2.0,
                original.width * 3.0,
                original.height * 3.0
            )
        );
        assert_reversible(&mut editor, &before);
        let grab = handle_position(&editor, ResizeHandle::BottomRight);
        editor.begin_selection(grab, 1.0);
        assert!(editor.finish(
            Point::new(
                grab.x + original.width * 3.0,
                grab.y + original.height * 3.0
            ),
            1.0
        ));
        let resized = editor.selected().unwrap().bounds();
        assert!(resized.x < 0.0 && resized.y < 0.0);
        assert!(resized.x + resized.width > 400.0 && resized.y + resized.height > 300.0);
    }
}

#[test]
fn every_resize_handle_changes_only_its_edges_and_can_cross_the_anchor() {
    for (handle, x, y) in [
        (ResizeHandle::TopLeft, -1.0, -1.0),
        (ResizeHandle::Top, 0.0, -1.0),
        (ResizeHandle::TopRight, 1.0, -1.0),
        (ResizeHandle::Right, 1.0, 0.0),
        (ResizeHandle::BottomRight, 1.0, 1.0),
        (ResizeHandle::Bottom, 0.0, 1.0),
        (ResizeHandle::BottomLeft, -1.0, 1.0),
        (ResizeHandle::Left, -1.0, 0.0),
    ] {
        for crossing in [false, true] {
            let mut editor = selected_editor(Tool::Rectangle);
            let before = editor.strokes().to_vec();
            let grab = handle_position(&editor, handle);
            let (dx, dy) = if crossing {
                (-180.0 * x, -120.0 * y)
            } else {
                (30.0 * x, 20.0 * y)
            };
            let expected = if crossing {
                bounds(
                    if x < 0.0 {
                        220.0
                    } else if x > 0.0 {
                        40.0
                    } else {
                        100.0
                    },
                    if y < 0.0 {
                        200.0
                    } else if y > 0.0 {
                        80.0
                    } else {
                        120.0
                    },
                    if x == 0.0 { 120.0 } else { 60.0 },
                    if y == 0.0 { 80.0 } else { 40.0 },
                )
            } else {
                bounds(
                    if x < 0.0 { 70.0 } else { 100.0 },
                    if y < 0.0 { 100.0 } else { 120.0 },
                    if x == 0.0 { 120.0 } else { 150.0 },
                    if y == 0.0 { 80.0 } else { 100.0 },
                )
            };
            editor.begin_selection(grab, 1.0);
            assert!(editor.finish(Point::new(grab.x + dx, grab.y + dy), 1.0));
            assert_eq!(
                editor.selected().unwrap().bounds(),
                expected,
                "{handle:?} crossing={crossing}"
            );
            assert_reversible(&mut editor, &before);
        }
    }
}

#[test]
fn constrained_resize_keeps_ratio_and_releasing_shift_restores_free_resize() {
    let mut editor = selected_editor(Tool::Rectangle);
    let before = editor.strokes().to_vec();
    let grab = handle_position(&editor, ResizeHandle::Right);
    editor.begin_selection(grab, 1.0);
    editor.update_constrained(Point::new(grab.x + 60.0, grab.y), 1.0, true);
    assert_eq!(editor.selected().unwrap().bounds().height, 120.0);
    editor.update_constrained(Point::new(2000.0, grab.y), 1.0, true);
    let rect = editor.selected().unwrap().bounds();
    assert!((rect.width / rect.height - 1.5).abs() < 0.02);
    assert!(rect.y < 0.0 && rect.x + rect.width > 400.0 && rect.y + rect.height > 300.0);
    assert!(editor.finish(Point::new(grab.x + 60.0, grab.y), 1.0));
    assert_eq!(
        editor.selected().unwrap().bounds(),
        bounds(100.0, 120.0, 180.0, 80.0)
    );
    assert_reversible(&mut editor, &before);
}

#[test]
fn bending_and_moving_an_arrow_preserves_endpoints_and_control_geometry() {
    let mut editor = selected_editor(Tool::Arrow);
    let before = editor.strokes().to_vec();
    let grab = handle_position(&editor, ResizeHandle::ArrowMiddle);
    editor.begin_selection(grab, 1.0);
    assert!(editor.finish(Point::new(grab.x, -200.0), 1.0));
    let bent = editor.selected().unwrap().clone();
    assert_eq!((bent.start, bent.end), (before[0].start, before[0].end));
    assert_eq!(bent.arrow_point(0.5), Point::new(grab.x, -200.0));
    assert_reversible(&mut editor, &before);
    assert!(editor.nudge_selected(Point::new(10.0, 20.0)));
    let moved = editor.selected().unwrap();
    let control = bent.arrow_control.unwrap();
    assert_eq!(
        moved.arrow_control,
        Some(Point::new(control.x + 10.0, control.y + 20.0))
    );
}

#[test]
fn arrow_and_line_endpoints_can_cross_the_canvas_with_and_without_shift() {
    for tool in [Tool::Arrow, Tool::Line(LineStyle::Solid)] {
        for constrained in [false, true] {
            let mut editor = selected_editor(tool);
            let before = editor.strokes().to_vec();
            let grab = handle_position(&editor, ResizeHandle::ArrowEnd);
            editor.begin_selection(grab, 1.0);
            assert!(editor.finish_constrained(Point::new(-140.0, -40.0), 1.0, constrained));
            let resized = editor.selected().unwrap();
            assert_eq!(resized.start, before[0].start);
            assert_eq!(resized.end, Point::new(-140.0, -40.0));
            assert_reversible(&mut editor, &before);
        }
    }
}

#[test]
fn hit_testing_follows_shape_geometry_and_ignores_cropped_layers() {
    let mut editor = Editor::new(200.0, 200.0);
    draw(
        &mut editor,
        Tool::Ellipse,
        Point::new(120.0, 120.0),
        Point::new(180.0, 180.0),
    );
    assert_eq!(editor.hit_index(Point::new(120.0, 120.0), 1.0), None);
    assert_eq!(editor.hit_index(Point::new(150.0, 120.0), 1.0), Some(0));
    editor.begin(Tool::Brush, Point::new(40.0, 40.0), 0, 3.0);
    editor.update(Point::new(80.0, 40.0), 1.0);
    assert!(editor.finish(Point::new(80.0, 80.0), 1.0));
    assert_eq!(editor.hit_index(Point::new(60.0, 60.0), 1.0), None);
    assert_eq!(editor.hit_index(Point::new(80.0, 60.0), 1.0), Some(1));
    assert!(editor.crop(bounds(0.0, 0.0, 20.0, 20.0)));
    assert_eq!(editor.hit_index(Point::new(0.0, 0.0), 1.0), None);
    assert_eq!(editor.annotation_count(), 2);
}

#[test]
fn crop_normalizes_pixel_edges_and_rejects_empty_or_invalid_changes() {
    let mut editor = selected_editor(Tool::Rectangle);
    assert!(editor.undo());
    for crop in [
        bounds(0.0, 0.0, 400.0, 300.0),
        bounds(0.0, 0.0, 0.0, 10.0),
        bounds(500.0, 0.0, 10.0, 10.0),
        bounds(f32::NAN, 0.0, 10.0, 10.0),
        bounds(0.0, 0.0, f32::INFINITY, 10.0),
    ] {
        assert!(!editor.crop(crop));
        assert!(editor.can_redo());
    }
    assert!(editor.crop(bounds(150.2, 120.8, -100.0, -80.0)));
    assert_eq!(editor.image_bounds(), bounds(50.0, 41.0, 100.0, 80.0));
    assert!(!editor.can_redo());
}

#[test]
fn text_resize_scales_font_and_wrapping_without_flipping() {
    let mut editor = selected_editor(Tool::Text);
    let before = editor.strokes().to_vec();
    let grab = handle_position(&editor, ResizeHandle::BottomRight);
    editor.begin_selection(grab, 1.0);
    assert!(editor.finish(Point::new(grab.x + 60.0, grab.y + 40.0), 1.0));
    let mark = editor.selected().unwrap();
    assert_eq!(mark.bounds(), bounds(100.0, 120.0, 180.0, 120.0));
    let annotation = mark.text.as_ref().unwrap();
    assert_eq!((annotation.font_size, annotation.wrap_width), (30.0, 180.0));
    assert_eq!(annotation.content, before[0].text.as_ref().unwrap().content);
    assert_reversible(&mut editor, &before);
    let grab = handle_position(&editor, ResizeHandle::BottomRight);
    editor.begin_selection(grab, 1.0);
    editor.update(Point::new(-100.0, -100.0), 1.0);
    let mark = editor.selected().unwrap();
    assert!(mark.end.x > mark.start.x && mark.end.y > mark.start.y);
    assert!(mark.text.as_ref().unwrap().font_size > 0.0);
    assert!(editor.cancel());
}

#[test]
fn line_endpoint_and_style_changes_preserve_the_other_endpoint() {
    for style in [LineStyle::Solid, LineStyle::Dashed, LineStyle::Wavy] {
        let mut editor = selected_editor(Tool::Line(style));
        let before = editor.strokes().to_vec();
        let grab = handle_position(&editor, ResizeHandle::ArrowEnd);
        editor.begin_selection(grab, 1.0);
        assert!(editor.finish(Point::new(280.0, 124.0), 1.0));
        assert_eq!(editor.selected().unwrap().start, before[0].start);
        assert_eq!(editor.selected().unwrap().end, Point::new(280.0, 120.0));
        assert_eq!(editor.selected().unwrap().tool, Tool::Line(style));
        assert_reversible(&mut editor, &before);
    }
    let mut editor = selected_editor(Tool::Line(LineStyle::Solid));
    let before = editor.strokes().to_vec();
    assert!(editor.set_selected_line_style(LineStyle::Dashed));
    let mut expected = before[0].clone();
    expected.tool = Tool::Line(LineStyle::Dashed);
    assert_eq!(editor.strokes(), [expected]);
    assert_reversible(&mut editor, &before);
}
