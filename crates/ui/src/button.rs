use crate::Icon;
use gpui::{
    AnyElement, App, BoxShadow, ClickEvent, ElementId, Entity, FocusHandle, InteractiveElement,
    Interactivity, IntoElement, MouseButton, ParentElement, Pixels, RenderOnce, Role, SharedString,
    StatefulInteractiveElement as _, StyleRefinement, Styled, Toggled, Window, div,
    prelude::FluentBuilder as _, px, transparent_black,
};
use gpui_base::{
    Button as BaseButton, ElementExt as _, GlobalState, StyledExt as _, Theme, Transition,
    animation::cubic_bezier, transition,
};
use std::time::Duration;

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Variant {
    Primary,
    Secondary,
    Ghost,
}

struct PressState {
    pressed: bool,
    inset: Pixels,
}

#[derive(IntoElement)]
pub struct Button {
    base: BaseButton,
    id: ElementId,
    variant: Variant,
    disabled: bool,
    selected: bool,
    selected_accent: bool,
    hover_focus: Option<FocusHandle>,
    on_click: Option<ClickHandler>,
    children: Vec<AnyElement>,
}

impl Button {
    pub fn new(id: impl Into<ElementId>) -> Self {
        let id = id.into();
        Self {
            base: BaseButton::new(id.clone()),
            id,
            variant: Variant::Secondary,
            disabled: false,
            selected: false,
            selected_accent: true,
            hover_focus: None,
            on_click: None,
            children: Vec::new(),
        }
    }

    pub fn icon(id: impl Into<ElementId>, icon: Icon) -> Self {
        Self::new(id).ghost().size_8().p_0().child(icon)
    }

    pub fn primary(mut self) -> Self {
        self.variant = Variant::Primary;
        self
    }

    pub fn ghost(mut self) -> Self {
        self.variant = Variant::Ghost;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub(crate) fn is_disabled(&self) -> bool {
        self.disabled
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn toggled(mut self, toggled: bool) -> Self {
        self.base = self.base.aria_toggled(if toggled {
            Toggled::True
        } else {
            Toggled::False
        });
        self.selected(toggled)
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        let label = label.into();
        self.base = self.base.accessibility_label(label.clone());
        self.child(div().min_w_0().text_ellipsis().child(label))
    }

    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.base = self.base.accessibility_label(label);
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }

    pub(crate) fn menu_trigger(mut self, open: bool) -> Self {
        self.selected_accent = false;
        self.base = self.base.aria_expanded(open);
        self.selected(open)
    }

    pub(crate) fn menu_item(
        mut self,
        focus: &FocusHandle,
        dismiss: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.selected_accent = false;
        self.hover_focus = Some(focus.clone());
        self.base = self.base.role(Role::MenuItem).track_focus(focus);
        let on_click = self.on_click.take();
        self.on_click(move |event, window, cx| {
            dismiss(event, window, cx);
            if let Some(on_click) = &on_click {
                on_click(event, window, cx);
            }
        })
    }
}

impl Styled for Button {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

impl ParentElement for Button {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl InteractiveElement for Button {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.base.interactivity()
    }
}

impl RenderOnce for Button {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = Theme::global(cx).tokens.colors;
        let (background, foreground, hover_background) = match self.variant {
            Variant::Primary => (
                colors.primary,
                colors.primary_foreground,
                colors.background.blend(colors.primary.opacity(0.9)),
            ),
            Variant::Secondary => (
                colors.secondary,
                colors.secondary_foreground,
                colors.background.blend(colors.secondary.opacity(0.8)),
            ),
            Variant::Ghost => (transparent_black(), colors.foreground, colors.muted),
        };
        let press_state =
            window.use_keyed_state((self.id.clone(), "pressed"), cx, |_, _| PressState {
                pressed: false,
                inset: px(1.),
            });
        if self.disabled || !window.is_window_active() {
            press_state.update(cx, |state, _| state.pressed = false);
        }
        let is_pressed = press_state.read(cx).pressed;
        let progress = transition(
            (self.id.clone(), "press-inset"),
            if is_pressed { 1.0 } else { 0.0 },
            Transition::new(Duration::from_millis(if is_pressed { 80 } else { 180 }))
                .ease(cubic_bezier(0.23, 1.0, 0.32, 1.0)),
            window,
            cx,
        );
        let inset = press_state.read(cx).inset * progress;
        let mut style = self.base.style().clone();
        let surface_style = StyleRefinement {
            background: style.background.take(),
            corner_radii: style.corner_radii.clone(),
            ..Default::default()
        };
        let hover_group = format!("button-hover-{}", self.id);
        let keyboard_hover = window.last_input_was_keyboard()
            && self
                .hover_focus
                .as_ref()
                .is_some_and(|focus| focus.is_focused(window));
        let surface = div()
            .absolute()
            .inset(inset)
            .rounded_md()
            .bg(background)
            .refine_style(&surface_style)
            .when(self.selected, |this| this.bg(colors.muted))
            .when(!self.disabled && !self.selected, |this| {
                this.group_hover(hover_group.clone(), |style| style.bg(hover_background))
                    .when(keyboard_hover, |this| this.bg(hover_background))
            });
        let content = div()
            .relative()
            .min_w_0()
            .flex()
            .items_center()
            .justify_center()
            .gap_1()
            .whitespace_nowrap()
            .children(self.children);
        let selected_foreground = if self.selected_accent {
            colors.primary
        } else {
            foreground
        };

        self.base
            .disabled(self.disabled)
            .selected(self.selected)
            .when_some(self.on_click, |this, on_click| {
                this.on_click(move |event, window, cx| on_click(event, window, cx))
            })
            .styles(|styles| {
                styles
                    .selected(|style| style.text_color(selected_foreground))
                    .disabled(|style| style.opacity(0.5))
            })
            .h_7()
            .relative()
            .group(hover_group)
            .px_2()
            .flex_shrink_0()
            .rounded_md()
            .text_size(px(13.))
            .bg(transparent_black())
            .text_color(foreground)
            .on_prepaint({
                let press_state = press_state.clone();
                move |bounds, _, cx| {
                    let extent = bounds.size.width.max(bounds.size.height).max(px(1.));
                    let inset = px((px(48.) / extent).clamp(0.5, 1.));
                    if press_state.read(cx).inset != inset {
                        press_state.update(cx, |state, cx| {
                            state.inset = inset;
                            cx.notify();
                        });
                    }
                }
            })
            .when(!self.disabled, |this| {
                this.on_mouse_down(MouseButton::Left, {
                    let press_state = press_state.clone();
                    move |_, window, cx| {
                        set_pressed(&press_state, true, cx);
                        window.prevent_default();
                        GlobalState::suppress_text_selection(cx);
                    }
                })
                .capture_any_mouse_up({
                    let press_state = press_state.clone();
                    move |event, _, cx| {
                        if event.button == MouseButton::Left {
                            set_pressed(&press_state, false, cx);
                        }
                    }
                })
                .on_mouse_up_out(MouseButton::Left, move |_, _, cx| {
                    set_pressed(&press_state, false, cx);
                })
                .when(self.hover_focus.is_none(), |this| {
                    this.focus_visible(|style| {
                        style.shadow(vec![
                            BoxShadow::new(px(0.), px(0.), colors.ring.opacity(0.2))
                                .spread_radius(px(2.)),
                        ])
                    })
                })
            })
            .child(surface)
            .child(content)
            .refine_style(&style)
    }
}

fn set_pressed(state: &Entity<PressState>, pressed: bool, cx: &mut App) {
    state.update(cx, |state, cx| {
        if state.pressed != pressed {
            state.pressed = pressed;
            cx.notify();
        }
    });
}
