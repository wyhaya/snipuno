// TODO: Revisit Linux client-side decorations when needed: desktops without server-side
// decorations currently have no custom border, shadow, or edge-resize handles.

mod assets;
mod button;
mod color_picker;
mod color_swatch;
mod divider;
mod dropdown;
mod form;
mod icon;
mod kbd;
mod notice;
mod root;
mod scroll_area;
mod segmented_control;
mod slider;
mod switch;
pub mod theme;
mod tooltip;
mod window_controls;

pub use assets::IconAssets;
pub use button::Button;
pub use color_picker::ColorPicker;
pub use color_swatch::ColorSwatch;
pub use divider::Divider;
pub use dropdown::Dropdown;
pub use form::Form;
pub use icon::Icon;
pub use kbd::{Kbd, PRIMARY, SHIFT};
pub use notice::notice;
pub use scroll_area::ScrollArea;
pub use segmented_control::{Segment, SegmentedControl};
pub use slider::{Slider, SliderEvent};
pub use switch::Switch;
pub use theme::ThemeMode;
pub use tooltip::{ToolTooltip, ToolTooltipExt};
pub use window_controls::{WINDOW_CONTROLS_WIDTH, window_controls};

pub use gpui_base::{Root, Theme, ThemeAppearance};

pub fn init(cx: &mut gpui::App) {
    gpui_base::init(cx);
    dropdown::init(cx);
    theme::init(cx);
    root::init(cx);
}
