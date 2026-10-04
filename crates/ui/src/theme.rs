use gpui::{App, Global, Hsla, Window, WindowAppearance, font, px, rgb, transparent_black};
use gpui_base::{
    ColorTokens, ScrollbarEntrance, ScrollbarMode, ScrollbarMotion, ScrollbarStyles,
    ScrollbarTheme, ShadowTokens, Theme, ThemeAppearance,
};
use std::time::Duration;

pub const BRAND: u32 = 0x377dec;
pub const PALETTE: [(u32, &str); 6] = [
    (0xfff05263, "Red"),
    (0xffedaa32, "Yellow"),
    (0xff32b58a, "Green"),
    (0xff000000 | BRAND, "Blue"),
    (0xff9254de, "Purple"),
    (0xff26364d, "Dark gray"),
];
pub const CONTROL_RADIUS: f32 = 6.0;
pub const PANEL_RADIUS: f32 = 8.0;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

impl Global for ThemeMode {}

pub fn mode(cx: &App) -> ThemeMode {
    cx.try_global::<ThemeMode>().copied().unwrap_or_default()
}

pub fn set_mode(mode: ThemeMode, window: Option<&mut Window>, cx: &mut App) {
    cx.set_global(mode);
    sync_system_appearance(window, cx);
}

pub fn brand(opacity: f32) -> Hsla {
    let mut color: Hsla = rgb(BRAND).into();
    color.a = opacity;
    color
}

pub(crate) fn init(cx: &mut App) {
    cx.set_global(ThemeMode::System);
    resolve_fonts(cx);
    sync_system_appearance(None, cx);
}

pub fn observe_window(window: &mut Window, cx: &mut App) {
    sync_system_appearance(Some(window), cx);
    window
        .observe_window_appearance(|window, cx| sync_system_appearance(Some(window), cx))
        .detach();
}

fn sync_system_appearance(window: Option<&mut Window>, cx: &mut App) {
    let appearance = window
        .as_ref()
        .map(|window| window.appearance())
        .unwrap_or_else(|| cx.window_appearance());
    let dark = match mode(cx) {
        ThemeMode::System => matches!(
            appearance,
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        ),
        ThemeMode::Light => false,
        ThemeMode::Dark => true,
    };
    let mode = if cx.should_auto_hide_scrollbars() {
        ScrollbarMode::Scrolling
    } else {
        ScrollbarMode::Hover
    };
    let theme = Theme::global_mut(cx);
    theme.appearance = if dark {
        ThemeAppearance::Dark
    } else {
        ThemeAppearance::Light
    };
    let colors = ColorTokens {
        primary: brand(1.0),
        primary_foreground: rgb(0xffffff).into(),
        ring: brand(1.0),
        accent: brand(0.1),
        accent_foreground: brand(1.0),
        ..if dark {
            ColorTokens::dark()
        } else {
            ColorTokens::light()
        }
    };
    theme.tokens.colors = colors;
    theme.tokens.radius.md = px(CONTROL_RADIUS);
    theme.tokens.radius.lg = px(PANEL_RADIUS);
    theme.tokens.shadow = ShadowTokens::elevations(transparent_black().alpha(0.18));
    let thumb = Hsla::from(rgb(if dark { 0x525252 } else { 0xa3a3a3 }));
    theme.scrollbar = ScrollbarTheme::new()
        .with_mode(mode)
        .with_motion(
            ScrollbarMotion::default()
                .with_idle(Duration::from_secs(2))
                .with_enter(Duration::from_millis(300))
                .with_exit(Duration::from_millis(500))
                .with_expand(Duration::from_millis(300))
                .with_thumb_hover_entrance(if mode == ScrollbarMode::Hover {
                    ScrollbarEntrance::SlideAndFade
                } else {
                    ScrollbarEntrance::Fade
                }),
        )
        .with_styles(
            ScrollbarStyles::default()
                .track(|style| style.bg(transparent_black()))
                .track_hover(|style| style.bg(transparent_black()))
                .track_active(|style| style.bg(transparent_black()).border_color(colors.border))
                .thumb(|style| {
                    style
                        .bg(thumb.alpha(230.0 / 255.0))
                        .radius(px(CONTROL_RADIUS))
                })
                .thumb_hover(|style| style.bg(thumb).radius(px(CONTROL_RADIUS)))
                .thumb_active(|style| style.bg(thumb).radius(px(CONTROL_RADIUS))),
        );
    cx.refresh_windows();
}

fn resolve_fonts(cx: &mut App) {
    let text_system = cx.text_system();
    let installed = text_system.all_font_names();
    if installed.is_empty() {
        return;
    }
    let resolved = text_system
        .get_font_for_id(text_system.resolve_font(&font(".SystemUIFont")))
        .map(|font| font.family);
    let typography = &mut Theme::global_mut(cx).tokens.typography;
    if let Some(family) =
        resolved.filter(|family| installed.iter().any(|name| name == family.as_ref()))
    {
        typography.sans = family;
    }
    let alternates = if cfg!(target_os = "macos") {
        &["Monaco", "Courier New"][..]
    } else if cfg!(target_os = "windows") {
        &["Cascadia Mono", "Courier New"][..]
    } else {
        &["Noto Sans Mono", "Liberation Mono", "Ubuntu Mono"][..]
    };
    typography.mono = std::iter::once(typography.mono.as_ref())
        .chain(alternates.iter().copied())
        .find(|candidate| installed.iter().any(|name| name == candidate))
        .map(|family| family.to_owned().into())
        .unwrap_or_else(|| typography.sans.clone());
}
