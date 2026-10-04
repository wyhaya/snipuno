use crate::desktop::pinned_geometry::fitted_size;
use gpui::{App, Window};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::mem::size_of;
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
    Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow},
    UI::{
        HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi},
        Input::KeyboardAndMouse::{ReleaseCapture, SetFocus},
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::*,
    },
};

fn native_window(window: &Window) -> Result<HWND, String> {
    match HasWindowHandle::window_handle(window)
        .map_err(|error| error.to_string())?
        .as_raw()
    {
        RawWindowHandle::Win32(handle) => Ok(HWND(handle.hwnd.get() as *mut _)),
        _ => Err("Not a Windows window.".into()),
    }
}

pub fn show_window(window: &mut Window) -> Result<(), String> {
    let hwnd = native_window(window)?;
    unsafe {
        let _ = ShowWindow(
            hwnd,
            if IsIconic(hwnd).as_bool() {
                SW_RESTORE
            } else {
                SW_SHOW
            },
        );
        let _ = SetForegroundWindow(hwnd);
    }
    Ok(())
}

pub fn start_window_move(window: &Window, cx: &App) {
    let Ok(hwnd) = native_window(window) else {
        return;
    };
    cx.foreground_executor()
        .spawn(async move {
            unsafe {
                if !IsWindow(Some(hwnd)).as_bool() {
                    return;
                }
                let _ = ReleaseCapture();
                let mut point = POINT::default();
                if GetCursorPos(&mut point).is_ok() {
                    let position = (point.x as u16 as u32) | ((point.y as u16 as u32) << 16);
                    let _ = PostMessageW(
                        Some(hwnd),
                        WM_SYSCOMMAND,
                        WPARAM((SC_MOVE | 2) as usize),
                        LPARAM(position as isize),
                    );
                }
            }
        })
        .detach();
}

struct PinnedGeometry {
    frame: RECT,
    monitor: RECT,
    work_area: RECT,
    insets: (i32, i32),
    client_size: (f64, f64),
    scale: f64,
}

impl PinnedGeometry {
    fn read(hwnd: HWND) -> Result<Self, String> {
        let mut frame = RECT::default();
        let mut client = RECT::default();
        let mut monitor = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let dpi = unsafe {
            GetWindowRect(hwnd, &mut frame).map_err(|error| error.to_string())?;
            GetClientRect(hwnd, &mut client).map_err(|error| error.to_string())?;
            GetMonitorInfoW(
                MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST),
                &mut monitor,
            )
            .ok()
            .map_err(|error| error.to_string())?;
            GetDpiForWindow(hwnd)
        };
        let client_size = (client.right - client.left, client.bottom - client.top);
        let insets = if unsafe { IsIconic(hwnd).as_bool() } {
            let border = unsafe {
                GetSystemMetricsForDpi(SM_CXSIZEFRAME, dpi)
                    + GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi)
            };
            (
                border * 2,
                border
                    * if unsafe { IsZoomed(hwnd).as_bool() } {
                        2
                    } else {
                        1
                    },
            )
        } else {
            (
                frame.right - frame.left - client_size.0,
                frame.bottom - frame.top - client_size.1,
            )
        };
        Ok(Self {
            frame,
            monitor: monitor.rcMonitor,
            work_area: monitor.rcWork,
            insets,
            client_size: (f64::from(client_size.0), f64::from(client_size.1)),
            scale: if dpi == 0 { 1.0 } else { f64::from(dpi) / 96.0 },
        })
    }

    fn fitted_frame_size(
        &self,
        logical_size: (f64, f64),
        zoom: f64,
        area_scale: f64,
    ) -> (i32, i32) {
        let available = (
            (f64::from(self.work_area.right - self.work_area.left) * area_scale
                - f64::from(self.insets.0))
            .max(1.0)
                / self.scale,
            (f64::from(self.work_area.bottom - self.work_area.top) * area_scale
                - f64::from(self.insets.1))
            .max(1.0)
                / self.scale,
        );
        let size = fitted_size(logical_size, available, zoom, 288.0);
        (
            (size.0 * self.scale).round() as i32 + self.insets.0,
            (size.1 * self.scale).round() as i32 + self.insets.1,
        )
    }

    fn clamp_origin(&self, x: i32, y: i32, size: (i32, i32)) -> (i32, i32) {
        (
            x.clamp(
                self.work_area.left,
                (self.work_area.right - size.0).max(self.work_area.left),
            ),
            y.clamp(
                self.work_area.top,
                (self.work_area.bottom - size.1).max(self.work_area.top),
            ),
        )
    }
}

unsafe extern "system" fn pinned_window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    id: usize,
    data: usize,
) -> LRESULT {
    if message == WM_NCDESTROY {
        unsafe {
            let _ = RemoveWindowSubclass(hwnd, Some(pinned_window_proc), id);
            drop(Box::from_raw(data as *mut (f64, f64)));
        }
    } else if message == WM_SIZING {
        if let Ok(geometry) = PinnedGeometry::read(hwnd) {
            let logical_size = unsafe { *(data as *const (f64, f64)) };
            let frame = unsafe { &mut *(lparam.0 as *mut RECT) };
            let edge = wparam.0 as u32;
            let zoom = if matches!(edge, WMSZ_TOP | WMSZ_BOTTOM) {
                f64::from(frame.bottom - frame.top - geometry.insets.1)
                    / geometry.scale
                    / logical_size.1
            } else {
                f64::from(frame.right - frame.left - geometry.insets.0)
                    / geometry.scale
                    / logical_size.0
            };
            let size = geometry.fitted_frame_size(logical_size, zoom, 1.0);
            if matches!(edge, WMSZ_LEFT | WMSZ_TOPLEFT | WMSZ_BOTTOMLEFT) {
                frame.left = frame.right - size.0;
            } else {
                frame.right = frame.left + size.0;
            }
            if matches!(edge, WMSZ_TOP | WMSZ_TOPLEFT | WMSZ_TOPRIGHT) {
                frame.top = frame.bottom - size.1;
            } else {
                frame.bottom = frame.top + size.1;
            }
            return LRESULT(1);
        }
    } else if message == WM_GETMINMAXINFO {
        let result = unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };
        if let Ok(mut geometry) = PinnedGeometry::read(hwnd) {
            let logical_size = unsafe { *(data as *const (f64, f64)) };
            let limits = unsafe { &mut *(lparam.0 as *mut MINMAXINFO) };
            let minimum = geometry.fitted_frame_size(logical_size, 0.0, 1.0);
            limits.ptMinTrackSize = POINT {
                x: minimum.0,
                y: minimum.1,
            };
            let maximum_track = geometry.fitted_frame_size(logical_size, f64::MAX, 1.0);
            limits.ptMaxTrackSize = POINT {
                x: maximum_track.0,
                y: maximum_track.1,
            };
            // GPUI keeps the top resize border inside the client area only when maximized.
            if !unsafe { IsZoomed(hwnd).as_bool() } {
                geometry.insets.1 += geometry.insets.0 / 2;
            }
            let maximum = geometry.fitted_frame_size(logical_size, f64::MAX, 1.0);
            limits.ptMaxSize = POINT {
                x: maximum.0,
                y: maximum.1,
            };
            limits.ptMaxPosition = POINT {
                x: geometry.work_area.left - geometry.monitor.left
                    + (geometry.work_area.right - geometry.work_area.left - maximum.0) / 2,
                y: geometry.work_area.top - geometry.monitor.top
                    + (geometry.work_area.bottom - geometry.work_area.top - maximum.1) / 2,
            };
        }
        return result;
    }
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

pub fn configure_pinned_window(window: &Window, logical_size: (f64, f64)) -> Result<(), String> {
    let hwnd = native_window(window)?;
    let geometry = PinnedGeometry::read(hwnd)?;
    let size = geometry.fitted_frame_size(logical_size, 1.0, 0.8);
    let origin = geometry.clamp_origin(geometry.frame.left, geometry.frame.top, size);
    // The subclass owns this allocation until the native window is destroyed.
    let data = Box::into_raw(Box::new(logical_size));
    if !unsafe { SetWindowSubclass(hwnd, Some(pinned_window_proc), 1, data as usize).as_bool() } {
        unsafe {
            drop(Box::from_raw(data));
        }
        return Err("Unable to configure pinned window resizing.".into());
    }
    unsafe {
        SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            origin.0,
            origin.1,
            size.0,
            size.1,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        )
        .map_err(|error| error.to_string())?;
        let _ = SetForegroundWindow(hwnd);
        let _ = SetFocus(Some(hwnd));
    }
    Ok(())
}

pub fn pinned_window_contains_pointer(window: &Window) -> bool {
    let Ok(hwnd) = native_window(window) else {
        return false;
    };
    let mut frame = RECT::default();
    let mut point = POINT::default();
    unsafe {
        IsWindowVisible(hwnd).as_bool()
            && !IsIconic(hwnd).as_bool()
            && GetWindowRect(hwnd, &mut frame).is_ok()
            && GetCursorPos(&mut point).is_ok()
            && point.x >= frame.left
            && point.x < frame.right
            && point.y >= frame.top
            && point.y < frame.bottom
    }
}

pub fn resize_pinned_window(
    window: &Window,
    logical_size: (f64, f64),
    factor: f64,
    cx: &App,
) -> Result<(), String> {
    let hwnd = native_window(window)?;
    cx.foreground_executor()
        .spawn(async move {
            let result = (|| {
                if !unsafe { IsWindowVisible(hwnd).as_bool() }
                    || unsafe { IsIconic(hwnd).as_bool() }
                {
                    return Ok(());
                }
                let mut geometry = PinnedGeometry::read(hwnd)?;
                let zoom = geometry.client_size.0 / geometry.scale / logical_size.0 * factor;
                if unsafe { IsZoomed(hwnd).as_bool() } {
                    unsafe {
                        let _ = ShowWindow(hwnd, SW_RESTORE);
                    }
                    geometry = PinnedGeometry::read(hwnd)?;
                }
                let size = geometry.fitted_frame_size(logical_size, zoom, 1.0);
                let mut pointer = POINT::default();
                unsafe {
                    GetCursorPos(&mut pointer).map_err(|error| error.to_string())?;
                }
                let frame = geometry.frame;
                let x = f64::from(pointer.x)
                    - f64::from(pointer.x - frame.left) * f64::from(size.0)
                        / f64::from(frame.right - frame.left);
                let y = f64::from(pointer.y)
                    - f64::from(pointer.y - frame.top) * f64::from(size.1)
                        / f64::from(frame.bottom - frame.top);
                let origin = geometry.clamp_origin(x.round() as i32, y.round() as i32, size);
                unsafe {
                    SetWindowPos(
                        hwnd,
                        None,
                        origin.0,
                        origin.1,
                        size.0,
                        size.1,
                        SWP_NOACTIVATE | SWP_NOZORDER,
                    )
                }
                .map_err(|error| error.to_string())
            })();
            if let Err(error) = result {
                eprintln!("Unable to resize pinned screenshot: {error}");
            }
        })
        .detach();
    Ok(())
}

pub struct CaptureContext {
    frontmost: HWND,
}

impl CaptureContext {
    pub fn new(_: &App) -> Self {
        Self {
            frontmost: unsafe { GetForegroundWindow() },
        }
    }

    pub fn restore_focus(self, _: &mut App) {
        unsafe {
            if IsWindow(Some(self.frontmost)).as_bool() {
                let _ = SetForegroundWindow(self.frontmost);
            }
        }
    }
}
