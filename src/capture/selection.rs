use super::{CaptureBounds, CaptureDisplayId, CapturePoint, DisplaySnapshot, WindowTarget};
use crate::dimensions::{DimensionMode, PixelGrid};
use image::{RgbaImage, imageops};
use std::sync::Arc;

const DRAG_THRESHOLD: f64 = 4.0;

#[derive(Clone)]
pub enum CaptureSelection {
    Window(WindowTarget),
    Region {
        display_id: CaptureDisplayId,
        bounds: CaptureBounds,
    },
}

struct Press {
    origin: CapturePoint,
    start: CapturePoint,
    current: CapturePoint,
    window: Option<WindowTarget>,
    dragging: bool,
}

pub(super) struct SelectionState {
    display_id: CaptureDisplayId,
    display_bounds: CaptureBounds,
    windows: Arc<Vec<WindowTarget>>,
    hovered: Option<WindowTarget>,
    press: Option<Press>,
    drag_threshold: f64,
    grid: PixelGrid,
}

impl SelectionState {
    pub fn new(
        display_id: CaptureDisplayId,
        display_bounds: CaptureBounds,
        windows: Arc<Vec<WindowTarget>>,
    ) -> Self {
        Self {
            display_id,
            display_bounds,
            windows,
            hovered: None,
            press: None,
            drag_threshold: DRAG_THRESHOLD,
            grid: PixelGrid::default(),
        }
    }

    pub fn set_ui_scale(&mut self, scale: f64) {
        self.drag_threshold = DRAG_THRESHOLD * scale;
    }

    pub fn set_grid(&mut self, display: &DisplaySnapshot, mode: DimensionMode) {
        self.grid = match mode {
            DimensionMode::Logical => PixelGrid::new(display.ui_scale, display.ui_scale),
            DimensionMode::Original => PixelGrid::new(
                display.bounds.width / f64::from(display.pixels.width()),
                display.bounds.height / f64::from(display.pixels.height()),
            ),
        };
    }

    fn snap(&self, point: CapturePoint) -> CapturePoint {
        let bounds = self.display_bounds;
        CapturePoint {
            x: bounds.x + self.grid.clamp_x(point.x - bounds.x, 0.0, bounds.width),
            y: bounds.y + self.grid.clamp_y(point.y - bounds.y, 0.0, bounds.height),
        }
    }

    pub fn pointer_down(&mut self, point: CapturePoint) {
        if self.press.is_some() || !contains(self.display_bounds, point) {
            return;
        }
        self.hovered = self.window_at(point);
        let snapped = self.snap(point);
        self.press = Some(Press {
            origin: point,
            start: snapped,
            current: snapped,
            window: self.hovered.clone(),
            dragging: false,
        });
    }

    pub fn pointer_move(&mut self, point: CapturePoint) {
        if !point.x.is_finite() || !point.y.is_finite() {
            return;
        }
        let snapped = self.snap(point);
        if let Some(press) = &mut self.press {
            press.dragging |=
                (point.x - press.origin.x).hypot(point.y - press.origin.y) > self.drag_threshold;
            press.current = snapped;
        } else {
            self.hovered = self.window_at(point);
        }
    }

    pub fn pointer_up(&mut self, point: CapturePoint) -> Option<CaptureSelection> {
        self.pointer_move(point);
        let press = self.press.take()?;
        self.hovered = self.window_at(point);
        if press.dragging {
            let bounds = between(press.start, press.current);
            valid_bounds(bounds).then_some(CaptureSelection::Region {
                display_id: self.display_id,
                bounds,
            })
        } else {
            press.window.map(|window| {
                if window.capture_from_display {
                    CaptureSelection::Region {
                        display_id: self.display_id,
                        bounds: window.bounds,
                    }
                } else {
                    CaptureSelection::Window(window)
                }
            })
        }
    }

    pub fn cancel_gesture(&mut self) {
        self.press = None;
    }

    pub fn pointer_exit(&mut self) {
        self.hovered = None;
    }

    pub fn is_pressed(&self) -> bool {
        self.press.is_some()
    }

    pub fn selected_pixel_size(&self, display: &DisplaySnapshot) -> Option<(usize, usize)> {
        if let Some(window) = self.selected_window()
            && !window.capture_from_display
        {
            return window.pixel_size;
        }
        let (_, _, width, height) = pixel_bounds(display, self.selected_bounds()?).ok()?;
        Some((width as usize, height as usize))
    }

    pub fn size_label(&self, display: &DisplaySnapshot, mode: DimensionMode) -> Option<String> {
        let (width, height) = self.selected_pixel_size(display)?;
        let scale = self
            .selected_window()
            .filter(|target| !target.capture_from_display)
            .map_or_else(|| display.pixel_scale(), |target| target.pixel_scale);
        Some(scale.display(mode).size(width as f64, height as f64))
    }

    pub fn selected_bounds(&self) -> Option<CaptureBounds> {
        match &self.press {
            Some(press) if press.dragging => Some(between(press.start, press.current)),
            Some(press) => press.window.as_ref().map(|window| window.bounds),
            None => self.hovered.as_ref().map(|window| window.bounds),
        }
    }

    pub fn selected_window(&self) -> Option<&WindowTarget> {
        match &self.press {
            Some(press) if !press.dragging => press.window.as_ref(),
            Some(_) => None,
            None => self.hovered.as_ref(),
        }
    }

    fn window_at(&self, point: CapturePoint) -> Option<WindowTarget> {
        if !contains(self.display_bounds, point) {
            return None;
        }
        self.windows
            .iter()
            .find(|window| contains(window.bounds, point))
            .cloned()
    }
}

pub fn fullscreen_selection(
    displays: &[DisplaySnapshot],
    pointer: Option<CapturePoint>,
) -> Option<CaptureSelection> {
    let display = pointer
        .and_then(|point| {
            displays
                .iter()
                .find(|display| contains(display.bounds, point))
        })
        .or_else(|| {
            displays
                .iter()
                .find(|display| contains(display.bounds, CapturePoint { x: 0.0, y: 0.0 }))
        })
        .or_else(|| displays.first())?;
    Some(CaptureSelection::Region {
        display_id: display.id,
        bounds: display.bounds,
    })
}

pub fn crop_region(display: &DisplaySnapshot, bounds: CaptureBounds) -> Result<RgbaImage, String> {
    let (x, y, width, height) = pixel_bounds(display, bounds)?;
    Ok(imageops::crop_imm(display.pixels.as_ref(), x, y, width, height).to_image())
}

pub(super) fn pixel_bounds(
    display: &DisplaySnapshot,
    bounds: CaptureBounds,
) -> Result<(u32, u32, u32, u32), String> {
    let bounds = intersect_bounds(bounds, display.bounds)
        .ok_or_else(|| "Select a non-empty area within the display".to_string())?;
    let (pixel_width, pixel_height) = display.pixels.dimensions();
    if pixel_width == 0 || pixel_height == 0 {
        return Err("The captured display image is empty".into());
    }
    let scale_x = f64::from(pixel_width) / display.bounds.width;
    let scale_y = f64::from(pixel_height) / display.bounds.height;
    let left = ((bounds.x - display.bounds.x) * scale_x)
        .round()
        .clamp(0.0, f64::from(pixel_width)) as u32;
    let top = ((bounds.y - display.bounds.y) * scale_y)
        .round()
        .clamp(0.0, f64::from(pixel_height)) as u32;
    let right = ((bounds.x + bounds.width - display.bounds.x) * scale_x)
        .round()
        .clamp(0.0, f64::from(pixel_width)) as u32;
    let bottom = ((bounds.y + bounds.height - display.bounds.y) * scale_y)
        .round()
        .clamp(0.0, f64::from(pixel_height)) as u32;
    if right <= left || bottom <= top {
        return Err("Select a non-empty area within the display".into());
    }
    Ok((left, top, right - left, bottom - top))
}

pub(super) fn intersect_bounds(a: CaptureBounds, b: CaptureBounds) -> Option<CaptureBounds> {
    if !valid_bounds(a) || !valid_bounds(b) {
        return None;
    }
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    let bounds = CaptureBounds {
        x,
        y,
        width: (a.x + a.width).min(b.x + b.width) - x,
        height: (a.y + a.height).min(b.y + b.height) - y,
    };
    valid_bounds(bounds).then_some(bounds)
}

fn valid_bounds(bounds: CaptureBounds) -> bool {
    bounds.x.is_finite()
        && bounds.y.is_finite()
        && bounds.width.is_finite()
        && bounds.height.is_finite()
        && bounds.width > 0.0
        && bounds.height > 0.0
        && (bounds.x + bounds.width).is_finite()
        && (bounds.y + bounds.height).is_finite()
}

fn contains(bounds: CaptureBounds, point: CapturePoint) -> bool {
    valid_bounds(bounds)
        && point.x >= bounds.x
        && point.y >= bounds.y
        && point.x < bounds.x + bounds.width
        && point.y < bounds.y + bounds.height
}

fn between(a: CapturePoint, b: CapturePoint) -> CaptureBounds {
    CaptureBounds {
        x: a.x.min(b.x),
        y: a.y.min(b.y),
        width: (a.x - b.x).abs(),
        height: (a.y - b.y).abs(),
    }
}

#[cfg(test)]
mod tests;
