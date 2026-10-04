#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

mod app;
mod canvas;
mod capture;
mod clipboard;
mod color_format;
mod crop;
mod desktop;
mod dimensions;
mod editor;
mod erase;
mod export;
mod frame_cache;
mod image_processing;
mod magnifier;
mod measurement;
mod mosaic;
mod pixel_loupe;
mod rounded;
mod selection;
mod spotlight;
#[cfg(test)]
mod test_support;
mod theme;

use ui::IconAssets;

gpui::actions!(screenshot, [Quit]);

fn main() {
    let autostart = std::env::args_os().any(|argument| argument == "--autostart");
    #[cfg(target_os = "windows")]
    let _instance = match desktop::initialize(autostart) {
        Ok(Some(instance)) => instance,
        Ok(None) => return,
        Err(error) => {
            eprintln!("Unable to initialize desktop integration: {error}");
            return;
        }
    };
    #[cfg(target_os = "windows")]
    if let Err(error) = capture::initialize() {
        eprintln!("Unable to initialize screen capture: {error}");
        return;
    }
    let application = gpui_platform::application().with_assets(IconAssets);
    application.on_reopen(desktop::reopen);
    application.run(move |cx| {
        // GPUI runs its initialization callback inside applicationDidFinishLaunching.
        #[cfg(target_os = "macos")]
        let autostart = autostart || autostart::was_launched_at_login();
        ui::init(cx);
        clipboard::init(cx);
        cx.on_action(|_: &Quit, cx| cx.quit());
        capture::install(cx);
        app::install(cx);
        desktop::install(cx, autostart);
    });
}
