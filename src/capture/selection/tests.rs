use super::*;
use crate::capture::CaptureWindowId;
use image::Rgba;

fn point(x: f64, y: f64) -> CapturePoint {
    CapturePoint { x, y }
}

fn bounds(x: f64, y: f64, width: f64, height: f64) -> CaptureBounds {
    CaptureBounds {
        x,
        y,
        width,
        height,
    }
}

fn state() -> SelectionState {
    SelectionState::new(
        CaptureDisplayId(10),
        bounds(-100.0, -50.0, 200.0, 100.0),
        Arc::new(vec![
            WindowTarget {
                id: CaptureWindowId(1),
                process_id: 100,
                app_name: "Front app".into(),
                title: "Front".into(),
                bounds: bounds(-50.0, -25.0, 50.0, 50.0),
                capture_from_display: false,
                pixel_size: Some((100, 100)),
                pixel_scale: Default::default(),
            },
            WindowTarget {
                id: CaptureWindowId(2),
                process_id: 200,
                app_name: "Back app".into(),
                title: "Back".into(),
                bounds: bounds(-60.0, -30.0, 120.0, 60.0),
                capture_from_display: false,
                pixel_size: Some((240, 120)),
                pixel_scale: Default::default(),
            },
        ]),
    )
}

fn display() -> DisplaySnapshot {
    DisplaySnapshot {
        id: CaptureDisplayId(10),
        ui_scale: 1.0,
        #[cfg(target_os = "windows")]
        orientation: 0,
        mode_pixel_size: (200, 120),
        bounds: bounds(-100.0, -50.0, 100.0, 80.0),
        pixels: Arc::new(RgbaImage::from_fn(200, 120, |x, y| {
            Rgba([x as u8, y as u8, 7, 255])
        })),
    }
}

#[test]
fn fullscreen_selection_captures_the_pointer_display_at_original_resolution() {
    let secondary = DisplaySnapshot {
        ui_scale: 1.5,
        ..display()
    };
    let primary = DisplaySnapshot {
        id: CaptureDisplayId(20),
        bounds: bounds(0.0, 0.0, 100.0, 80.0),
        ..display()
    };
    let displays = [primary, secondary];
    for (pointer, expected) in [(point(-25.0, -10.0), 1), (point(0.0, 0.0), 0)] {
        let Some(CaptureSelection::Region { display_id, bounds }) =
            fullscreen_selection(&displays, Some(pointer))
        else {
            panic!("Expected a full display selection");
        };
        let display = &displays[expected];
        assert_eq!(display_id, display.id);
        assert_eq!(bounds, display.bounds);
        assert_eq!(crop_region(display, bounds).unwrap(), *display.pixels);
        let placement = crate::capture::placement::CapturePlacement::new(
            &CaptureSelection::Region { display_id, bounds },
            &displays,
        )
        .unwrap();
        assert_eq!(placement.display_id, display.id);
        assert_eq!(placement.bounds, display.bounds);
        assert_eq!(placement.ui_scale, display.ui_scale);
    }
}

#[test]
fn fullscreen_selection_falls_back_to_primary_then_first_available_display() {
    let secondary = display();
    let primary = DisplaySnapshot {
        id: CaptureDisplayId(20),
        bounds: bounds(0.0, 0.0, 100.0, 80.0),
        ..display()
    };
    let displays = [secondary, primary];
    for pointer in [None, Some(point(999.0, 999.0)), Some(point(f64::NAN, 0.0))] {
        let Some(CaptureSelection::Region { display_id, .. }) =
            fullscreen_selection(&displays, pointer)
        else {
            panic!("Expected the primary display");
        };
        assert_eq!(display_id, displays[1].id);
    }
    assert!(matches!(
        fullscreen_selection(&displays[..1], None),
        Some(CaptureSelection::Region {
            display_id: CaptureDisplayId(10),
            ..
        })
    ));
    assert!(fullscreen_selection(&[], None).is_none());
}

#[test]
fn click_selects_the_frontmost_window_at_press_and_drag_latches_to_region() {
    let mut state = state();
    state.pointer_down(point(-2.0, 0.0));
    let Some(CaptureSelection::Window(window)) = state.pointer_up(point(2.0, 0.0)) else {
        panic!("Expected a window");
    };
    assert_eq!(window.id, CaptureWindowId(1));
    state.pointer_down(point(-20.0, 0.0));
    state.pointer_move(point(50.0, 20.0));
    let Some(CaptureSelection::Region {
        display_id,
        bounds: area,
    }) = state.pointer_up(point(-18.0, 2.0))
    else {
        panic!("Expected a region");
    };
    assert_eq!(display_id, CaptureDisplayId(10));
    assert_eq!(area, bounds(-20.0, 0.0, 2.0, 2.0));
    state.pointer_down(point(20.0, 10.0));
    let Some(CaptureSelection::Region { bounds: area, .. }) =
        state.pointer_up(point(-200.0, -100.0))
    else {
        panic!("Expected a region");
    };
    assert_eq!(area, bounds(-100.0, -50.0, 120.0, 60.0));
}

#[test]
fn drag_threshold_scales_and_cancel_allows_a_new_selection() {
    for scale in [1.0, 1.5, 2.0] {
        let mut state = state();
        state.set_ui_scale(scale);
        state.pointer_down(point(-20.0, 0.0));
        assert!(matches!(
            state.pointer_up(point(-20.0 + 4.0 * scale, 0.0)),
            Some(CaptureSelection::Window(_))
        ));
        state.pointer_down(point(-20.0, 0.0));
        assert!(matches!(
            state.pointer_up(point(-20.0 + 5.0 * scale, scale)),
            Some(CaptureSelection::Region { .. })
        ));
        state.pointer_down(point(-20.0, 0.0));
        state.cancel_gesture();
        assert!(state.pointer_up(point(50.0, 20.0)).is_none());
        state.pointer_down(point(-20.0, 0.0));
        assert!(matches!(
            state.pointer_up(point(-20.0, 0.0)),
            Some(CaptureSelection::Window(_))
        ));
    }
}

#[test]
fn capture_crop_converts_negative_origin_and_unequal_density_to_exact_pixels() {
    let display = display();
    let crop = crop_region(&display, bounds(-99.75, -49.5, 1.0, 1.0)).unwrap();
    assert_eq!(crop.dimensions(), (2, 1));
    assert_eq!(crop.as_raw(), &[1, 1, 7, 255, 2, 1, 7, 255]);
    assert_eq!(
        crop_region(&display, bounds(-200.0, -100.0, 400.0, 300.0)).unwrap(),
        *display.pixels
    );
    for area in [
        bounds(-90.0, -40.0, 0.0, 5.0),
        bounds(0.0, 0.0, 5.0, 5.0),
        bounds(f64::NAN, 0.0, 5.0, 5.0),
        bounds(-90.0, -40.0, 0.01, 0.01),
        bounds(-90.0, -40.0, f64::INFINITY, 5.0),
    ] {
        assert!(crop_region(&display, area).is_err(), "{area:?}");
    }
    let empty = DisplaySnapshot {
        pixels: Arc::new(RgbaImage::new(0, 0)),
        ..display
    };
    assert!(crop_region(&empty, empty.bounds).is_err());
}
