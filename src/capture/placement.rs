use super::{
    CaptureBounds, CaptureDisplayId, DisplaySnapshot,
    selection::{CaptureSelection, intersect_bounds, pixel_bounds},
};
use crate::{canvas::ImageViewport, theme};
use gpui::{Bounds, Pixels, Point, Size, point, px, size};

#[cfg(target_os = "macos")]
pub const MIN_EDITOR_WIDTH: f32 = 960.0;
#[cfg(target_os = "windows")]
pub const MIN_EDITOR_WIDTH: f32 = 1000.0;
#[cfg(target_os = "linux")]
pub const MIN_EDITOR_WIDTH: f32 = 1000.0;

// Toolbar + gap + options panel + gap = 44 + 12 + (32 + 6 * 2 + 1 * 2) + 12 = 114 px.
// The two gaps keep equal space above and below the options panel.
pub const MIN_EDITOR_HEIGHT: f32 =
    theme::TOOLBAR_HEIGHT + theme::OPTIONS_PANEL_HEIGHT + theme::OPTIONS_PANEL_MARGIN * 2.0;

#[derive(Clone, Copy)]
pub struct CapturePlacement {
    pub bounds: CaptureBounds,
    pub display_id: CaptureDisplayId,
    pub display_bounds: CaptureBounds,
    pub ui_scale: f64,
}

impl CapturePlacement {
    pub fn new(selection: &CaptureSelection, displays: &[DisplaySnapshot]) -> Option<Self> {
        let (display, bounds) = match selection {
            CaptureSelection::Region { display_id, bounds } => {
                let display = displays.iter().find(|display| display.id == *display_id)?;
                let (x, y, width, height) = pixel_bounds(display, *bounds).ok()?;
                let scale_x = display.bounds.width / f64::from(display.pixels.width());
                let scale_y = display.bounds.height / f64::from(display.pixels.height());
                let bounds = CaptureBounds {
                    x: display.bounds.x + f64::from(x) * scale_x,
                    y: display.bounds.y + f64::from(y) * scale_y,
                    width: f64::from(width) * scale_x,
                    height: f64::from(height) * scale_y,
                };
                (display, bounds)
            }
            CaptureSelection::Window(target) => {
                let display = displays
                    .iter()
                    .filter_map(|display| {
                        intersect_bounds(display.bounds, target.bounds)
                            .map(|bounds| (display, bounds.width * bounds.height))
                    })
                    .max_by(|a, b| a.1.total_cmp(&b.1))?
                    .0;
                (display, target.bounds)
            }
        };
        Some(Self {
            bounds,
            display_id: display.id,
            display_bounds: display.bounds,
            ui_scale: display.ui_scale,
        })
    }
}

#[derive(Clone, Copy, Default)]
pub struct FrameInsets {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

impl FrameInsets {
    pub fn between(frame: CaptureBounds, content: CaptureBounds) -> Self {
        Self {
            left: content.x - frame.x,
            top: content.y - frame.y,
            right: frame.x + frame.width - content.x - content.width,
            bottom: frame.y + frame.height - content.y - content.height,
        }
    }
}

pub struct EditorLayout {
    pub frame: CaptureBounds,
    pub image_alignment: Point<f32>,
}

pub fn minimum_size(work_area: CaptureBounds, ui_scale: f64) -> Size<Pixels> {
    size(
        px(MIN_EDITOR_WIDTH.min((work_area.width / ui_scale) as f32)),
        px(MIN_EDITOR_HEIGHT.min((work_area.height / ui_scale) as f32)),
    )
}

pub fn editor_layout(
    placement: CapturePlacement,
    work_area: CaptureBounds,
    insets: FrameInsets,
    image_size: (u32, u32),
    scale_factor: f32,
) -> EditorLayout {
    let ui_scale = placement.ui_scale;
    let area = CaptureBounds {
        x: work_area.x + insets.left,
        y: work_area.y + insets.top,
        width: work_area.width - insets.left - insets.right,
        height: work_area.height - insets.top - insets.bottom,
    };
    let device_pixels = |value: f32| {
        let value = value * scale_factor;
        let nearest = value.round();
        let tolerance = value.abs().max(1.0) * f32::EPSILON * 4.0;
        if (value - nearest).abs() <= tolerance {
            nearest
        } else {
            value
        }
    };
    let pixel_floor = |value: f32| device_pixels(value).floor() / scale_factor;
    let pixel_ceil = |value: f32| device_pixels(value).ceil() / scale_factor;
    let max_width = pixel_floor((area.width / ui_scale) as f32);
    let max_height = pixel_floor((area.height / ui_scale) as f32);
    let canvas = Bounds::new(
        point(px(0.0), px(theme::TOOLBAR_HEIGHT)),
        size(px(max_width), px(max_height - theme::TOOLBAR_HEIGHT)),
    );
    let viewport = ImageViewport::fit(
        image_size.0 as f32,
        image_size.1 as f32,
        canvas,
        scale_factor,
    );
    let width = pixel_ceil(
        (f32::from(viewport.bounds.size.width) + theme::CANVAS_PADDING * 2.0).max(MIN_EDITOR_WIDTH),
    )
    .min(max_width);
    let height = pixel_ceil(
        (f32::from(viewport.bounds.size.height)
            + theme::CANVAS_PADDING * 2.0
            + theme::TOOLBAR_HEIGHT)
            .max(MIN_EDITOR_HEIGHT),
    )
    .min(max_height);
    let target_center = point(
        (placement.bounds.x + placement.bounds.width / 2.0) / ui_scale,
        (placement.bounds.y + placement.bounds.height / 2.0) / ui_scale,
    );
    let snap_origin =
        |value: f64| (value * f64::from(scale_factor)).round() / f64::from(scale_factor);
    let x = snap_origin(target_center.x - f64::from(width) / 2.0).clamp(
        area.x / ui_scale,
        ((area.x + area.width) / ui_scale - f64::from(width)).max(area.x / ui_scale),
    );
    let y = snap_origin(target_center.y - f64::from(height + theme::TOOLBAR_HEIGHT) / 2.0).clamp(
        area.y / ui_scale,
        ((area.y + area.height) / ui_scale - f64::from(height)).max(area.y / ui_scale),
    );
    let alignment = |target: f64, origin: f32, available: f32, image: f32| {
        let room = available - theme::CANVAS_PADDING * 2.0 - image;
        if room * scale_factor > 1.0 {
            ((target as f32 - origin - theme::CANVAS_PADDING - image / 2.0) / room).clamp(0.0, 1.0)
        } else {
            0.5
        }
    };
    EditorLayout {
        frame: CaptureBounds {
            x: x * ui_scale - insets.left,
            y: y * ui_scale - insets.top,
            width: f64::from(width) * ui_scale + insets.left + insets.right,
            height: f64::from(height) * ui_scale + insets.top + insets.bottom,
        },
        image_alignment: point(
            alignment(
                target_center.x - x,
                0.0,
                width,
                viewport.bounds.size.width.into(),
            ),
            alignment(
                target_center.y - y,
                theme::TOOLBAR_HEIGHT,
                height - theme::TOOLBAR_HEIGHT,
                viewport.bounds.size.height.into(),
            ),
        ),
    }
}

#[cfg(test)]
mod tests;
