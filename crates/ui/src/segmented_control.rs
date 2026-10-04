use crate::{Icon, ToolTooltip, ToolTooltipExt};
use gpui::{
    App, Bounds, BoxShadow, ElementId, InteractiveElement as _, IntoElement, MouseButton,
    ParentElement as _, Pixels, Point, RenderOnce, SharedString, StatefulInteractiveElement as _,
    StyleRefinement, Styled, Window, div, prelude::FluentBuilder as _, px,
};
use gpui_base::{
    ElementExt as _, GlobalState, Spring, StyledExt as _, Theme, Toggle, ToggleGroup, spring,
};
use std::{cell::RefCell, rc::Rc, time::Duration};

type ChangeHandler<T> = Rc<dyn Fn(&T, &mut Window, &mut App)>;

#[derive(Default)]
struct IndicatorLayout {
    origin: Point<Pixels>,
    segments: Vec<Bounds<Pixels>>,
}

pub struct Segment<T> {
    value: T,
    label: SharedString,
    icon: Option<Icon>,
    tooltip: Option<ToolTooltip>,
}

impl<T> Segment<T> {
    pub fn new(value: T, label: impl Into<SharedString>) -> Self {
        Self {
            value,
            label: label.into(),
            icon: None,
            tooltip: None,
        }
    }

    pub fn icon(value: T, icon: Icon, label: impl Into<SharedString>) -> Self {
        Self {
            icon: Some(icon),
            ..Self::new(value, label)
        }
    }

    pub fn tooltip(mut self, tooltip: ToolTooltip) -> Self {
        self.tooltip = Some(tooltip);
        self
    }
}

#[derive(IntoElement)]
pub struct SegmentedControl<T: PartialEq + 'static> {
    id: ElementId,
    base: ToggleGroup,
    selected: T,
    items: Vec<Segment<T>>,
    on_change: Option<ChangeHandler<T>>,
}

impl<T: PartialEq + 'static> SegmentedControl<T> {
    pub fn new(id: impl Into<ElementId>, selected: T) -> Self {
        let id = id.into();
        Self {
            base: ToggleGroup::new(id.clone()),
            id,
            selected,
            items: Vec::new(),
            on_change: None,
        }
    }

    pub fn items(mut self, items: impl IntoIterator<Item = Segment<T>>) -> Self {
        self.items.extend(items);
        self
    }

    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.base = self.base.aria_label(label);
        self
    }

    pub fn on_change(mut self, handler: impl Fn(&T, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }
}

impl<T: PartialEq + 'static> Styled for SegmentedControl<T> {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

impl<T: PartialEq + 'static> RenderOnce for SegmentedControl<T> {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let style = self.base.style().clone();
        let colors = Theme::global(cx).tokens.colors;
        let layout = window
            .use_keyed_state((self.id.clone(), "layout"), cx, |_, _| {
                Rc::new(RefCell::new(IndicatorLayout::default()))
            })
            .read(cx)
            .clone();
        let target = {
            let mut layout = layout.borrow_mut();
            layout.segments.resize(self.items.len(), Bounds::default());
            self.items
                .iter()
                .position(|item| item.value == self.selected)
                .map(|index| layout.segments[index])
                .filter(|bounds| bounds.size.width > px(0.))
        };
        let mut indicator = target.map(|bounds| {
            let motion = Spring::new(Duration::from_millis(160))
                .with_damping(0.8)
                .with_epsilon(0.1);
            let left = spring(
                (self.id.clone(), "left"),
                bounds.origin.x,
                motion,
                window,
                cx,
            );
            let width = spring((self.id, "width"), bounds.size.width, motion, window, cx);
            div()
                .absolute()
                .top(bounds.origin.y)
                .left(left)
                .w(width)
                .h(bounds.size.height)
                .rounded_md()
                .bg(colors.background)
                .shadow_xs()
        });
        let indicator_ready = indicator.is_some();

        self.base
            .flex()
            .items_center()
            .flex_shrink_0()
            .h_7()
            .p_0p5()
            .gap_0p5()
            .rounded_lg()
            .bg(colors.muted)
            .refine_style(&style)
            .children(self.items.into_iter().enumerate().map(|(index, item)| {
                let selected = self.selected == item.value;
                let toggle = Toggle::new(("segment", index))
                    .pressed(selected)
                    .accessibility_label(item.label.clone())
                    .h_full()
                    .flex_shrink_0()
                    .rounded_md()
                    .text_size(px(13.))
                    .text_color(colors.muted_foreground)
                    .whitespace_nowrap()
                    .styles(|styles| {
                        styles.pressed(|style| {
                            style
                                .text_color(colors.foreground)
                                .when(!indicator_ready, |style| {
                                    style.bg(colors.background).shadow_xs()
                                })
                        })
                    })
                    .when(!selected, |this| {
                        this.hover(|style| style.text_color(colors.foreground))
                            .active(|style| style.text_color(colors.foreground))
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
                    .when_else(
                        item.icon.is_some(),
                        |this| this.w_7(),
                        |this| this.min_w(px(52.)).px_2(),
                    )
                    .child(match item.icon {
                        Some(icon) => icon.into_any_element(),
                        None => div().child(item.label).into_any_element(),
                    })
                    .when_some(item.tooltip, |this, tooltip| this.tool_tooltip(tooltip))
                    .when_some(self.on_change.clone(), |this, on_change| {
                        this.on_change(move |pressed, _, window, cx| {
                            if pressed {
                                on_change(&item.value, window, cx);
                            }
                        })
                    });

                let layout = layout.clone();
                div()
                    .relative()
                    .h_full()
                    .flex_shrink_0()
                    .on_prepaint(move |bounds, window, _| {
                        let mut layout = layout.borrow_mut();
                        if index == 0 {
                            layout.origin = bounds.origin;
                        }
                        let bounds = Bounds::new(bounds.origin - layout.origin, bounds.size);
                        if layout.segments[index] != bounds {
                            layout.segments[index] = bounds;
                            window.request_animation_frame();
                        }
                    })
                    .when(index == 0, |this| {
                        this.when_some(indicator.take(), |this, indicator| this.child(indicator))
                    })
                    .child(toggle)
            }))
    }
}
