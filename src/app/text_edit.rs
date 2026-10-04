use super::*;
use crate::canvas::{measure_text, text_bubble_path};
use gpui::canvas;
use gpui_base::{
    Theme,
    input::{InputEditorStyle, InputEvent, Textarea, WrappingIndent},
};

// The unstyled textarea reserves this space for its caret and horizontal scrolling.
const INPUT_RIGHT_MARGIN: f32 = 10.0;

impl ScreenshotEditor {
    pub(super) fn start_text(
        &mut self,
        position: ImagePoint,
        existing: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(viewport) = self.viewport.get().filter(|v| v.scale > 0.0) else {
            return;
        };
        self.finish_text(true, window, cx);
        let selected = self
            .editor
            .selected()
            .filter(|s| existing && s.text.is_some());
        if existing && selected.is_none() {
            return;
        }
        let font_size = DEFAULT_TEXT_SIZE / viewport.scale;
        let mut text = selected
            .and_then(|s| s.text.clone())
            .unwrap_or(TextAnnotation {
                content: Arc::default(),
                font_size,
                wrap_width: self.editor.image_bounds().width,
                bubble: self.preferences.text_bubble.get(),
                tail: self.preferences.text_tail.get(),
            });
        let color = selected.map_or(self.preferences.color.get(), |s| s.color);
        let position = selected.map_or_else(|| text.placement_origin(position), |s| s.start);
        if selected.is_none() {
            text.wrap_width = (self.editor.image_bounds().width - position.x)
                .max(text.font_size + text.padding().x * 2.0 + text.tail_width());
        }
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .rows(1)
                .submit_on_enter(true)
                .wrapping_indent(WrappingIndent::None)
                .cursor_surrounding_lines(Some(0))
                .placeholder("")
                .default_value(text.content.clone())
        });
        let subscription = cx.subscribe_in(
            &input,
            window,
            |this, input, event: &InputEvent, window, cx| {
                if !this
                    .text_edit
                    .as_ref()
                    .is_some_and(|edit| edit.input == *input)
                {
                    return;
                }
                match event {
                    InputEvent::Change | InputEvent::PressEnter { shift: true, .. } => {
                        this.refresh_text_preview(window, cx);
                    }
                    InputEvent::PressEnter { shift: false, .. } => {
                        let composing = input.update(cx, |input, cx| {
                            input.marked_text_range(window, cx).is_some()
                        });
                        if !composing {
                            this.finish_text(true, window, cx);
                        }
                    }
                    InputEvent::Blur => {
                        let input = input.clone();
                        cx.defer_in(window, move |this, window, cx| {
                            if this
                                .text_edit
                                .as_ref()
                                .is_some_and(|edit| edit.input == input)
                                && !input.read(cx).focus_handle(cx).is_focused(window)
                            {
                                this.finish_text(true, window, cx);
                            }
                        });
                    }
                    InputEvent::Focus => {}
                }
            },
        );
        let observation = cx.observe_in(&input, window, |this, input, window, cx| {
            let changed = this.text_edit.as_ref().is_some_and(|edit| {
                edit.input == input && edit.text.content.as_ref() != input.read(cx).value().as_str()
            });
            if changed {
                this.refresh_text_preview(window, cx);
            }
        });
        if !existing {
            self.editor.deselect();
            self.tool = Some(Tool::Text);
        }
        self.text_edit = Some(TextEdit {
            input: input.clone(),
            position,
            text,
            color,
            existing,
            preview: None,
            input_size: ImagePoint::default(),
            _subscriptions: vec![subscription, observation],
        });
        input.update(cx, |input, cx| input.focus(window, cx));
        self.hovered_handle = None;
        self.refresh_text_preview(window, cx);
    }

    pub(super) fn refresh_text_preview(&mut self, window: &Window, cx: &mut Context<Self>) {
        let scale = self
            .viewport
            .get()
            .filter(|v| v.scale > 0.0)
            .map_or(1.0, |v| v.scale);
        if let Some(edit) = self.text_edit.as_mut() {
            edit.text.content = edit.input.read(cx).value().as_str().into();
            edit.preview = measure_text(&edit.text, scale, window, cx).and_then(|measured| {
                edit.input_size = measured;
                if edit.existing {
                    self.editor
                        .selected_text_preview(edit.text.clone(), edit.color, measured)
                } else {
                    self.editor
                        .text_preview(edit.position, edit.text.clone(), edit.color, measured)
                }
            });
        }
        cx.notify();
    }

    pub(super) fn finish_text(
        &mut self,
        commit: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.text_edit.is_none() {
            return;
        }
        if commit {
            self.refresh_text_preview(window, cx);
        }
        let edit = self.text_edit.take().unwrap();
        let restore_focus = edit.input.read(cx).focus_handle(cx).is_focused(window);
        if commit {
            if let Some(preview) = edit.preview {
                let measured = ImagePoint::new(preview.bounds().width, preview.bounds().height);
                if edit.existing {
                    self.editor
                        .set_selected_text(edit.text, edit.color, measured);
                } else {
                    self.editor
                        .add_text(edit.position, edit.text, edit.color, measured);
                }
            } else if edit.existing && edit.text.content.trim().is_empty() {
                self.editor.delete_selected();
            }
        }
        if self.tool == Some(Tool::Text) {
            self.tool = None;
            self.preview_position = None;
        }
        self.hovered_handle = None;
        if restore_focus {
            self.focus.focus(window, cx);
        }
        cx.notify();
    }

    pub(super) fn update_text_style(
        &mut self,
        tail: Option<TailDirection>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.preferences.text_bubble.set(tail.is_some());
        if let Some(tail) = tail {
            self.preferences.text_tail.set(tail);
        }
        self.update_text(
            |text| {
                text.bubble = tail.is_some();
                if let Some(tail) = tail {
                    text.tail = tail;
                }
            },
            window,
            cx,
        );
    }

    fn update_text(
        &mut self,
        update: impl FnOnce(&mut TextAnnotation),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(edit) = self.text_edit.as_mut() {
            update(&mut edit.text);
            let input = edit.input.clone();
            self.refresh_text_preview(window, cx);
            input.update(cx, |input, cx| input.focus(window, cx));
        } else if let Some(selected) = self.editor.selected().cloned() {
            if let Some(mut text) = selected.text {
                update(&mut text);
                let scale = self
                    .viewport
                    .get()
                    .filter(|v| v.scale > 0.0)
                    .map_or(1.0, |v| v.scale);
                if let Some(measured) = measure_text(&text, scale, window, cx) {
                    self.editor
                        .set_selected_text(text, selected.color, measured);
                }
            }
            self.focus.focus(window, cx);
        } else {
            self.focus.focus(window, cx);
        }
        cx.notify();
    }

    pub(super) fn inline_text_input(&self, cx: &mut Context<Self>) -> AnyElement {
        let (Some(edit), Some(viewport)) = (&self.text_edit, self.viewport.get()) else {
            return div().into_any_element();
        };
        if viewport.scale <= 0.0 {
            return div().into_any_element();
        }
        let font_size = edit.text.font_size * viewport.scale;
        let line_height = edit.text.line_height() * viewport.scale;
        let padding = edit.text.padding();
        let offset = edit.text.content_offset();
        let bounds = edit.preview.as_ref().map(Stroke::bounds);
        let position = bounds.map_or(edit.position, |b| ImagePoint::new(b.x, b.y));
        let horizontal_inset = (offset.x + padding.x) * viewport.scale;
        let width = edit.text.content_width() * viewport.scale + horizontal_inset;
        let height = (edit.input_size.y * viewport.scale)
            .ceil()
            .max(line_height + padding.y * viewport.scale * 2.0)
            .min(f32::from(viewport.bounds.size.height));
        let content_width = if edit.text.content.is_empty() {
            font_size * 0.6 + horizontal_inset
        } else {
            edit.input_size.x * viewport.scale
        };
        let origin = viewport.to_window(position);
        let content_bounds =
            Bounds::new(origin, size(px(content_width), px(height))).dilate(px(2.0));
        let foreground = Hsla::from(rgba(edit.text.foreground_color(edit.color).rotate_left(8)));
        edit.input.update(cx, |input, _| {
            input.set_editor_style(InputEditorStyle {
                foreground,
                caret: foreground,
                ..Default::default()
            });
        });
        let input = div()
            .id("inline-text-input")
            .relative()
            .w(px(width + INPUT_RIGHT_MARGIN))
            .h(px(height))
            .pl(px(offset.x * viewport.scale))
            .pr(px(padding.x * viewport.scale))
            .py(px(padding.y * viewport.scale))
            .font_family(Theme::global(cx).tokens.typography.sans)
            .text_size(px(font_size))
            .line_height(px(line_height))
            .text_color(foreground)
            .cursor(CursorStyle::IBeam)
            .occlude()
            .capture_any_mouse_down(
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    if event.button != MouseButton::Left || content_bounds.contains(&event.position)
                    {
                        return;
                    }
                    window.prevent_default();
                    if viewport.contains(event.position) {
                        this.pointer_down(event, window, cx);
                    } else {
                        this.finish_text(true, window, cx);
                    }
                    cx.stop_propagation();
                }),
            )
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .when(edit.text.bubble, |this| {
                let text = edit.text.clone();
                let color = rgba(edit.color.rotate_left(8));
                this.child(
                    canvas(
                        |bounds, _, _| bounds,
                        move |_, bounds, window, _| {
                            if let Some(path) = text_bubble_path(&text, bounds, viewport.scale) {
                                window.paint_path(path, color);
                            }
                        },
                    )
                    .absolute()
                    .top_0()
                    .left_0()
                    .w(px(content_width))
                    .h(px(height)),
                )
            })
            .child(Textarea::new(&edit.input));
        canvas(
            move |_, window, cx| {
                let mut input = input.into_any_element();
                input.layout_as_root(AvailableSpace::min_size(), window, cx);
                window.with_content_mask(
                    Some(ContentMask {
                        bounds: viewport.bounds,
                    }),
                    |window| {
                        input.prepaint_at(origin, window, cx);
                    },
                );
                input
            },
            move |_, mut input, window, cx| {
                window.with_content_mask(
                    Some(ContentMask {
                        bounds: viewport.bounds,
                    }),
                    |window| {
                        input.paint(window, cx);
                    },
                );
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .into_any_element()
    }
}
