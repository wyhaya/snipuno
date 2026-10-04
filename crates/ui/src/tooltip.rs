use crate::{Kbd, root};
use gpui::{
    Animation, AnimationExt as _, AnyElement, AnyView, App, AppContext as _, Bounds, Context, Div,
    ElementId, Entity, EntityId, FontWeight, InteractiveElement, IntoElement, ParentElement,
    Pixels, Render, SharedString, SpringAnimation, SpringConfig, Styled as _, Task, Window, div,
    prelude::FluentBuilder as _, px,
};
use gpui_base::{
    Align, ElementExt as _, Placement, Positioner, Theme, Tooltip as BaseTooltip, TooltipOverlay,
    TooltipRequest, TooltipTransition, Transition,
    animation::{ease_in_out_cubic, ease_out_cubic},
    transition,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};

const TOOLTIP_OFFSET: Pixels = px(10.0);
const TOOLTIP_MARGIN: Pixels = px(8.0);
const TOOLTIP_ENTER_DISTANCE: Pixels = px(4.0);
const TOOLTIP_HIDE_DELAY: Duration = Duration::from_millis(100);
const TOOLTIP_FADE_DURATION: Duration = Duration::from_millis(100);
const TOOLTIP_SLIDE_DURATION: Duration = Duration::from_millis(200);
const TOOLTIP_SPRING_RESPONSE: Duration = Duration::from_millis(180);
const TOOLTIP_SPRING_DAMPING: f32 = 0.7;

#[derive(Clone)]
pub struct ToolTooltip {
    title: SharedString,
    description: Option<SharedString>,
    shortcut: Vec<&'static str>,
    hints: Vec<(&'static str, Vec<&'static str>)>,
    placement: Placement,
    offset: Pixels,
    margin: Pixels,
}

impl ToolTooltip {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            description: None,
            shortcut: Vec::new(),
            hints: Vec::new(),
            placement: Placement::Bottom,
            offset: TOOLTIP_OFFSET,
            margin: TOOLTIP_MARGIN,
        }
    }

    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn placement(mut self, placement: Placement) -> Self {
        self.placement = placement;
        self
    }

    pub fn offset(mut self, offset: Pixels) -> Self {
        self.offset = offset;
        self
    }

    pub fn margin(mut self, margin: Pixels) -> Self {
        self.margin = margin;
        self
    }

    pub fn shortcut(mut self, keys: impl IntoIterator<Item = &'static str>) -> Self {
        self.shortcut = keys.into_iter().collect();
        self
    }

    pub fn hint(
        mut self,
        action: &'static str,
        keys: impl IntoIterator<Item = &'static str>,
    ) -> Self {
        self.hints.push((action, keys.into_iter().collect()));
        self
    }

    fn build(&self, trigger_bounds: Entity<Bounds<Pixels>>, cx: &mut App) -> AnyView {
        cx.new(|_| TooltipPopup {
            tooltip: self.clone(),
            trigger_bounds,
        })
        .into()
    }

    fn content(&self, cx: &App) -> Div {
        let colors = Theme::global(cx).tokens.colors;
        div()
            .flex()
            .flex_col()
            .gap_1p5()
            .text_xs()
            .line_height(px(18.0))
            .text_color(colors.muted_foreground)
            .when(
                self.description.is_some() || !self.hints.is_empty(),
                |this| this.w(px(260.0)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_4()
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(colors.foreground)
                            .child(self.title.clone()),
                    )
                    .when(!self.shortcut.is_empty(), |this| {
                        this.child(Kbd::new(self.shortcut.iter().copied()))
                    }),
            )
            .when_some(self.description.clone(), |this, description| {
                this.child(div().child(description))
            })
            .when(!self.hints.is_empty(), |this| {
                this.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1p5()
                        .mt_0p5()
                        .pt_2()
                        .border_t_1()
                        .border_color(colors.border)
                        .children(self.hints.iter().map(|(action, keys)| {
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap_3()
                                .child(div().flex_1().child(*action))
                                .when(!keys.is_empty(), |this| {
                                    this.child(Kbd::new(keys.iter().copied()))
                                })
                        })),
                )
            })
    }
}

struct TooltipPopup {
    tooltip: ToolTooltip,
    trigger_bounds: Entity<Bounds<Pixels>>,
}

#[derive(Clone)]
pub(crate) struct WindowTooltips {
    pub overlay: Entity<TooltipOverlay>,
    trigger: Rc<Cell<Option<EntityId>>>,
    hide_task: Rc<RefCell<Option<Task<()>>>>,
}

impl WindowTooltips {
    pub fn new(cx: &mut App) -> Self {
        Self {
            overlay: cx.new(|_| TooltipOverlay::new().render_with(render_tooltip)),
            trigger: Rc::default(),
            hide_task: Rc::default(),
        }
    }

    fn show(
        &self,
        tooltip: &ToolTooltip,
        trigger: Entity<Bounds<Pixels>>,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.hide_task.borrow_mut().take();
        self.trigger.set(Some(trigger.entity_id()));
        let bounds = *trigger.read(cx);
        let view = tooltip.build(trigger, cx);
        let request =
            TooltipRequest::new(bounds, move |_, _| view.clone()).placement(tooltip.placement);
        self.overlay
            .update(cx, |overlay, cx| overlay.request_show(request, window, cx));
    }

    fn request_hide(&self, trigger: EntityId, window: &mut Window, cx: &mut App) {
        if self.trigger.get() == Some(trigger) {
            self.trigger.set(None);
            let task = self.overlay.update(cx, |overlay, cx| {
                overlay.request_hide(window, cx);
                // Base fixes its grace period at 300ms; shorten it while preserving handoff.
                cx.spawn(async move |overlay, cx| {
                    cx.background_executor().timer(TOOLTIP_HIDE_DELAY).await;
                    let _ = overlay.update(cx, |overlay, cx| overlay.hide(cx));
                })
            });
            *self.hide_task.borrow_mut() = Some(task);
        }
    }

    pub fn hide(&self, cx: &mut App) {
        self.hide_task.borrow_mut().take();
        self.trigger.set(None);
        self.overlay.update(cx, |overlay, cx| overlay.hide(cx));
    }
}

impl Render for TooltipPopup {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::global(cx);
        let colors = theme.tokens.colors;
        BaseTooltip::new("tooltip-popup")
            .flex()
            .items_center()
            .font_family(theme.tokens.typography.sans)
            .bg(colors.surface)
            .text_color(colors.surface_foreground)
            .border_1()
            .border_color(colors.border.opacity(0.65))
            .p_2p5()
            .rounded_lg()
            .shadow_xs()
            .when(
                self.tooltip.description.is_none() && self.tooltip.hints.is_empty(),
                |this| this.whitespace_nowrap(),
            )
            .child(self.tooltip.content(cx))
    }
}

#[derive(Default)]
struct TooltipMotion {
    epoch: Option<usize>,
    placement: Option<Placement>,
    slide: bool,
}

fn render_tooltip(
    view: AnyView,
    phase: TooltipTransition,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let popup = view
        .clone()
        .downcast::<TooltipPopup>()
        .expect("the tooltip overlay only accepts TooltipPopup views");
    let popup = popup.read(cx);
    let mut bounds = *popup.trigger_bounds.read(cx);
    let placement = popup.tooltip.placement;
    let offset = popup.tooltip.offset;
    let margin = popup.tooltip.margin;
    let epoch = match phase {
        TooltipTransition::Enter { epoch } | TooltipTransition::Switch { epoch, .. } => epoch,
    };
    let motion = window.use_keyed_state("tooltip-motion", cx, |_, _| TooltipMotion::default());
    let slide = motion.update(cx, |motion, _| {
        if motion.epoch != Some(epoch) {
            motion.slide = match phase {
                TooltipTransition::Switch {
                    previous, current, ..
                } if motion.placement == Some(placement) => match placement {
                    Placement::Top | Placement::Bottom => {
                        (current.origin.y - previous.origin.y).abs() < px(10.)
                    }
                    Placement::Left | Placement::Right => {
                        (current.origin.x - previous.origin.x).abs() < px(10.)
                    }
                },
                _ => false,
            };
            motion.epoch = Some(epoch);
            motion.placement = Some(placement);
        }
        motion.slide
    });
    let policy = Transition::new(if slide {
        TOOLTIP_SLIDE_DURATION
    } else {
        Duration::ZERO
    })
    .ease(ease_in_out_cubic);
    let center = transition("tooltip-position", bounds.center(), policy, window, cx);
    bounds.origin.x = center.x - bounds.size.width / 2.;
    bounds.origin.y = center.y - bounds.size.height / 2.;
    let entering = matches!(phase, TooltipTransition::Enter { .. });
    let content = div().child(view).map(|this| {
        if entering {
            this.with_animation(
                ElementId::NamedInteger("tooltip-opacity".into(), epoch as u64),
                Animation::new(TOOLTIP_FADE_DURATION).with_easing(ease_out_cubic),
                |this, progress| this.opacity(progress),
            )
            .into_any_element()
        } else {
            this.into_any_element()
        }
    });
    // Base owns the lifecycle; this positioner preserves our per-tooltip spacing and margin.
    let positioned = Positioner::side(bounds)
        .placement(placement)
        .align(Align::Center)
        .offset(offset)
        .margin(margin)
        .child(content);
    if entering {
        let frequency = std::f32::consts::TAU / TOOLTIP_SPRING_RESPONSE.as_secs_f32();
        let motion = SpringAnimation::new(SpringConfig::new(
            frequency * frequency,
            2.0 * TOOLTIP_SPRING_DAMPING * frequency,
            1.0,
        ))
        .to(offset)
        .from(offset - offset.clamp(px(0.), TOOLTIP_ENTER_DISTANCE))
        .with_epsilon(0.05);
        positioned
            .with_spring(
                ElementId::NamedInteger("tooltip-enter".into(), epoch as u64),
                motion,
                |this, offset| this.offset(offset),
            )
            .into_any_element()
    } else {
        positioned.into_any_element()
    }
}

pub trait ToolTooltipExt: InteractiveElement + ParentElement + Sized {
    fn tool_tooltip(self, tooltip: ToolTooltip) -> Self {
        let trigger: Rc<RefCell<Option<Entity<Bounds<Pixels>>>>> = Rc::default();
        let trigger_writer = trigger.clone();
        let mut this = self.on_prepaint(move |bounds, window, cx| {
            let state = window.use_keyed_state("tooltip-trigger", cx, |_, _| bounds);
            state.update(cx, |current, _| {
                if *current != bounds {
                    *current = bounds;
                    window.request_animation_frame();
                }
            });
            *trigger_writer.borrow_mut() = Some(state);
        });
        this.interactivity().on_hover(move |hovered, window, cx| {
            let (Some(tooltips), Some(trigger)) =
                (root::tooltips(window, cx), trigger.borrow().clone())
            else {
                return;
            };
            if *hovered {
                tooltips.show(&tooltip, trigger, window, cx);
            } else {
                tooltips.request_hide(trigger.entity_id(), window, cx);
            }
        });
        this
    }
}

impl<T: InteractiveElement + ParentElement> ToolTooltipExt for T {}
