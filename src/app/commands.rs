use super::*;
use gpui_base::input::{Copy, Escape, Paste, Redo, Undo};

actions!(screenshot, [SaveImage]);

impl ScreenshotEditor {
    pub(super) fn capture_escape(&mut self, _: &Escape, _: &mut Window, cx: &mut Context<Self>) {
        // Bound actions run before key-down listeners, including key repeats.
        if std::mem::replace(&mut self.escape_pressed, true) {
            cx.stop_propagation();
        } else if self.options_panel.borrow_mut().cancel_drag() {
            cx.stop_propagation();
            cx.notify();
        }
    }

    pub(super) fn escape(&mut self, _: &Escape, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        if self.importing_image || self.exporting_image {
            return;
        }
        if self.text_edit.is_some() {
            self.finish_text(true, window, cx);
            return;
        }
        if self.crop.is_some() {
            self.cancel_crop(cx);
            self.focus.focus(window, cx);
            return;
        }

        let canceled = self.editor.cancel();
        let tool_exited = self.tool.take().is_some();
        let measurement_canceled = self.measurement_drag.take().is_some();
        if canceled || tool_exited || measurement_canceled || self.editor.selected().is_some() {
            self.preview_position = None;
            self.hovered_handle = None;
            self.toolbar_drag_pending = false;
            if !canceled {
                self.editor.deselect();
            }
            self.focus.focus(window, cx);
            cx.notify();
            return;
        }
        if !self.focus.is_focused(window) {
            self.focus.focus(window, cx);
            return;
        }

        self.export_image(Destination::Clipboard, window, cx);
    }
}

pub(crate) fn install(cx: &mut App) {
    let primary = if cfg!(target_os = "macos") {
        "cmd"
    } else {
        "ctrl"
    };
    let editor = Some("ScreenshotEditor && !Input");
    cx.bind_keys([
        KeyBinding::new("escape", Escape, Some("ScreenshotEditor")),
        KeyBinding::new(&format!("{primary}-s"), SaveImage, Some("ScreenshotEditor")),
        KeyBinding::new(&format!("{primary}-c"), Copy, editor),
        KeyBinding::new(&format!("{primary}-shift-c"), CopyPixelColor, editor),
        KeyBinding::new(&format!("{primary}-v"), Paste, editor),
        KeyBinding::new(&format!("{primary}-z"), Undo, editor),
        KeyBinding::new(&format!("{primary}-shift-z"), Redo, editor),
        KeyBinding::new(&format!("{primary}-y"), Redo, editor),
    ]);
}
