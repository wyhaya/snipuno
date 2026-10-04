use super::*;

fn bounds(x: f64, y: f64, width: f64, height: f64) -> CaptureBounds {
    CaptureBounds {
        x,
        y,
        width,
        height,
    }
}

fn placement(bounds: CaptureBounds, ui_scale: f64) -> CapturePlacement {
    CapturePlacement {
        bounds,
        display_id: CaptureDisplayId(1),
        display_bounds: bounds,
        ui_scale,
    }
}

fn assert_close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 0.001, "{actual} != {expected}");
}

fn assert_within(frame: CaptureBounds, area: CaptureBounds) {
    assert!(frame.x >= area.x - 0.001);
    assert!(frame.y >= area.y - 0.001);
    assert!(frame.x + frame.width <= area.x + area.width + 0.001);
    assert!(frame.y + frame.height <= area.y + area.height + 0.001);
}

#[test]
fn fullscreen_width_follows_height_limited_image_across_display_scales() {
    for (ui_scale, scale_factor) in [(1.0, 1.0), (1.0, 2.0), (1.5, 1.5), (2.0, 2.0)] {
        let display = bounds(
            -1440.0 * ui_scale,
            -900.0 * ui_scale,
            1440.0 * ui_scale,
            900.0 * ui_scale,
        );
        let work_area = bounds(
            display.x,
            -876.0 * ui_scale,
            display.width,
            820.0 * ui_scale,
        );
        let layout = editor_layout(
            placement(display, ui_scale),
            work_area,
            FrameInsets::default(),
            (
                (1440.0 * scale_factor) as u32,
                (900.0 * scale_factor) as u32,
            ),
            scale_factor,
        );
        assert_close(layout.frame.height / ui_scale, 820.0);
        assert_close(layout.frame.width / ui_scale, 1242.0);
        assert_within(layout.frame, work_area);
    }
}

#[test]
fn wide_screenshot_height_follows_width_limited_image() {
    let work_area = bounds(100.0, 40.0, 1600.0, 1000.0);
    let layout = editor_layout(
        placement(work_area, 1.0),
        work_area,
        FrameInsets::default(),
        (6000, 1000),
        1.0,
    );
    assert_close(layout.frame.width, 1600.0);
    assert_close(layout.frame.height, 311.0);
    assert_within(layout.frame, work_area);
}

#[test]
fn native_frame_and_toolbar_are_excluded_before_fitting_the_image() {
    let display = bounds(-2560.0, 0.0, 2560.0, 1440.0);
    let work_area = bounds(display.x, 0.0, display.width, 1400.0);
    let insets = FrameInsets {
        left: 16.0,
        top: 60.0,
        right: 16.0,
        bottom: 16.0,
    };
    let layout = editor_layout(
        placement(display, 2.0),
        work_area,
        insets,
        (2560, 1440),
        2.0,
    );
    let content_width = (layout.frame.width - insets.left - insets.right) / 2.0;
    let content_height = (layout.frame.height - insets.top - insets.bottom) / 2.0;
    assert_close(layout.frame.height, work_area.height);
    assert_close(content_height - 44.0, 618.0);
    assert_close(content_width, 1099.0);
    assert_within(layout.frame, work_area);
}

#[test]
fn regions_that_fit_keep_their_scale_position_and_minimum_window_size() {
    let work_area = bounds(0.0, 24.0, 1800.0, 1200.0);
    for (width, height) in [(100, 40), (1100, 700)] {
        let region = bounds(300.0, 250.0, f64::from(width), f64::from(height));
        let layout = editor_layout(
            placement(region, 1.0),
            work_area,
            FrameInsets::default(),
            (width * 2, height * 2),
            2.0,
        );
        let canvas = Bounds::new(
            point(px(0.0), px(theme::TOOLBAR_HEIGHT)),
            size(
                px(layout.frame.width as f32),
                px(layout.frame.height as f32 - theme::TOOLBAR_HEIGHT),
            ),
        );
        let viewport = ImageViewport::fit((width * 2) as f32, (height * 2) as f32, canvas, 2.0)
            .aligned(canvas, layout.image_alignment);
        assert_close(f64::from(viewport.scale), 0.5);
        assert_close(
            f64::from(f32::from(viewport.bounds.size.width)),
            region.width,
        );
        assert_close(
            f64::from(f32::from(viewport.bounds.size.height)),
            region.height,
        );
        assert_close(
            layout.frame.x + f64::from(f32::from(viewport.bounds.origin.x)),
            region.x,
        );
        assert_close(
            layout.frame.y + f64::from(f32::from(viewport.bounds.origin.y)),
            region.y,
        );
        assert!(layout.frame.width >= f64::from(MIN_EDITOR_WIDTH));
        assert!(layout.frame.height >= f64::from(MIN_EDITOR_HEIGHT));
        assert_within(layout.frame, work_area);
    }
}

#[test]
fn fractional_display_scales_do_not_add_a_pixel_to_integral_image_widths() {
    for (image_size, work_height, scale_factor, expected_width) in [
        ((1920, 1080), 993.0, 1.5, 1648.0),
        ((3840, 2160), 2116.0, 1.25, 3664.0),
        ((1920, 1080), 1067.0, 1.75, 1760.0),
    ] {
        let display = bounds(0.0, 0.0, f64::from(image_size.0), f64::from(image_size.1));
        let work_area = CaptureBounds {
            height: work_height,
            ..display
        };
        let layout = editor_layout(
            placement(display, f64::from(scale_factor)),
            work_area,
            FrameInsets::default(),
            image_size,
            scale_factor,
        );
        assert_close(layout.frame.width, expected_width);
        assert_close(layout.frame.height, work_height);
        assert_within(layout.frame, work_area);
    }
}

#[test]
fn fitted_image_edges_fill_the_window_after_device_pixel_rounding() {
    for (width, height, image_size, scale_factor, ui_scale) in [
        (1742.25, 1040.0, (6000, 1000), 1.0, 1.0),
        (1512.0, 893.25, (3024, 1964), 2.0, 1.0),
        (1920.0, 1040.3, (3840, 2160), 1.25, 1.0),
        (1600.0, 960.4, (3440, 1440), 1.5, 1.0),
        (1920.0, 993.0, (1920, 1080), 1.5, 1.5),
        (3840.0, 2116.0, (3840, 2160), 1.25, 1.25),
        (1920.0, 1067.0, (1920, 1080), 1.75, 1.75),
    ] {
        let work_area = bounds(0.0, 0.0, width, height);
        let layout = editor_layout(
            placement(work_area, ui_scale),
            work_area,
            FrameInsets::default(),
            image_size,
            scale_factor,
        );
        let device_pixel = |value: f32| (value * scale_factor - 0.5).ceil();
        let width = device_pixel((layout.frame.width / ui_scale) as f32) / scale_factor;
        let height = device_pixel((layout.frame.height / ui_scale) as f32) / scale_factor;
        let canvas = Bounds::new(
            point(px(0.0), px(theme::TOOLBAR_HEIGHT)),
            size(px(width), px(height - theme::TOOLBAR_HEIGHT)),
        );
        let viewport = ImageViewport::fit(
            image_size.0 as f32,
            image_size.1 as f32,
            canvas,
            scale_factor,
        )
        .aligned(canvas, layout.image_alignment);
        for (image_edge, canvas_edge) in [
            (viewport.bounds.left(), canvas.left()),
            (viewport.bounds.right(), canvas.right()),
            (viewport.bounds.top(), canvas.top()),
            (viewport.bounds.bottom(), canvas.bottom()),
        ] {
            assert_eq!(
                device_pixel(image_edge.into()),
                device_pixel(canvas_edge.into()),
                "{image_size:?} at {scale_factor}x"
            );
        }
    }
}
