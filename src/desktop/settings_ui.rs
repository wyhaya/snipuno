use super::{
    resident::Desktop,
    shortcut::{COLOR_PICKER_SHORTCUT, FULLSCREEN_SHORTCUT, SCREENSHOT_SHORTCUT},
};
use crate::{capture, color_format::ColorFormat, dimensions::DimensionMode};
use autostart::Status as AutostartStatus;
use gpui::{prelude::*, *};
use gpui_base::Theme;
use std::sync::{Arc, LazyLock};
use ui::{Button, Form, Kbd, ScrollArea, Segment, SegmentedControl, Switch, ToolTooltip, notice};

static APP_ICON: LazyLock<Arc<Image>> = LazyLock::new(|| {
    Arc::new(Image::from_bytes(
        ImageFormat::Png,
        include_bytes!("../../assets/128x128.png").to_vec(),
    ))
});

pub(super) struct SettingsView {
    desktop: WeakEntity<Desktop>,
    pub message: Option<String>,
    permission: bool,
    permission_requested: bool,
    autostart: Result<AutostartStatus, String>,
    autostart_error: Option<String>,
    focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl SettingsView {
    pub fn new(
        desktop: Entity<Desktop>,
        message: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let subscription = cx.observe(&desktop, |_, _, cx| cx.notify());
        let color_subscription = cx.observe_global::<ColorFormat>(|_, cx| cx.notify());
        let activation = cx.observe_window_activation(window, |this, window, cx| {
            if window.is_window_active() {
                this.permission = capture::has_permission();
                this.autostart = autostart::status();
                this.autostart_error = None;
            }
            cx.notify();
        });
        Self {
            desktop: desktop.downgrade(),
            message,
            permission: capture::has_permission(),
            permission_requested: false,
            autostart: autostart::status(),
            autostart_error: None,
            focus,
            _subscriptions: vec![subscription, activation, color_subscription],
        }
    }

    fn set_autostart(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.autostart_error = autostart::set_enabled(enabled).err();
        self.autostart = autostart::status();
        if enabled
            && self.autostart_error.is_none()
            && self.autostart == Ok(AutostartStatus::RequiresApproval)
        {
            self.autostart_error = autostart::open_settings().err();
        }
        cx.notify();
    }

    fn general_settings(&self, busy: bool, can_restart: bool, cx: &mut Context<Self>) -> Form {
        let permission_missing = cfg!(target_os = "macos") && !self.permission;
        let requires_approval = self.autostart == Ok(AutostartStatus::RequiresApproval);
        let error = self
            .autostart_error
            .as_ref()
            .or(self.autostart.as_ref().err());
        let on_change = cx.listener(|this, checked: &bool, _, cx| {
            this.set_autostart(*checked, cx);
        });
        Form::new()
            .title("General")
            .when(permission_missing, |this| {
                this.field(
                    "Screen Recording Permission",
                    "Allow access to take screenshots.",
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(
                            Button::new("allow-screen-recording")
                                .primary()
                                .label(if self.permission_requested {
                                    "Open Settings"
                                } else {
                                    "Allow Access"
                                })
                                .disabled(busy)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if this.permission_requested {
                                        capture::open_permission_settings(cx);
                                    } else {
                                        this.permission_requested = true;
                                        if !capture::request_permission() {
                                            capture::open_permission_settings(cx);
                                        }
                                    }
                                    this.permission = capture::has_permission();
                                    cx.notify();
                                })),
                        )
                        .when(self.permission_requested && can_restart, |this| {
                            this.child(
                                Button::new("restart-after-permission")
                                    .label("Restart")
                                    .on_click(|_, _, cx| capture::restart(cx)),
                            )
                        }),
                )
                .when(self.permission_requested, |this| {
                    this.field_footer(notice(
                        "Enable Snipuno in Privacy & Security → Screen & System Audio Recording. Restart if needed.",
                        false,
                        cx,
                    ))
                })
            })
            .field(
                "Launch at login",
                if requires_approval {
                    "Allow Snipuno in system settings to launch at login."
                } else {
                    "Start in the background when you sign in."
                },
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .when(requires_approval, |this| {
                        this.child(
                            Button::new("open-login-settings")
                                .label("Open Settings")
                                .disabled(busy)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.autostart_error = autostart::open_settings().err();
                                    cx.notify();
                                })),
                        )
                    })
                    .child(
                        Switch::new("launch-at-login")
                            .accessibility_label("Launch at login")
                            .checked(self.autostart == Ok(AutostartStatus::Enabled))
                            .disabled(busy || self.autostart.is_err())
                            .on_change(move |checked, window, cx| on_change(&checked, window, cx)),
                    ),
            )
            .when_some(error, |this, error| {
                this.field_footer(notice(error.clone(), true, cx))
            })
    }

    fn sound_switch(&self, enabled: bool, cx: &mut Context<Self>) -> Switch {
        let on_change = cx.listener(|this, checked: &bool, _, cx| {
            let _ = this.desktop.update(cx, |desktop, cx| {
                desktop.set_sound_effects_enabled(*checked, cx);
            });
        });
        Switch::new("sound-effects")
            .accessibility_label("Sound effects")
            .checked(enabled)
            .on_change(move |checked, window, cx| on_change(&checked, window, cx))
    }
}

impl Render for SettingsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = Theme::global(cx).tokens.colors;
        let Some(desktop) = self.desktop.upgrade() else {
            return div().into_any_element();
        };
        let state = desktop.read(cx);
        let busy = state.gate.busy;
        let sound_effects_enabled = state.settings.sound_effects_enabled;
        let error = state.error.clone();
        let startup_error = state.startup_error.clone();
        let can_restart = state.can_restart();
        let color_format = ColorFormat::current(cx);
        let shortcut_settings = Form::new()
            .title("Shortcuts")
            .field(
                "Full Screen Screenshot",
                if cfg!(target_os = "linux") {
                    "Capture your screen. Shortcut works while Snipuno is focused."
                } else {
                    "Capture the entire screen under the pointer."
                },
                Kbd::new(FULLSCREEN_SHORTCUT.key_labels()),
            )
            .field(
                "Area Screenshot",
                "Capture an area of your screen.",
                if cfg!(target_os = "linux") {
                    div()
                        .text_xs()
                        .text_color(colors.muted_foreground)
                        .child("Tray menu")
                        .into_any_element()
                } else {
                    Kbd::new(SCREENSHOT_SHORTCUT.key_labels()).into_any_element()
                },
            )
            .field(
                "Color picker",
                "Pick a color from your screen and copy it.",
                if cfg!(target_os = "linux") {
                    div()
                        .text_xs()
                        .text_color(colors.muted_foreground)
                        .child("Tray menu")
                        .into_any_element()
                } else {
                    Kbd::new(COLOR_PICKER_SHORTCUT.key_labels()).into_any_element()
                },
            );
        // TODO: Resolve logical dimensions for opened images.
        // Captures retain their source scale, while decoded files default to 1x, so
        // a 2x screenshot can read 96 in both modes instead of 48 logical / 96 original.
        // File DPI is only a hint, not a universal screen scale; handle missing metadata
        // across formats and preserve known scale when exporting and reopening images.
        let preferences_settings = Form::new().title("Preferences").field(
            "Dimensions",
            "Size labels only. Exports stay full resolution.",
            SegmentedControl::new("dimensions", state.settings.dimension_mode)
                .accessibility_label("Dimensions")
                .items([
                    (DimensionMode::Logical, "Logical"),
                    (DimensionMode::Original, "Original"),
                ].into_iter().map(|(mode, label)| {
                    Segment::new(mode, label).tooltip(
                        ToolTooltip::new(format!("{label} dimensions")).description(match mode {
                            DimensionMode::Logical => "Show sizes in logical pixels. Exports keep their original resolution.",
                            DimensionMode::Original => "Show sizes in original image pixels. Exports keep their original resolution.",
                        }),
                    )
                }))
                .on_change(cx.listener(|this, mode: &DimensionMode, _, cx| {
                    let _ = this.desktop.update(cx, |desktop, cx| {
                        desktop.set_dimension_mode(*mode, cx);
                    });
                    cx.notify();
                })),
        )
        .field(
            "Color format",
            "Press \"F\" to switch formats.",
            SegmentedControl::new("color-format", color_format)
                .accessibility_label("Color format")
                .items([
                    Segment::new(ColorFormat::Hex, "HEX"),
                    Segment::new(ColorFormat::Rgb, "RGB"),
                ])
                .on_change(|format, _, cx| cx.set_global(*format)),
        )
        .field(
            "Sound effects",
            "Play sounds when taking screenshots or picking colors.",
            self.sound_switch(sound_effects_enabled, cx),
        );
        div()
            .size_full()
            .flex()
            .flex_col()
            .track_focus(&self.focus)
            .map(|element| super::window_actions(element, |_, window, _| window.remove_window()))
            .on_key_down(|event, _, cx| {
                ColorFormat::handle_key_down(event, cx);
            })
            .bg(colors.muted.opacity(0.35))
            .text_color(colors.foreground)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .flex_shrink_0()
                    .px(px(24.))
                    .py(px(16.))
                    .child(
                        img(APP_ICON.clone())
                            .size(px(64.))
                            .flex_shrink_0()
                            .rounded(px(16.))
                            .border_1()
                            .border_color(colors.border),
                    ),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .px(px(24.))
                    .pb(px(20.))
                    .flex()
                    .flex_col()
                    .gap(px(16.))
                    .when_some(startup_error, |this, error| {
                        this.child(notice(error, true, cx))
                    })
                    .when_some(error, |this, error| this.child(notice(error, true, cx)))
                    .when_some(self.message.clone(), |this, message| {
                        this.child(notice(message, false, cx))
                    })
                    .child(self.general_settings(busy, can_restart, cx))
                    .child(shortcut_settings)
                    .child(preferences_settings),
            )
            .map(|content| ScrollArea::new("desktop-settings", content))
            .into_any_element()
    }
}
