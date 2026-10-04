use crate::Divider;
use gpui::{
    AnyElement, App, FontWeight, Hsla, IntoElement, ParentElement, RenderOnce, SharedString,
    Styled as _, Window, div, prelude::FluentBuilder as _, px,
};
use gpui_base::Theme;

struct Field {
    title: SharedString,
    description: SharedString,
    control: AnyElement,
    footer: Option<AnyElement>,
}

#[derive(Default, IntoElement)]
pub struct Form {
    title: Option<SharedString>,
    fields: Vec<Field>,
    footer: Option<AnyElement>,
    border_color: Option<Hsla>,
}

impl Form {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn field(
        mut self,
        title: impl Into<SharedString>,
        description: impl Into<SharedString>,
        control: impl IntoElement,
    ) -> Self {
        self.fields.push(Field {
            title: title.into(),
            description: description.into(),
            control: control.into_any_element(),
            footer: None,
        });
        self
    }

    pub fn field_footer(mut self, content: impl IntoElement) -> Self {
        if let Some(field) = self.fields.last_mut() {
            field.footer = Some(content.into_any_element());
        }
        self
    }

    pub fn footer(mut self, content: impl IntoElement) -> Self {
        self.footer = Some(content.into_any_element());
        self
    }

    pub fn border_color(mut self, color: impl Into<Hsla>) -> Self {
        self.border_color = Some(color.into());
        self
    }
}

impl RenderOnce for Form {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = Theme::global(cx).tokens.colors;
        let content = div()
            .flex()
            .flex_col()
            .rounded(px(10.))
            .border_1()
            .border_color(self.border_color.unwrap_or(colors.border))
            .bg(colors.background)
            .children(self.fields.into_iter().enumerate().map(|(index, field)| {
                div()
                    .when(index > 0, |this| this.child(Divider::horizontal().mx_4()))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_4()
                            .min_h(px(60.))
                            .px_4()
                            .py_2p5()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .flex_1()
                                    .min_w_0()
                                    .gap(px(3.))
                                    .child(div().font_weight(FontWeight::MEDIUM).child(field.title))
                                    .when(!field.description.is_empty(), |this| {
                                        this.child(
                                            div()
                                                .text_xs()
                                                .line_height(px(17.))
                                                .text_color(colors.muted_foreground)
                                                .child(field.description),
                                        )
                                    }),
                            )
                            .child(div().flex_shrink_0().child(field.control)),
                    )
                    .when_some(field.footer, |this, footer| {
                        this.child(div().px_4().pb_3().child(footer))
                    })
            }))
            .when_some(self.footer, |this, footer| {
                this.child(div().px_4().pb_3().child(footer))
            });
        div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .gap_2()
            .text_size(px(13.))
            .line_height(px(18.))
            .when_some(self.title, |this, title| {
                this.child(
                    div()
                        .px_0p5()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(colors.muted_foreground)
                        .child(title),
                )
            })
            .child(content)
    }
}
