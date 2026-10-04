use gpui::{App, Global, KeyDownEvent, Modifiers};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorFormat {
    #[default]
    Hex,
    Rgb,
}

impl Global for ColorFormat {}

impl ColorFormat {
    pub fn current(cx: &App) -> Self {
        cx.try_global::<Self>().copied().unwrap_or_default()
    }

    pub fn format(self, color: u32) -> String {
        let [_, red, green, blue] = color.to_be_bytes();
        match self {
            Self::Hex => format!("#{red:02X}{green:02X}{blue:02X}"),
            Self::Rgb => format!("rgb({red}, {green}, {blue})"),
        }
    }

    pub fn handle_key_down(event: &KeyDownEvent, cx: &mut App) -> bool {
        if !event.keystroke.key.eq_ignore_ascii_case("f")
            || event.keystroke.modifiers != Modifiers::default()
        {
            return false;
        }
        if !event.is_held {
            cx.set_global(match Self::current(cx) {
                Self::Hex => Self::Rgb,
                Self::Rgb => Self::Hex,
            });
        }
        cx.stop_propagation();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::ColorFormat;

    #[test]
    fn default_format_is_uppercase_six_digit_hex() {
        let format = ColorFormat::default();
        assert_eq!(format, ColorFormat::Hex);
        assert_eq!(format.format(0), "#000000");
        assert_eq!(format.format(0x010aff), "#010AFF");
        assert_eq!(format.format(0xffffff), "#FFFFFF");
    }

    #[test]
    fn formats_preserve_rgb_channel_order_and_ignore_alpha() {
        for color in [0x1234ab, 0x801234ab, 0xff1234ab] {
            assert_eq!(ColorFormat::Hex.format(color), "#1234AB");
            assert_eq!(ColorFormat::Rgb.format(color), "rgb(18, 52, 171)");
        }
        assert_eq!(ColorFormat::Rgb.format(0), "rgb(0, 0, 0)");
        assert_eq!(ColorFormat::Rgb.format(0xffffff), "rgb(255, 255, 255)");
    }
}
