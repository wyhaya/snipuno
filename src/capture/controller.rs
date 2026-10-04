#[cfg(not(target_os = "linux"))]
use super::CaptureWindowFilter;
use super::{
    CapturePoint, CaptureSnapshot, CapturedImage,
    color::{ColorOverlay, ColorOverlayEvent},
    overlay::{CaptureOverlay, CaptureOverlayEvent},
    placement::{CapturePlacement, MIN_EDITOR_HEIGHT, MIN_EDITOR_WIDTH, minimum_size},
    platform,
    selection::{CaptureSelection, crop_region, fullscreen_selection},
};
#[cfg(not(target_os = "linux"))]
use crate::desktop::COLOR_PICKER_SHORTCUT;
use crate::{
    app::{ScreenshotEditor, ScreenshotState},
    desktop::{self, SoundEffect, WindowRole},
    image_processing::{SourceImage, render_image},
};
use futures_lite::future::race;
use gpui::{prelude::*, *};
use std::{cell::RefCell, rc::Rc, sync::Arc, time::Duration};
use ui::{Root, theme::observe_window};

actions!(
    capture,
    [
        NewScreenshot,
        TrayScreenshot,
        FullscreenScreenshot,
        TrayFullscreenScreenshot,
        PickColor,
        TrayPickColor,
        ReopenLastScreenshot,
        CancelCapture
    ]
);

#[derive(Clone, Copy, PartialEq)]
enum Stage {
    Idle,
    Preparing,
    Selecting,
    Finalizing,
}

#[derive(Clone, Copy, PartialEq)]
enum CaptureMode {
    Screenshot,
    Fullscreen,
    Color,
}

struct CaptureGlobal(Entity<CaptureController>);
impl Global for CaptureGlobal {}

struct OverlayRoot(AnyView);

impl Render for OverlayRoot {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.0.clone()
    }
}

struct CaptureController {
    stage: Stage,
    mode: CaptureMode,
    session: u64,
    snapshot: Option<Arc<CaptureSnapshot>>,
    overlays: Vec<AnyWindowHandle>,
    last_screenshot: Option<Rc<RefCell<ScreenshotState>>>,
    subscriptions: Vec<Subscription>,
    capture_task: Option<Task<()>>,
    desktop_context: Option<desktop::CaptureContext>,
}

pub fn install(cx: &mut App) {
    cx.set_quit_mode(QuitMode::Explicit);
    let controller = cx.new(|_| CaptureController {
        stage: Stage::Idle,
        mode: CaptureMode::Screenshot,
        session: 0,
        snapshot: None,
        overlays: Vec::new(),
        last_screenshot: None,
        subscriptions: Vec::new(),
        capture_task: None,
        desktop_context: None,
    });
    cx.set_global(CaptureGlobal(controller.clone()));
    cx.on_action({
        let controller = controller.clone();
        move |_: &NewScreenshot, cx| {
            let controller = controller.clone();
            cx.defer(move |cx| {
                controller.update(cx, |this, cx| {
                    this.begin_mode(CaptureMode::Screenshot, false, cx)
                })
            });
        }
    });
    cx.on_action({
        let controller = controller.clone();
        move |_: &TrayScreenshot, cx| {
            let controller = controller.clone();
            cx.defer(move |cx| {
                controller.update(cx, |this, cx| {
                    this.begin_mode(CaptureMode::Screenshot, true, cx);
                })
            });
        }
    });
    cx.on_action({
        let controller = controller.clone();
        move |_: &FullscreenScreenshot, cx| {
            let controller = controller.clone();
            cx.defer(move |cx| {
                controller.update(cx, |this, cx| {
                    this.begin_mode(CaptureMode::Fullscreen, false, cx)
                })
            });
        }
    });
    cx.on_action({
        let controller = controller.clone();
        move |_: &TrayFullscreenScreenshot, cx| {
            let controller = controller.clone();
            cx.defer(move |cx| {
                controller.update(cx, |this, cx| {
                    this.begin_mode(CaptureMode::Fullscreen, true, cx)
                })
            });
        }
    });
    cx.on_action({
        let controller = controller.clone();
        move |_: &PickColor, cx| {
            let controller = controller.clone();
            cx.defer(move |cx| {
                controller.update(cx, |this, cx| {
                    this.begin_mode(CaptureMode::Color, false, cx)
                })
            });
        }
    });
    cx.on_action({
        let controller = controller.clone();
        move |_: &TrayPickColor, cx| {
            let controller = controller.clone();
            cx.defer(move |cx| {
                controller.update(cx, |this, cx| this.begin_mode(CaptureMode::Color, true, cx))
            });
        }
    });
    cx.on_action({
        let controller = controller.clone();
        move |_: &ReopenLastScreenshot, cx| {
            let controller = controller.clone();
            cx.defer(move |cx| controller.update(cx, |this, cx| this.reopen_editor(cx)));
        }
    });
    cx.on_action({
        let controller = controller.clone();
        move |_: &CancelCapture, cx| {
            let controller = controller.clone();
            cx.defer(move |cx| {
                controller.update(cx, |this, cx| {
                    if this.stage != Stage::Idle {
                        this.finish(None, cx);
                    }
                });
            });
        }
    });
    cx.bind_keys([KeyBinding::new(
        "escape",
        CancelCapture,
        Some("CaptureOverlay"),
    )]);
    #[cfg(target_os = "linux")]
    cx.bind_keys([
        KeyBinding::new("ctrl-shift-1", FullscreenScreenshot, None),
        KeyBinding::new("ctrl-shift-2", NewScreenshot, None),
    ]);
    #[cfg(not(target_os = "linux"))]
    cx.bind_keys([KeyBinding::new(
        &COLOR_PICKER_SHORTCUT.keystroke(),
        PickColor,
        None,
    )]);
    cx.on_window_closed({
        let controller = controller.clone();
        move |cx, _| {
            let controller = controller.clone();
            cx.defer(move |cx| {
                controller.update(cx, |this, cx| {
                    let windows = cx.windows();
                    if matches!(this.stage, Stage::Preparing | Stage::Selecting)
                        && this.overlays.iter().any(|overlay| {
                            !windows
                                .iter()
                                .any(|window| window.window_id() == overlay.window_id())
                        })
                    {
                        this.finish(None, cx);
                    }
                });
            });
        }
    })
    .detach();
    cx.on_app_quit(move |cx| {
        controller.update(cx, |this, cx| {
            this.session = this.session.wrapping_add(1);
            this.capture_task = None;
            this.close_overlays(cx);
            this.desktop_context = None;
        });
        async {}
    })
    .detach();
}

impl CaptureController {
    fn begin_mode(&mut self, mode: CaptureMode, from_tray: bool, cx: &mut Context<Self>) {
        if self.stage != Stage::Idle || !desktop::can_capture(cx) {
            return;
        }
        self.mode = mode;
        #[cfg(target_os = "linux")]
        self.begin_portal(from_tray, cx);
        #[cfg(not(target_os = "linux"))]
        self.begin(from_tray, cx);
    }

    #[cfg(target_os = "linux")]
    fn begin(&mut self, cx: &mut Context<Self>) {
        self.begin_portal(false, cx);
    }

    #[cfg(target_os = "linux")]
    fn begin_portal(&mut self, from_tray: bool, cx: &mut Context<Self>) {
        if self.stage != Stage::Idle || !desktop::can_capture(cx) {
            return;
        }
        self.session = self.session.wrapping_add(1);
        self.stage = Stage::Preparing;
        desktop::capture_started(cx);
        let session = self.session;
        self.capture_task = Some(cx.spawn(async move |this, cx| {
            // TODO: D-Bus Menu has a `closed` event, but ksni currently ignores it.
            // Use it when exposed, retaining a fallback and allowing for host exit animations.
            if from_tray {
                cx.background_executor()
                    .timer(Duration::from_millis(300))
                    .await;
            }
            let result = cx
                .background_executor()
                .spawn(async {
                    platform::portal_screenshot().await.map(|pixels| {
                        pixels.map(|pixels| {
                            let preview = render_image(pixels.clone());
                            (pixels, preview)
                        })
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.session != session || this.stage != Stage::Preparing {
                    return;
                }
                match result {
                    Ok(Some((pixels, preview))) => {
                        let displays = cx.displays();
                        if displays.len() > 1 {
                            this.finish(Some("Fullscreen portal selection currently supports one display. Multi-display screenshot mapping is not available yet.".into()), cx);
                            return;
                        }
                        let Some(display) = displays.into_iter().next() else {
                            this.finish(Some("No display is available for screenshot selection.".into()), cx);
                            return;
                        };
                        let bounds = display.bounds();
                        let width = f64::from(f32::from(bounds.size.width));
                        let height = f64::from(f32::from(bounds.size.height));
                        if width <= 0.0 || height <= 0.0
                            || (f64::from(pixels.width()) * height / width - f64::from(pixels.height())).abs() > 2.0
                        {
                            this.finish(Some("The portal screenshot does not match the display dimensions. Fullscreen selection cannot place it accurately.".into()), cx);
                            return;
                        }
                        let snapshot = CaptureSnapshot {
                            displays: vec![super::DisplaySnapshot {
                                id: super::CaptureDisplayId(u64::from(display.id())),
                                bounds: super::CaptureBounds {
                                    x: f64::from(f32::from(bounds.origin.x)),
                                    y: f64::from(f32::from(bounds.origin.y)),
                                    width,
                                    height,
                                },
                                ui_scale: 1.0,
                                mode_pixel_size: (
                                    pixels.width() as usize,
                                    pixels.height() as usize,
                                ),
                                pixels: Arc::new(pixels),
                            }],
                            windows: vec![],
                        };
                        this.select(snapshot, vec![preview], None, cx);
                    }
                    Ok(None) => this.finish(None, cx),
                    Err(error) => this.finish(Some(error), cx),
                }
            });
        }));
    }

    #[cfg(not(target_os = "linux"))]
    fn begin(&mut self, from_tray: bool, cx: &mut Context<Self>) {
        if self.stage != Stage::Idle || !desktop::can_capture(cx) {
            return;
        }
        self.session = self.session.wrapping_add(1);
        self.stage = Stage::Preparing;
        desktop::capture_started(cx);
        let mode = self.mode;
        let pointer = if mode == CaptureMode::Fullscreen {
            platform::pointer_position()
        } else {
            None
        };
        let mut visible_app_windows = Vec::new();
        for handle in cx.windows() {
            match handle
                .update(cx, |_, window, _| platform::visible_window_id(window))
                .map_err(|error| error.to_string())
                .and_then(|result| result)
            {
                Ok(Some(id)) => visible_app_windows.push(id),
                Ok(None) => {}
                Err(error) => {
                    self.finish(Some(error), cx);
                    return;
                }
            }
        }
        let window_filter = CaptureWindowFilter::new(visible_app_windows);
        self.desktop_context = Some(desktop::CaptureContext::new(cx));

        let session = self.session;
        self.capture_task = Some(cx.spawn(async move |this, cx| {
            if from_tray {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
            }
            let result = race(platform::snapshot(window_filter), async {
                cx.background_executor()
                    .timer(Duration::from_secs(15))
                    .await;
                Err("Screen capture took too long. Please try again.".into())
            })
            .await;
            let result = match result {
                Ok(snapshot) => Ok(cx
                    .background_executor()
                    .spawn(async move {
                        let previews = if mode == CaptureMode::Fullscreen {
                            Vec::new()
                        } else {
                            snapshot
                                .displays
                                .iter()
                                .map(|display| render_image(display.pixels.as_ref().clone()))
                                .collect()
                        };
                        (snapshot, previews)
                    })
                    .await),
                Err(error) => Err(error),
            };
            let _ = this.update(cx, |this, cx| {
                if this.session != session || this.stage != Stage::Preparing {
                    return;
                }
                match result {
                    Ok((snapshot, previews)) => this.select(snapshot, previews, pointer, cx),
                    Err(error) => this.finish(Some(error), cx),
                }
            });
        }));
    }

    fn select(
        &mut self,
        snapshot: CaptureSnapshot,
        previews: Vec<Arc<RenderImage>>,
        pointer: Option<CapturePoint>,
        cx: &mut Context<Self>,
    ) {
        if snapshot.displays.is_empty() {
            self.finish(Some("No displays are available for capture.".into()), cx);
            return;
        }
        let snapshot = Arc::new(snapshot);
        self.snapshot = Some(snapshot.clone());
        if self.mode == CaptureMode::Fullscreen {
            if let Some(selection) = fullscreen_selection(&snapshot.displays, pointer) {
                self.capture(selection, cx);
            }
            return;
        }
        let windows = Arc::new(snapshot.windows.clone());
        let session = self.session;
        let mode = self.mode;
        #[cfg(target_os = "macos")]
        let mut native_overlays = Vec::new();
        for (display, image) in snapshot.displays.iter().zip(previews) {
            let display = display.clone();
            let bounds = display.bounds;
            let ui_scale = display.ui_scale;
            let mut overlay = None;
            let mut color_overlay = None;
            let overlay_bounds = Bounds::new(
                point(
                    px((bounds.x / ui_scale) as f32),
                    px((bounds.y / ui_scale) as f32),
                ),
                size(
                    px((bounds.width / ui_scale) as f32),
                    px((bounds.height / ui_scale) as f32),
                ),
            );
            let mut open_overlay = |kind, fullscreen, cx: &mut Context<Self>| {
                cx.open_window(
                    WindowOptions {
                        kind,
                        show: cfg!(target_os = "linux"),
                        focus: cfg!(target_os = "linux"),
                        titlebar: None,
                        is_movable: false,
                        is_resizable: cfg!(target_os = "linux"),
                        is_minimizable: false,
                        app_owns_titlebar_drag: true,
                        display_id: Some(DisplayId::new(display.id.0)),
                        window_bounds: Some(if fullscreen {
                            WindowBounds::Fullscreen(overlay_bounds)
                        } else {
                            WindowBounds::Windowed(overlay_bounds)
                        }),
                        #[cfg(target_os = "linux")]
                        app_id: Some("com.snipuno.selection".into()),
                        ..Default::default()
                    },
                    |window, cx| {
                        observe_window(window, cx);
                        if mode == CaptureMode::Color {
                            let view = cx.new(|cx| {
                                ColorOverlay::new(display.clone(), image.clone(), window, cx)
                            });
                            color_overlay = Some(view.clone());
                            cx.new(|_| OverlayRoot(view.into()))
                        } else {
                            let view = cx.new(|cx| {
                                CaptureOverlay::new(
                                    display.clone(),
                                    image.clone(),
                                    windows.clone(),
                                    window,
                                    cx,
                                )
                            });
                            overlay = Some(view.clone());
                            cx.new(|_| OverlayRoot(view.into()))
                        }
                    },
                )
            };
            #[cfg(target_os = "linux")]
            let handle = {
                use gpui::layer_shell::{
                    Anchor, KeyboardInteractivity, Layer, LayerShellNotSupportedError,
                    LayerShellOptions,
                };

                let kind = WindowKind::LayerShell(LayerShellOptions {
                    namespace: "snipuno-selection".into(),
                    layer: Layer::Overlay,
                    anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
                    exclusive_zone: Some(px(-1.0)),
                    keyboard_interactivity: KeyboardInteractivity::Exclusive,
                    ..Default::default()
                });
                match open_overlay(kind, false, cx) {
                    Err(error)
                        if error
                            .downcast_ref::<LayerShellNotSupportedError>()
                            .is_some() =>
                    {
                        open_overlay(WindowKind::Normal, true, cx)
                    }
                    result => result,
                }
            };
            #[cfg(not(target_os = "linux"))]
            let handle = open_overlay(WindowKind::PopUp, false, cx);
            let handle = match handle {
                Ok(handle) => handle,
                Err(error) => {
                    self.finish(
                        Some(format!("Unable to open capture selection: {error}")),
                        cx,
                    );
                    return;
                }
            };
            self.overlays.push(handle.into());
            #[cfg(target_os = "macos")]
            {
                let prepared = handle
                    .update(cx, |_, window, cx| {
                        platform::prepare_overlay(window, bounds, cx)
                    })
                    .map_err(|error| error.to_string())
                    .and_then(|result| result);
                match prepared {
                    Ok(overlay) => native_overlays.push((handle.into(), overlay)),
                    Err(error) => {
                        self.finish(Some(error), cx);
                        return;
                    }
                }
            }
            #[cfg(not(target_os = "macos"))]
            {
                let configured = handle.update(cx, |_, window, cx| {
                    platform::configure_overlay(window, bounds, cx)
                });
                if let Err(error) = configured
                    .map_err(|error| error.to_string())
                    .and_then(|result| result)
                {
                    self.finish(Some(error), cx);
                    return;
                }
            }
            if let Some(overlay) = overlay {
                self.subscriptions
                    .push(cx.subscribe(&overlay, move |this, _, event, cx| {
                        if this.session != session || this.stage != Stage::Selecting {
                            return;
                        }
                        match event {
                            CaptureOverlayEvent::Selected(selection) => {
                                this.capture(selection.clone(), cx)
                            }
                            CaptureOverlayEvent::Cancelled => this.finish(None, cx),
                        }
                    }));
            }
            if let Some(overlay) = color_overlay {
                self.subscriptions
                    .push(cx.subscribe(&overlay, move |this, _, event, cx| {
                        if this.session != session || this.stage != Stage::Selecting {
                            return;
                        }
                        match event {
                            ColorOverlayEvent::Picked(color) => {
                                #[cfg(not(target_os = "linux"))]
                                {
                                    let Some(snapshot) = this.snapshot.as_ref() else {
                                        this.finish(
                                            Some(
                                                "The capture session is no longer available."
                                                    .into(),
                                            ),
                                            cx,
                                        );
                                        return;
                                    };
                                    if !platform::display_layout_matches(&snapshot.displays) {
                                        this.finish(
                                            Some(
                                                "The display layout changed. Please start again."
                                                    .into(),
                                            ),
                                            cx,
                                        );
                                        return;
                                    }
                                }
                                cx.write_to_clipboard(ClipboardItem::new_string(color.clone()));
                                desktop::play_sound_effect(SoundEffect::PickColor, cx);
                                this.finish(None, cx);
                            }
                            ColorOverlayEvent::Cancelled => this.finish(None, cx),
                        }
                    }));
            }
        }
        #[cfg(target_os = "macos")]
        self.show_macos_overlays(native_overlays, cx);
        #[cfg(not(target_os = "macos"))]
        {
            self.stage = Stage::Selecting;
            for handle in self.overlays.clone() {
                let shown = handle.update(cx, |_, window, _| {
                    platform::set_window_visible(window, true)
                });
                if let Err(error) = shown
                    .map_err(|error| error.to_string())
                    .and_then(|result| result)
                {
                    self.finish(Some(error), cx);
                    return;
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    fn show_macos_overlays(
        &mut self,
        overlays: Vec<(AnyWindowHandle, platform::OverlayWindow)>,
        cx: &mut Context<Self>,
    ) {
        let session = self.session;
        cx.spawn(async move |this, cx| {
            for (handle, overlay) in &overlays {
                let current = this.update(cx, |this, cx| {
                    this.overlay_is_current(session, Stage::Preparing, *handle, cx)
                });
                if !matches!(current, Ok(true)) {
                    return;
                }
                overlay.configure();
            }
            let selecting = this.update(cx, |this, _| {
                if this.session != session || this.stage != Stage::Preparing {
                    return false;
                }
                this.stage = Stage::Selecting;
                true
            });
            if !matches!(selecting, Ok(true)) {
                return;
            }
            for (handle, overlay) in &overlays {
                let current = this.update(cx, |this, cx| {
                    this.overlay_is_current(session, Stage::Selecting, *handle, cx)
                });
                if !matches!(current, Ok(true)) {
                    return;
                }
                overlay.show();
            }
        })
        .detach();
    }

    #[cfg(target_os = "macos")]
    fn overlay_is_current(
        &mut self,
        session: u64,
        stage: Stage,
        handle: AnyWindowHandle,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.session != session || self.stage != stage {
            return false;
        }
        if !cx.windows().contains(&handle) {
            self.finish(None, cx);
            return false;
        }
        true
    }

    fn capture(&mut self, selection: CaptureSelection, cx: &mut Context<Self>) {
        let Some(snapshot) = self.snapshot.clone() else {
            self.finish(
                Some("The capture session is no longer available.".into()),
                cx,
            );
            return;
        };
        #[cfg(not(target_os = "linux"))]
        if self.stage == Stage::Selecting && !platform::display_layout_matches(&snapshot.displays) {
            self.finish(
                Some("The display layout changed. Please start again.".into()),
                cx,
            );
            return;
        }
        self.stage = Stage::Finalizing;
        self.close_overlays(cx);
        let placement = if cfg!(target_os = "linux") {
            None
        } else {
            CapturePlacement::new(&selection, &snapshot.displays)
        };
        let session = self.session;
        self.capture_task = Some(cx.spawn(async move |this, cx| {
            let result = match selection {
                CaptureSelection::Window(target) => {
                    race(platform::capture_window(target), async {
                        cx.background_executor()
                            .timer(Duration::from_secs(15))
                            .await;
                        Err("Window capture took too long. Please try again.".into())
                    })
                    .await
                }
                CaptureSelection::Region { display_id, bounds } => {
                    cx.background_executor()
                        .spawn(async move {
                            snapshot
                                .displays
                                .iter()
                                .find(|display| display.id == display_id)
                                .ok_or_else(|| {
                                    "The selected display is no longer available.".to_string()
                                })
                                .and_then(|display| {
                                    crop_region(display, bounds).map(|pixels| CapturedImage {
                                        pixels,
                                        pixel_scale: display.pixel_scale(),
                                    })
                                })
                        })
                        .await
                }
            };
            let result = match result {
                Ok(captured) => {
                    cx.background_executor()
                        .spawn(async move {
                            SourceImage::from_rgba(captured.pixels).map(|mut source| {
                                source.pixel_scale = captured.pixel_scale;
                                source
                            })
                        })
                        .await
                }
                Err(error) => Err(error),
            };
            let _ = this.update(cx, |this, cx| {
                if this.session != session || this.stage != Stage::Finalizing {
                    return;
                }
                match result {
                    Ok(source) => {
                        this.remember_screenshot(source, placement, cx);
                        desktop::play_sound_effect(SoundEffect::Capture, cx);
                        this.show_editor(cx);
                    }
                    Err(error) => this.finish(Some(error), cx),
                }
            });
        }));
    }

    fn remember_screenshot(
        &mut self,
        source: SourceImage,
        placement: Option<CapturePlacement>,
        cx: &mut Context<Self>,
    ) {
        // TODO: Replace the package name with a screenshot-specific window title.
        self.last_screenshot = Some(ScreenshotState::new(
            source,
            env!("CARGO_PKG_NAME").into(),
            placement,
        ));
        desktop::screenshot_ready(cx);
    }

    fn reopen_editor(&mut self, cx: &mut Context<Self>) {
        if self.stage != Stage::Idle || !desktop::can_capture(cx) || self.last_screenshot.is_none()
        {
            return;
        }
        self.show_editor(cx);
    }

    fn show_editor(&mut self, cx: &mut Context<Self>) {
        let Some(screenshot) = self.last_screenshot.as_ref() else {
            return;
        };
        desktop::prepare_window(cx);
        match open_editor(screenshot.clone(), cx) {
            Ok(editor) => {
                self.desktop_context = None;
                self.finish(None, cx);
                desktop::activate_window(editor, cx);
            }
            Err(error) => self.finish(Some(error), cx),
        }
    }

    fn close_overlays(&mut self, cx: &mut Context<Self>) {
        self.subscriptions.clear();
        for handle in self.overlays.drain(..) {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        }
    }

    fn finish(&mut self, error: Option<String>, cx: &mut Context<Self>) {
        self.session = self.session.wrapping_add(1);
        self.capture_task = None;
        self.close_overlays(cx);
        self.snapshot = None;
        self.stage = Stage::Idle;
        desktop::capture_finished(cx);
        if let Some(context) = self.desktop_context.take()
            && error.is_none()
        {
            context.restore_focus(cx);
        }
        if error.is_some() {
            desktop::show_settings(error, cx);
        }
    }
}

pub(crate) fn remember_editor(screenshot: Rc<RefCell<ScreenshotState>>, cx: &mut App) {
    cx.defer(move |cx| {
        if let Some(global) = cx.try_global::<CaptureGlobal>() {
            let controller = global.0.clone();
            controller.update(cx, |this, _| this.last_screenshot = Some(screenshot));
            desktop::screenshot_ready(cx);
        }
    });
}

pub(crate) fn open_editor(
    screenshot: Rc<RefCell<ScreenshotState>>,
    cx: &mut App,
) -> Result<AnyWindowHandle, String> {
    let (image_size, title, placement, previous) = {
        let state = screenshot.borrow();
        (
            state.source.pixels.dimensions(),
            state.title.clone(),
            state.placement,
            state
                .window
                .filter(|(handle, _)| cx.windows().contains(handle)),
        )
    };
    if let Some((handle, WindowRole::Editor)) = previous {
        return Ok(handle);
    }
    desktop::prepare_window(cx);
    let display_id = placement.and_then(|placement| {
        if cfg!(target_os = "macos") {
            cx.find_display(DisplayId::new(placement.display_id.0))
        } else {
            cx.displays().into_iter().find(|display| {
                let bounds = display.bounds();
                let expected = placement.display_bounds;
                [
                    (bounds.origin.x, expected.x),
                    (bounds.origin.y, expected.y),
                    (bounds.size.width, expected.width),
                    (bounds.size.height, expected.height),
                ]
                .into_iter()
                .all(|(actual, expected)| {
                    (f64::from(f32::from(actual)) - expected / placement.ui_scale).abs() < 1.0
                })
            })
        }
        .map(|display| display.id())
    });
    let placement = placement.filter(|_| display_id.is_some());
    let work_area = placement.map(|placement| {
        platform::display_work_area(placement.display_id).unwrap_or(placement.display_bounds)
    });
    let window_min_size = placement.zip(work_area).map_or_else(
        || size(px(MIN_EDITOR_WIDTH), px(MIN_EDITOR_HEIGHT)),
        |(placement, area)| minimum_size(area, placement.ui_scale),
    );
    let initial_size = placement.zip(work_area).map_or_else(
        || size(px(1200.), px(860.)),
        |(placement, area)| {
            size(
                px(1200.0_f32.min((area.width / placement.ui_scale) as f32)),
                px(860.0_f32.min((area.height / placement.ui_scale) as f32)),
            )
        },
    );
    #[cfg(target_os = "linux")]
    let display_id = display_id.or_else(|| {
        cx.primary_display()
            .or_else(|| cx.displays().into_iter().next())
            .map(|display| display.id())
    });
    #[cfg(target_os = "linux")]
    let initial_size =
        display_id
            .and_then(|id| cx.find_display(id))
            .map_or(initial_size, |display| {
                let available = display.bounds().size;
                size(
                    initial_size.width.min(available.width * 0.9),
                    initial_size.height.min(available.height * 0.9),
                )
            });
    let bounds = Bounds::centered(display_id, initial_size, cx);
    let mut editor = None;
    let handle: AnyWindowHandle = cx
        .open_window(
            WindowOptions {
                #[cfg(target_os = "linux")]
                app_id: Some("com.snipuno.editor".into()),
                titlebar: Some(TitlebarOptions {
                    title: Some(title.into()),
                    appears_transparent: true,
                    ..Default::default()
                }),
                app_owns_titlebar_drag: true,
                is_resizable: !cfg!(target_os = "linux"),
                is_minimizable: !cfg!(target_os = "linux"),
                inactive_frame_interval: Some(Duration::from_millis(500)),
                display_id,
                show: placement.is_none() || cfg!(target_os = "windows"),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(window_min_size),
                ..Default::default()
            },
            |window, cx| {
                observe_window(window, cx);
                let view = cx.new(|cx| ScreenshotEditor::new(screenshot.clone(), window, cx));
                editor = Some(view.clone());
                cx.new(|cx| Root::new(view, window, cx))
            },
        )
        .map_err(|error| format!("Unable to open the screenshot: {error}"))?
        .into();
    if let Some((placement, work_area)) = placement.zip(work_area) {
        let _ = handle.update(cx, |_, window, cx| {
            let alignment =
                match platform::position_editor(window, placement, work_area, image_size) {
                    Ok(alignment) => alignment,
                    Err(error) => {
                        eprintln!("Unable to position the screenshot editor: {error}");
                        point(0.5, 0.5)
                    }
                };
            if let Some(editor) = editor.as_ref() {
                editor.update(cx, |editor, cx| {
                    editor.set_image_alignment(alignment, cx);
                });
            }
            if let Err(error) = platform::set_window_visible(window, true) {
                eprintln!("Unable to show the screenshot editor: {error}");
            }
        });
    }
    screenshot.borrow_mut().window = Some((handle, WindowRole::Editor));
    desktop::track_window(handle, WindowRole::Editor, cx);
    remember_editor(screenshot, cx);
    if let Some((previous, _)) = previous {
        let _ = previous.update(cx, |_, window, _| window.remove_window());
    }
    Ok(handle)
}
