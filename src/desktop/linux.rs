use super::{DesktopEvent, settings::ShortcutBackend, shortcut::Shortcut};
use futures_channel::mpsc::UnboundedSender;
use gpui::{App, Window};

pub struct Hotkeys;

impl Hotkeys {
    pub fn new(_: UnboundedSender<DesktopEvent>) -> Self {
        Self
    }
}

impl ShortcutBackend for Hotkeys {
    async fn register(&mut self, _: &Shortcut) -> Result<(), String> {
        Err("Global shortcuts are not supported on Linux.".into())
    }
}

pub fn hotkey_id(_: &Shortcut) -> Option<u32> {
    None
}

pub struct Platform;

impl Platform {
    pub fn new(_: UnboundedSender<DesktopEvent>) -> (Self, Option<String>) {
        (Self, None)
    }

    pub fn set_dock_visible(&self, _: bool) {}
}

pub fn set_dock_visible(_: bool) {}

pub fn show_window(window: &mut Window) -> Result<(), String> {
    window.activate_window();
    Ok(())
}

pub struct CaptureContext;

impl CaptureContext {
    pub fn new(_: &App) -> Self {
        Self
    }

    pub fn restore_focus(self, _: &mut App) {}
}
