use gpui::{App, Div, ParentElement as _, SharedString, Styled as _, div, px};
use gpui_base::Theme;

pub fn notice(message: impl Into<SharedString>, error: bool, cx: &App) -> Div {
    let colors = Theme::global(cx).tokens.colors;
    div()
        .flex_shrink_0()
        .p_3()
        .rounded_lg()
        .text_xs()
        .line_height(px(17.))
        .bg(if error {
            colors.destructive.opacity(0.08)
        } else {
            colors.muted
        })
        .text_color(if error {
            colors.destructive
        } else {
            colors.muted_foreground
        })
        .child(message.into())
}
