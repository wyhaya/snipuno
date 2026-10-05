pub use ui::theme::{BRAND, PALETTE, PANEL_RADIUS, brand};

// Image and annotation colors remain fixed regardless of the interface theme.
pub const TEXT: u32 = 0x26364d;
pub const SURFACE: u32 = 0xffffff;
pub const DANGER: u32 = 0xf05263;
pub const GUIDE_COLOR: u32 = 0xff000000 | DANGER;
pub const ALIGNMENT_GUIDE_ALPHA: u32 = 0xcc;
pub const DIMMING: u32 = 0x000000;

pub const ANNOTATION_RADIUS: f32 = 10.0;
pub const SELECTION_RADIUS: f32 = 4.0;
pub const DIMENSION_FONT_SIZE: f32 = 11.0;
pub const DIMENSION_LINE_HEIGHT: f32 = 16.0;
pub const DIMENSION_PADDING: f32 = 4.0;
pub const DIMENSION_RADIUS: f32 = 4.0;
pub const SELECTION_BORDER_WIDTH: f32 = 1.0;
pub const HANDLE_SIZE: f32 = 7.0;
pub const HANDLE_BORDER_WIDTH: f32 = 1.5;
pub const HANDLE_HIT_RADIUS: f32 = 6.0;
pub const HOVER_FILL_OPACITY: f32 = 0.1;
pub const HOVER_STROKE_OPACITY: f32 = 0.5;
pub const HOVER_BORDER_OPACITY: f32 = 0.75;
pub const HOVER_BORDER_WIDTH: f32 = 1.5;
pub const HOVER_STROKE_EXPANSION: f32 = 4.0;
pub const TOOLBAR_HEIGHT: f32 = 44.0;
pub const TOOLBAR_GAP: f32 = 1.0;
pub const ICON_BUTTON_SIZE: f32 = 32.0;
pub const OPTIONS_PANEL_MARGIN: f32 = 12.0;
pub const OPTIONS_PANEL_PADDING: f32 = 6.0;
pub const OPTIONS_PANEL_BORDER_WIDTH: f32 = 1.0;
pub const OPTIONS_PANEL_HEIGHT: f32 =
    ICON_BUTTON_SIZE + (OPTIONS_PANEL_PADDING + OPTIONS_PANEL_BORDER_WIDTH) * 2.0;
pub const ICON_SIZE: f32 = 16.0;
pub const CANVAS_PADDING: f32 = 0.0;
pub const CHECKERBOARD_SIZE: f32 = 12.0;

pub fn contrasting_text(background: u32) -> u32 {
    let red = (background >> 16) & 255;
    let green = (background >> 8) & 255;
    let blue = background & 255;
    if red * 299 + green * 587 + blue * 114 > 150_000 {
        TEXT
    } else {
        SURFACE
    }
}
