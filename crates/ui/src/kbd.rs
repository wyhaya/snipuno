use gpui::{
    App, IntoElement, ParentElement as _, RenderOnce, SharedString, Styled as _, Window, div,
    prelude::FluentBuilder as _, px,
};
use gpui_base::Theme;

pub const PRIMARY: &str = if cfg!(target_os = "macos") {
    "⌘"
} else {
    "Ctrl"
};
pub const SHIFT: &str = if cfg!(target_os = "macos") {
    "⇧"
} else {
    "Shift"
};

#[derive(IntoElement)]
pub struct Kbd {
    keys: Vec<SharedString>,
}

impl Kbd {
    pub fn new(keys: impl IntoIterator<Item = impl Into<SharedString>>) -> Self {
        Self {
            keys: keys.into_iter().map(Into::into).collect(),
        }
    }
}

impl RenderOnce for Kbd {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = Theme::global(cx).tokens.colors;
        div()
            .flex()
            .items_center()
            .flex_shrink_0()
            .gap(px(3.))
            .children(self.keys.into_iter().map(|key| {
                div()
                    .text_size(px(11.))
                    .line_height(px(18.))
                    .text_color(colors.muted_foreground)
                    .whitespace_nowrap()
                    .when(key.as_ref() != "or", |this| {
                        this.min_w_5()
                            .px_1()
                            .text_center()
                            .bg(colors.muted)
                            .border_1()
                            .border_color(colors.border)
                            .rounded_sm()
                    })
                    .child(key)
            }))
    }
}
