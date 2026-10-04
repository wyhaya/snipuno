use super::*;
use std::collections::HashSet;

#[test]
fn edits_undo_and_redo_in_order() {
    let mut editor = Editor::new(400.0, 300.0);
    assert!(!editor.undo());
    assert!(!editor.redo());
    let mut states = vec![editor.strokes().to_vec()];
    for tool in [Tool::Rectangle, Tool::Ellipse] {
        draw(
            &mut editor,
            tool,
            Point::new(20.0, 30.0),
            Point::new(80.0, 90.0),
        );
        states.push(editor.strokes().to_vec());
    }
    assert!(editor.set_selected_color(0xff123456));
    states.push(editor.strokes().to_vec());
    assert!(editor.delete_selected());
    states.push(editor.strokes().to_vec());
    for expected in states[..states.len() - 1].iter().rev() {
        assert!(editor.undo());
        assert_eq!(editor.strokes(), expected);
    }
    assert!(!editor.undo());
    for expected in &states[1..] {
        assert!(editor.redo());
        assert_eq!(editor.strokes(), expected);
    }
    assert!(!editor.redo());
}

#[test]
fn canceled_and_ineffective_edits_preserve_redo() {
    let mut editor = selected_editor(Tool::Rectangle);
    let before = editor.strokes().to_vec();
    assert!(editor.set_selected_color(0xff123456));
    let changed = editor.strokes().to_vec();
    assert!(editor.undo());
    let revision = editor.history_revision();
    editor.preview_selected_width(8.0);
    assert!(editor.cancel());
    editor.preview_selected_width(8.0);
    editor.preview_selected_width(before[0].width);
    assert!(!editor.finish_style_change());
    editor.begin_move(Point::new(100.0, 160.0), 1.0);
    editor.update(Point::new(130.0, 170.0), 1.0);
    assert!(editor.cancel());
    assert!(!editor.set_selected_color(before[0].color));
    for width in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        assert!(!editor.set_selected_width(width));
    }
    editor.begin(Tool::Arrow, Point::new(10.0, 10.0), 0, 3.0);
    assert!(!editor.finish(Point::new(10.0, 10.0), 1.0));
    editor.begin(Tool::Rectangle, Point::new(10.0, 10.0), 0, 3.0);
    editor.update(Point::new(80.0, 80.0), 1.0);
    assert!(editor.cancel());
    assert_eq!(editor.strokes(), before);
    assert_eq!(editor.history_revision(), revision);
    assert!(editor.redo());
    assert_eq!(editor.strokes(), changed);
    assert!(editor.undo());
    draw(
        &mut editor,
        Tool::Ellipse,
        Point::new(10.0, 10.0),
        Point::new(50.0, 50.0),
    );
    assert!(!editor.can_redo());
}

#[test]
fn style_drag_commits_once_or_restores_the_original() {
    for tool in [
        Tool::Rectangle,
        Tool::RectangleFill,
        Tool::EllipseFill,
        Tool::Redact(RedactionStyle::Blur),
        Tool::Magnifier,
    ] {
        let mut editor = selected_editor(tool);
        let before = editor.strokes().to_vec();
        for cancel in [true, false] {
            for value in [2.0, 3.0, 4.0] {
                match tool {
                    Tool::RectangleFill | Tool::EllipseFill => {
                        editor.preview_fill_opacity(value / 10.0)
                    }
                    Tool::Magnifier => editor.preview_magnifier_zoom(value),
                    Tool::Redact(_) => editor.preview_effect_strength(EffectStrength::Max),
                    _ => editor.preview_selected_width(value),
                }
            }
            if cancel {
                assert!(editor.cancel());
                assert_eq!(editor.strokes(), before);
            } else {
                assert!(editor.finish_style_change());
                let mark = editor.selected().unwrap();
                match tool {
                    Tool::RectangleFill | Tool::EllipseFill => assert_eq!(mark.fill_opacity, 0.4),
                    Tool::Magnifier => assert_eq!(mark.magnifier.as_ref().unwrap().zoom, 4.0),
                    Tool::Redact(_) => assert_eq!(mark.effect_strength, EffectStrength::Max),
                    _ => assert_eq!(mark.width, 4.0),
                }
                assert_reversible(&mut editor, &before);
            }
        }
    }
}

#[test]
fn repeated_crops_and_clear_restore_the_entire_document() {
    let mut editor = selected_editor(Tool::Brush);
    assert!(editor.add_image(7, 20.0, 10.0));
    let original = editor.strokes().to_vec();
    let mut states = vec![(editor.image_bounds(), original.clone())];
    for crop in [
        bounds(20.0, 30.0, 300.0, 240.0),
        bounds(10.0, 10.0, 80.0, 70.0),
    ] {
        assert!(editor.crop(crop));
        states.push((editor.image_bounds(), editor.strokes().to_vec()));
    }
    assert_eq!(editor.image_bounds(), bounds(30.0, 40.0, 80.0, 70.0));
    for (before, after) in original.iter().zip(editor.strokes()) {
        assert_eq!(
            after.start,
            Point::new(before.start.x - 30.0, before.start.y - 40.0)
        );
        assert_eq!(
            after.end,
            Point::new(before.end.x - 30.0, before.end.y - 40.0)
        );
        assert_eq!(
            after.points.as_ref(),
            &before
                .points
                .iter()
                .map(|p| Point::new(p.x - 30.0, p.y - 40.0))
                .collect::<Vec<_>>()
        );
    }
    assert!(editor.clear());
    assert!(editor.strokes().is_empty());
    assert_eq!(editor.image_bounds(), bounds(0.0, 0.0, 400.0, 300.0));
    assert!(!editor.clear());
    for (crop, strokes) in states.iter().rev() {
        assert!(editor.undo());
        assert_eq!(editor.image_bounds(), *crop);
        assert_eq!(editor.strokes(), strokes);
    }
    for (crop, strokes) in &states[1..] {
        assert!(editor.redo());
        assert_eq!(editor.image_bounds(), *crop);
        assert_eq!(editor.strokes(), strokes);
    }
    assert!(editor.redo());
    assert!(editor.strokes().is_empty());
}

#[test]
fn image_resources_live_as_long_as_their_undo_or_redo_history() {
    let mut editor = Editor::new(100.0, 100.0);
    assert!(editor.add_image(1, 20.0, 20.0));
    assert!(editor.add_image(2, 20.0, 20.0));
    assert!(editor.undo());
    assert_eq!(editor.referenced_image_ids(), HashSet::from([1, 2]));
    draw(
        &mut editor,
        Tool::Rectangle,
        Point::new(10.0, 10.0),
        Point::new(30.0, 30.0),
    );
    assert_eq!(editor.referenced_image_ids(), HashSet::from([1]));
    assert!(editor.clear());
    assert_eq!(editor.referenced_image_ids(), HashSet::from([1]));
    assert!(editor.undo());
    assert_eq!(editor.strokes()[0].tool, Tool::Image(1));
}
