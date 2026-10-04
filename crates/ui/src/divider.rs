use gpui::{App, Axis, IntoElement, RenderOnce, StyleRefinement, Styled, Window, div};
use gpui_base::{StyledExt as _, Theme};

#[derive(IntoElement)]
pub struct Divider {
    axis: Axis,
    style: StyleRefinement,
}

impl Divider {
    pub fn horizontal() -> Self {
        Self {
            axis: Axis::Horizontal,
            style: StyleRefinement::default(),
        }
    }

    pub fn vertical() -> Self {
        Self {
            axis: Axis::Vertical,
            style: StyleRefinement::default(),
        }
    }
}

impl Styled for Divider {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Divider {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let base = div()
            .flex_shrink_0()
            .bg(Theme::global(cx).tokens.colors.border);
        match self.axis {
            Axis::Horizontal => base.h_px(),
            Axis::Vertical => base.w_px().h_full(),
        }
        .refine_style(&self.style)
    }
}
