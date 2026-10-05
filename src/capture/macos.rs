use super::{
    CaptureBounds, CaptureDisplayId, CapturePoint, CaptureSnapshot, CaptureWindowFilter,
    CaptureWindowId, CapturedImage, DisplaySnapshot, WindowTarget,
    placement::{CapturePlacement, FrameInsets, editor_layout},
};
use crate::{dimensions::PixelScale, image_processing::captured_pixels};
use block2::RcBlock;
use futures_channel::oneshot;
use gpui::{App, Context, Point, Window};
use image::RgbaImage;
use objc2::{AnyThread, MainThreadMarker, rc::Retained};
use objc2_app_kit::{
    NSScreen, NSView, NSWindow, NSWindowAnimationBehavior, NSWindowCollectionBehavior,
    NSWindowSharingType, NSWindowStyleMask,
};
use objc2_core_foundation::{
    CFDictionary, CFNumber, CFNumberType, CFString, CGPoint, CGRect, CGSize,
};
use objc2_core_graphics::{
    CGDataProvider, CGDisplayBounds, CGDisplayCopyDisplayMode, CGDisplayMode, CGError, CGEvent,
    CGGetActiveDisplayList, CGImage, CGImageAlphaInfo, CGImageByteOrderInfo, CGMainDisplayID,
    CGPreflightScreenCaptureAccess, CGRectMakeWithDictionaryRepresentation,
    CGRequestScreenCaptureAccess, CGWindowListCopyWindowInfo, CGWindowListOption,
    kCGColorSpaceSRGB, kCGMainMenuWindowLevel, kCGScreenSaverWindowLevel, kCGWindowAlpha,
    kCGWindowBounds, kCGWindowLayer, kCGWindowNumber, kCGWindowOwnerPID,
};
use objc2_foundation::{NSArray, NSError, NSOperatingSystemVersion, NSProcessInfo};
use objc2_screen_capture_kit::{
    SCContentFilter, SCScreenshotManager, SCShareableContent, SCStreamConfiguration,
    SCStreamErrorCode, SCStreamErrorDomain, SCWindow,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

type ImageReceiver = oneshot::Receiver<Result<RgbaImage, String>>;

struct PendingDisplay {
    id: u32,
    bounds: CaptureBounds,
    mode_pixel_size: (usize, usize),
    image: ImageReceiver,
}

pub fn has_permission() -> bool {
    CGPreflightScreenCaptureAccess()
}

pub fn request_permission() -> bool {
    CGRequestScreenCaptureAccess()
}

pub fn open_permission_settings(cx: &App) {
    cx.open_url("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture");
}

pub fn restart(cx: &mut App) {
    cx.restart();
}

pub fn pointer_position() -> Option<CapturePoint> {
    let event = CGEvent::new(None)?;
    let point = CGEvent::location(Some(&event));
    Some(CapturePoint {
        x: point.x,
        y: point.y,
    })
}

pub async fn snapshot(window_filter: CaptureWindowFilter) -> Result<CaptureSnapshot, String> {
    ensure_capture_supported()?;
    let (pending, windows) = shareable_content(move |content| {
        let windows = selectable_windows(content, &window_filter)?;
        let excluded: Vec<_> = unsafe { content.windows() }
            .into_iter()
            .filter(|window| unsafe {
                window.owningApplication().is_some_and(|application| {
                    !window_filter.includes(
                        CaptureWindowId(u64::from(window.windowID())),
                        application.processID(),
                    )
                })
            })
            .collect();
        let excluded = NSArray::from_retained_slice(&excluded);
        let mut pending = Vec::new();
        for display in unsafe { content.displays() } {
            let id = unsafe { display.displayID() };
            let bounds = capture_bounds(CGDisplayBounds(id));
            if !valid_bounds(bounds) {
                continue;
            }
            let filter = unsafe {
                SCContentFilter::initWithDisplay_excludingWindows(
                    SCContentFilter::alloc(),
                    &display,
                    &excluded,
                )
            };
            if NSProcessInfo::processInfo().isOperatingSystemAtLeastVersion(
                NSOperatingSystemVersion {
                    majorVersion: 14,
                    minorVersion: 2,
                    patchVersion: 0,
                },
            ) {
                unsafe { filter.setIncludeMenuBar(true) };
            }
            pending.push(PendingDisplay {
                id,
                bounds,
                mode_pixel_size: display_mode_pixel_size(id)
                    .ok_or("The display resolution could not be read. Try again.")?,
                image: capture_image(filter, false)?,
            });
        }
        if pending.is_empty() {
            return Err("No display is available for capture.".into());
        }
        Ok((pending, windows))
    })
    .await?;

    let mut displays = Vec::with_capacity(pending.len());
    for display in pending {
        let pixels = receive(display.image).await?;
        displays.push(DisplaySnapshot {
            id: CaptureDisplayId(u64::from(display.id)),
            ui_scale: 1.0,
            bounds: display.bounds,
            mode_pixel_size: display.mode_pixel_size,
            pixels: Arc::new(pixels),
        });
    }
    if !display_layout_matches(&displays) {
        return Err("The display layout changed. Start a new capture.".into());
    }
    Ok(CaptureSnapshot { displays, windows })
}

pub async fn capture_window(target: WindowTarget) -> Result<CapturedImage, String> {
    ensure_capture_supported()?;
    let pending = shareable_content(move |content| {
        let window = unsafe { content.windows() }
            .iter()
            .find(|window| {
                (unsafe { u64::from(window.windowID()) == target.id.0 })
                    && window_target(window)
                        .is_some_and(|window| window.process_id == target.process_id)
            })
            .ok_or_else(|| {
                "The selected window closed or is no longer available. Start a new capture."
                    .to_string()
            })?;
        let filter = unsafe {
            SCContentFilter::initWithDesktopIndependentWindow(SCContentFilter::alloc(), &window)
        };
        let scale = f64::from(unsafe { filter.pointPixelScale() });
        Ok((capture_image(filter, true)?, PixelScale::new(scale, scale)))
    })
    .await?;
    Ok(CapturedImage {
        pixels: receive(pending.0).await?,
        pixel_scale: pending.1,
    })
}

fn ensure_capture_supported() -> Result<(), String> {
    if !NSProcessInfo::processInfo().isOperatingSystemAtLeastVersion(NSOperatingSystemVersion {
        majorVersion: 14,
        minorVersion: 0,
        patchVersion: 0,
    }) {
        return Err("Screen capture requires macOS 14 or later.".into());
    }
    Ok(())
}

async fn shareable_content<T: Send + 'static>(
    prepare: impl FnOnce(&SCShareableContent) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let (sender, receiver) = oneshot::channel();
    let completion = Mutex::new(Some((sender, prepare)));
    let block = RcBlock::new(
        move |content: *mut SCShareableContent, error: *mut NSError| {
            let Some((sender, prepare)) = completion.lock().unwrap().take() else {
                return;
            };
            let result = unsafe {
                if let Some(error) = error.as_ref() {
                    Err(capture_error(error))
                } else if let Some(content) = content.as_ref() {
                    prepare(content)
                } else {
                    Err("macOS returned no content to capture.".into())
                }
            };
            let _ = sender.send(result);
        },
    );
    unsafe {
        SCShareableContent::getShareableContentExcludingDesktopWindows_onScreenWindowsOnly_completionHandler(
            true, true, &block,
        );
    }
    drop(block);
    receive(receiver).await
}

fn capture_dimensions(filter: &SCContentFilter) -> Result<(usize, usize), String> {
    let (rect, scale) = unsafe { (filter.contentRect(), f64::from(filter.pointPixelScale())) };
    Ok((
        pixel_dimension(rect.size.width * scale)?,
        pixel_dimension(rect.size.height * scale)?,
    ))
}

fn capture_image(filter: Retained<SCContentFilter>, window: bool) -> Result<ImageReceiver, String> {
    let (width, height) = capture_dimensions(&filter)?;
    let configuration = unsafe { SCStreamConfiguration::new() };
    unsafe {
        configuration.setWidth(width);
        configuration.setHeight(height);
        configuration.setShowsCursor(false);
        configuration.setCapturesAudio(false);
        configuration.setPixelFormat(u32::from_be_bytes(*b"BGRA"));
        configuration.setColorSpaceName(kCGColorSpaceSRGB);
        configuration.setIgnoreShadowsSingleWindow(window);
    }
    let (sender, receiver) = oneshot::channel();
    let sender = Mutex::new(Some(sender));
    // The callback owns the native inputs until ScreenCaptureKit finishes using them.
    let keep_filter = filter.clone();
    let keep_configuration = configuration.clone();
    let block = RcBlock::new(move |image: *mut CGImage, error: *mut NSError| {
        let _inputs = (&keep_filter, &keep_configuration);
        let Some(sender) = sender.lock().unwrap().take() else {
            return;
        };
        let result = unsafe {
            if let Some(error) = error.as_ref() {
                Err(capture_error(error))
            } else if let Some(image) = image.as_ref() {
                rgba_image(image)
            } else {
                Err("macOS returned no image. Try again.".into())
            }
        };
        let _ = sender.send(result);
    });
    unsafe {
        SCScreenshotManager::captureImageWithFilter_configuration_completionHandler(
            &filter,
            &configuration,
            Some(&block),
        );
    }
    Ok(receiver)
}

async fn receive<T>(receiver: oneshot::Receiver<Result<T, String>>) -> Result<T, String> {
    receiver
        .await
        .map_err(|_| "The system interrupted the capture. Try again.".to_string())?
}

fn pixel_dimension(value: f64) -> Result<usize, String> {
    if !value.is_finite() || value < 1.0 || value > u32::MAX as f64 {
        return Err("The capture dimensions are invalid.".into());
    }
    Ok(value.round() as usize)
}

fn capture_error(error: &NSError) -> String {
    if error
        .domain()
        .isEqualToString(unsafe { SCStreamErrorDomain })
        && error.code() == SCStreamErrorCode::UserDeclined.0
    {
        "Allow Snipuno to record the screen in System Settings, then try again.".into()
    } else {
        format!("Screen capture failed: {}", error.localizedDescription())
    }
}

fn selectable_windows(
    content: &SCShareableContent,
    window_filter: &CaptureWindowFilter,
) -> Result<Vec<WindowTarget>, String> {
    let mut targets: HashMap<_, _> = unsafe { content.windows() }
        .iter()
        .filter_map(|window| window_target(&window).map(|target| (target.id, target)))
        .collect();
    let info = CGWindowListCopyWindowInfo(
        CGWindowListOption::OptionOnScreenOnly | CGWindowListOption::ExcludeDesktopElements,
        0,
    )
    .ok_or_else(|| "The window order could not be read. Try again.".to_string())?;
    let mut windows = Vec::new();
    for index in 0..info.count() {
        // CoreGraphics guarantees a CFDictionary for every entry in this array.
        let dictionary = unsafe { &*info.value_at_index(index).cast::<CFDictionary>() };
        let id = unsafe { dictionary_number(dictionary, kCGWindowNumber) };
        let alpha = unsafe { dictionary_number(dictionary, kCGWindowAlpha) };
        if alpha.is_some_and(|alpha| alpha <= 0.0) {
            continue;
        }
        if let Some(target) = id
            .and_then(|id| targets.remove(&CaptureWindowId(id as u64)))
            .or_else(|| menu_bar_target(dictionary))
            .filter(|target| window_filter.includes(target.id, target.process_id))
        {
            windows.push(target);
        }
    }
    Ok(windows)
}

fn menu_bar_target(dictionary: &CFDictionary) -> Option<WindowTarget> {
    unsafe {
        if dictionary_number(dictionary, kCGWindowLayer)? != f64::from(kCGMainMenuWindowLevel) {
            return None;
        }
        let value = dictionary.value((kCGWindowBounds as *const CFString).cast());
        let bounds_dictionary = value.cast::<CFDictionary>().as_ref()?;
        let mut rect = CGRect::default();
        if !CGRectMakeWithDictionaryRepresentation(Some(bounds_dictionary), &mut rect) {
            return None;
        }
        let bounds = capture_bounds(rect);
        if !valid_bounds(bounds) {
            return None;
        }
        Some(WindowTarget {
            id: CaptureWindowId(dictionary_number(dictionary, kCGWindowNumber)? as u64),
            process_id: dictionary_number(dictionary, kCGWindowOwnerPID)? as i32,
            app_name: String::new(),
            title: String::new(),
            bounds,
            capture_from_display: true,
            pixel_size: None,
            pixel_scale: PixelScale::default(),
        })
    }
}

unsafe fn dictionary_number(dictionary: &CFDictionary, key: &CFString) -> Option<f64> {
    let value = unsafe { dictionary.value((key as *const CFString).cast()) };
    let number = unsafe { value.cast::<CFNumber>().as_ref() }?;
    let mut result = 0.0_f64;
    unsafe { number.value(CFNumberType::Float64Type, (&mut result as *mut f64).cast()) }
        .then_some(result)
}

fn window_target(window: &SCWindow) -> Option<WindowTarget> {
    unsafe {
        if !window.isOnScreen() || window.windowLayer() != 0 {
            return None;
        }
        let application = window.owningApplication()?;
        let process_id = application.processID();
        let bounds = capture_bounds(window.frame());
        if !valid_bounds(bounds) {
            return None;
        }
        let filter =
            SCContentFilter::initWithDesktopIndependentWindow(SCContentFilter::alloc(), window);
        Some(WindowTarget {
            id: CaptureWindowId(u64::from(window.windowID())),
            process_id,
            app_name: application.applicationName().to_string(),
            title: window
                .title()
                .map(|title| title.to_string())
                .unwrap_or_default(),
            bounds,
            capture_from_display: false,
            pixel_size: Some(capture_dimensions(&filter).ok()?),
            pixel_scale: PixelScale::new(
                f64::from(filter.pointPixelScale()),
                f64::from(filter.pointPixelScale()),
            ),
        })
    }
}

fn capture_bounds(rect: CGRect) -> CaptureBounds {
    CaptureBounds {
        x: rect.origin.x,
        y: rect.origin.y,
        width: rect.size.width,
        height: rect.size.height,
    }
}

fn valid_bounds(bounds: CaptureBounds) -> bool {
    bounds.x.is_finite()
        && bounds.y.is_finite()
        && bounds.width.is_finite()
        && bounds.height.is_finite()
        && bounds.width > 0.0
        && bounds.height > 0.0
}

fn rgba_image(image: &CGImage) -> Result<RgbaImage, String> {
    if CGImage::bits_per_component(Some(image)) != 8
        || CGImage::bits_per_pixel(Some(image)) != 32
        || CGImage::byte_order_info(Some(image)) != CGImageByteOrderInfo::Order32Little
        || CGImage::alpha_info(Some(image)) != CGImageAlphaInfo::PremultipliedFirst
    {
        return Err("The captured pixel format is not premultiplied BGRA8.".into());
    }
    let width = CGImage::width(Some(image));
    let height = CGImage::height(Some(image));
    let width = u32::try_from(width).map_err(|_| "The capture dimensions are too large.")?;
    let height = u32::try_from(height).map_err(|_| "The capture dimensions are too large.")?;
    let source_stride = CGImage::bytes_per_row(Some(image));
    let provider =
        CGImage::data_provider(Some(image)).ok_or("The captured image has no pixel data.")?;
    let data =
        CGDataProvider::data(Some(&provider)).ok_or("The captured pixels could not be read.")?;
    // CopyData returns immutable data; keep it alive throughout this borrow.
    let bytes = unsafe { data.as_bytes_unchecked() };
    captured_pixels(bytes, source_stride, width, height)
}

pub fn display_layout_matches(displays: &[DisplaySnapshot]) -> bool {
    let mut count = 0;
    if unsafe { CGGetActiveDisplayList(0, std::ptr::null_mut(), &mut count) } != CGError::Success {
        return false;
    }
    let mut ids = vec![0; count as usize];
    if unsafe { CGGetActiveDisplayList(count, ids.as_mut_ptr(), &mut count) } != CGError::Success {
        return false;
    }
    ids.truncate(count as usize);
    if ids.len() != displays.len() {
        return false;
    }
    displays.iter().all(|display| {
        if !ids.iter().any(|id| u64::from(*id) == display.id.0)
            || capture_bounds(CGDisplayBounds(display.id.0 as u32)) != display.bounds
        {
            return false;
        }
        display_mode_pixel_size(display.id.0 as u32) == Some(display.mode_pixel_size)
    })
}

fn display_mode_pixel_size(id: u32) -> Option<(usize, usize)> {
    let mode = CGDisplayCopyDisplayMode(id)?;
    Some((
        CGDisplayMode::pixel_width(Some(&mode)),
        CGDisplayMode::pixel_height(Some(&mode)),
    ))
}

pub struct OverlayWindow {
    native: Retained<NSWindow>,
    view: Retained<NSView>,
    bounds: CaptureBounds,
}

pub fn prepare_overlay<T: 'static>(
    window: &mut Window,
    bounds: CaptureBounds,
    cx: &mut Context<T>,
) -> Result<OverlayWindow, String> {
    let view = native_view(window)?;
    let native = view.window().ok_or("The capture window has closed.")?;
    let cleanup_view = view.clone();
    cx.on_release(move |_, cx| {
        // GPUI 0.3.5's AccessKit adapter retains the content view after window close.
        // Detach its GPUI subview after teardown to release the state and renderer.
        cx.spawn(async move |_| cleanup_view.removeFromSuperview())
            .detach();
    })
    .detach();
    Ok(OverlayWindow {
        native,
        view,
        bounds,
    })
}

impl OverlayWindow {
    // AppKit can synchronously reenter GPUI, so these calls must run outside an App update.
    pub fn configure(&self) {
        let native = &self.native;
        let bounds = self.bounds;
        native.orderOut(None);
        native.setStyleMask(NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel);
        native.setHasShadow(false);
        native.setLevel(isize::try_from(kCGScreenSaverWindowLevel + 1).unwrap());
        native.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::FullScreenAuxiliary
                | NSWindowCollectionBehavior::Stationary
                | NSWindowCollectionBehavior::IgnoresCycle,
        );
        native.setAnimationBehavior(NSWindowAnimationBehavior::None);
        native.setHidesOnDeactivate(false);
        native.setIgnoresMouseEvents(false);
        native.setAcceptsMouseMovedEvents(true);
        native.setSharingType(NSWindowSharingType::None);
        let primary = CGDisplayBounds(CGMainDisplayID());
        native.setFrame_display(
            CGRect::new(
                CGPoint::new(bounds.x, primary.size.height - bounds.y - bounds.height),
                CGSize::new(bounds.width, bounds.height),
            ),
            false,
        );
    }

    pub fn show(&self) {
        self.native.orderFrontRegardless();
        self.native.makeKeyWindow();
        // Changing the style mask can clear AppKit's first responder.
        self.native.makeFirstResponder(Some(&self.view));
    }
}

pub fn display_work_area(id: CaptureDisplayId) -> Option<CaptureBounds> {
    let mtm = MainThreadMarker::new()?;
    let primary = CGDisplayBounds(CGMainDisplayID());
    let display = CGDisplayBounds(id.0 as u32);
    let frame = CGRect::new(
        CGPoint::new(
            display.origin.x,
            primary.size.height - display.origin.y - display.size.height,
        ),
        display.size,
    );
    let screen = NSScreen::screens(mtm)
        .into_iter()
        .find(|screen| screen.frame() == frame)?;
    let visible = screen.visibleFrame();
    Some(CaptureBounds {
        x: visible.origin.x,
        y: primary.size.height - visible.origin.y - visible.size.height,
        width: visible.size.width,
        height: visible.size.height,
    })
}

pub fn position_editor(
    window: &Window,
    placement: CapturePlacement,
    work_area: CaptureBounds,
    image_size: (u32, u32),
) -> Result<Point<f32>, String> {
    let view = native_view(window)?;
    let native = native_window(window)?;
    let primary = CGDisplayBounds(CGMainDisplayID());
    let bounds = |rect: CGRect| CaptureBounds {
        x: rect.origin.x,
        y: primary.size.height - rect.origin.y - rect.size.height,
        width: rect.size.width,
        height: rect.size.height,
    };
    let content = native.convertRectToScreen(view.convertRect_toView(view.bounds(), None));
    let insets = FrameInsets::between(bounds(native.frame()), bounds(content));
    let layout = editor_layout(
        placement,
        work_area,
        insets,
        image_size,
        window.scale_factor(),
    );
    let frame = layout.frame;
    native.setFrame_display(
        CGRect::new(
            CGPoint::new(frame.x, primary.size.height - frame.y - frame.height),
            CGSize::new(frame.width, frame.height),
        ),
        false,
    );
    Ok(layout.image_alignment)
}

pub fn set_window_visible(window: &mut Window, visible: bool) -> Result<(), String> {
    let native = native_window(window)?;
    if visible {
        native.orderFrontRegardless();
        native.makeKeyWindow();
    } else {
        native.orderOut(None);
    }
    Ok(())
}

pub fn visible_window_id(window: &Window) -> Result<Option<CaptureWindowId>, String> {
    let native = native_window(window)?;
    Ok((native.isVisible() && !native.isMiniaturized())
        .then(|| CaptureWindowId(native.windowNumber() as u64)))
}

fn native_window(window: &Window) -> Result<Retained<NSWindow>, String> {
    native_view(window)?
        .window()
        .ok_or_else(|| "The capture window has closed.".into())
}

fn native_view(window: &Window) -> Result<Retained<NSView>, String> {
    MainThreadMarker::new().ok_or("Capture windows must be configured on the main thread.")?;
    let handle = HasWindowHandle::window_handle(window).map_err(|error| error.to_string())?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return Err("The current window is not a macOS window.".into());
    };
    // The GPUI window keeps the view alive while we acquire our own reference.
    unsafe { Retained::retain(handle.ns_view.as_ptr().cast::<NSView>()) }
        .ok_or_else(|| "The capture view has closed.".into())
}
