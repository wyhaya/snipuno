use super::*;
use gpui_base::{
    Theme,
    input::{Copy, Paste, Redo, Undo},
};

impl Render for ScreenshotEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match self
            .sampling_position
            .and_then(|position| self.pixel_color_at(position))
        {
            Some(color) => self.sampled_color = color,
            None => self.sampling_position = None,
        }
        self.sync_adjustment_slider(window, cx);
        self.prepare_effects(window, cx);
        self.prepare_image_layers(window, cx);
        let colors = Theme::global(cx).tokens.colors;
        div()
            .id("screenshot-editor")
            .relative()
            .track_focus(&self.focus)
            .key_context("ScreenshotEditor")
            .map(|element| {
                desktop::window_actions(
                    element,
                    cx.listener(|this, _, window, cx| this.close(window, cx)),
                )
            })
            .size_full()
            .flex()
            .flex_col()
            .bg(colors.background)
            .text_color(colors.foreground)
            .capture_action(cx.listener(Self::capture_escape))
            .capture_key_up(cx.listener(|this, event: &KeyUpEvent, _, _| {
                this.pressed_nudge_keys
                    .remove(&event.keystroke.key.to_lowercase());
                if event.keystroke.key.eq_ignore_ascii_case("escape") {
                    this.escape_pressed = false;
                }
            }))
            .on_key_down(cx.listener(Self::key_down))
            .on_action(cx.listener(|this, _: &CopyPixelColor, _, cx| {
                this.copy_pixel_color(cx);
            }))
            .when(!self.importing_image && !self.exporting_image, |element| {
                element.on_action(cx.listener(|this, _: &SaveImage, window, cx| {
                    this.export_image(Destination::File, window, cx);
                }))
            })
            .when(self.text_edit.is_none(), |element| {
                element
                    .when(self.editor.can_undo() || self.crop.is_some(), |element| {
                        element.on_action(cx.listener(|this, _: &Undo, _, cx| {
                            this.undo();
                            cx.notify();
                        }))
                    })
                    .when(self.editor.can_redo(), |element| {
                        element.on_action(cx.listener(|this, _: &Redo, _, cx| {
                            this.redo();
                            cx.notify();
                        }))
                    })
                    .when(!self.importing_image && !self.exporting_image, |element| {
                        element
                            .on_action(cx.listener(|this, _: &Copy, window, cx| {
                                this.export_image(Destination::Clipboard, window, cx);
                            }))
                            .on_action(cx.listener(|this, _: &Paste, window, cx| {
                                this.paste_image(window, cx);
                            }))
                    })
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    if !window.default_prevented() {
                        this.finish_text(true, window, cx);
                    }
                }),
            )
            .can_drop({
                let ready = !self.importing_image;
                move |value, _, _| ready && value.is::<ExternalPaths>()
            })
            .on_drop(cx.listener(Self::drop_images))
            .on_action(cx.listener(Self::escape))
            .on_modifiers_changed(
                cx.listener(|this, event: &ModifiersChangedEvent, window, cx| {
                    if let Some(crop) = this.crop.as_mut()
                        && crop.is_dragging()
                        && let Some(viewport) = this.viewport.get()
                    {
                        crop.update(
                            viewport.to_image(window.mouse_position()),
                            event.modifiers.shift,
                        );
                        cx.notify();
                    } else if this.editor.is_dragging()
                        && let Some(viewport) = this.viewport.get()
                    {
                        this.editor.update_constrained(
                            viewport.to_image(window.mouse_position()),
                            viewport.scale,
                            event.modifiers.shift,
                        );
                        cx.notify();
                    }
                }),
            )
            .child(self.toolbar(window, cx))
            .child(self.workspace(window, cx))
    }
}
