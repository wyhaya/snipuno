use super::{DesktopEvent, MenuState, shortcut::Shortcut};
use futures_channel::mpsc::UnboundedSender;
use gpui::App;
use tray_icon::{
    Icon, TrayIcon, TrayIconBuilder,
    menu::{
        Menu, MenuEvent, MenuItem, PredefinedMenuItem,
        accelerator::{Accelerator, Code, Modifiers},
    },
};
#[cfg(target_os = "windows")]
use tray_icon::{MouseButton, MouseButtonState, TrayIconEvent};
#[cfg(target_os = "windows")]
use windows::{
    Win32::{
        Foundation::ERROR_SUCCESS,
        System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW},
    },
    core::w,
};

const CAPTURE: &str = "snipuno.capture";
const CAPTURE_FULLSCREEN: &str = "snipuno.capture_fullscreen";
const EDIT_LAST: &str = "snipuno.edit_last_screenshot";
const PICK_COLOR: &str = "snipuno.pick_color";
const SETTINGS: &str = "snipuno.settings";
const QUIT: &str = "snipuno.quit";
const TRAY: &str = "com.snipuno.desktop";

pub struct Tray {
    icon: TrayIcon,
    capture: MenuItem,
    capture_fullscreen: MenuItem,
    edit_last: MenuItem,
    pick_color: MenuItem,
    shortcut: Option<Shortcut>,
    fullscreen_shortcut: Option<Shortcut>,
    color_shortcut: Option<Shortcut>,
    warning: bool,
    #[cfg(target_os = "windows")]
    foreground: Option<(u8, u8)>,
}

impl Tray {
    pub fn new(sender: UnboundedSender<DesktopEvent>, cx: &App) -> Result<Self, String> {
        let menu = Menu::new();
        let capture = MenuItem::with_id(CAPTURE, "Area Screenshot", false, None);
        let capture_fullscreen =
            MenuItem::with_id(CAPTURE_FULLSCREEN, "Full Screen Screenshot", false, None);
        let edit_last = MenuItem::with_id(EDIT_LAST, "Edit Last Screenshot", false, None);
        let pick_color = MenuItem::with_id(PICK_COLOR, "Pick Color", false, None);
        let settings = MenuItem::with_id(SETTINGS, "Settings…", true, None);
        let quit = MenuItem::with_id(QUIT, "Quit", true, None);
        menu.append_items(&[
            &capture_fullscreen,
            &capture,
            &edit_last,
            &PredefinedMenuItem::separator(),
            &pick_color,
            &PredefinedMenuItem::separator(),
            &settings,
            &PredefinedMenuItem::separator(),
            &quit,
        ])
        .map_err(|error| error.to_string())?;
        #[cfg(target_os = "windows")]
        let foreground = system_foreground();
        #[cfg(not(target_os = "windows"))]
        let foreground = None;
        let icon = TrayIconBuilder::new()
            .with_id(TRAY)
            .with_guid(0x90ae7a0a_6d98_40e0_b423_d86733496046)
            .with_icon(load_icon(cx, foreground)?)
            .with_icon_as_template(cfg!(target_os = "macos"))
            .with_tooltip("Snipuno")
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(!cfg!(target_os = "windows"))
            .build()
            .map_err(|error| format!("Unable to create the system tray icon: {error}"))?;

        #[cfg(target_os = "windows")]
        {
            let sender = sender.clone();
            TrayIconEvent::set_event_handler(Some(move |event| {
                if let TrayIconEvent::Click {
                    id,
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = event
                    && id.as_ref() == TRAY
                {
                    let _ = sender.unbounded_send(DesktopEvent::Reopen);
                }
            }));
        }
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let event = match event.id.as_ref() {
                CAPTURE => DesktopEvent::Capture,
                CAPTURE_FULLSCREEN => DesktopEvent::CaptureFullscreen,
                EDIT_LAST => DesktopEvent::EditLastScreenshot,
                PICK_COLOR => DesktopEvent::PickColor,
                SETTINGS => DesktopEvent::Settings,
                QUIT => DesktopEvent::Quit,
                _ => return,
            };
            let _ = sender.unbounded_send(event);
        }));

        Ok(Self {
            icon,
            capture,
            capture_fullscreen,
            edit_last,
            pick_color,
            shortcut: None,
            fullscreen_shortcut: None,
            color_shortcut: None,
            warning: false,
            #[cfg(target_os = "windows")]
            foreground,
        })
    }

    #[cfg(target_os = "windows")]
    pub fn sync_appearance(&mut self, cx: &App) -> Result<(), String> {
        let Some(foreground) = system_foreground() else {
            return Ok(());
        };
        if self.foreground != Some(foreground) {
            self.icon
                .set_icon(Some(load_icon(cx, Some(foreground))?))
                .map_err(|error| error.to_string())?;
            self.foreground = Some(foreground);
        }
        Ok(())
    }

    pub fn update(&mut self, state: MenuState) -> Result<(), String> {
        if self.shortcut.as_ref() != state.shortcut {
            let accelerator = state.shortcut.map(accelerator).transpose()?;
            self.capture
                .set_accelerator(accelerator)
                .map_err(|error| error.to_string())?;
            self.shortcut = state.shortcut.cloned();
        }
        if self.color_shortcut.as_ref() != state.color_shortcut {
            let accelerator = state.color_shortcut.map(accelerator).transpose()?;
            self.pick_color
                .set_accelerator(accelerator)
                .map_err(|error| error.to_string())?;
            self.color_shortcut = state.color_shortcut.cloned();
        }
        if self.fullscreen_shortcut.as_ref() != state.fullscreen_shortcut {
            let accelerator = state.fullscreen_shortcut.map(accelerator).transpose()?;
            self.capture_fullscreen
                .set_accelerator(accelerator)
                .map_err(|error| error.to_string())?;
            self.fullscreen_shortcut = state.fullscreen_shortcut.cloned();
        }
        if self.capture.is_enabled() != state.can_capture {
            self.capture.set_enabled(state.can_capture);
        }
        if self.capture_fullscreen.is_enabled() != state.can_capture {
            self.capture_fullscreen.set_enabled(state.can_capture);
        }
        if self.edit_last.is_enabled() != state.can_reopen {
            self.edit_last.set_enabled(state.can_reopen);
        }
        if self.pick_color.is_enabled() != state.can_capture {
            self.pick_color.set_enabled(state.can_capture);
        }
        if self.warning != state.warning {
            self.icon
                .set_tooltip(Some(if state.warning {
                    "Snipuno — Open Settings to resolve a problem"
                } else {
                    "Snipuno"
                }))
                .map_err(|error| error.to_string())?;
            self.warning = state.warning;
        }
        Ok(())
    }
}

fn accelerator(shortcut: &Shortcut) -> Result<Accelerator, String> {
    let key = shortcut
        .key
        .parse::<Code>()
        .map_err(|error| error.to_string())?;
    let mut modifiers = Modifiers::empty();
    for (enabled, flag) in [
        (shortcut.modifiers.control, Modifiers::CONTROL),
        (shortcut.modifiers.alt, Modifiers::ALT),
        (shortcut.modifiers.shift, Modifiers::SHIFT),
        (shortcut.modifiers.meta, Modifiers::META),
    ] {
        if enabled {
            modifiers |= flag;
        }
    }
    Ok(Accelerator::new(modifiers, key))
}

fn load_icon(cx: &App, foreground: Option<(u8, u8)>) -> Result<Icon, String> {
    #[cfg(target_os = "windows")]
    let svg: &[u8] = include_bytes!("../../assets/tray-windows.svg");
    #[cfg(not(target_os = "windows"))]
    let svg: &[u8] = include_bytes!("../../assets/tray.svg");
    let image = cx
        .svg_renderer()
        .render_single_frame(svg, 1.)
        .map_err(|error| error.to_string())?;
    let size = image.size(0);
    let mut rgba = image
        .as_bytes(0)
        .ok_or("Tray icon has no image data.")?
        .to_vec();
    for pixel in rgba.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
        if let Some((value, opacity)) = foreground {
            pixel[..3].fill(value);
            pixel[3] = ((u16::from(pixel[3]) * u16::from(opacity) + 127) / 255) as u8;
        }
    }
    Icon::from_rgba(rgba, size.width.into(), size.height.into()).map_err(|error| error.to_string())
}

#[cfg(target_os = "windows")]
fn system_foreground() -> Option<(u8, u8)> {
    let mut light = 0u32;
    let mut size = std::mem::size_of_val(&light) as u32;
    let result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("SystemUsesLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some((&raw mut light).cast()),
            Some(&mut size),
        )
    };
    (result == ERROR_SUCCESS && size == std::mem::size_of_val(&light) as u32)
        .then_some(if light == 0 { (0xff, 0xff) } else { (0, 0xe4) })
}
