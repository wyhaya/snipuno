#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shortcut {
    pub key: &'static str,
    pub modifiers: Modifiers,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub control: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
}

pub const FULLSCREEN_SHORTCUT: Shortcut = Shortcut {
    key: "Digit1",
    modifiers: Modifiers {
        meta: cfg!(target_os = "macos"),
        control: !cfg!(target_os = "macos"),
        shift: true,
        alt: false,
    },
};

pub const SCREENSHOT_SHORTCUT: Shortcut = Shortcut {
    key: "Digit2",
    modifiers: FULLSCREEN_SHORTCUT.modifiers,
};

pub const COLOR_PICKER_SHORTCUT: Shortcut = Shortcut {
    key: if cfg!(target_os = "windows") {
        "Digit3"
    } else {
        "KeyP"
    },
    modifiers: Modifiers {
        alt: cfg!(target_os = "macos"),
        control: !cfg!(target_os = "macos"),
        shift: !cfg!(target_os = "macos"),
        meta: false,
    },
};

impl Shortcut {
    #[cfg(not(target_os = "linux"))]
    pub fn keystroke(&self) -> String {
        let m = self.modifiers;
        let key = self
            .key
            .strip_prefix("Key")
            .or_else(|| self.key.strip_prefix("Digit"))
            .unwrap_or(self.key)
            .to_ascii_lowercase();
        [
            (m.control, "ctrl"),
            (m.alt, "alt"),
            (m.shift, "shift"),
            (m.meta, "cmd"),
        ]
        .into_iter()
        .filter_map(|(pressed, modifier)| pressed.then_some(modifier))
        .chain([key.as_str()])
        .collect::<Vec<_>>()
        .join("-")
    }

    pub fn validate(&self) -> Result<(), String> {
        let m = self.modifiers;
        if !(m.control || m.alt || m.meta) {
            return Err(if cfg!(target_os = "macos") {
                "Include Command, Control or Option in the shortcut."
            } else {
                "Include Ctrl or Alt in the shortcut."
            }
            .into());
        }
        let letter = self
            .key
            .strip_prefix("Key")
            .is_some_and(|key| key.len() == 1 && key.as_bytes()[0].is_ascii_uppercase());
        let digit = self
            .key
            .strip_prefix("Digit")
            .is_some_and(|key| key.len() == 1 && key.as_bytes()[0].is_ascii_digit());
        let function = self
            .key
            .strip_prefix('F')
            .and_then(|key| key.parse::<u8>().ok())
            .is_some_and(|number| (1..=20).contains(&number) && self.key == format!("F{number}"));
        let special = matches!(
            self.key,
            "Space"
                | "Tab"
                | "Enter"
                | "Escape"
                | "Backspace"
                | "Delete"
                | "ArrowUp"
                | "ArrowDown"
                | "ArrowLeft"
                | "ArrowRight"
                | "Minus"
                | "Equal"
                | "BracketLeft"
                | "BracketRight"
                | "Backslash"
                | "Semicolon"
                | "Quote"
                | "Backquote"
                | "Comma"
                | "Period"
                | "Slash"
                | "Home"
                | "End"
                | "PageUp"
                | "PageDown"
        );
        if !(letter || digit || function || special) {
            return Err("Use an ordinary key or function key with modifiers.".into());
        }
        let internal = ((m.meta || m.control) && matches!(self.key, "KeyZ" | "KeyY"))
            || ((m.meta || m.control) && !m.alt && matches!(self.key, "KeyS"))
            || ((m.meta || m.control)
                && !m.alt
                && !m.shift
                && matches!(
                    self.key,
                    "KeyQ" | "KeyW" | "KeyC" | "KeyV" | "KeyX" | "KeyA" | "Comma"
                ));
        if internal {
            return Err(
                "This shortcut is already used by Snipuno. Choose another combination.".into(),
            );
        }
        Ok(())
    }

    pub fn key_labels(&self) -> Vec<String> {
        let m = self.modifiers;
        let key = self
            .key
            .strip_prefix("Key")
            .or_else(|| self.key.strip_prefix("Digit"))
            .unwrap_or(self.key);
        let key = match key {
            "ArrowUp" => "↑",
            "ArrowDown" => "↓",
            "ArrowLeft" => "←",
            "ArrowRight" => "→",
            "Escape" => "Esc",
            "Enter" if cfg!(target_os = "macos") => "↩",
            "Backspace" if cfg!(target_os = "macos") => "⌫",
            "Delete" if cfg!(target_os = "macos") => "⌦",
            "Minus" => "-",
            "Equal" => "=",
            "BracketLeft" => "[",
            "BracketRight" => "]",
            "Backslash" => "\\",
            "Semicolon" => ";",
            "Quote" => "'",
            "Backquote" => "`",
            "Comma" => ",",
            "Period" => ".",
            "Slash" => "/",
            "PageUp" => "Page Up",
            "PageDown" => "Page Down",
            key => key,
        };
        let modifiers = if cfg!(target_os = "macos") {
            [
                (m.control, "⌃"),
                (m.alt, "⌥"),
                (m.shift, "⇧"),
                (m.meta, "⌘"),
            ]
        } else {
            [
                (m.control, "Ctrl"),
                (m.alt, "Alt"),
                (m.shift, "Shift"),
                (m.meta, "Win"),
            ]
        };
        modifiers
            .into_iter()
            .filter_map(|(pressed, label)| pressed.then_some(label))
            .chain([key])
            .map(str::to_owned)
            .collect()
    }
}

#[derive(Default)]
pub struct TriggerGate {
    pressed: bool,
    pub busy: bool,
    pub applying: bool,
}

impl TriggerGate {
    pub fn key_event(&mut self, pressed: bool) -> bool {
        let trigger = pressed && !self.pressed && self.can_capture();
        self.pressed = pressed;
        trigger
    }

    pub fn can_capture(&self) -> bool {
        !self.busy && !self.applying
    }

    pub fn reset(&mut self) {
        self.pressed = false;
    }
}

pub fn show_dock(ordinary_windows: usize, menu_available: bool) -> bool {
    ordinary_windows > 0 || !menu_available
}
