use gpui::{App, IntoElement, Window, div};
#[cfg(target_os = "windows")]
use gpui::{
    InteractiveElement as _, ParentElement as _, StatefulInteractiveElement as _, Styled as _,
    WindowControlArea, prelude::FluentBuilder as _, px, rgba,
};
#[cfg(target_os = "windows")]
use gpui_base::{Theme, ThemeAppearance};

const WINDOW_CONTROL_WIDTH: f32 = 46.0;
pub const WINDOW_CONTROLS_WIDTH: f32 = WINDOW_CONTROL_WIDTH * 3.0;

#[cfg(target_os = "windows")]
pub fn window_controls(window: &Window, cx: &App) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .h_full()
        .flex_shrink_0()
        .font_family("Segoe Fluent Icons")
        .child(window_control(
            "minimize-window",
            "\u{e921}",
            WindowControlArea::Min,
            window.is_minimizable(),
            cx,
        ))
        .child(window_control(
            "maximize-window",
            if window.is_maximized() {
                "\u{e923}"
            } else {
                "\u{e922}"
            },
            WindowControlArea::Max,
            window.is_resizable(),
            cx,
        ))
        .child(window_control(
            "close-window",
            "\u{e8bb}",
            WindowControlArea::Close,
            true,
            cx,
        ))
}

#[cfg(not(target_os = "windows"))]
pub fn window_controls(_: &Window, _: &App) -> impl IntoElement {
    div()
}

#[cfg(target_os = "windows")]
fn window_control(
    id: &'static str,
    label: &'static str,
    area: WindowControlArea,
    enabled: bool,
    cx: &App,
) -> impl IntoElement {
    let close = matches!(area, WindowControlArea::Close);
    let dark = Theme::global(cx).appearance == ThemeAppearance::Dark;
    let foreground = if dark {
        rgba(0xffff_ffff)
    } else {
        rgba(0x0000_00e4)
    };
    let disabled_foreground = if dark {
        rgba(0xffff_ff5d)
    } else {
        rgba(0x0000_005c)
    };
    let (hover_background, active_background, hover_foreground, active_foreground) = if close {
        (
            rgba(0xc42b_1cff),
            rgba(0xc42b_1ce6),
            rgba(0xffff_ffff),
            rgba(0xffff_ffb3),
        )
    } else if dark {
        (rgba(0xffff_ff0f), rgba(0xffff_ff0b), foreground, foreground)
    } else {
        (rgba(0x0000_000f), rgba(0x0000_000a), foreground, foreground)
    };
    div()
        .id(id)
        .w(px(WINDOW_CONTROL_WIDTH))
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .text_size(px(10.0))
        .text_color(if enabled {
            foreground
        } else {
            disabled_foreground
        })
        .when(enabled, |this| {
            this.hover(|style| style.bg(hover_background).text_color(hover_foreground))
                .active(|style| style.bg(active_background).text_color(active_foreground))
        })
        .window_control_area(area)
        .child(label)
}
