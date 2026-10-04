use crate::Status;

const UNSUPPORTED: &str = "Launch at login is not supported on Linux yet.";

pub fn status() -> Result<Status, String> {
    Err(UNSUPPORTED.into())
}

pub fn set_enabled(_enabled: bool) -> Result<(), String> {
    Err(UNSUPPORTED.into())
}

pub fn open_settings() -> Result<(), String> {
    Err(UNSUPPORTED.into())
}
