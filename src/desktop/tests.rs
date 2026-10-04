use super::{
    settings::Settings,
    shortcut::{
        COLOR_PICKER_SHORTCUT, FULLSCREEN_SHORTCUT, Modifiers, SCREENSHOT_SHORTCUT, Shortcut,
        TriggerGate,
    },
};
use crate::{color_format::ColorFormat, dimensions::DimensionMode, test_support::TestDir};
use std::fs;

#[test]
fn settings_round_trip_and_create_missing_directories() {
    let dir = TestDir::new();
    let path = dir.join("nested/settings.json");
    let settings = Settings {
        dimension_mode: DimensionMode::Original,
        color_format: ColorFormat::Rgb,
        sound_effects_enabled: false,
    };
    settings.save(&path).unwrap();
    assert_eq!(Settings::load(&path), settings);
    assert!(Settings::default().sound_effects_enabled);
    Settings::default().save(&path).unwrap();
    assert_eq!(Settings::load(&path), Settings::default());
    assert!(
        settings
            .save(&dir.join("nested/settings.json/child"))
            .is_err()
    );
}

#[test]
fn missing_corrupt_and_partial_settings_have_safe_defaults() {
    let dir = TestDir::new();
    let path = dir.join("settings.json");
    assert_eq!(Settings::load(&path), Settings::default());
    for bytes in ["", "{", "[]", r#"{"dimension_mode":"unknown"}"#] {
        fs::write(&path, bytes).unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
    }
    fs::write(&path, r#"{"dimension_mode":"original","unknown":true}"#).unwrap();
    assert_eq!(
        Settings::load(&path),
        Settings {
            dimension_mode: DimensionMode::Original,
            ..Settings::default()
        }
    );
}

#[test]
fn shortcuts_validate_key_boundaries_modifiers_and_reserved_combinations() {
    let control = Modifiers {
        control: true,
        ..Modifiers::default()
    };
    for (key, valid) in [
        ("KeyB", true),
        ("Digit0", true),
        ("F1", true),
        ("F20", true),
        ("ArrowLeft", true),
        ("F0", false),
        ("F21", false),
        ("F01", false),
        ("KeyAA", false),
        ("Keya", false),
        ("Digit12", false),
        ("", false),
    ] {
        assert_eq!(
            Shortcut {
                key,
                modifiers: control
            }
            .validate()
            .is_ok(),
            valid,
            "{key}"
        );
    }
    for modifiers in [
        Modifiers::default(),
        Modifiers {
            shift: true,
            ..Modifiers::default()
        },
    ] {
        assert!(
            Shortcut {
                key: "KeyB",
                modifiers
            }
            .validate()
            .is_err()
        );
    }
    for meta in [false, true] {
        for key in ["KeyS", "KeyZ", "KeyY", "KeyQ", "KeyC", "KeyV", "Comma"] {
            let modifiers = Modifiers {
                control: !meta,
                meta,
                ..Modifiers::default()
            };
            assert!(Shortcut { key, modifiers }.validate().is_err(), "{key}");
        }
    }
    for shift in [false, true] {
        assert!(
            Shortcut {
                key: "KeyS",
                modifiers: Modifiers { shift, ..control }
            }
            .validate()
            .is_err()
        );
    }
    assert!(SCREENSHOT_SHORTCUT.validate().is_ok());
    assert!(FULLSCREEN_SHORTCUT.validate().is_ok());
    assert!(COLOR_PICKER_SHORTCUT.validate().is_ok());
}

#[test]
fn fullscreen_shortcut_uses_the_platform_primary_modifier() {
    let shortcut = FULLSCREEN_SHORTCUT;
    assert_eq!(shortcut.key, "Digit1");
    assert!(shortcut.modifiers.shift);
    assert_eq!(shortcut.modifiers.meta, cfg!(target_os = "macos"));
    assert_eq!(shortcut.modifiers.control, !cfg!(target_os = "macos"));
    assert!(!shortcut.modifiers.alt);
    assert_ne!(shortcut, SCREENSHOT_SHORTCUT);
    assert_ne!(shortcut, COLOR_PICKER_SHORTCUT);
}

#[cfg(not(target_os = "linux"))]
#[test]
fn fullscreen_shortcut_registration_failure_preserves_other_shortcuts_and_can_retry() {
    use super::settings::{ShortcutBackend, ShortcutService};
    use futures_lite::future::block_on;

    #[derive(Default)]
    struct Backend {
        registered: Vec<Shortcut>,
        fail_fullscreen: bool,
    }

    impl ShortcutBackend for Backend {
        async fn register(&mut self, shortcut: &Shortcut) -> Result<(), String> {
            if self.fail_fullscreen && shortcut == &FULLSCREEN_SHORTCUT {
                return Err("Already registered".into());
            }
            self.registered.push(shortcut.clone());
            Ok(())
        }
    }

    let mut service = ShortcutService::new(Backend {
        fail_fullscreen: true,
        ..Default::default()
    });
    block_on(service.initialize());
    assert_eq!(service.active, Some(SCREENSHOT_SHORTCUT));
    assert_eq!(service.color_active, Some(COLOR_PICKER_SHORTCUT));
    assert_eq!(service.fullscreen_active, None);
    assert_eq!(
        service.error.as_deref(),
        Some("Full screen screenshot shortcut: Already registered")
    );

    service.backend.fail_fullscreen = false;
    block_on(service.initialize());
    block_on(service.initialize());
    assert_eq!(service.fullscreen_active, Some(FULLSCREEN_SHORTCUT));
    assert_eq!(service.error, None);
    assert_eq!(
        service.backend.registered,
        [
            SCREENSHOT_SHORTCUT,
            COLOR_PICKER_SHORTCUT,
            FULLSCREEN_SHORTCUT
        ]
    );
}

#[test]
fn capture_triggers_once_per_press_and_does_not_queue_while_busy() {
    for (busy, applying) in [(false, false), (true, false), (false, true), (true, true)] {
        let mut gate = TriggerGate::default();
        gate.busy = busy;
        gate.applying = applying;
        assert_eq!(gate.key_event(true), !busy && !applying);
        gate.busy = false;
        gate.applying = false;
        assert!(!gate.key_event(true));
        assert!(!gate.key_event(false));
        assert!(gate.key_event(true));
        gate.reset();
        assert!(gate.key_event(true));
    }
}
