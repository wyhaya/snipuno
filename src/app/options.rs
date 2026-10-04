use super::*;
use crate::editor::GuideOrientation;

fn cycle_option<T: Copy + PartialEq>(options: &[T], current: T, reverse: bool) -> T {
    let index = options.iter().position(|option| *option == current);
    let next = match index {
        Some(index) if reverse => (index + options.len() - 1) % options.len(),
        Some(index) => (index + 1) % options.len(),
        None if reverse => options.len() - 1,
        None => 0,
    };
    options[next]
}

impl ScreenshotEditor {
    pub(super) fn active_measurement_option(&self) -> Option<(MeasurementMode, MeasurementAxis)> {
        match self.active_tool()? {
            Tool::Measure(axis) => Some((
                if self.editor.selected().is_some() {
                    MeasurementMode::Measure
                } else {
                    self.preferences.measurement_mode.get()
                },
                axis,
            )),
            Tool::Guide(orientation) => Some((
                MeasurementMode::Guide,
                match orientation {
                    GuideOrientation::Horizontal => MeasurementAxis::X,
                    GuideOrientation::Vertical => MeasurementAxis::Y,
                },
            )),
            _ => None,
        }
    }

    pub(super) fn choose_measurement_option(
        &mut self,
        (mode, axis): (MeasurementMode, MeasurementAxis),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.preferences.measurement_mode.set(mode);
        self.preferences.measurement_axis.set(axis);
        self.choose_tool(Some(Tool::Measure(axis)), window, cx);
    }

    pub(super) fn set_arrow_heads(
        &mut self,
        both: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.preferences.arrow_double_headed.set(both);
        self.editor.set_arrow_double_headed(both);
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn set_line_style(
        &mut self,
        style: LineStyle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.preferences.line_style.set(style);
        self.editor.set_selected_line_style(style);
        if matches!(self.tool, Some(Tool::Line(_))) {
            self.tool = Some(Tool::Line(style));
        }
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn set_redaction_style(
        &mut self,
        style: RedactionStyle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.preferences.redaction_style.set(style);
        self.editor.set_redaction_style(style);
        if self.tool.is_some_and(Tool::is_obscuring) {
            self.tool = Some(Tool::Redact(style));
        }
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn set_number_tail(
        &mut self,
        tail: TailDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.preferences.number_tail.set(tail);
        self.editor.set_number_tail(tail);
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn cycle_style(
        &mut self,
        reverse: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tool) = self.active_tool() else {
            return;
        };
        let selected = self.editor.selected();
        match tool {
            Tool::Arrow => {
                let both = selected.map_or(self.preferences.arrow_double_headed.get(), |s| {
                    s.arrow_double_headed
                });
                self.set_arrow_heads(!both, window, cx);
            }
            Tool::Line(style) => {
                let next = cycle_option(
                    &[LineStyle::Solid, LineStyle::Dashed, LineStyle::Wavy],
                    style,
                    reverse,
                );
                self.set_line_style(next, window, cx);
            }
            Tool::Redact(style) => {
                let next = cycle_option(
                    &[RedactionStyle::Blur, RedactionStyle::Mosaic],
                    style,
                    reverse,
                );
                self.set_redaction_style(next, window, cx);
            }
            Tool::Number(_) => {
                let tail = selected.map_or(self.preferences.number_tail.get(), |s| s.number_tail);
                self.set_number_tail(tail.cycle(reverse), window, cx);
            }
            Tool::Text => {
                let style = selected.and_then(|s| s.text.as_ref()).map_or(
                    self.preferences
                        .text_bubble
                        .get()
                        .then_some(self.preferences.text_tail.get()),
                    |text| text.bubble.then_some(text.tail),
                );
                self.update_text_style(
                    TextAnnotation::cycle_bubble_style(style, reverse),
                    window,
                    cx,
                );
            }
            Tool::Measure(_) | Tool::Guide(_) => {
                if let Some(option) = self.active_measurement_option() {
                    let next = cycle_option(&MeasurementMode::OPTIONS, option, reverse);
                    self.choose_measurement_option(next, window, cx);
                }
            }
            _ => {}
        }
    }

    pub(super) fn active_color(&self, tool: Tool) -> Option<u32> {
        if tool.is_filled_shape() {
            return self.editor.selected().map_or_else(
                || self.preferences.fill_color(tool),
                |stroke| stroke.fill_color,
            );
        }
        if tool == Tool::Brush
            && self
                .editor
                .selected()
                .map_or(self.preferences.brush_auto_color.get(), |stroke| {
                    stroke.brush_auto_color
                })
        {
            return None;
        }
        Some(self.text_edit.as_ref().map_or_else(
            || {
                self.editor
                    .selected()
                    .map_or(self.preferences.color.get(), |s| s.color)
            },
            |edit| edit.color,
        ))
    }

    pub(super) fn set_tool_color(
        &mut self,
        tool: Tool,
        color: Option<u32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if tool.is_filled_shape() {
            self.set_fill_color(color, window, cx);
            return;
        }
        if tool == Tool::Brush {
            self.preferences.brush_auto_color.set(color.is_none());
        }
        if let Some(color) = color {
            self.preferences.color.set(color);
            if let Some(edit) = self.text_edit.as_mut() {
                edit.color = color;
                let input = edit.input.clone();
                self.refresh_text_preview(window, cx);
                input.update(cx, |input, cx| input.focus(window, cx));
            } else {
                self.editor.set_selected_color(color);
            }
        } else if tool == Tool::Brush {
            self.apply_brush_auto_color();
        } else {
            return;
        }
        if self.text_edit.is_none() {
            self.focus.focus(window, cx);
        }
        cx.notify();
    }

    pub(super) fn cycle_color(
        &mut self,
        reverse: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tool) = self.active_tool().filter(|tool| tool.has_color()) else {
            return;
        };
        let colors: Vec<_> = tool
            .has_auto_color()
            .then_some(None)
            .into_iter()
            .chain(COLORS.map(|(color, _)| Some(color)))
            .collect();
        let next = cycle_option(&colors, self.active_color(tool), reverse);
        self.set_tool_color(tool, next, window, cx);
    }

    pub(super) fn sync_adjustment_slider(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(tool) = self.active_tool() else {
            return false;
        };
        let (label, range, value, step) = if tool == Tool::Magnifier {
            let zoom = self
                .editor
                .selected()
                .and_then(|stroke| stroke.magnifier.as_ref())
                .map_or(self.preferences.magnifier_zoom.get(), |magnifier| {
                    magnifier.zoom
                });
            (
                format!("Magnification: {zoom:.1}×"),
                Magnifier::MIN_ZOOM..=Magnifier::MAX_ZOOM,
                zoom,
                Magnifier::ZOOM_STEP,
            )
        } else if tool.is_filled_shape() {
            let opacity = self.active_fill_opacity(tool);
            (
                format!("Opacity: {:.0}%", opacity * 100.0),
                0.0..=100.0,
                opacity * 100.0,
                10.0,
            )
        } else if tool.has_strength() {
            let strength = self
                .editor
                .selected()
                .map_or(self.preferences.effect_strength.get(), |s| {
                    s.effect_strength
                });
            let label = if tool == Tool::Redact(RedactionStyle::Blur) {
                "Blur strength"
            } else {
                "Mosaic strength"
            };
            (
                format!("{label}: {}", strength.label()),
                0.0..=(EffectStrength::ALL.len() - 1) as f32,
                strength.index() as f32,
                1.0,
            )
        } else if tool.has_width() {
            (
                "Stroke width (logical pixels)".into(),
                MIN_STROKE_WIDTH..=MAX_STROKE_WIDTH,
                self.active_width(),
                STROKE_WIDTH_STEP,
            )
        } else {
            return false;
        };
        self.adjustment_slider.update(cx, |slider, cx| {
            slider.sync(label, range, value, step, window, cx);
        });
        true
    }

    pub(super) fn option_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let modifiers = event.keystroke.modifiers;
        if modifiers.platform
            || modifiers.control
            || modifiers.alt
            || !self.focus.is_focused(window)
            || self.crop.is_some()
            || self.text_edit.is_some()
            || self.editor.is_dragging()
            || self.measurement_drag.is_some()
            || self.options_panel.borrow().is_dragging()
        {
            return false;
        }
        let Some(tool) = self.active_tool() else {
            return false;
        };
        match event.keystroke.key.as_str() {
            "space" if tool.has_style_cycle() => {
                if !event.is_held {
                    self.cycle_style(modifiers.shift, window, cx);
                }
            }
            "," | "." if !modifiers.shift && tool.has_color() => {
                if !event.is_held {
                    self.cycle_color(event.keystroke.key == ",", window, cx);
                }
            }
            "[" | "]" if !modifiers.shift && self.sync_adjustment_slider(window, cx) => {
                self.adjustment_slider.update(cx, |slider, cx| {
                    if event.keystroke.key == "[" {
                        slider.decrement(window, cx);
                    } else {
                        slider.increment(window, cx);
                    }
                });
            }
            _ => return false,
        }
        true
    }
}
