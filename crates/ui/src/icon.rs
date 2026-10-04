use gpui::{App, IntoElement, RenderOnce, Styled as _, Window, svg};

macro_rules! icons {
    ($($variant:ident => $name:literal),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, IntoElement)]
        pub enum Icon {
            $($variant),+
        }

        impl Icon {
            pub const ALL: &[Self] = &[$(Self::$variant),+];

            pub fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => $name),+
                }
            }

            pub(crate) fn path(self) -> &'static str {
                match self {
                    $(Self::$variant => concat!("icons/", $name, ".svg")),+
                }
            }

            pub(crate) fn bytes(self) -> &'static [u8] {
                match self {
                    $(Self::$variant => include_bytes!(concat!("../icons/", $name, ".svg"))),+
                }
            }
        }
    };
}

icons! {
    ArrowBackUp => "arrow-back-up",
    ArrowForwardUp => "arrow-forward-up",
    ArrowRight => "arrow-right",
    ArrowUpRight => "arrow-up-right",
    ArrowsHorizontal => "arrows-horizontal",
    Blur => "blur",
    Brush => "brush",
    Check => "check",
    ChevronDown => "chevron-down",
    CircleFilled => "circle-filled",
    Circle => "circle",
    Copy => "copy",
    Crop => "crop",
    DeviceFloppy => "device-floppy",
    Eraser => "eraser",
    GuideHorizontal => "guide-horizontal",
    GuideVertical => "guide-vertical",
    LetterT => "letter-t",
    LineDashed => "line-dashed",
    Line => "line",
    Magnifier => "magnifier",
    Mosaic => "mosaic",
    NumberTailBottomLeft => "number-tail-bottom-left",
    NumberTailBottomRight => "number-tail-bottom-right",
    NumberTailTopLeft => "number-tail-top-left",
    NumberTailTopRight => "number-tail-top-right",
    PhotoPlus => "photo-plus",
    Pin => "pin",
    Pointer => "pointer",
    Restore => "restore",
    ReturnToEditor => "return-to-editor",
    RulerVertical => "ruler-vertical",
    Ruler => "ruler",
    Spotlight => "spotlight",
    SquareFilled => "square-filled",
    Square => "square",
    TextBubble => "text-bubble",
    TextTailBottomRight => "text-tail-bottom-right",
    TextTailTopLeft => "text-tail-top-left",
    TextTailTopRight => "text-tail-top-right",
    WaveSine => "wave-sine",
    X => "x",
}

impl RenderOnce for Icon {
    fn render(self, window: &mut Window, _: &mut App) -> impl IntoElement {
        svg()
            .flex_shrink_0()
            .size_4()
            .text_color(window.text_style().color)
            .path(self.path())
    }
}
