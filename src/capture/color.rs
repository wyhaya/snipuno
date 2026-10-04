use super::{DisplaySnapshot, overlay::paint_dimming};
use crate::{color_format::ColorFormat, pixel_loupe};
use gpui::{prelude::*, *};
use image::Rgba;
use std::sync::Arc;

pub enum ColorOverlayEvent {
    Picked(String),
    Cancelled,
}

pub struct ColorOverlay {
    display: DisplaySnapshot,
    image: Arc<RenderImage>,
    pointer: Option<Point<Pixels>>,
    focus: FocusHandle,
    completed: bool,
    _color_subscription: Subscription,
}

impl EventEmitter<ColorOverlayEvent> for ColorOverlay {}

impl ColorOverlay {
    pub fn new(
        display: DisplaySnapshot,
        image: Arc<RenderImage>,
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
        let color_subscription = cx.observe_global::<ColorFormat>(|_, cx| cx.notify());
        Self {
            display,
            image,
            pointer: Some(window.mouse_position()),
            focus,
            completed: false,
            _color_subscription: color_subscription,
        }
    }

    fn move_pointer(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if !self.completed {
            self.pointer = Some(event.position);
            cx.notify();
        }
    }

    fn pick(&mut self, event: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.completed {
            return;
        }
        self.pointer = Some(event.position);
        if let Some((x, y)) = pixel_position(&self.display, event.position) {
            let Rgba([red, green, blue, _]) = *self.display.pixels.get_pixel(x, y);
            self.completed = true;
            let color = u32::from_be_bytes([0, red, green, blue]);
            cx.emit(ColorOverlayEvent::Picked(
                ColorFormat::current(cx).format(color),
            ));
        }
        cx.stop_propagation();
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        if !self.completed {
            self.completed = true;
            cx.emit(ColorOverlayEvent::Cancelled);
        }
        cx.stop_propagation();
    }
}

impl Render for ColorOverlay {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let image = self.image.clone();
        let display = self.display.clone();
        let pointer = self.pointer;
        let mouse_move = cx.listener(Self::move_pointer);
        div()
            .id("color-overlay")
            .size_full()
            .overflow_hidden()
            .track_focus(&self.focus)
            .key_context("CaptureOverlay")
            .cursor(CursorStyle::Crosshair)
            .on_mouse_down(MouseButton::Left, cx.listener(Self::pick))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _, _, cx| this.cancel(cx)),
            )
            .on_mouse_exit(cx.listener(|this, _, _, cx| {
                this.pointer = None;
                cx.notify();
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    this.cancel(cx);
                } else if !this.completed {
                    ColorFormat::handle_key_down(event, cx);
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
                        paint_dimming(bounds, None, window);
                        if let Some(pointer) = pointer
                            && bounds.contains(&pointer)
                            && let Some((x, y)) = pixel_position(&display, pointer)
                        {
                            paint_magnifier(&display, (x, y), pointer, bounds, window, cx);
                        }
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                            if phase == DispatchPhase::Bubble {
                                mouse_move(event, window, cx);
                            }
                        });
                    },
                )
                .size_full(),
            )
    }
}

fn pixel_position(display: &DisplaySnapshot, pointer: Point<Pixels>) -> Option<(u32, u32)> {
    if display.pixels.width() == 0
        || display.pixels.height() == 0
        || display.bounds.width <= 0.0
        || display.bounds.height <= 0.0
        || display.ui_scale <= 0.0
    {
        return None;
    }
    let point = display.capture_point(
        f64::from(f32::from(pointer.x)),
        f64::from(f32::from(pointer.y)),
    );
    let x = ((point.x - display.bounds.x) * f64::from(display.pixels.width())
        / display.bounds.width)
        .floor();
    let y = ((point.y - display.bounds.y) * f64::from(display.pixels.height())
        / display.bounds.height)
        .floor();
    if x < 0.0
        || y < 0.0
        || x >= f64::from(display.pixels.width())
        || y >= f64::from(display.pixels.height())
    {
        None
    } else {
        Some((x as u32, y as u32))
    }
}

fn paint_magnifier(
    display: &DisplaySnapshot,
    center: (u32, u32),
    pointer: Point<Pixels>,
    available: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let pixel = display.pixels.get_pixel(center.0, center.1);
    let label =
        ColorFormat::current(cx).format(u32::from_be_bytes([0, pixel[0], pixel[1], pixel[2]]));
    pixel_loupe::paint(
        pointer,
        available,
        label,
        |dx, dy| {
            let x = (i64::from(center.0) + i64::from(dx))
                .clamp(0, i64::from(display.pixels.width() - 1)) as u32;
            let y = (i64::from(center.1) + i64::from(dy))
                .clamp(0, i64::from(display.pixels.height() - 1)) as u32;
            let pixel = display.pixels.get_pixel(x, y);
            u32::from_be_bytes([0, pixel[0], pixel[1], pixel[2]])
        },
        window,
        cx,
    );
}
