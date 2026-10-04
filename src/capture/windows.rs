#[path = "windows/frame.rs"]
mod frame;
#[path = "windows/worker.rs"]
mod worker;

use super::{
    CaptureBounds, CaptureDisplayId, CapturePoint, CaptureSnapshot, CaptureWindowFilter,
    CaptureWindowId, CapturedImage, DisplaySnapshot, WindowTarget,
    placement::{CapturePlacement, FrameInsets, editor_layout},
};
use crate::dimensions::PixelScale;
use gpui::{App, Context, Point, Window};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{
    collections::{HashMap, HashSet},
    mem::size_of,
    path::Path,
    ptr, slice,
    sync::Arc,
};
use windows::{
    Wdk::System::SystemServices::RtlGetVersion,
    Win32::{
        Devices::Display::{
            DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO,
            DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_HEADER,
            DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO, DISPLAYCONFIG_SOURCE_DEVICE_NAME,
            DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QDC_ONLY_ACTIVE_PATHS,
            QueryDisplayConfig,
        },
        Foundation::{CloseHandle, HWND, LPARAM, POINT, RECT},
        Graphics::{
            Dwm::{DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, DwmFlush, DwmGetWindowAttribute},
            Gdi::{
                ClientToScreen, DEVMODEW, ENUM_CURRENT_SETTINGS, EnumDisplayMonitors,
                EnumDisplaySettingsW, GetMonitorInfoW, HDC, HMONITOR, MONITOR_DEFAULTTONEAREST,
                MONITORINFOEXW, MonitorFromWindow,
            },
        },
        Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW},
        System::{
            SystemInformation::OSVERSIONINFOW,
            Threading::{
                OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
                QueryFullProcessImageNameW,
            },
        },
        UI::{
            HiDpi::{
                AreDpiAwarenessContextsEqual, DPI_AWARENESS_CONTEXT,
                DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForMonitor,
                GetThreadDpiAwarenessContext, MDT_EFFECTIVE_DPI, SetProcessDpiAwarenessContext,
                SetThreadDpiAwarenessContext,
            },
            Input::KeyboardAndMouse::SetFocus,
            WindowsAndMessaging::{
                EnumWindows, GWL_EXSTYLE, GWL_STYLE, GetClassNameW, GetClientRect, GetCursorPos,
                GetWindowLongPtrW, GetWindowRect, GetWindowTextLengthW, GetWindowTextW,
                GetWindowThreadProcessId, HWND_TOPMOST, IsIconic, IsWindow, IsWindowVisible,
                SW_HIDE, SW_SHOW, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOZORDER,
                SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow, WS_CAPTION,
                WS_EX_APPWINDOW, WS_EX_TOOLWINDOW, WS_POPUP, WS_THICKFRAME,
            },
        },
    },
    core::{BOOL, PCWSTR, PWSTR, w},
};

fn error(error: windows::core::Error) -> String {
    format!("Windows capture failed: {error}")
}

pub fn initialize() -> Result<(), String> {
    unsafe {
        if SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2).is_err()
            && !AreDpiAwarenessContextsEqual(
                GetThreadDpiAwarenessContext(),
                DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
            )
            .as_bool()
        {
            return Err("Snipuno requires per-monitor DPI awareness.".into());
        }
    }
    Ok(())
}

pub fn has_permission() -> bool {
    true
}

pub fn request_permission() -> bool {
    true
}

pub fn open_permission_settings(_: &App) {}

pub fn restart(cx: &mut App) {
    cx.restart();
}

pub fn pointer_position() -> Option<CapturePoint> {
    let _dpi = DpiContext::new().ok()?;
    let mut point = POINT::default();
    unsafe { GetCursorPos(&mut point) }.ok()?;
    Some(CapturePoint {
        x: f64::from(point.x),
        y: f64::from(point.y),
    })
}

struct DpiContext(DPI_AWARENESS_CONTEXT);

impl DpiContext {
    fn new() -> Result<Self, String> {
        let previous =
            unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        if previous.0.is_null() {
            return Err(error(windows::core::Error::from_thread()));
        }
        Ok(Self(previous))
    }
}

impl Drop for DpiContext {
    fn drop(&mut self) {
        unsafe { SetThreadDpiAwarenessContext(self.0) };
    }
}

fn ensure_available() -> Result<(), String> {
    let mut version = OSVERSIONINFOW {
        dwOSVersionInfoSize: size_of::<OSVERSIONINFOW>() as u32,
        ..Default::default()
    };
    unsafe { RtlGetVersion(&mut version).ok() }.map_err(error)?;
    if version.dwMajorVersion < 10 || version.dwBuildNumber < 22000 {
        return Err("Screen capture requires Windows 11 or later.".into());
    }
    if !windows::Graphics::Capture::GraphicsCaptureSession::IsSupported().map_err(error)? {
        return Err("Windows Graphics Capture is not available in this session.".into());
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq)]
struct Monitor {
    id: CaptureDisplayId,
    bounds: CaptureBounds,
    scale: f64,
    orientation: u32,
}

pub async fn snapshot(window_filter: CaptureWindowFilter) -> Result<CaptureSnapshot, String> {
    worker::run(move |cancel| {
        ensure_available()
            .map_err(|failure| format!("Capture availability check failed: {failure}"))?;
        let _dpi = DpiContext::new()?;
        unsafe { DwmFlush() }
            .map_err(|failure| format!("Unable to synchronize the desktop: {failure}"))?;
        let monitors = monitors()?;
        let mut windows = selectable_windows()
            .map_err(|failure| format!("Unable to list selectable windows: {failure}"))?;
        windows.retain(|target| window_filter.includes(target.id, target.process_id));
        let device = frame::Device::new()
            .map_err(|failure| format!("Unable to create a capture device: {failure}"))?;
        let mut displays = Vec::with_capacity(monitors.len());
        for monitor in &monitors {
            cancel.check()?;
            let pixels = device.monitor(monitor.id, cancel)?;
            if pixels.dimensions() != (monitor.bounds.width as u32, monitor.bounds.height as u32) {
                return Err("The display size changed. Start a new screenshot.".into());
            }
            displays.push(DisplaySnapshot {
                id: monitor.id,
                bounds: monitor.bounds,
                ui_scale: monitor.scale,
                orientation: monitor.orientation,
                mode_pixel_size: (pixels.width() as usize, pixels.height() as usize),
                pixels: Arc::new(pixels),
            });
        }
        if monitors != self::monitors()? {
            return Err("The display layout changed. Start a new screenshot.".into());
        }
        Ok(CaptureSnapshot { displays, windows })
    })
    .await
}

pub async fn capture_window(target: WindowTarget) -> Result<CapturedImage, String> {
    worker::run(move |cancel| {
        ensure_available()?;
        let _dpi = DpiContext::new()?;
        let layout = monitors()?;
        validate_window(&target)?;
        let pixels = frame::Device::new()?.window(&target, cancel)?;
        validate_window(&target)?;
        if layout != monitors()? {
            return Err("The display layout changed. Start a new screenshot.".into());
        }
        let pixel_scale = window_pixel_scale(HWND(target.id.0 as usize as *mut _))
            .ok_or("Unable to read the window's display scale.")?;
        Ok(CapturedImage {
            pixels,
            pixel_scale,
        })
    })
    .await
}

fn validate_window(target: &WindowTarget) -> Result<(), String> {
    let hwnd = HWND(target.id.0 as usize as *mut _);
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if !unsafe { IsWindow(Some(hwnd)).as_bool() }
        || unsafe { IsIconic(hwnd).as_bool() }
        || pid != target.process_id as u32
    {
        return Err(
            "The selected window closed or is no longer available. Start a new screenshot.".into(),
        );
    }
    Ok(())
}

fn monitors() -> Result<Vec<Monitor>, String> {
    let _dpi = DpiContext::new()?;
    check_sdr()?;
    let mut handles = Vec::<HMONITOR>::new();
    unsafe {
        EnumDisplayMonitors(
            None,
            None,
            Some(enum_monitor),
            LPARAM((&mut handles as *mut Vec<HMONITOR>) as isize),
        )
        .ok()
        .map_err(|failure| format!("Unable to enumerate displays: {failure}"))?;
    }
    let mut monitors = Vec::with_capacity(handles.len());
    for handle in handles {
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
        unsafe { GetMonitorInfoW(handle, &mut info.monitorInfo).ok() }
            .map_err(|failure| format!("Unable to read monitor bounds: {failure}"))?;
        let mut mode = DEVMODEW {
            dmSize: size_of::<DEVMODEW>() as u16,
            ..Default::default()
        };
        unsafe {
            EnumDisplaySettingsW(
                PCWSTR(info.szDevice.as_ptr()),
                ENUM_CURRENT_SETTINGS,
                &mut mode,
            )
            .ok()
        }
        .map_err(|failure| {
            format!(
                "Unable to read the display mode for {}: {failure}",
                utf16(&info.szDevice)
            )
        })?;
        let (mut x, mut y) = (0, 0);
        unsafe { GetDpiForMonitor(handle, MDT_EFFECTIVE_DPI, &mut x, &mut y) }.map_err(error)?;
        if x == 0 || x != y {
            return Err("The display DPI could not be read.".into());
        }
        monitors.push(Monitor {
            id: CaptureDisplayId(handle.0 as usize as u64),
            bounds: rect_bounds(info.monitorInfo.rcMonitor)?,
            scale: f64::from(x) / 96.0,
            orientation: unsafe { mode.Anonymous1.Anonymous2.dmDisplayOrientation.0 },
        });
    }
    monitors.sort_by_key(|monitor| monitor.id.0);
    if monitors.is_empty() {
        return Err("No displays are available for capture.".into());
    }
    Ok(monitors)
}

unsafe extern "system" fn enum_monitor(
    handle: HMONITOR,
    _: HDC,
    _: *mut RECT,
    data: LPARAM,
) -> BOOL {
    unsafe { &mut *(data.0 as *mut Vec<HMONITOR>) }.push(handle);
    true.into()
}

fn check_sdr() -> Result<(), String> {
    let (mut path_count, mut mode_count) = (0, 0);
    unsafe {
        GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut path_count, &mut mode_count).ok()
    }
    .map_err(error)?;
    let mut paths = vec![Default::default(); path_count as usize];
    let mut modes = vec![Default::default(); mode_count as usize];
    unsafe {
        QueryDisplayConfig(
            QDC_ONLY_ACTIVE_PATHS,
            &mut path_count,
            paths.as_mut_ptr(),
            &mut mode_count,
            modes.as_mut_ptr(),
            None,
        )
        .ok()
    }
    .map_err(error)?;
    for path in paths.iter().take(path_count as usize) {
        let mut color = DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO {
            header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                r#type: DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO,
                size: size_of::<DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO>() as u32,
                adapterId: path.targetInfo.adapterId,
                id: path.targetInfo.id,
            },
            ..Default::default()
        };
        let status = unsafe { DisplayConfigGetDeviceInfo(&mut color.header) };
        if status != 0 {
            return Err(format!(
                "Unable to check the display color mode (Windows error {status})."
            ));
        }
        // advancedColorEnabled is bit 1; conservatively reject wide-color SDR as well.
        if unsafe { color.Anonymous.value } & 2 != 0 {
            let mut name = DISPLAYCONFIG_SOURCE_DEVICE_NAME {
                header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                    r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
                    size: size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32,
                    adapterId: path.sourceInfo.adapterId,
                    id: path.sourceInfo.id,
                },
                ..Default::default()
            };
            unsafe { DisplayConfigGetDeviceInfo(&mut name.header) };
            return Err(format!(
                "Turn off HDR / advanced color for {} in Windows Display Settings, then try again. Snipuno currently captures SDR only.",
                utf16(&name.viewGdiDeviceName)
            ));
        }
    }
    Ok(())
}

fn utf16(value: &[u16]) -> String {
    String::from_utf16_lossy(
        &value[..value
            .iter()
            .position(|value| *value == 0)
            .unwrap_or(value.len())],
    )
}

fn rect_bounds(rect: RECT) -> Result<CaptureBounds, String> {
    let width = i64::from(rect.right) - i64::from(rect.left);
    let height = i64::from(rect.bottom) - i64::from(rect.top);
    if width <= 0 || height <= 0 {
        return Err("The capture dimensions are empty.".into());
    }
    Ok(CaptureBounds {
        x: f64::from(rect.left),
        y: f64::from(rect.top),
        width: width as f64,
        height: height as f64,
    })
}

fn selectable_windows() -> Result<Vec<WindowTarget>, String> {
    let mut windows: Vec<WindowTarget> = Vec::new();
    unsafe {
        EnumWindows(
            Some(enum_window),
            LPARAM((&mut windows as *mut Vec<WindowTarget>) as isize),
        )
    }
    .map_err(error)?;
    let mut app_names = HashMap::new();
    for window in &mut windows {
        window.app_name = app_names
            .entry(window.process_id)
            .or_insert_with(|| application_name(window.process_id as u32).unwrap_or_default())
            .clone();
    }
    Ok(windows)
}

fn application_name(pid: u32) -> Option<String> {
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let mut path = vec![0u16; 32768];
    let mut length = path.len() as u32;
    let result = unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(path.as_mut_ptr()),
            &mut length,
        )
    };
    let _ = unsafe { CloseHandle(process) };
    result.ok()?;
    path.truncate(length as usize);
    path.push(0);
    executable_description(&path).or_else(|| {
        Path::new(&utf16(&path))
            .file_stem()
            .map(|name| name.to_string_lossy().into_owned())
    })
}

fn executable_description(path: &[u16]) -> Option<String> {
    unsafe {
        let path = PCWSTR(path.as_ptr());
        let length = GetFileVersionInfoSizeW(path, None);
        if length == 0 {
            return None;
        }
        let mut info = vec![0u32; (length as usize).div_ceil(size_of::<u32>())];
        GetFileVersionInfoW(path, None, length, info.as_mut_ptr().cast()).ok()?;
        let mut translations = ptr::null_mut();
        let mut translation_length = 0;
        if !VerQueryValueW(
            info.as_ptr().cast(),
            w!("\\VarFileInfo\\Translation"),
            &mut translations,
            &mut translation_length,
        )
        .as_bool()
            || translations.is_null()
        {
            return None;
        }
        let translations = slice::from_raw_parts(
            translations.cast::<u16>(),
            translation_length as usize / size_of::<u16>(),
        );
        for translation in translations.chunks_exact(2) {
            let key: Vec<_> = format!(
                "\\StringFileInfo\\{:04x}{:04x}\\FileDescription",
                translation[0], translation[1]
            )
            .encode_utf16()
            .chain(Some(0))
            .collect();
            let mut value = ptr::null_mut();
            let mut value_length = 0;
            if VerQueryValueW(
                info.as_ptr().cast(),
                PCWSTR(key.as_ptr()),
                &mut value,
                &mut value_length,
            )
            .as_bool()
                && !value.is_null()
                && value_length > 0
            {
                let name = utf16(slice::from_raw_parts(value.cast(), value_length as usize));
                if !name.trim().is_empty() {
                    return Some(name);
                }
            }
        }
        None
    }
}

unsafe extern "system" fn enum_window(hwnd: HWND, data: LPARAM) -> BOOL {
    if let Some(target) = window_target(hwnd) {
        unsafe { &mut *(data.0 as *mut Vec<WindowTarget>) }.push(target);
    }
    true.into()
}

fn window_target(hwnd: HWND) -> Option<WindowTarget> {
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
            return None;
        }
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return None;
        }
        let mut cloaked = 0u32;
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            (&mut cloaked as *mut u32).cast(),
            size_of::<u32>() as u32,
        )
        .ok()?;
        if cloaked != 0 {
            return None;
        }
        let mut class = [0u16; 256];
        GetClassNameW(hwnd, &mut class);
        let class = utf16(&class);
        if matches!(class.as_str(), "Progman" | "WorkerW") {
            return None;
        }
        let capture_from_display =
            matches!(class.as_str(), "Shell_TrayWnd" | "Shell_SecondaryTrayWnd");
        let mut rect = RECT::default();
        if DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            (&mut rect as *mut RECT).cast(),
            size_of::<RECT>() as u32,
        )
        .is_err()
        {
            GetWindowRect(hwnd, &mut rect).ok()?;
        }
        let bounds = rect_bounds(rect).ok()?;
        let mut title = vec![0u16; (GetWindowTextLengthW(hwnd).max(0) as usize + 1).min(32768)];
        GetWindowTextW(hwnd, &mut title);
        Some(WindowTarget {
            id: CaptureWindowId(hwnd.0 as usize as u64),
            process_id: pid as i32,
            app_name: String::new(),
            title: utf16(&title),
            bounds,
            capture_from_display,
            pixel_size: Some((bounds.width as usize, bounds.height as usize)),
            pixel_scale: window_pixel_scale(hwnd)?,
        })
    }
}

fn window_pixel_scale(hwnd: HWND) -> Option<PixelScale> {
    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    let (mut x, mut y) = (0, 0);
    unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut x, &mut y) }.ok()?;
    Some(PixelScale::new(f64::from(x) / 96.0, f64::from(y) / 96.0))
}

pub fn display_layout_matches(displays: &[DisplaySnapshot]) -> bool {
    let Ok(monitors) = monitors() else {
        return false;
    };
    let ids: HashSet<_> = displays.iter().map(|display| display.id).collect();
    monitors.len() == displays.len()
        && monitors.iter().all(|monitor| {
            ids.contains(&monitor.id)
                && displays.iter().any(|display| {
                    display.id == monitor.id
                        && display.bounds == monitor.bounds
                        && display.ui_scale == monitor.scale
                        && display.orientation == monitor.orientation
                        && display.mode_pixel_size
                            == (
                                monitor.bounds.width as usize,
                                monitor.bounds.height as usize,
                            )
                })
        })
}

fn native_window(window: &Window) -> Result<HWND, String> {
    match HasWindowHandle::window_handle(window)
        .map_err(|error| error.to_string())?
        .as_raw()
    {
        RawWindowHandle::Win32(handle) => Ok(HWND(handle.hwnd.get() as *mut _)),
        _ => Err("The capture window is not a Windows window.".into()),
    }
}

pub fn configure_overlay<T: 'static>(
    window: &mut Window,
    bounds: CaptureBounds,
    _: &mut Context<T>,
) -> Result<(), String> {
    let _dpi = DpiContext::new()?;
    let hwnd = native_window(window)?;
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        SetWindowLongPtrW(
            hwnd,
            GWL_STYLE,
            ((style & !(WS_CAPTION.0 | WS_THICKFRAME.0)) | WS_POPUP.0) as isize,
        );
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        SetWindowLongPtrW(
            hwnd,
            GWL_EXSTYLE,
            ((style & !WS_EX_APPWINDOW.0) | WS_EX_TOOLWINDOW.0) as isize,
        );
        SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            bounds.x as i32,
            bounds.y as i32,
            bounds.width as i32,
            bounds.height as i32,
            SWP_NOACTIVATE | SWP_FRAMECHANGED,
        )
    }
    .map_err(error)
}

pub fn display_work_area(id: CaptureDisplayId) -> Option<CaptureBounds> {
    let _dpi = DpiContext::new().ok()?;
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
    unsafe {
        GetMonitorInfoW(HMONITOR(id.0 as usize as *mut _), &mut info.monitorInfo)
            .ok()
            .ok()?;
    }
    rect_bounds(info.monitorInfo.rcWork).ok()
}

pub fn position_editor(
    window: &Window,
    placement: CapturePlacement,
    work_area: CaptureBounds,
    image_size: (u32, u32),
) -> Result<Point<f32>, String> {
    let _dpi = DpiContext::new()?;
    let hwnd = native_window(window)?;
    let mut frame = RECT::default();
    let mut client = RECT::default();
    let mut origin = POINT::default();
    unsafe {
        GetWindowRect(hwnd, &mut frame).map_err(error)?;
        GetClientRect(hwnd, &mut client).map_err(error)?;
        ClientToScreen(hwnd, &mut origin).ok().map_err(error)?;
    }
    let content = CaptureBounds {
        x: f64::from(origin.x),
        y: f64::from(origin.y),
        width: f64::from(client.right - client.left),
        height: f64::from(client.bottom - client.top),
    };
    let insets = FrameInsets::between(rect_bounds(frame)?, content);
    let layout = editor_layout(
        placement,
        work_area,
        insets,
        image_size,
        window.scale_factor(),
    );
    let frame = layout.frame;
    unsafe {
        SetWindowPos(
            hwnd,
            None,
            frame.x.round() as i32,
            frame.y.round() as i32,
            frame.width.round() as i32,
            frame.height.round() as i32,
            SWP_NOACTIVATE | SWP_NOZORDER,
        )
        .map_err(error)?;
    }
    Ok(layout.image_alignment)
}

pub fn set_window_visible(window: &mut Window, visible: bool) -> Result<(), String> {
    let hwnd = native_window(window)?;
    if !unsafe { IsWindow(Some(hwnd)).as_bool() } {
        return Err("The window has closed.".into());
    }
    unsafe {
        let _ = ShowWindow(hwnd, if visible { SW_SHOW } else { SW_HIDE });
        if visible {
            // Global hotkeys can open an overlay while another app owns the foreground.
            let _ = SetForegroundWindow(hwnd);
            let _ = SetFocus(Some(hwnd));
        }
    }
    Ok(())
}

pub fn visible_window_id(window: &Window) -> Result<Option<CaptureWindowId>, String> {
    let hwnd = native_window(window)?;
    Ok(
        (unsafe { IsWindowVisible(hwnd).as_bool() && !IsIconic(hwnd).as_bool() })
            .then_some(CaptureWindowId(hwnd.0 as usize as u64)),
    )
}
