mod color;
mod controller;
mod overlay;
mod placement;
#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod platform;
mod selection;

#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod platform;
#[cfg(target_os = "windows")]
#[path = "windows.rs"]
mod platform;

use crate::dimensions::PixelScale;
use image::RgbaImage;
use std::sync::Arc;

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub(crate) use controller::open_editor;
pub(crate) use controller::remember_editor;
pub use controller::{
    FullscreenScreenshot, NewScreenshot, PickColor, ReopenLastScreenshot, TrayFullscreenScreenshot,
    TrayPickColor, TrayScreenshot, install,
};
pub(crate) use placement::CapturePlacement;
#[cfg(target_os = "windows")]
pub use platform::initialize;
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub(crate) use platform::set_window_visible;
pub use platform::{has_permission, open_permission_settings, request_permission, restart};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CaptureDisplayId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CaptureWindowId(pub u64);

pub(super) struct CaptureWindowFilter {
    visible_app_windows: Vec<CaptureWindowId>,
}

impl CaptureWindowFilter {
    pub fn new(visible_app_windows: Vec<CaptureWindowId>) -> Self {
        Self {
            visible_app_windows,
        }
    }

    pub fn includes(&self, id: CaptureWindowId, process_id: i32) -> bool {
        process_id as u32 != std::process::id() || self.visible_app_windows.contains(&id)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CapturePoint {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CaptureBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone)]
pub struct DisplaySnapshot {
    pub id: CaptureDisplayId,
    pub bounds: CaptureBounds,
    pub ui_scale: f64,
    #[cfg(target_os = "windows")]
    pub orientation: u32,
    pub mode_pixel_size: (usize, usize),
    pub pixels: Arc<RgbaImage>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowTarget {
    pub id: CaptureWindowId,
    pub process_id: i32,
    pub app_name: String,
    pub title: String,
    pub bounds: CaptureBounds,
    pub capture_from_display: bool,
    pub pixel_size: Option<(usize, usize)>,
    pub pixel_scale: PixelScale,
}

pub struct CapturedImage {
    pub pixels: RgbaImage,
    pub pixel_scale: PixelScale,
}

pub struct CaptureSnapshot {
    pub displays: Vec<DisplaySnapshot>,
    pub windows: Vec<WindowTarget>,
}

impl DisplaySnapshot {
    pub fn pixel_scale(&self) -> PixelScale {
        PixelScale::new(
            f64::from(self.pixels.width()) / self.bounds.width * self.ui_scale,
            f64::from(self.pixels.height()) / self.bounds.height * self.ui_scale,
        )
    }

    pub fn capture_point(&self, x: f64, y: f64) -> CapturePoint {
        CapturePoint {
            x: self.bounds.x + x * self.ui_scale,
            y: self.bounds.y + y * self.ui_scale,
        }
    }

    pub fn local_bounds(&self, bounds: CaptureBounds) -> CaptureBounds {
        CaptureBounds {
            x: (bounds.x - self.bounds.x) / self.ui_scale,
            y: (bounds.y - self.bounds.y) / self.ui_scale,
            width: bounds.width / self.ui_scale,
            height: bounds.height / self.ui_scale,
        }
    }
}
