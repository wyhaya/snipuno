#[cfg(target_os = "macos")]
use super::platform;
use super::resident::show_settings;
use crate::{Quit, app::SaveImage, capture::ReopenLastScreenshot};
use gpui::*;
use gpui_base::input::{Copy, Cut, Paste, Redo, SelectAll, Undo};

actions!(
    desktop,
    [
        ShowSettings,
        CloseWindow,
        MinimizeWindow,
        ZoomWindow,
        ToggleFullScreen
    ]
);

#[cfg(target_os = "macos")]
actions!(desktop, [About, Hide, HideOthers, ShowAll, BringAllToFront]);

pub(super) fn install(cx: &mut App) {
    cx.on_action(|_: &ShowSettings, cx| show_settings(None, cx));
    let primary = if cfg!(target_os = "macos") {
        "cmd"
    } else {
        "ctrl"
    };
    cx.bind_keys([
        KeyBinding::new(&format!("{primary}-,"), ShowSettings, None),
        KeyBinding::new(&format!("{primary}-q"), Quit, None),
        KeyBinding::new(&format!("{primary}-w"), CloseWindow, None),
        KeyBinding::new("escape", CloseWindow, Some("PinnedScreenshot")),
        KeyBinding::new(&format!("{primary}-m"), MinimizeWindow, None),
    ]);

    #[cfg(target_os = "macos")]
    {
        cx.on_action(|_: &About, _| platform::show_about());
        cx.on_action(|_: &Hide, cx| cx.hide());
        cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
        cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
        cx.on_action(|_: &BringAllToFront, _| platform::bring_all_to_front());
        cx.bind_keys([
            KeyBinding::new("cmd-h", Hide, None),
            KeyBinding::new("cmd-alt-h", HideOthers, None),
            KeyBinding::new("ctrl-cmd-f", ToggleFullScreen, None),
        ]);
    }

    update(false, cx);
}

pub(super) fn update(can_reopen: bool, cx: &App) {
    let mut app_items = Vec::new();
    #[cfg(target_os = "macos")]
    app_items.extend([
        MenuItem::action("About Snipuno", About),
        MenuItem::separator(),
    ]);
    app_items.push(MenuItem::action("Settings…", ShowSettings));
    app_items.push(MenuItem::separator());
    #[cfg(target_os = "macos")]
    app_items.extend([
        MenuItem::os_submenu("Services", SystemMenuType::Services),
        MenuItem::separator(),
        MenuItem::action("Hide Snipuno", Hide),
        MenuItem::action("Hide Others", HideOthers),
        MenuItem::action("Show All", ShowAll),
        MenuItem::separator(),
    ]);
    app_items.push(MenuItem::action("Quit Snipuno", Quit));

    let window_items = [
        MenuItem::action("Minimize", MinimizeWindow),
        MenuItem::action("Zoom", ZoomWindow),
    ];
    #[cfg(target_os = "macos")]
    let window_items = window_items.into_iter().chain([
        MenuItem::separator(),
        MenuItem::action("Bring All to Front", BringAllToFront),
    ]);

    cx.set_menus([
        Menu::new("Snipuno").items(app_items),
        Menu::new("File").items([
            MenuItem::action("Edit Last Screenshot", ReopenLastScreenshot).disabled(!can_reopen),
            MenuItem::separator(),
            MenuItem::action("Save Image As…", SaveImage),
            MenuItem::separator(),
            MenuItem::action("Close Window", CloseWindow),
        ]),
        Menu::new("Edit").items([
            MenuItem::os_action("Undo", Undo, OsAction::Undo),
            MenuItem::os_action("Redo", Redo, OsAction::Redo),
            MenuItem::separator(),
            MenuItem::os_action("Cut", Cut, OsAction::Cut),
            MenuItem::os_action("Copy", Copy, OsAction::Copy),
            MenuItem::os_action("Paste", Paste, OsAction::Paste),
            MenuItem::separator(),
            MenuItem::os_action("Select All", SelectAll, OsAction::SelectAll),
        ]),
        Menu::new("View").items([MenuItem::action("Toggle Full Screen", ToggleFullScreen)]),
        Menu::new("Window").items(window_items),
    ]);
}

pub(crate) fn window_actions<E: InteractiveElement>(
    element: E,
    close: impl Fn(&CloseWindow, &mut Window, &mut App) + 'static,
) -> E {
    element
        .on_action(close)
        .on_action(|_: &MinimizeWindow, window, _| window.minimize_window())
        .on_action(|_: &ZoomWindow, window, _| window.zoom_window())
        .on_action(|_: &ToggleFullScreen, window, _| window.toggle_fullscreen())
}
