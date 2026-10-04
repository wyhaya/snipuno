use crate::Button;
use gpui::{
    Anchor, Animation, AnimationExt as _, App, ElementId, FocusHandle, Focusable as _,
    InteractiveElement as _, IntoElement, KeyBinding, KeyDownEvent, MouseButton, NoAction,
    ParentElement as _, RenderOnce, Role, SharedString, StatefulInteractiveElement as _,
    StyleRefinement, Styled, Window, div, prelude::FluentBuilder as _, px,
};
use gpui_base::{Popover, StyledExt as _, Theme, animation::ease_out_cubic};
use std::time::Duration;

const MENU_CONTEXT: &str = "DropdownMenu";

pub(crate) fn init(cx: &mut App) {
    // Let menu buttons handle activation without the popover's toggle bindings.
    cx.bind_keys(
        [
            "enter",
            "space",
            "up",
            "down",
            "home",
            "end",
            "tab",
            "shift-tab",
        ]
        .map(|key| KeyBinding::new(key, NoAction, Some(MENU_CONTEXT))),
    );
}

#[derive(IntoElement)]
pub struct Dropdown {
    id: ElementId,
    trigger: Button,
    items: Vec<Button>,
    menu_label: SharedString,
    style: StyleRefinement,
}

impl Dropdown {
    pub fn new(id: impl Into<ElementId>, trigger: Button) -> Self {
        Self {
            id: id.into(),
            trigger,
            items: Vec::new(),
            menu_label: "More actions".into(),
            style: StyleRefinement::default(),
        }
    }

    pub fn items(mut self, items: impl IntoIterator<Item = Button>) -> Self {
        self.items.extend(items);
        self
    }

    pub fn menu_label(mut self, label: impl Into<SharedString>) -> Self {
        self.menu_label = label.into();
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.trigger = self.trigger.disabled(disabled);
        self
    }
}

impl Styled for Dropdown {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Dropdown {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = Theme::global(cx).tokens.colors;
        let disabled = self.trigger.is_disabled() || self.items.is_empty();
        let focus_handles = window
            .use_keyed_state((self.id.clone(), "item-focus"), cx, |_, _| {
                Vec::<FocusHandle>::new()
            })
            .update(cx, |handles, cx| {
                handles.resize_with(self.items.len(), || cx.focus_handle());
                handles.clone()
            });
        Popover::new(self.id)
            .flex_shrink_0()
            .anchor(Anchor::TopLeft)
            .offset(px(2.))
            .when(disabled, |this| this.open(false))
            .trigger_with(move |open, _, _| {
                self.trigger
                    .menu_trigger(open)
                    .disabled(disabled)
                    .into_any_element()
            })
            .content(move |state, window, cx| {
                let enabled_focus: Vec<_> = self
                    .items
                    .iter()
                    .zip(&focus_handles)
                    .filter(|(item, _)| !item.is_disabled())
                    .map(|(_, focus)| focus.clone())
                    .collect();
                if state.focus_handle(cx).is_focused(window)
                    && let Some(focus) = enabled_focus.first()
                {
                    focus.focus(window, cx);
                }
                div()
                    .id("items")
                    .relative()
                    .key_context(MENU_CONTEXT)
                    .role(Role::Menu)
                    .aria_label(self.menu_label)
                    .min_w(px(160.))
                    .p_1()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .rounded_lg()
                    .border_1()
                    .border_color(colors.border)
                    .bg(colors.surface)
                    .text_color(colors.surface_foreground)
                    .shadow_xs()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_key_down(cx.listener(move |state, event: &KeyDownEvent, window, cx| {
                        cx.stop_propagation();
                        if event.keystroke.key == "tab" {
                            state.dismiss(window, cx);
                            window.refresh();
                            let backwards = event.keystroke.modifiers.shift;
                            window.on_next_frame(move |window, cx| {
                                if backwards {
                                    window.focus_prev(cx);
                                } else {
                                    window.focus_next(cx);
                                }
                            });
                            return;
                        }
                        if enabled_focus.is_empty() {
                            return;
                        }
                        let current = enabled_focus
                            .iter()
                            .position(|focus| focus.is_focused(window))
                            .unwrap_or(0);
                        let count = enabled_focus.len();
                        let next = match event.keystroke.key.as_str() {
                            "down" => (current + 1) % count,
                            "up" => (current + count - 1) % count,
                            "home" => 0,
                            "end" => count - 1,
                            _ => return,
                        };
                        enabled_focus[next].focus(window, cx);
                    }))
                    .children(
                        self.items
                            .into_iter()
                            .zip(focus_handles)
                            .map(|(mut item, focus)| {
                                let style = item.style().clone();
                                item.menu_item(
                                    &focus,
                                    cx.listener(move |state, _, window, cx| {
                                        state.dismiss(window, cx);
                                        window.refresh();
                                        cx.stop_propagation();
                                    }),
                                )
                                .h_8()
                                .w_full()
                                .justify_start()
                                .refine_style(&style)
                            }),
                    )
                    .with_animation(
                        "menu-enter",
                        Animation::new(Duration::from_millis(160)).with_easing(ease_out_cubic),
                        |this, progress| this.opacity(progress).top(px(-4.) * (1. - progress)),
                    )
            })
            .refine_style(&self.style)
    }
}
