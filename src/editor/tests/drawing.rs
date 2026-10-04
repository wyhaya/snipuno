use super::*;

#[test]
fn drawing_uses_final_image_coordinates_and_clamps_to_edges() {
    for scale in [0.5, 1.0, 2.0] {
        let mut editor = Editor::new(100.0, 80.0);
        editor.begin(Tool::Rectangle, Point::new(80.0, 60.0), 0xff123456, 3.0);
        editor.update(Point::new(50.0, 40.0), scale);
        assert!(editor.finish(Point::new(-20.0, -10.0), scale));
        assert_eq!(editor.strokes()[0].bounds(), bounds(0.0, 0.0, 80.0, 60.0));
    }
}

#[test]
fn short_drags_are_rejected_in_display_pixels() {
    for (tool, end, scale, accepted) in [
        (Tool::Rectangle, Point::new(12.0, 12.0), 1.0, false),
        (Tool::Rectangle, Point::new(13.0, 13.0), 1.0, true),
        (Tool::Rectangle, Point::new(20.0, 12.0), 1.0, false),
        (Tool::Rectangle, Point::new(12.0, 12.0), 2.0, true),
        (Tool::Arrow, Point::new(13.0, 10.0), 1.0, true),
        (Tool::Brush, Point::new(10.0, 10.0), 2.0, false),
        (
            Tool::Line(LineStyle::Solid),
            Point::new(12.0, 10.0),
            1.0,
            false,
        ),
    ] {
        let mut editor = Editor::new(100.0, 100.0);
        editor.begin(tool, Point::new(10.0, 10.0), 0, 3.0);
        assert_eq!(
            editor.finish(end, scale),
            accepted,
            "{tool:?} {end:?} {scale}"
        );
        assert_eq!(editor.annotation_count(), usize::from(accepted));
        assert_eq!(editor.can_undo(), accepted);
    }
}

#[test]
fn constrained_shapes_remain_square_at_every_image_edge() {
    for tool in [
        Tool::Rectangle,
        Tool::RectangleFill,
        Tool::Ellipse,
        Tool::EllipseFill,
        Tool::Magnifier,
    ] {
        for target in [
            Point::new(-100.0, -50.0),
            Point::new(200.0, -50.0),
            Point::new(-100.0, 200.0),
            Point::new(200.0, 200.0),
        ] {
            let mut editor = Editor::new(100.0, 80.0);
            editor.begin(tool, Point::new(50.0, 40.0), 0, 3.0);
            assert!(editor.finish_constrained(target, 1.0, true));
            let rect = editor.strokes()[0].bounds();
            assert_eq!((rect.width, rect.height), (40.0, 40.0));
            assert!(rect.x >= 0.0 && rect.y >= 0.0);
            assert!(rect.x + rect.width <= 100.0 && rect.y + rect.height <= 80.0);
        }
    }
}

#[test]
fn fractional_grid_snaps_without_accumulating_pointer_jitter() {
    let mut editor = Editor::new(100.0, 100.0);
    editor.set_grid(PixelGrid::new(1.5, 2.0));
    editor.begin(Tool::Rectangle, Point::new(10.1, 10.9), 0, 3.0);
    assert!(editor.finish(Point::new(30.1, 31.1), 1.0));
    assert_eq!(editor.strokes()[0].bounds(), bounds(10.5, 10.0, 19.5, 22.0));
    editor.begin_move(Point::new(20.0, 10.0), 1.0);
    for x in [20.2, 20.4, 20.6] {
        editor.update(Point::new(x, 10.1), 1.0);
        assert_eq!(editor.selected().unwrap().start, Point::new(10.5, 10.0));
    }
    assert!(editor.finish(Point::new(20.8, 11.1), 1.0));
    assert_eq!(editor.selected().unwrap().start, Point::new(12.0, 12.0));
}

#[test]
fn lines_snap_near_axes_but_keep_intentional_diagonals() {
    for (end, expected) in [
        (Point::new(80.0, 22.0), Point::new(80.0, 20.0)),
        (Point::new(22.0, 80.0), Point::new(20.0, 80.0)),
        (Point::new(80.0, 50.0), Point::new(80.0, 50.0)),
    ] {
        let mut editor = Editor::new(100.0, 100.0);
        draw(
            &mut editor,
            Tool::Line(LineStyle::Solid),
            Point::new(20.0, 20.0),
            end,
        );
        assert_eq!(editor.strokes()[0].end, expected);
    }
}

#[test]
fn brush_smoothing_reduces_jitter_without_moving_endpoints_or_corners() {
    let mut editor = Editor::new(100.0, 100.0);
    let points = [
        Point::new(20.0, 50.0),
        Point::new(30.0, 52.0),
        Point::new(40.0, 48.0),
        Point::new(50.0, 52.0),
        Point::new(60.0, 50.0),
    ];
    editor.begin(Tool::Brush, points[0], 0, 3.0);
    for point in &points[1..] {
        editor.update(*point, 1.0);
    }
    assert!(editor.finish(points[4], 1.0));
    let smoothed = &editor.strokes()[0].points;
    assert_eq!(smoothed.first(), points.first());
    assert_eq!(smoothed.last(), points.last());
    for (actual, input) in smoothed[1..4].iter().zip(&points[1..4]) {
        assert!((actual.y - 50.0).abs() < (input.y - 50.0).abs());
        assert!((actual.x - input.x).hypot(actual.y - input.y) <= 3.0);
    }
    let corner = [
        Point::new(20.0, 20.0),
        Point::new(40.0, 20.0),
        Point::new(40.0, 40.0),
        Point::new(40.0, 20.0),
    ];
    editor.begin(Tool::Brush, corner[0], 0, 3.0);
    for point in &corner[1..] {
        editor.update(*point, 1.0);
    }
    assert!(editor.finish(corner[3], 1.0));
    assert_eq!(editor.strokes()[1].points.as_slice(), corner);
}
