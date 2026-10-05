use super::{
    CapturePoint, DisplaySnapshot, WindowTarget,
    selection::{CaptureSelection, SelectionState, intersect_bounds},
};
use crate::{dimensions::DimensionMode, selection::paint_dimension_label, theme};
use gpui::{prelude::*, *};
use lyon_path::LineCap;
use std::sync::Arc;

pub enum CaptureOverlayEvent {
    Selected(CaptureSelection),
    Cancelled,
}

pub struct CaptureOverlay {
    display: DisplaySnapshot,
    image: Arc<RenderImage>,
    selection: SelectionState,
    pointer: Option<Point<Pixels>>,
    focus: FocusHandle,
    completed: bool,
}

impl EventEmitter<CaptureOverlayEvent> for CaptureOverlay {}

impl CaptureOverlay {
    pub fn new(
        display: DisplaySnapshot,
        image: Arc<RenderImage>,
        windows: Arc<Vec<WindowTarget>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        window
            .observe_release(&cx.entity(), cx, |this, window, cx| {
                cx.drop_image(this.image.clone(), Some(window));
            })
            .detach();
        let pointer = window.mouse_position();
        let mut selection = SelectionState::new(display.id, display.bounds, windows);
        selection.set_ui_scale(display.ui_scale);
        selection.set_grid(&display, DimensionMode::current(cx));
        selection.pointer_move(global_point(pointer, &display));
        Self {
            display,
            image,
            selection,
            pointer: Some(pointer),
            focus,
            completed: false,
        }
    }

    fn pointer_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.completed {
            return;
        }
        self.focus.focus(window, cx);
        self.pointer = Some(event.position);
        self.selection
            .pointer_down(global_point(event.position, &self.display));
        cx.stop_propagation();
        cx.notify();
    }

    fn pointer_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.completed {
            return;
        }
        if self.selection.is_pressed() && !event.dragging() {
            self.selection.cancel_gesture();
        }
        self.pointer = Some(event.position);
        self.selection
            .pointer_move(global_point(event.position, &self.display));
        cx.notify();
    }

    fn pointer_up(&mut self, event: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.completed || event.button != MouseButton::Left {
            return;
        }
        self.pointer = Some(event.position);
        if let Some(selection) = self
            .selection
            .pointer_up(global_point(event.position, &self.display))
        {
            self.completed = true;
            cx.emit(CaptureOverlayEvent::Selected(selection));
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        if self.completed {
            return;
        }
        self.completed = true;
        cx.emit(CaptureOverlayEvent::Cancelled);
        cx.stop_propagation();
    }
}

impl Render for CaptureOverlay {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let image = self.image.clone();
        let display_bounds = self.display.bounds;
        let display = self.display.clone();
        let selection = self
            .selection
            .selected_bounds()
            .and_then(|bounds| intersect_bounds(bounds, display_bounds));
        let label = self
            .selection
            .size_label(&self.display, DimensionMode::current(cx))
            .map(|mut label| {
                if let Some(target) = self
                    .selection
                    .selected_window()
                    .filter(|target| !target.capture_from_display)
                {
                    let app_name = target
                        .app_name
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ");
                    if !app_name.is_empty() {
                        label.push(' ');
                        label.push_str(&app_name);
                    }
                }
                label
            });
        let guide_pointer = if self.completed { None } else { self.pointer };
        let mouse_move = cx.listener(Self::pointer_move);
        let mouse_up = cx.listener(Self::pointer_up);

        div()
            .id("capture-overlay")
            .size_full()
            .overflow_hidden()
            .track_focus(&self.focus)
            .key_context("CaptureOverlay")
            .cursor(CursorStyle::Crosshair)
            .on_mouse_down(MouseButton::Left, cx.listener(Self::pointer_down))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _, _, cx| this.cancel(cx)),
            )
            .on_mouse_exit(cx.listener(|this, _, _, cx| {
                this.selection.pointer_exit();
                this.pointer = None;
                cx.notify();
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    this.cancel(cx);
                }
            }))
            .child(
                canvas(
                    |bounds, _, _| bounds,
                    move |_, bounds, window, cx| {
                        if let Err(error) =
                            window.paint_image(bounds, bounds, Corners::default(), image, 0, false)
                        {
                            eprintln!("Failed to display captured screen: {error}");
                        }
                        let selected = selection.map(|selected| {
                            let selected = display.local_bounds(selected);
                            Bounds::new(
                                bounds.origin + point(px(selected.x as f32), px(selected.y as f32)),
                                size(px(selected.width as f32), px(selected.height as f32)),
                            )
                        });
                        paint_dimming(bounds, selected, window);
                        if let Some(pointer) = guide_pointer
                            && bounds.contains(&pointer)
                        {
                            paint_guides(bounds, pointer, window);
                        }
                        if let Some(selected) = selected {
                            let mut frame =
                                outline(selected, rgb(theme::BRAND), BorderStyle::Solid);
                            frame.border_widths = px(1.0).into();
                            window.paint_quad(frame);
                            if let Some(label) = label {
                                paint_dimension_label(label, selected, bounds, window, cx);
                            }
                        }
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                            if phase == DispatchPhase::Bubble {
                                mouse_move(event, window, cx);
                            }
                        });
                        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                            if phase == DispatchPhase::Bubble {
                                mouse_up(event, window, cx);
                            }
                        });
                    },
                )
                .size_full(),
            )
    }
}

fn global_point(point: Point<Pixels>, display: &DisplaySnapshot) -> CapturePoint {
    display.capture_point(f64::from(f32::from(point.x)), f64::from(f32::from(point.y)))
}

fn paint_guides(bounds: Bounds<Pixels>, pointer: Point<Pixels>, window: &mut Window) {
    let mut path = PathBuilder::stroke(px(1.0));
    if let PathStyle::Stroke(options) = &mut path.style {
        *options = options.with_line_cap(LineCap::Round);
    }
    let dash = 3.0;
    let period = dash * 1.75;
    let first_dash = |edge: f32, cursor: f32| {
        let center = cursor - dash / 2.0;
        center + ((edge - center) / period).floor() * period
    };
    let left = f32::from(bounds.left());
    let right = f32::from(bounds.right());
    let mut x = first_dash(left, f32::from(pointer.x));
    while x < right {
        let start = x.max(left);
        let end = (x + dash).min(right);
        if start < end {
            path.move_to(point(px(start), pointer.y));
            path.line_to(point(px(end), pointer.y));
        }
        x += period;
    }
    let top = f32::from(bounds.top());
    let bottom = f32::from(bounds.bottom());
    let mut y = first_dash(top, f32::from(pointer.y));
    while y < bottom {
        let start = y.max(top);
        let end = (y + dash).min(bottom);
        if start < end {
            path.move_to(point(pointer.x, px(start)));
            path.line_to(point(pointer.x, px(end)));
        }
        y += period;
    }
    if let Ok(path) = path.build() {
        window.paint_path(
            path,
            theme::brand(theme::ALIGNMENT_GUIDE_ALPHA as f32 / 255.0),
        );
    }
}

pub(super) fn paint_dimming(
    bounds: Bounds<Pixels>,
    selected: Option<Bounds<Pixels>>,
    window: &mut Window,
) {
    let rectangles = if let Some(selected) = selected {
        vec![
            Bounds::from_corners(bounds.origin, point(bounds.right(), selected.top())),
            Bounds::from_corners(
                point(bounds.left(), selected.bottom()),
                bounds.bottom_right(),
            ),
            Bounds::from_corners(
                point(bounds.left(), selected.top()),
                point(selected.left(), selected.bottom()),
            ),
            Bounds::from_corners(
                point(selected.right(), selected.top()),
                point(bounds.right(), selected.bottom()),
            ),
        ]
    } else {
        vec![bounds]
    };
    for rectangle in rectangles {
        if rectangle.size.width > px(0.0) && rectangle.size.height > px(0.0) {
            window.paint_quad(fill(rectangle, rgba(0x00000066)));
        }
    }
}
