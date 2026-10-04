#[path = "windows/keyboard.rs"]
mod keyboard;
#[path = "windows/window.rs"]
mod window;

pub use window::{
    CaptureContext, configure_pinned_window, pinned_window_contains_pointer, resize_pinned_window,
    show_window, start_window_move,
};

use super::{DesktopEvent, settings::ShortcutBackend, shortcut::Shortcut};
use futures_channel::mpsc::UnboundedSender;
use std::{rc::Rc, thread, time::Duration};
use windows::{
    Win32::{
        Foundation::{
            CloseHandle, ERROR_ALREADY_EXISTS, ERROR_CLASS_ALREADY_EXISTS, GetLastError, HANDLE,
            HWND, LPARAM, LRESULT, WPARAM,
        },
        System::{LibraryLoader::GetModuleHandleW, Threading::CreateMutexW},
        UI::{
            Input::KeyboardAndMouse::{MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey},
            WindowsAndMessaging::*,
        },
    },
    core::{Error, PCWSTR, w},
};

const CLASS: PCWSTR = w!("Snipuno.Desktop");
const REOPEN_MESSAGE: u32 = WM_APP + 2;

pub struct Instance(HANDLE);

impl Drop for Instance {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}

pub fn initialize(autostart: bool) -> Result<Option<Instance>, String> {
    let instance = unsafe { CreateMutexW(None, false, w!("Local\\Snipuno.Desktop")) }
        .map_err(|error| error.to_string())?;
    let exists = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
    let instance = Instance(instance);
    if !exists {
        return Ok(Some(instance));
    }
    if autostart {
        return Ok(None);
    }
    for _ in 0..100 {
        if let Ok(hwnd) = unsafe { FindWindowW(CLASS, None) } {
            let mut pid = 0;
            unsafe {
                GetWindowThreadProcessId(hwnd, Some(&mut pid));
                let _ = AllowSetForegroundWindow(pid);
                PostMessageW(Some(hwnd), REOPEN_MESSAGE, WPARAM(0), LPARAM(0))
                    .map_err(|error| error.to_string())?;
            }
            return Ok(None);
        }
        thread::sleep(Duration::from_millis(50));
    }
    Err("Snipuno is already starting. Try opening it again in a moment.".into())
}

struct EventState {
    sender: UnboundedSender<DesktopEvent>,
}

impl EventState {
    fn send(&self, event: DesktopEvent) {
        let _ = self.sender.unbounded_send(event);
    }
}

unsafe extern "system" fn event_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        if message == WM_NCCREATE {
            let create = &*(lparam.0 as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
        }
        let data = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const EventState;
        let state = if data.is_null() {
            None
        } else {
            Rc::increment_strong_count(data);
            Some(Rc::from_raw(data))
        };
        if let Some(state) = state {
            match message {
                WM_HOTKEY => {
                    // MOD_NOREPEAT supplies one message per press, without a key-up message.
                    state.send(DesktopEvent::Hotkey {
                        id: wparam.0 as u32,
                        pressed: true,
                    });
                    state.send(DesktopEvent::Hotkey {
                        id: wparam.0 as u32,
                        pressed: false,
                    });
                    return LRESULT(0);
                }
                REOPEN_MESSAGE => {
                    state.send(DesktopEvent::Reopen);
                    return LRESULT(0);
                }
                WM_SETTINGCHANGE | WM_THEMECHANGED => {
                    state.send(DesktopEvent::SystemAppearanceChanged);
                }
                WM_NCDESTROY => {
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                }
                _ => {}
            }
        }
        DefWindowProcW(hwnd, message, wparam, lparam)
    }
}

struct EventWindow {
    hwnd: HWND,
    _state: Rc<EventState>,
}

impl EventWindow {
    fn new(sender: UnboundedSender<DesktopEvent>, message_only: bool) -> Result<Self, String> {
        unsafe {
            let instance = GetModuleHandleW(None).map_err(|error| error.to_string())?;
            let class = WNDCLASSW {
                lpfnWndProc: Some(event_proc),
                hInstance: instance.into(),
                lpszClassName: CLASS,
                ..Default::default()
            };
            if RegisterClassW(&class) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
                return Err(Error::from_thread().to_string());
            }
            let state = Rc::new(EventState { sender });
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW,
                CLASS,
                w!("Snipuno"),
                WS_POPUP,
                0,
                0,
                0,
                0,
                if message_only {
                    Some(HWND_MESSAGE)
                } else {
                    None
                },
                None,
                Some(instance.into()),
                Some((&*state as *const EventState).cast()),
            );
            hwnd.map(|hwnd| Self {
                hwnd,
                _state: state,
            })
            .map_err(|error| error.to_string())
        }
    }
}

impl Drop for EventWindow {
    fn drop(&mut self) {
        let _ = unsafe { DestroyWindow(self.hwnd) };
    }
}

pub struct Hotkeys {
    window: Result<EventWindow, String>,
    registered: Vec<i32>,
}

impl Hotkeys {
    pub fn new(sender: UnboundedSender<DesktopEvent>) -> Self {
        Self {
            window: EventWindow::new(sender, true),
            registered: Vec::new(),
        }
    }
}

pub fn hotkey_id(shortcut: &Shortcut) -> Option<u32> {
    keyboard::binding(shortcut)
        .ok()
        .map(|(modifiers, key)| (modifiers.0 << 8) | key)
}

impl ShortcutBackend for Hotkeys {
    async fn register(&mut self, shortcut: &Shortcut) -> Result<(), String> {
        shortcut.validate()?;
        keyboard::validate_system_shortcut(shortcut)?;
        let (modifiers, key) = keyboard::binding(shortcut)?;
        let id = hotkey_id(shortcut).ok_or("This key is not supported.")? as i32;
        let window = self.window.as_ref().map_err(Clone::clone)?;
        unsafe { RegisterHotKey(Some(window.hwnd), id, modifiers | MOD_NOREPEAT, key) }
            .map_err(|error| format!("Unable to register this shortcut. It may be reserved by Windows or another application. {error}"))?;
        self.registered.push(id);
        Ok(())
    }
}

impl Drop for Hotkeys {
    fn drop(&mut self) {
        if let Ok(window) = &self.window {
            for id in &self.registered {
                let _ = unsafe { UnregisterHotKey(Some(window.hwnd), *id) };
            }
        }
    }
}

pub struct Platform {
    _window: Option<EventWindow>,
}

impl Platform {
    pub fn new(sender: UnboundedSender<DesktopEvent>) -> (Self, Option<String>) {
        let (window, error) = match EventWindow::new(sender, false) {
            Ok(window) => (Some(window), None),
            Err(error) => (None, Some(error)),
        };
        (Self { _window: window }, error)
    }

    pub fn set_dock_visible(&self, _: bool) {}
}

pub fn set_dock_visible(_: bool) {}
