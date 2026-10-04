use super::{ScreenshotState, show_error};
use crate::{
    capture, clipboard,
    desktop::{self, WindowRole},
    dimensions::DimensionMode,
    theme,
};
use gpui::{prelude::*, *};
use gpui_base::{Theme, Toolbar};
use image::RgbaImage;
use std::{cell::RefCell, rc::Rc, sync::Arc};
use ui::{Button, Icon, Root, ToolTooltip, ToolTooltipExt, theme::observe_window, window_controls};

struct PinnedScreenshot {
    screenshot: Rc<RefCell<ScreenshotState>>,
    image: Arc<RenderImage>,
    pixels: Arc<RgbaImage>,
    logical_size: (f64, f64),
    focus: FocusHandle,
    hovered: bool,
    drag_pending: bool,
    _subscriptions: Vec<Subscription>,
}

pub(super) fn open(
    screenshot: Rc<RefCell<ScreenshotState>>,
    image: Arc<RenderImage>,
    pixels: Arc<RgbaImage>,
    editor_window: &Window,
    cx: &mut App,
) -> Result<(), String> {
    let image_size = image.size(0);
    let dimensions = (image_size.width.0 as u32, image_size.height.0 as u32);
    let (title, logical_size) = {
        let state = screenshot.borrow();
        let display = state.source.pixel_scale.display(DimensionMode::Logical);
        let width = display.width(f64::from(dimensions.0));
        (
            state.title.clone(),
            (
                width,
                width * f64::from(dimensions.1) / f64::from(dimensions.0),
            ),
        )
    };
    let display = editor_window.display(cx);
    let available = display
        .as_ref()
        .map_or(editor_window.bounds().size, |display| display.bounds().size);
    let scale = (f64::from(f32::from(available.width)) * 0.8 / logical_size.0)
        .min(f64::from(f32::from(available.height)) * 0.8 / logical_size.1)
        .min(1.0);
    let bounds = Bounds::new(
        editor_window.bounds().origin,
        size(
            px((logical_size.0 * scale) as f32),
            px((logical_size.1 * scale) as f32),
        ),
    );
    let mut pinned = None;
    let handle = cx
        .open_window(
            WindowOptions {
                kind: WindowKind::Floating,
                titlebar: Some(TitlebarOptions {
                    title: Some(title.into()),
                    appears_transparent: true,
                    ..Default::default()
                }),
                app_owns_titlebar_drag: true,
                display_id: display.map(|display| display.id()),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                is_minimizable: cfg!(target_os = "windows"),
                focus: false,
                show: false,
                ..Default::default()
            },
            |window, cx| {
                #[cfg(target_os = "macos")]
                if let Err(error) = desktop::center_window_controls(window, theme::TOOLBAR_HEIGHT) {
                    eprintln!("Unable to position window controls: {error}");
                }
                observe_window(window, cx);
                let view = cx.new(|cx| {
                    let focus = cx.focus_handle();
                    focus.focus(window, cx);
                    let activation = cx.observe_window_activation(
                        window,
                        |this: &mut PinnedScreenshot, window, cx| {
                            if window.is_window_active() {
                                #[cfg(target_os = "windows")]
                                this.focus.focus(window, cx);
                                desktop::window_activated(window.window_handle(), cx);
                            }
                            this.sync_hover(window, cx);
                        },
                    );
                    let bounds = cx.observe_window_bounds(
                        window,
                        |this: &mut PinnedScreenshot, window, cx| {
                            this.sync_hover(window, cx);
                            cx.notify();
                        },
                    );
                    let visibility = cx.observe_window_visibility(
                        window,
                        |this: &mut PinnedScreenshot, _, window, cx| {
                            this.sync_hover(window, cx);
                        },
                    );
                    window
                        .observe_release(
                            &cx.entity(),
                            cx,
                            |this: &mut PinnedScreenshot, window, cx| {
                                let mut state = this.screenshot.borrow_mut();
                                if state
                                    .window
                                    .is_some_and(|(handle, _)| handle == window.window_handle())
                                {
                                    state.window = None;
                                }
                                cx.drop_image(this.image.clone(), Some(window));
                            },
                        )
                        .detach();
                    PinnedScreenshot {
                        screenshot: screenshot.clone(),
                        image,
                        pixels,
                        logical_size,
                        focus,
                        hovered: false,
                        drag_pending: false,
                        _subscriptions: vec![activation, bounds, visibility],
                    }
                });
                pinned = Some(view.clone());
                cx.new(|cx| Root::new(view, window, cx))
            },
        )
        .map_err(|error| format!("Unable to open pinned screenshot: {error}"))?;
    let configured = handle
        .update(cx, |_, window, cx| {
            desktop::configure_pinned_window(window, logical_size)?;
            window.bounds_changed(cx);
            if let Some(pinned) = pinned.as_ref() {
                pinned.update(cx, |this, cx| this.sync_hover(window, cx));
            }
            Ok::<_, String>(())
        })
        .map_err(|error| error.to_string())
        .and_then(|result| result);
    if let Err(error) = configured {
        let _ = handle.update(cx, |_, window, _| window.remove_window());
        return Err(error);
    }
    screenshot.borrow_mut().window = Some((handle.into(), WindowRole::Pinned));
    desktop::track_window(handle.into(), WindowRole::Pinned, cx);
    Ok(())
}

impl PinnedScreenshot {
    fn update_hover(&mut self, window: &Window) -> bool {
        let hovered = desktop::pinned_window_contains_pointer(window);
        if self.hovered == hovered {
            return false;
        }
        #[cfg(target_os = "macos")]
        desktop::set_pinned_controls_visible(window, hovered);
        self.hovered = hovered;
        if !hovered {
            self.drag_pending = false;
        }
        true
    }

    fn sync_hover(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.update_hover(window) {
            cx.notify();
        }
    }

    fn copy_image(&self, window: &mut Window, cx: &mut Context<Self>) {
        clipboard::write_image(self.pixels.clone(), cx).detach();
        window.remove_window();
    }

    fn edit(&mut self, window: &Window, cx: &mut Context<Self>) {
        self.drag_pending = false;
        let screenshot = self.screenshot.clone();
        let previous = window.window_handle();
        cx.defer(move |cx| match capture::open_editor(screenshot, cx) {
            Ok(handle) => {
                desktop::activate_window(handle, cx);
            }
            Err(error) => {
                let _ = previous.update(cx, |_, window, cx| {
                    show_error("Unable to edit screenshot", &error, window, cx);
                });
            }
        });
    }

    fn scroll(&mut self, event: &ScrollWheelEvent, window: &Window, cx: &mut Context<Self>) {
        let delta = f64::from(f32::from(event.delta.pixel_delta(px(24.0)).y));
        if delta.is_finite() && delta != 0.0 {
            let factor = (delta.clamp(-120.0, 120.0) * 0.0025).exp();
            if let Err(error) = desktop::resize_pinned_window(window, self.logical_size, factor, cx)
            {
                eprintln!("Unable to resize pinned screenshot: {error}");
            }
        }
        cx.stop_propagation();
    }

    fn move_window(&mut self, event: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_hover(window, cx);
        if self.drag_pending && event.dragging() {
            self.drag_pending = false;
            desktop::start_window_move(window, cx);
        }
    }

    fn toolbar(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = Theme::global(cx).tokens.colors;
        let zoom = f64::from(f32::from(window.viewport_size().width)) / self.logical_size.0;
        Toolbar::new("pinned-toolbar")
            .absolute()
            .top_0()
            .left_0()
            .w_full()
            .flex()
            .items_center()
            .gap(px(theme::OPTIONS_PANEL_PADDING))
            .h(px(theme::TOOLBAR_HEIGHT))
            .px_3()
            .when(cfg!(target_os = "windows"), |this| this.pr_0())
            .map(|this| {
                #[cfg(target_os = "macos")]
                {
                    this.pl(desktop::titlebar_content_start(window))
                }
                #[cfg(not(target_os = "macos"))]
                {
                    this
                }
            })
            .border_b(px(theme::OPTIONS_PANEL_BORDER_WIDTH))
            .border_color(colors.border)
            .bg(colors.surface)
            .occlude()
            .aria_label("Pinned screenshot")
            .hover_listener_mode(HoverListenerMode::InputModalityIndependent)
            .on_hover(cx.listener(|this, _: &bool, window, cx| this.sync_hover(window, cx)))
            .on_any_mouse_down(cx.listener(|this, event: &MouseDownEvent, window, cx| {
                #[cfg(target_os = "windows")]
                if event.position.x >= window.viewport_size().width - px(ui::WINDOW_CONTROLS_WIDTH)
                {
                    this.drag_pending = false;
                    return;
                }
                this.drag_pending =
                    event.button == MouseButton::Left && !window.default_prevented();
                window.prevent_default();
                cx.stop_propagation();
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| {
                    this.drag_pending = false;
                }),
            )
            .on_mouse_move(
                cx.listener(|this, event, window, cx| this.move_window(event, window, cx)),
            )
            .on_scroll_wheel(cx.listener(|this, event, window, cx| this.scroll(event, window, cx)))
            .child(
                Button::icon("edit-image", Icon::ReturnToEditor)
                    .accessibility_label("Return to editor")
                    .tool_tooltip(
                        ToolTooltip::new("Return to editor")
                            .description("Continue editing this screenshot."),
                    )
                    .on_click(cx.listener(|this, _, window, cx| this.edit(window, cx))),
            )
            .child(
                Button::icon("copy-image", Icon::Copy)
                    .accessibility_label("Copy image")
                    .tool_tooltip(
                        ToolTooltip::new("Copy image")
                            .description("Copy the annotated image and close this window."),
                    )
                    .on_click(cx.listener(|this, _, window, cx| this.copy_image(window, cx))),
            )
            .child(
                div()
                    .min_w(px(48.0))
                    .text_sm()
                    .text_center()
                    .text_color(colors.foreground)
                    .child(format!("{:.0}%", zoom * 100.0)),
            )
            .when(cfg!(target_os = "windows"), |this| {
                this.child(div().flex_1())
                    .child(window_controls(window, cx))
            })
    }
}

impl Render for PinnedScreenshot {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Windows reports mouse leave as a window refresh without an element exit event.
        #[cfg(target_os = "windows")]
        self.update_hover(window);

        div()
            .id("pinned-screenshot")
            .track_focus(&self.focus)
            .key_context("PinnedScreenshot")
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(Theme::global(cx).tokens.colors.background)
            .on_action(|_: &desktop::CloseWindow, window, _| window.remove_window())
            .hover_listener_mode(HoverListenerMode::InputModalityIndependent)
            .on_hover(cx.listener(|this, _: &bool, window, cx| this.sync_hover(window, cx)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    #[cfg(target_os = "windows")]
                    if this.hovered
                        && event.position.y < px(theme::TOOLBAR_HEIGHT)
                        && event.position.x
                            >= window.viewport_size().width - px(ui::WINDOW_CONTROLS_WIDTH)
                    {
                        this.drag_pending = false;
                        return;
                    }
                    if event.click_count == 2 {
                        this.edit(window, cx);
                    } else {
                        this.drag_pending = true;
                    }
                    window.prevent_default();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| {
                    this.drag_pending = false;
                }),
            )
            .on_mouse_move(
                cx.listener(|this, event, window, cx| this.move_window(event, window, cx)),
            )
            .on_mouse_exit(cx.listener(|this, _, window, cx| this.sync_hover(window, cx)))
            .on_scroll_wheel(cx.listener(|this, event, window, cx| this.scroll(event, window, cx)))
            .child(
                img(self.image.clone())
                    .size_full()
                    .object_fit(ObjectFit::Cover),
            )
            .when(self.hovered, |this| this.child(self.toolbar(window, cx)))
    }
}
