use super::*;
use crate::measurement::Interval;
use crate::test_support::{bounds, text};

mod annotations;
mod drawing;
mod history;
mod transforms;

fn draw(editor: &mut Editor, tool: Tool, start: Point, end: Point) {
    editor.begin(tool, start, 0xffe02040, 3.0);
    assert!(editor.finish(end, 1.0));
}

fn selected_editor(tool: Tool) -> Editor {
    let mut editor = Editor::new(400.0, 300.0);
    match tool {
        Tool::Image(id) => assert!(editor.add_image_at(id, 120.0, 80.0, Point::new(160.0, 160.0))),
        Tool::Number(_) => assert!(editor.add_number(
            Point::new(160.0, 160.0),
            80.0,
            0xffe02040,
            TailDirection::default()
        )),
        Tool::Text => assert!(editor.add_text(
            Point::new(100.0, 120.0),
            text("Hello 世界"),
            0xffe02040,
            Point::new(120.0, 80.0)
        )),
        Tool::Guide(orientation) => {
            let stroke = editor
                .guide_preview(orientation, Point::new(160.0, 160.0), 1.0)
                .unwrap();
            assert!(editor.add_guide_stroke(stroke));
        }
        Tool::Measure(axis) => {
            let interval = match axis {
                MeasurementAxis::X => Interval {
                    start: Point::new(100.0, 160.0),
                    end: Point::new(220.0, 160.0),
                },
                MeasurementAxis::Y => Interval {
                    start: Point::new(160.0, 120.0),
                    end: Point::new(160.0, 200.0),
                },
            };
            let stroke = editor
                .measurement_preview(interval, axis, 1.0, 30.0)
                .unwrap();
            editor.add_measurement(stroke);
            editor.begin_move(Point::new(160.0, 160.0), 1.0);
            editor.cancel();
        }
        _ => draw(
            &mut editor,
            tool,
            Point::new(100.0, 120.0),
            Point::new(220.0, 200.0),
        ),
    }
    editor
}

fn handle_position(editor: &Editor, handle: ResizeHandle) -> Point {
    editor
        .selected()
        .unwrap()
        .selection_handles(1.0)
        .into_iter()
        .find(|(candidate, _)| *candidate == handle)
        .unwrap()
        .1
}

fn assert_reversible(editor: &mut Editor, before: &[Stroke]) {
    let after = editor.strokes().to_vec();
    assert_ne!(after, before);
    assert!(editor.undo());
    assert_eq!(editor.strokes(), before);
    assert!(editor.redo());
    assert_eq!(editor.strokes(), after);
}
