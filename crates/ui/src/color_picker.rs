use crate::{ColorSwatch, theme::PALETTE};
use gpui::{
    App, BoxShadow, ElementId, InteractiveElement as _, IntoElement, MouseButton,
    ParentElement as _, RenderOnce, StatefulInteractiveElement as _, Styled as _, Window, div,
    prelude::FluentBuilder as _, px, rgb,
};
use gpui_base::{GlobalState, Spring, Theme, Toggle, ToggleGroup, spring};
use std::{rc::Rc, time::Duration};

type ChangeHandler = Rc<dyn Fn(&Option<u32>, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct ColorPicker {
    id: ElementId,
    selected: Option<u32>,
    allow_auto: bool,
    on_change: Option<ChangeHandler>,
}

impl ColorPicker {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            selected: Some(PALETTE[0].0),
            allow_auto: false,
            on_change: None,
        }
    }

    pub fn selected(mut self, color: Option<u32>) -> Self {
        self.selected = color;
        self
    }

    pub fn allow_auto(mut self, allow: bool) -> Self {
        self.allow_auto = allow;
        self
    }

    pub fn on_change(
        mut self,
        handler: impl Fn(&Option<u32>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for ColorPicker {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = Theme::global(cx).tokens.colors;
        let swatch_width = px(28.);
        let selected = self
            .selected
            .or_else(|| (!self.allow_auto).then_some(PALETTE[0].0));
        let swatches = self
            .allow_auto
            .then_some((None, "Automatic"))
            .into_iter()
            .chain(
                PALETTE
                    .into_iter()
                    .map(|(color, label)| (Some(color), label)),
            )
            .collect::<Vec<_>>();
        let indicator = swatches
            .iter()
            .position(|(color, _)| *color == selected)
            .map(|index| {
                let left = spring(
                    (self.id.clone(), "selection"),
                    swatch_width * index as f32,
                    Spring::new(Duration::from_millis(160))
                        .with_damping(0.8)
                        .with_epsilon(0.08),
                    window,
                    cx,
                );
                div()
                    .absolute()
                    .top_0()
                    .left(left)
                    .w(swatch_width)
                    .h_7()
                    .rounded_md()
                    .bg(colors.muted)
            });
        ToggleGroup::new(self.id)
            .aria_label("Color")
            .relative()
            .flex()
            .items_center()
            .flex_shrink_0()
            .children(indicator)
            .children(
                swatches
                    .into_iter()
                    .enumerate()
                    .map(|(index, (color, label))| {
                        Toggle::new(("color", index))
                            .w(swatch_width)
                            .h_7()
                            .flex_shrink_0()
                            .p_0()
                            .rounded_md()
                            .pressed(selected == color)
                            .accessibility_label(label)
                            .when(selected != color, |this| {
                                this.hover(|style| style.opacity(0.8))
                            })
                            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                                window.prevent_default();
                                GlobalState::suppress_text_selection(cx);
                            })
                            .focus_visible(|style| {
                                style.shadow(vec![
                                    BoxShadow::new(px(0.), px(0.), colors.ring.opacity(0.2))
                                        .spread_radius(px(2.)),
                                ])
                            })
                            .child(
                                ColorSwatch::new(color.map(|color| rgb(color & 0xffffff).into()))
                                    .when(color.is_none(), |this| this.marker("A")),
                            )
                            .when_some(self.on_change.clone(), |this, on_change| {
                                this.on_change(move |_, _, window, cx| {
                                    on_change(&color, window, cx)
                                })
                            })
                    }),
            )
    }
}
