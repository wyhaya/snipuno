use crate::desktop::shortcut::Shortcut;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN,
};

const SPECIAL_KEYS: &[(u32, &str)] = &[
    (0x08, "Backspace"),
    (0x09, "Tab"),
    (0x0d, "Enter"),
    (0x1b, "Escape"),
    (0x20, "Space"),
    (0x21, "PageUp"),
    (0x22, "PageDown"),
    (0x23, "End"),
    (0x24, "Home"),
    (0x25, "ArrowLeft"),
    (0x26, "ArrowUp"),
    (0x27, "ArrowRight"),
    (0x28, "ArrowDown"),
    (0x2e, "Delete"),
    (0xba, "Semicolon"),
    (0xbb, "Equal"),
    (0xbc, "Comma"),
    (0xbd, "Minus"),
    (0xbe, "Period"),
    (0xbf, "Slash"),
    (0xc0, "Backquote"),
    (0xdb, "BracketLeft"),
    (0xdc, "Backslash"),
    (0xdd, "BracketRight"),
    (0xde, "Quote"),
];

fn key_name(key: u32) -> Option<String> {
    match key {
        0x30..=0x39 => Some(format!("Digit{}", char::from_u32(key)?)),
        0x41..=0x5a => Some(format!("Key{}", char::from_u32(key)?)),
        0x70..=0x83 => Some(format!("F{}", key - 0x6f)),
        _ => SPECIAL_KEYS
            .iter()
            .find(|(code, _)| *code == key)
            .map(|(_, name)| (*name).into()),
    }
}

pub(super) fn binding(shortcut: &Shortcut) -> Result<(HOT_KEY_MODIFIERS, u32), String> {
    let key = (0..=255)
        .find(|code| key_name(*code).as_deref() == Some(shortcut.key))
        .ok_or("This key is not supported on Windows.")?;
    let mut modifiers = HOT_KEY_MODIFIERS(0);
    for (enabled, flag) in [
        (shortcut.modifiers.control, MOD_CONTROL),
        (shortcut.modifiers.alt, MOD_ALT),
        (shortcut.modifiers.shift, MOD_SHIFT),
        (shortcut.modifiers.meta, MOD_WIN),
    ] {
        if enabled {
            modifiers |= flag;
        }
    }
    Ok((modifiers, key))
}

pub(super) fn validate_system_shortcut(shortcut: &Shortcut) -> Result<(), String> {
    let m = shortcut.modifiers;
    if m.meta
        || shortcut.key == "F12"
        || (m.alt && matches!(shortcut.key, "Tab" | "Escape" | "F4"))
        || (m.control && shortcut.key == "Escape")
        || (m.control && m.alt && shortcut.key == "Delete")
    {
        return Err("This shortcut is reserved by Windows. Choose another combination.".into());
    }
    Ok(())
}
