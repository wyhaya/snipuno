use super::{
    AnyWindowHandle, App, DesktopEvent, MenuState, WindowRole,
    platform::{self, Hotkeys, Platform},
    settings::{Settings, ShortcutService},
    settings_ui::SettingsView,
    shortcut::{Shortcut, TriggerGate, show_dock},
    sound::SoundEffect,
    tray::Tray,
};
use crate::{
    capture::{
        FullscreenScreenshot, NewScreenshot, PickColor, ReopenLastScreenshot,
        TrayFullscreenScreenshot, TrayPickColor, TrayScreenshot,
    },
    color_format::ColorFormat,
    dimensions::DimensionMode,
};
use futures_channel::mpsc;
use futures_lite::StreamExt;
use gpui::{prelude::*, *};
use std::path::PathBuf;
use ui::{Root, theme};

struct DesktopGlobal(Entity<Desktop>);
impl Global for DesktopGlobal {}

pub(super) struct Desktop {
    pub platform: Option<Platform>,
    tray: Option<Tray>,
    service: Option<ShortcutService<Hotkeys>>,
    pub settings: Settings,
    settings_path: Option<PathBuf>,
    settings_task: Option<Task<()>>,
    pub active: Option<Shortcut>,
    pub error: Option<String>,
    pub startup_error: Option<String>,
    pub gate: TriggerGate,
    windows: Vec<(AnyWindowHandle, WindowRole)>,
    has_screenshot: bool,
    pub view: Option<(AnyWindowHandle, Entity<SettingsView>)>,
    task: Option<Task<()>>,
    events: Option<Task<()>>,
    _color_subscription: Subscription,
    shutting_down: bool,
}

fn entity(cx: &App) -> Option<Entity<Desktop>> {
    cx.try_global::<DesktopGlobal>()
        .map(|global| global.0.clone())
}

pub fn install(cx: &mut App, autostart: bool) {
    let settings_path = Settings::path();
    let settings = settings_path
        .as_deref()
        .map(Settings::load)
        .unwrap_or_default();
    cx.set_global(settings.dimension_mode);
    cx.set_global(settings.color_format);
    let (sender, mut receiver) = mpsc::unbounded();
    let hotkeys = Hotkeys::new(sender.clone());
    let (platform, platform_error) = Platform::new(sender.clone());
    let (tray, tray_error) = match Tray::new(sender, cx) {
        Ok(tray) => (Some(tray), None),
        Err(error) => (None, Some(error)),
    };
    let startup_error = match (platform_error, tray_error) {
        (Some(platform), Some(tray)) => Some(format!("{platform}\n{tray}")),
        (platform, tray) => platform.or(tray),
    };
    let service = ShortcutService::new(hotkeys);
    let desktop = cx.new(|cx: &mut Context<Desktop>| Desktop {
        platform: Some(platform),
        tray,
        service: Some(service),
        settings,
        settings_path,
        settings_task: None,
        active: None,
        error: None,
        startup_error,
        gate: TriggerGate::default(),
        windows: Vec::new(),
        has_screenshot: false,
        view: None,
        task: None,
        events: None,
        _color_subscription: cx.observe_global::<ColorFormat>(|this, cx| {
            let format = ColorFormat::current(cx);
            if this.settings.color_format != format {
                this.settings.color_format = format;
                this.save_settings(cx);
                this.refresh(cx);
            }
        }),
        shutting_down: false,
    });
    cx.set_global(DesktopGlobal(desktop.clone()));
    super::menu::install(cx);
    desktop.update(cx, |this, cx| {
        this.events = Some(cx.spawn(async move |this, cx| {
            while let Some(event) = receiver.next().await {
                if this
                    .update(cx, |this, cx| this.handle_event(event, cx))
                    .is_err()
                {
                    break;
                }
            }
        }));
        this.initialize_shortcuts(cx);
        if !autostart || this.tray.is_none() {
            show_settings(None, cx);
        }
        this.refresh(cx);
    });
    cx.on_window_closed({
        let desktop = desktop.clone();
        move |cx, id| {
            let desktop = desktop.clone();
            cx.defer(move |cx| {
                desktop.update(cx, |this, cx| {
                    this.windows.retain(|(window, _)| window.window_id() != id);
                    if this
                        .view
                        .as_ref()
                        .is_some_and(|(window, _)| window.window_id() == id)
                    {
                        this.view = None;
                    }
                    if !this.shutting_down && this.windows.is_empty() && this.tray.is_none() {
                        show_settings(None, cx);
                    }
                    this.refresh(cx);
                })
            });
        }
    })
    .detach();
    cx.on_system_wake({
        let desktop = desktop.clone();
        move |cx| {
            desktop.update(cx, |this, cx| {
                this.gate.reset();
                this.refresh(cx);
            })
        }
    })
    .detach();
    cx.on_app_quit(move |cx| {
        let settings_task = desktop.update(cx, |this, _| {
            this.shutting_down = true;
            this.task = None;
            this.events = None;
            this.service = None;
            this.tray = None;
            this.platform = None;
            this.settings_task.take()
        });
        async move {
            if let Some(task) = settings_task {
                task.await;
            }
        }
    })
    .detach();
}

pub fn prepare_window(_: &mut App) {
    platform::set_dock_visible(true);
}

pub fn track_window(window: AnyWindowHandle, role: WindowRole, cx: &mut App) {
    if let Some(entity) = entity(cx) {
        entity.update(cx, |this, cx| {
            this.windows
                .retain(|(handle, _)| handle.window_id() != window.window_id());
            this.windows.push((window, role));
            this.refresh(cx);
        });
    }
}

pub fn window_activated(window: AnyWindowHandle, cx: &mut App) {
    cx.defer(move |cx| {
        if let Some(entity) = entity(cx) {
            entity.update(cx, |this, _| {
                if let Some(index) = this
                    .windows
                    .iter()
                    .position(|(handle, _)| handle.window_id() == window.window_id())
                {
                    let entry = this.windows.remove(index);
                    this.windows.push(entry);
                }
            });
        }
    });
}

pub fn can_capture(cx: &App) -> bool {
    entity(cx).is_none_or(|entity| entity.read(cx).gate.can_capture())
}

pub fn capture_started(cx: &mut App) {
    if let Some(entity) = entity(cx) {
        entity.update(cx, |this, cx| {
            this.gate.busy = true;
            this.refresh(cx);
        });
    }
}

pub fn capture_finished(cx: &mut App) {
    if let Some(entity) = entity(cx) {
        entity.update(cx, |this, cx| {
            this.gate.busy = false;
            this.refresh(cx);
        });
    }
}

pub fn screenshot_ready(cx: &mut App) {
    if let Some(entity) = entity(cx) {
        entity.update(cx, |this, cx| {
            this.has_screenshot = true;
            this.refresh(cx);
        });
    }
}

pub fn play_sound_effect(effect: SoundEffect, cx: &App) {
    if let Some(entity) = entity(cx) {
        let desktop = entity.read(cx);
        if desktop.settings.sound_effects_enabled && !desktop.shutting_down {
            effect.play();
        }
    }
}

pub fn show_settings(message: Option<String>, cx: &mut App) {
    // Opening a window renders it synchronously, so Desktop must not be leased here.
    cx.defer(move |cx| {
        let Some(desktop) = entity(cx) else {
            return;
        };
        let state = desktop.read(cx);
        if state.gate.busy || state.shutting_down {
            return;
        }
        let existing = state.view.clone();
        if let Some((handle, view)) = existing {
            if let Some(message) = message.clone() {
                view.update(cx, |view, cx| {
                    view.message = Some(message);
                    cx.notify();
                });
            }
            if activate_window(handle, cx) {
                return;
            }
            desktop.update(cx, |this, _| {
                this.windows
                    .retain(|(window, _)| window.window_id() != handle.window_id());
                this.view = None;
            });
        }
        prepare_window(cx);
        let mut view = None;
        let bounds = Bounds::centered(None, size(px(600.), px(540.)), cx);
        let result = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(560.), px(440.))),
                titlebar: Some(TitlebarOptions {
                    title: Some("Snipuno".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |window, cx| {
                theme::observe_window(window, cx);
                let settings = cx.new(|cx| SettingsView::new(desktop.clone(), message, window, cx));
                view = Some(settings.clone());
                cx.new(|cx| Root::new(settings, window, cx))
            },
        );
        let opened = result.is_ok();
        desktop.update(cx, |this, cx| {
            match result {
                Ok(handle) => {
                    this.windows.push((handle.into(), WindowRole::Other));
                    this.view = view.map(|view| (handle.into(), view));
                }
                Err(error) => {
                    this.error = Some(format!("Unable to open Settings: {error}"));
                    eprintln!("Unable to open Settings: {error}");
                }
            }
            this.refresh(cx);
        });
        if opened {
            cx.activate(true);
        }
    });
}

pub fn reopen(cx: &mut App) {
    cx.defer(|cx| {
        let Some(desktop) = entity(cx) else {
            return;
        };
        let state = desktop.read(cx);
        if state.gate.busy || state.shutting_down {
            return;
        }
        if state.has_screenshot {
            cx.dispatch_action(&ReopenLastScreenshot);
        } else {
            show_settings(None, cx);
        }
    });
}

pub fn activate_window(handle: AnyWindowHandle, cx: &mut App) -> bool {
    prepare_window(cx);
    handle
        .update(cx, |_, window, cx| {
            let _ = platform::show_window(window);
            cx.activate(true);
            window.activate_window();
        })
        .is_ok()
}

impl Desktop {
    pub fn set_sound_effects_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if self.settings.sound_effects_enabled == enabled {
            return;
        }
        self.settings.sound_effects_enabled = enabled;
        if enabled {
            SoundEffect::Capture.play();
        }
        self.save_settings(cx);
        self.refresh(cx);
    }

    pub fn set_dimension_mode(&mut self, mode: DimensionMode, cx: &mut Context<Self>) {
        if self.gate.applying || self.gate.busy || self.settings.dimension_mode == mode {
            return;
        }
        self.settings.dimension_mode = mode;
        cx.set_global(mode);
        self.save_settings(cx);
        self.refresh(cx);
    }

    fn save_settings(&mut self, cx: &App) {
        let Some(path) = self.settings_path.clone() else {
            return;
        };
        let settings = self.settings.clone();
        let previous = self.settings_task.take();
        self.settings_task = Some(cx.background_executor().spawn(async move {
            if let Some(previous) = previous {
                previous.await;
            }
            let _ = settings.save(&path);
        }));
    }

    pub fn can_restart(&self) -> bool {
        !self.gate.busy
            && !self
                .windows
                .iter()
                .any(|(_, role)| matches!(role, WindowRole::Editor | WindowRole::Pinned))
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        let can_reopen = self.has_screenshot && self.gate.can_capture();
        super::menu::update(can_reopen, cx);
        if let Some(tray) = &mut self.tray
            && let Err(error) = tray.update(MenuState {
                shortcut: if cfg!(target_os = "linux") {
                    None
                } else {
                    self.active.as_ref()
                },
                color_shortcut: if cfg!(target_os = "linux") {
                    None
                } else {
                    self.service
                        .as_ref()
                        .and_then(|service| service.color_active.as_ref())
                },
                fullscreen_shortcut: self
                    .service
                    .as_ref()
                    .and_then(|service| service.fullscreen_active.as_ref()),
                can_capture: self.gate.can_capture(),
                can_reopen,
                warning: self.error.is_some() || self.startup_error.is_some(),
            })
        {
            self.startup_error = Some(format!("Unable to update the tray menu: {error}"));
        }
        if let Some(platform) = &self.platform {
            platform.set_dock_visible(show_dock(self.windows.len(), self.tray.is_some()));
        }
        cx.notify();
    }

    fn initialize_shortcuts(&mut self, cx: &mut Context<Self>) {
        let Some(mut service) = self.service.take() else {
            return;
        };
        self.gate.applying = true;
        self.refresh(cx);
        self.task = Some(cx.spawn(async move |this, cx| {
            service.initialize().await;
            let _ = this.update(cx, |this, cx| {
                this.gate.applying = false;
                this.gate.reset();
                this.active = service.active.clone();
                this.error = service.error.clone();
                this.service = Some(service);
                this.refresh(cx);
            });
        }));
    }

    fn handle_event(&mut self, event: DesktopEvent, cx: &mut Context<Self>) {
        if self.shutting_down {
            return;
        }
        match event {
            #[cfg(target_os = "windows")]
            DesktopEvent::Reopen => reopen(cx),
            #[cfg(target_os = "windows")]
            DesktopEvent::SystemAppearanceChanged => {
                if let Some(tray) = &mut self.tray
                    && let Err(error) = tray.sync_appearance(cx)
                {
                    self.startup_error = Some(format!("Unable to update the tray icon: {error}"));
                    self.refresh(cx);
                }
            }
            DesktopEvent::Capture if self.gate.can_capture() => {
                cx.defer(|cx| cx.dispatch_action(&TrayScreenshot))
            }
            DesktopEvent::Capture => {}
            DesktopEvent::CaptureFullscreen if self.gate.can_capture() => {
                cx.defer(|cx| cx.dispatch_action(&TrayFullscreenScreenshot))
            }
            DesktopEvent::CaptureFullscreen => {}
            DesktopEvent::EditLastScreenshot if self.has_screenshot && self.gate.can_capture() => {
                cx.defer(|cx| cx.dispatch_action(&ReopenLastScreenshot))
            }
            DesktopEvent::EditLastScreenshot => {}
            DesktopEvent::PickColor if self.gate.can_capture() => {
                cx.defer(|cx| cx.dispatch_action(&TrayPickColor))
            }
            DesktopEvent::PickColor => {}
            DesktopEvent::Settings if !self.gate.busy => show_settings(None, cx),
            DesktopEvent::Settings => {}
            DesktopEvent::Quit => cx.defer(|cx| cx.quit()),
            DesktopEvent::Hotkey { id, pressed } => {
                if self
                    .service
                    .as_ref()
                    .and_then(|service| service.fullscreen_active.as_ref())
                    .is_some_and(|shortcut| platform::hotkey_id(shortcut) == Some(id))
                {
                    if self.gate.key_event(pressed) {
                        cx.defer(|cx| cx.dispatch_action(&FullscreenScreenshot));
                    }
                    return;
                }
                if self
                    .service
                    .as_ref()
                    .and_then(|service| service.color_active.as_ref())
                    .is_some_and(|shortcut| platform::hotkey_id(shortcut) == Some(id))
                {
                    if self.gate.key_event(pressed) {
                        cx.defer(|cx| cx.dispatch_action(&PickColor));
                    }
                    return;
                }
                if !self
                    .active
                    .as_ref()
                    .is_some_and(|shortcut| platform::hotkey_id(shortcut) == Some(id))
                {
                    return;
                }
                if self.gate.key_event(pressed) {
                    cx.defer(|cx| cx.dispatch_action(&NewScreenshot));
                }
            }
        }
    }
}
