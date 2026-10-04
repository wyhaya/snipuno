use crate::{
    Status,
    windows_data::{command_line, is_approved},
};
use std::{env, os::windows::ffi::OsStrExt};
use windows::{
    Win32::{
        Foundation::{ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA},
        System::Registry::{
            HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_ROUTINE_FLAGS,
            REG_SZ, RRF_RT_REG_BINARY, RRF_RT_REG_SZ, RegCloseKey, RegCreateKeyExW,
            RegDeleteKeyValueW, RegGetValueW, RegSetValueExW,
        },
        UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
    },
    core::{PCWSTR, w},
};

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const APPROVAL_KEY: PCWSTR =
    w!("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\Run");
const VALUE_NAME: PCWSTR = w!("Snipuno");

struct Key(HKEY);

impl Drop for Key {
    fn drop(&mut self) {
        let _ = unsafe { RegCloseKey(self.0) };
    }
}

pub fn status() -> Result<Status, String> {
    let Some(command) = read_value(RUN_KEY, RRF_RT_REG_SZ)? else {
        return Ok(Status::Disabled);
    };
    if command != startup_command()? {
        return Ok(Status::Disabled);
    }
    match read_value(APPROVAL_KEY, RRF_RT_REG_BINARY)? {
        Some(value) if !is_approved(&value)? => Ok(Status::RequiresApproval),
        _ => Ok(Status::Enabled),
    }
}

pub fn set_enabled(enabled: bool) -> Result<(), String> {
    if !enabled {
        let result = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, VALUE_NAME) };
        return if result == ERROR_FILE_NOT_FOUND {
            Ok(())
        } else {
            result.ok().map_err(|error| error.to_string())
        };
    }
    let command = startup_command()?;
    let mut key = HKEY::default();
    unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            None,
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut key,
            None,
        )
    }
    .ok()
    .map_err(|error| error.to_string())?;
    let key = Key(key);
    // TODO: Clean up this installation's Run and StartupApproved values in the NSIS uninstaller.
    unsafe { RegSetValueExW(key.0, VALUE_NAME, None, REG_SZ, Some(&command)) }
        .ok()
        .map_err(|error| error.to_string())
}

fn startup_command() -> Result<Vec<u8>, String> {
    let executable = env::current_exe().map_err(|error| error.to_string())?;
    let command = command_line(&executable.as_os_str().encode_wide().collect::<Vec<_>>())?;
    Ok(command.into_iter().flat_map(u16::to_le_bytes).collect())
}

fn read_value(key: PCWSTR, flags: REG_ROUTINE_FLAGS) -> Result<Option<Vec<u8>>, String> {
    let mut data = Vec::<u8>::new();
    loop {
        let mut size = data.len() as u32;
        let result = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key,
                VALUE_NAME,
                flags,
                None,
                (!data.is_empty()).then(|| data.as_mut_ptr().cast()),
                Some(&mut size),
            )
        };
        if result == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        if result == ERROR_MORE_DATA || (result.is_ok() && size as usize > data.len()) {
            data.resize(size as usize, 0);
            continue;
        }
        result.ok().map_err(|error| error.to_string())?;
        data.truncate(size as usize);
        return Ok(Some(data));
    }
}

pub fn open_settings() -> Result<(), String> {
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            w!("ms-settings:startupapps"),
            None,
            None,
            SW_SHOWNORMAL,
        )
    };
    if result.0 as isize <= 32 {
        return Err("Unable to open Windows Startup settings.".into());
    }
    Ok(())
}
