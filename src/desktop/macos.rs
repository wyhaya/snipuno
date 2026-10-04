use super::{
    DesktopEvent, pinned_geometry::fitted_size, settings::ShortcutBackend, shortcut::Shortcut,
};
use futures_channel::mpsc::UnboundedSender;
use global_hotkey::{
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
    hotkey::{Code, HotKey, Modifiers as HotKeyModifiers},
};
use gpui::{AnyWindowHandle, App, Pixels, Window, point, px};
use objc2::{AnyThread, MainThreadMarker, rc::Retained};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationOptions, NSApplicationActivationPolicy, NSEvent,
    NSRunningApplication, NSTrackingArea, NSTrackingAreaOptions, NSView, NSWindow, NSWindowButton,
    NSWindowCollectionBehavior, NSWorkspace,
};
use objc2_core_foundation::{
    CFArray, CFBoolean, CFDictionary, CFNumber, CFRetained, CFString, CFType,
};
use objc2_foundation::{NSRect, NSSize};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::ptr::{self, NonNull};

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn CopySymbolicHotKeys(keys: *mut *mut CFArray<CFDictionary<CFString, CFType>>) -> i32;
}

pub struct Hotkeys {
    manager: Result<GlobalHotKeyManager, String>,
}

pub fn show_about() {
    let mtm = MainThreadMarker::new().expect("app menus run on the main thread");
    NSApplication::sharedApplication(mtm).orderFrontStandardAboutPanel(None);
}

pub fn bring_all_to_front() {
    let mtm = MainThreadMarker::new().expect("app menus run on the main thread");
    NSApplication::sharedApplication(mtm).arrangeInFront(None);
}

impl Hotkeys {
    pub fn new(sender: UnboundedSender<DesktopEvent>) -> Self {
        GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
            let _ = sender.unbounded_send(DesktopEvent::Hotkey {
                id: event.id,
                pressed: event.state == HotKeyState::Pressed,
            });
        }));
        Self {
            manager: GlobalHotKeyManager::new()
                .map_err(|error| format!("Global shortcuts are unavailable: {error}")),
        }
    }
}

pub fn hotkey_id(shortcut: &Shortcut) -> Option<u32> {
    hotkey(shortcut).ok().map(|hotkey| hotkey.id())
}

fn hotkey(shortcut: &Shortcut) -> Result<HotKey, String> {
    let key: Code = shortcut
        .key
        .parse()
        .map_err(|_| "This key is not supported.".to_string())?;
    let mut mods = HotKeyModifiers::empty();
    if shortcut.modifiers.meta {
        mods |= HotKeyModifiers::SUPER;
    }
    if shortcut.modifiers.control {
        mods |= HotKeyModifiers::CONTROL;
    }
    if shortcut.modifiers.alt {
        mods |= HotKeyModifiers::ALT;
    }
    if shortcut.modifiers.shift {
        mods |= HotKeyModifiers::SHIFT;
    }
    Ok(HotKey::new(Some(mods), key))
}

impl ShortcutBackend for Hotkeys {
    async fn register(&mut self, shortcut: &Shortcut) -> Result<(), String> {
        shortcut.validate()?;
        if reserved_by_system(shortcut)? {
            return Err("This shortcut is enabled in macOS Keyboard Shortcuts. Change it in System Settings to use this shortcut in Snipuno.".into());
        }
        self.manager.as_ref().map_err(Clone::clone)?.register(hotkey(shortcut)?)
            .map_err(|error| format!("Unable to register this shortcut. It may be reserved by macOS or another application. {error}"))
    }
}

fn reserved_by_system(shortcut: &Shortcut) -> Result<bool, String> {
    let code = KEY_CODES
        .iter()
        .find(|(_, key)| *key == shortcut.key)
        .map(|(code, _)| i64::from(*code))
        .ok_or("This key is not supported on macOS.")?;
    let m = shortcut.modifiers;
    let modifiers = i64::from(m.meta) * 256
        + i64::from(m.shift) * 512
        + i64::from(m.alt) * 2048
        + i64::from(m.control) * 4096;
    let mut keys = ptr::null_mut();
    let status = unsafe { CopySymbolicHotKeys(&mut keys) };
    if status != 0 {
        return Err(format!(
            "Unable to check macOS keyboard shortcuts ({status}). Try again."
        ));
    }
    let keys = NonNull::new(keys).ok_or("macOS returned no keyboard shortcut information.")?;
    // CopySymbolicHotKeys returns an owned array of dictionaries with CFString keys.
    let keys = unsafe { CFRetained::from_raw(keys) };
    let number = |dictionary: &CFDictionary<CFString, CFType>, key: &str| {
        dictionary
            .get(&CFString::from_str(key))?
            .downcast::<CFNumber>()
            .ok()?
            .as_i64()
    };
    Ok(keys.iter().any(|dictionary| {
        let enabled = dictionary
            .get(&CFString::from_str("kHISymbolicHotKeyEnabled"))
            .and_then(|value| value.downcast::<CFBoolean>().ok())
            .is_some_and(|value| value.value());
        enabled
            && number(&dictionary, "kHISymbolicHotKeyCode") == Some(code)
            && number(&dictionary, "kHISymbolicHotKeyModifiers").map(|mods| mods & 0x1b00)
                == Some(modifiers)
    }))
}

pub struct Platform;

impl Platform {
    pub fn new(_: UnboundedSender<DesktopEvent>) -> (Self, Option<String>) {
        (Self, None)
    }

    pub fn set_dock_visible(&self, visible: bool) {
        set_dock_visible(visible);
    }
}

pub fn set_dock_visible(visible: bool) {
    let mtm = MainThreadMarker::new().expect("window policy runs on the main thread");
    let application = NSApplication::sharedApplication(mtm);
    let policy = if visible {
        NSApplicationActivationPolicy::Regular
    } else {
        NSApplicationActivationPolicy::Accessory
    };
    if application.activationPolicy() != policy {
        application.setActivationPolicy(policy);
    }
}

fn native_view(window: &Window) -> Result<Retained<NSView>, String> {
    let handle = HasWindowHandle::window_handle(window).map_err(|error| error.to_string())?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return Err("Not a macOS window.".into());
    };
    unsafe { Retained::retain(handle.ns_view.as_ptr().cast::<NSView>()) }
        .ok_or_else(|| "The view has closed.".into())
}

fn native_window(window: &Window) -> Result<Retained<NSWindow>, String> {
    native_view(window)?
        .window()
        .ok_or_else(|| "The window has closed.".into())
}

pub fn show_window(window: &mut Window) -> Result<(), String> {
    let native = native_window(window)?;
    if native.isMiniaturized() {
        native.deminiaturize(None);
    }
    native.makeKeyAndOrderFront(None);
    Ok(())
}

pub fn center_window_controls(window: &Window, toolbar_height: f32) -> Result<(), String> {
    let native = native_window(window)?;
    let close = native
        .standardWindowButton(NSWindowButton::CloseButton)
        .ok_or("The window controls are unavailable.")?;
    let frame = close.frame();
    window.set_traffic_light_position(point(
        px(16.0),
        px((toolbar_height - frame.size.height as f32) / 2.0),
    ));
    Ok(())
}

pub fn titlebar_content_start(window: &Window) -> Pixels {
    let frames = (|| {
        let view = native_view(window).ok()?;
        let native = view.window()?;
        let close = native.standardWindowButton(NSWindowButton::CloseButton)?;
        let zoom = native.standardWindowButton(NSWindowButton::ZoomButton)?;
        Some((
            close.convertRect_toView(close.bounds(), Some(&view)),
            zoom.convertRect_toView(zoom.bounds(), Some(&view)),
        ))
    })();
    frames.map_or(px(12.0), |(close, zoom)| {
        px((zoom.origin.x + zoom.size.width + close.origin.x) as f32)
    })
}

pub fn configure_pinned_window(window: &Window, logical_size: (f64, f64)) -> Result<(), String> {
    let view = native_view(window)?;
    let native = native_window(window)?;
    let screen = native
        .screen()
        .ok_or("The display is no longer available.")?;
    let work_area = screen.visibleFrame();
    let minimum = pinned_size(logical_size, work_area.size, 0.0);
    let initial = pinned_size(
        logical_size,
        NSSize::new(work_area.size.width * 0.8, work_area.size.height * 0.8),
        1.0,
    );
    native.setContentAspectRatio(NSSize::new(logical_size.0, logical_size.1));
    native.setContentMinSize(minimum);
    native.setContentSize(initial);
    native.setHidesOnDeactivate(false);
    native.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary,
    );
    native.setMovableByWindowBackground(true);
    // Floating GPUI windows need an AppKit tracking area to receive mouse exits while inactive.
    let tracking = unsafe {
        NSTrackingArea::initWithRect_options_owner_userInfo(
            NSTrackingArea::alloc(),
            NSRect::ZERO,
            NSTrackingAreaOptions::MouseEnteredAndExited
                | NSTrackingAreaOptions::MouseMoved
                | NSTrackingAreaOptions::ActiveAlways
                | NSTrackingAreaOptions::InVisibleRect,
            Some(&view),
            None,
        )
    };
    view.addTrackingArea(&tracking);
    set_pinned_controls_visible(window, false);
    native.setFrame_display(clamp_pinned_frame(native.frame(), work_area), false);
    native.orderFront(None);
    Ok(())
}

fn pinned_size(logical_size: (f64, f64), available: NSSize, zoom: f64) -> NSSize {
    let (width, height) = fitted_size(
        logical_size,
        (available.width, available.height),
        zoom,
        208.0,
    );
    NSSize::new(width, height)
}

fn clamp_pinned_frame(mut frame: NSRect, work_area: NSRect) -> NSRect {
    frame.origin.x = frame.origin.x.clamp(
        work_area.origin.x,
        (work_area.origin.x + work_area.size.width - frame.size.width).max(work_area.origin.x),
    );
    frame.origin.y = frame.origin.y.clamp(
        work_area.origin.y,
        (work_area.origin.y + work_area.size.height - frame.size.height).max(work_area.origin.y),
    );
    frame
}

pub fn pinned_window_contains_pointer(window: &Window) -> bool {
    let Ok(native) = native_window(window) else {
        return false;
    };
    if !native.isVisible() || native.isMiniaturized() {
        return false;
    }
    let pointer = NSEvent::mouseLocation();
    let frame = native.frame();
    pointer.x >= frame.origin.x
        && pointer.y >= frame.origin.y
        && pointer.x < frame.origin.x + frame.size.width
        && pointer.y < frame.origin.y + frame.size.height
}

pub fn set_pinned_controls_visible(window: &Window, visible: bool) {
    if let Ok(native) = native_window(window) {
        for kind in [
            NSWindowButton::CloseButton,
            NSWindowButton::MiniaturizeButton,
            NSWindowButton::ZoomButton,
        ] {
            if let Some(button) = native.standardWindowButton(kind)
                && button.isHidden() == visible
            {
                button.setHidden(!visible);
            }
        }
    }
}

pub fn resize_pinned_window(
    window: &Window,
    logical_size: (f64, f64),
    factor: f64,
    cx: &App,
) -> Result<(), String> {
    let view = native_view(window)?;
    // AppKit resize callbacks must run after GPUI releases the current window.
    cx.foreground_executor()
        .spawn(async move {
            let Some(native) = view.window().filter(|native| native.isVisible()) else {
                return;
            };
            let Some(screen) = native.screen() else {
                return;
            };
            let work_area = screen.visibleFrame();
            let previous = native.frame();
            let content = view.bounds().size;
            let target = pinned_size(
                logical_size,
                work_area.size,
                content.width / logical_size.0 * factor,
            );
            let mut frame = previous;
            frame.size = NSSize::new(
                target.width + previous.size.width - content.width,
                target.height + previous.size.height - content.height,
            );
            let pointer = NSEvent::mouseLocation();
            frame.origin.x = pointer.x
                - (pointer.x - previous.origin.x) * frame.size.width / previous.size.width;
            frame.origin.y = pointer.y
                - (pointer.y - previous.origin.y) * frame.size.height / previous.size.height;
            native.setFrame_display(clamp_pinned_frame(frame, work_area), false);
        })
        .detach();
    Ok(())
}

pub struct CaptureContext {
    frontmost: Option<Retained<NSRunningApplication>>,
    key_window: Option<AnyWindowHandle>,
}

impl CaptureContext {
    pub fn new(cx: &App) -> Self {
        Self {
            frontmost: NSWorkspace::sharedWorkspace().frontmostApplication(),
            key_window: cx.active_window(),
        }
    }

    pub fn restore_focus(self, cx: &mut App) {
        let restored_key_window = self.key_window.is_some_and(|handle| {
            handle
                .update(cx, |_, window, _| {
                    if let Ok(native) = native_window(window)
                        && native.isVisible()
                        && !native.isMiniaturized()
                    {
                        native.makeKeyWindow();
                        true
                    } else {
                        false
                    }
                })
                .unwrap_or(false)
        });
        if let Some(application) = self.frontmost
            && !application.isTerminated()
            && (application.processIdentifier() as u32 != std::process::id() || restored_key_window)
        {
            #[allow(deprecated)]
            application
                .activateWithOptions(NSApplicationActivationOptions::ActivateIgnoringOtherApps);
        }
    }
}

const KEY_CODES: &[(u16, &str)] = &[
    (0x00, "KeyA"),
    (0x01, "KeyS"),
    (0x02, "KeyD"),
    (0x03, "KeyF"),
    (0x04, "KeyH"),
    (0x05, "KeyG"),
    (0x06, "KeyZ"),
    (0x07, "KeyX"),
    (0x08, "KeyC"),
    (0x09, "KeyV"),
    (0x0b, "KeyB"),
    (0x0c, "KeyQ"),
    (0x0d, "KeyW"),
    (0x0e, "KeyE"),
    (0x0f, "KeyR"),
    (0x10, "KeyY"),
    (0x11, "KeyT"),
    (0x12, "Digit1"),
    (0x13, "Digit2"),
    (0x14, "Digit3"),
    (0x15, "Digit4"),
    (0x16, "Digit6"),
    (0x17, "Digit5"),
    (0x18, "Equal"),
    (0x19, "Digit9"),
    (0x1a, "Digit7"),
    (0x1b, "Minus"),
    (0x1c, "Digit8"),
    (0x1d, "Digit0"),
    (0x1e, "BracketRight"),
    (0x1f, "KeyO"),
    (0x20, "KeyU"),
    (0x21, "BracketLeft"),
    (0x22, "KeyI"),
    (0x23, "KeyP"),
    (0x24, "Enter"),
    (0x25, "KeyL"),
    (0x26, "KeyJ"),
    (0x27, "Quote"),
    (0x28, "KeyK"),
    (0x29, "Semicolon"),
    (0x2a, "Backslash"),
    (0x2b, "Comma"),
    (0x2c, "Slash"),
    (0x2d, "KeyN"),
    (0x2e, "KeyM"),
    (0x2f, "Period"),
    (0x30, "Tab"),
    (0x31, "Space"),
    (0x32, "Backquote"),
    (0x33, "Backspace"),
    (0x35, "Escape"),
    (0x40, "F17"),
    (0x4f, "F18"),
    (0x50, "F19"),
    (0x5a, "F20"),
    (0x60, "F5"),
    (0x61, "F6"),
    (0x62, "F7"),
    (0x63, "F3"),
    (0x64, "F8"),
    (0x65, "F9"),
    (0x67, "F11"),
    (0x69, "F13"),
    (0x6a, "F16"),
    (0x6b, "F14"),
    (0x6d, "F10"),
    (0x6f, "F12"),
    (0x71, "F15"),
    (0x73, "Home"),
    (0x74, "PageUp"),
    (0x75, "Delete"),
    (0x76, "F4"),
    (0x77, "End"),
    (0x78, "F2"),
    (0x79, "PageDown"),
    (0x7a, "F1"),
    (0x7b, "ArrowLeft"),
    (0x7c, "ArrowRight"),
    (0x7d, "ArrowDown"),
    (0x7e, "ArrowUp"),
];
