use gpui::{
    App, FontWeight, Hsla, IntoElement, ParentElement as _, RenderOnce, SharedString,
    StyleRefinement, Styled, Window, checkerboard, div, prelude::FluentBuilder as _, px,
};
use gpui_base::{StyledExt as _, Theme};

#[derive(IntoElement)]
pub struct ColorSwatch {
    color: Option<Hsla>,
    marker: Option<SharedString>,
    style: StyleRefinement,
}

impl ColorSwatch {
    pub fn new(color: Option<Hsla>) -> Self {
        Self {
            color,
            marker: None,
            style: StyleRefinement::default(),
        }
    }

    pub fn marker(mut self, marker: impl Into<SharedString>) -> Self {
        self.marker = Some(marker.into());
        self
    }
}

impl Styled for ColorSwatch {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for ColorSwatch {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = Theme::global(cx).tokens.colors;
        let color = self.color.unwrap_or(colors.background);
        div()
            .relative()
            .size_4()
            .flex_shrink_0()
            .rounded_full()
            .border_1()
            .border_color(color.blend(colors.foreground.opacity(0.12)))
            .bg(color)
            .when(self.color.is_none(), |this| {
                this.child(
                    div()
                        .size_full()
                        .rounded_full()
                        .bg(checkerboard(colors.muted, 4.0)),
                )
            })
            .when_some(self.marker, |this, marker| {
                this.child(
                    div()
                        .absolute()
                        .right_px()
                        .bottom_0()
                        .text_size(px(8.0))
                        .line_height(px(9.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(colors.foreground)
                        .child(marker),
                )
            })
            .refine_style(&self.style)
    }
}
