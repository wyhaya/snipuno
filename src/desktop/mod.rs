mod menu;
mod pinned_geometry;
#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod platform;
#[cfg(target_os = "windows")]
#[path = "windows.rs"]
mod platform;
mod resident;
mod settings;
mod settings_ui;
mod shortcut;
mod sound;
#[cfg(test)]
mod tests;
mod tray;

use gpui::{AnyWindowHandle, App};

pub(crate) use menu::{CloseWindow, window_actions};
pub use platform::CaptureContext;
#[cfg(target_os = "windows")]
pub use platform::initialize;
#[cfg(target_os = "windows")]
pub use platform::start_window_move;
#[cfg(target_os = "macos")]
pub(crate) use platform::{
    center_window_controls, set_pinned_controls_visible, titlebar_content_start,
};
pub(crate) use platform::{
    configure_pinned_window, pinned_window_contains_pointer, resize_pinned_window,
};
pub use resident::{
    activate_window, can_capture, capture_finished, capture_started, install, play_sound_effect,
    prepare_window, reopen, screenshot_ready, show_settings, track_window, window_activated,
};
pub(crate) use shortcut::COLOR_PICKER_SHORTCUT;
pub use sound::SoundEffect;

#[derive(Clone, Copy, PartialEq)]
pub enum WindowRole {
    Editor,
    Pinned,
    Other,
}

pub enum DesktopEvent {
    Capture,
    CaptureFullscreen,
    EditLastScreenshot,
    PickColor,
    Settings,
    Quit,
    Hotkey {
        id: u32,
        pressed: bool,
    },
    #[cfg(target_os = "windows")]
    Reopen,
    #[cfg(target_os = "windows")]
    SystemAppearanceChanged,
}

pub struct MenuState<'a> {
    pub shortcut: Option<&'a shortcut::Shortcut>,
    pub fullscreen_shortcut: Option<&'a shortcut::Shortcut>,
    pub color_shortcut: Option<&'a shortcut::Shortcut>,
    pub can_capture: bool,
    pub can_reopen: bool,
    pub warning: bool,
}

#[cfg(target_os = "macos")]
pub fn start_window_move(window: &gpui::Window, _: &App) {
    window.start_window_move();
}
