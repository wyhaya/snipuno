#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod platform;
#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod platform;
#[cfg(target_os = "windows")]
#[path = "windows.rs"]
mod platform;
#[cfg(any(target_os = "windows", test))]
mod windows_data;

#[cfg(target_os = "macos")]
pub use platform::was_launched_at_login;
pub use platform::{open_settings, set_enabled, status};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Disabled,
    Enabled,
    RequiresApproval,
}
