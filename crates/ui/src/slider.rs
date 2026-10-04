use gpui::{prelude::*, *};
use gpui_base::{
    SliderIndicator, SliderThumb, SliderTrack, Theme,
    input::Escape,
    slider::{SliderEvent as BaseSliderEvent, SliderState},
};
use std::ops::RangeInclusive;

const TRACK_WIDTH: f32 = 92.;
const TRACK_HEIGHT: f32 = 12.;
const THUMB_SIZE: f32 = 16.;

pub enum SliderEvent {
    Change(f32),
    Release,
    Cancel,
}

pub struct Slider {
    state: Entity<SliderState>,
    focus: FocusHandle,
    label: SharedString,
    drag_start: Option<f32>,
    _subscription: Subscription,
}

impl EventEmitter<SliderEvent> for Slider {}

impl Slider {
    pub fn new(
        label: impl Into<SharedString>,
        range: RangeInclusive<f32>,
        value: f32,
        step: f32,
        cx: &mut Context<Self>,
    ) -> Self {
        let (min, max) = range.into_inner();
        let value = (min + ((value - min) / step).round() * step).clamp(min, max);
        let state = cx.new(|_| {
            SliderState::new()
                .min(min)
                .max(max)
                .step(step)
                .default_value(value)
        });
        let subscription = cx.subscribe(&state, |this, _, event, cx| {
            match event {
                BaseSliderEvent::Change(value) => cx.emit(SliderEvent::Change(value.end())),
                BaseSliderEvent::Release(_) => {
                    this.drag_start = None;
                    cx.emit(SliderEvent::Release);
                }
            }
            cx.notify();
        });
        Self {
            state,
            focus: cx.focus_handle(),
            label: label.into(),
            drag_start: None,
            _subscription: subscription,
        }
    }

    pub fn sync(
        &mut self,
        label: impl Into<SharedString>,
        range: RangeInclusive<f32>,
        value: f32,
        step: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (min, max) = range.into_inner();
        let value = (min + ((value - min) / step).round() * step).clamp(min, max);
        let label = label.into();
        if self.label != label {
            self.label = label;
            cx.notify();
        }
        let current = self.state.read(cx);
        let range_changed = current.min_value() != min
            || current.max_value() != max
            || current.step_value() != step;
        if range_changed || (current.value().end() - value).abs() > 0.001 {
            self.state.update(cx, |state, cx| {
                if range_changed {
                    *state = SliderState::new()
                        .min(min)
                        .max(max)
                        .step(step)
                        .default_value(value);
                    cx.notify();
                } else {
                    state.set_value(value, window, cx);
                }
            });
            cx.notify();
        }
    }

    pub fn increment(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let state = self.state.read(cx);
        self.set_from_keyboard(state.value().end() + state.step_value(), window, cx);
    }

    pub fn decrement(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let state = self.state.read(cx);
        self.set_from_keyboard(state.value().end() - state.step_value(), window, cx);
    }

    fn set_from_keyboard(&mut self, value: f32, window: &mut Window, cx: &mut Context<Self>) {
        let state = self.state.read(cx);
        let min = state.min_value();
        let step = state.step_value();
        let value = (min + ((value - min) / step).round() * step).clamp(min, state.max_value());
        if value == state.value().end() {
            return;
        }
        self.state
            .update(cx, |state, cx| state.set_value(value, window, cx));
        cx.emit(SliderEvent::Change(value));
        cx.emit(SliderEvent::Release);
        cx.notify();
    }
}

impl Render for Slider {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = Theme::global(cx).tokens.colors;
        let state = self.state.read(cx);
        let value = state.value().end();
        let min = state.min_value();
        let max = state.max_value();
        // The base state keeps the raw pointer percentage while dragging.
        let percentage = (value - min) / (max - min);
        let view = cx.entity().downgrade();
        let track = |color: Hsla| {
            svg()
                .data(include_bytes!("../assets/slider-track.svg"))
                .w(px(TRACK_WIDTH))
                .h(px(TRACK_HEIGHT))
                .flex_shrink_0()
                .text_color(color)
        };
        div()
            .id("slider")
            .track_focus(&self.focus)
            .aria_label(self.label.clone())
            .role(Role::Slider)
            .aria_numeric_value(f64::from(value))
            .aria_min_numeric_value(f64::from(min))
            .aria_max_numeric_value(f64::from(max))
            .aria_numeric_value_step(f64::from(state.step_value()))
            .aria_orientation(Orientation::Horizontal)
            .on_a11y_action(AccessibleAction::Increment, {
                let view = view.clone();
                move |_, window, cx| {
                    let _ = view.update(cx, |this, cx| {
                        this.increment(window, cx);
                    });
                }
            })
            .on_a11y_action(AccessibleAction::Decrement, move |_, window, cx| {
                let _ = view.update(cx, |this, cx| {
                    this.decrement(window, cx);
                });
            })
            .h_7()
            .w(px(TRACK_WIDTH + THUMB_SIZE))
            .px(px(THUMB_SIZE / 2.))
            .flex()
            .items_center()
            .flex_shrink_0()
            .rounded_md()
            .focus_visible(|style| style.bg(colors.primary.opacity(0.08)))
            .capture_any_mouse_down(cx.listener(|this, event: &MouseDownEvent, window, cx| {
                if event.button == MouseButton::Left {
                    this.drag_start = Some(this.state.read(cx).value().end());
                    this.focus.focus(window, cx);
                }
            }))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_action(cx.listener(|this, _: &Escape, window, cx| {
                if let Some(value) = this.drag_start.take() {
                    cx.stop_active_drag(window);
                    this.state.update(cx, |state, cx| {
                        state.set_value(value, window, cx);
                    });
                    cx.emit(SliderEvent::Change(value));
                }
                cx.emit(SliderEvent::Cancel);
                cx.stop_propagation();
                cx.notify();
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                match event.keystroke.key.as_str() {
                    "left" | "down" => this.decrement(window, cx),
                    "right" | "up" => this.increment(window, cx),
                    "home" => this.set_from_keyboard(this.state.read(cx).min_value(), window, cx),
                    "end" => this.set_from_keyboard(this.state.read(cx).max_value(), window, cx),
                    _ => return,
                }
                cx.stop_propagation();
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.drag_start = None;
                    this.state.update(cx, |state, cx| state.handle_release(cx));
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.drag_start = None;
                    this.state.update(cx, |state, cx| state.handle_release(cx));
                }),
            )
            .child(
                SliderTrack::new(&self.state)
                    .w_full()
                    .h_full()
                    .flex()
                    .items_center()
                    .child(
                        SliderIndicator::new(&self.state)
                            .relative()
                            .w_full()
                            .h(px(TRACK_HEIGHT))
                            .child(track(colors.muted))
                            .child(
                                div()
                                    .absolute()
                                    .top_0()
                                    .left_0()
                                    .w(relative(percentage))
                                    .h_full()
                                    .overflow_hidden()
                                    .child(track(colors.primary.opacity(0.9))),
                            )
                            .child(
                                SliderThumb::new(&self.state)
                                    .absolute()
                                    .top(px((TRACK_HEIGHT - THUMB_SIZE) / 2.))
                                    .left(relative(percentage))
                                    .ml(px(-THUMB_SIZE / 2.))
                                    .size(px(THUMB_SIZE))
                                    .rounded_full()
                                    .bg(colors.primary)
                                    .shadow_sm()
                                    .hover(|style| {
                                        style.bg(colors.primary.blend(black().opacity(0.04)))
                                    })
                                    .active(|style| {
                                        style.bg(colors.primary.blend(black().opacity(0.1)))
                                    }),
                            ),
                    ),
            )
    }
}
