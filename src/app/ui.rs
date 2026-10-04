use super::{ScreenshotEditor, export::Destination};
use crate::{
    color_format::ColorFormat,
    editor::{LineStyle, MeasurementAxis, RedactionStyle, TailDirection, Tool},
    measurement::MeasurementMode,
    theme::{self, TOOLBAR_GAP, TOOLBAR_HEIGHT},
};
use ::ui::{
    Button, ColorPicker, ColorSwatch, Divider, Dropdown, Icon, PRIMARY, SHIFT, Segment,
    SegmentedControl, ToolTooltip, ToolTooltipExt, window_controls,
};
use gpui::{prelude::*, *};
use gpui_base::{Toolbar, ToolbarGroup};

pub(super) fn show_error(title: &str, error: &str, window: &mut Window, cx: &mut App) {
    drop(window.prompt(
        PromptLevel::Critical,
        title,
        Some(error),
        &[PromptButton::Ok("OK".into())],
        cx,
    ));
}

impl ScreenshotEditor {
    fn text_options(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let text = self
            .text_edit
            .as_ref()
            .map(|edit| &edit.text)
            .or_else(|| self.editor.selected().and_then(|s| s.text.as_ref()));
        let bubble = text.map_or(self.preferences.text_bubble.get(), |text| text.bubble);
        let tail = text.map_or(self.preferences.text_tail.get(), |text| text.tail);
        let selected_style = bubble.then_some(tail);
        SegmentedControl::new("text-style", selected_style)
            .accessibility_label("Text style")
            .items(
                std::iter::once(None)
                    .chain(TailDirection::ALL.into_iter().map(Some))
                    .map(|style| {
                        let icon = style.map_or(Icon::LetterT, TailDirection::text_icon);
                        let label = style.map_or("Plain text", TailDirection::label);
                        Segment::icon(style, icon, label).tooltip(
                            ToolTooltip::new(label)
                                .description(if style.is_some() {
                                    "Use a speech bubble with this tail direction."
                                } else {
                                    "Display text without a speech bubble."
                                })
                                .hint("Next style", ["Space"])
                                .hint("Previous style", [SHIFT, "Space"]),
                        )
                    }),
            )
            .on_change(
                cx.listener(|this, style: &Option<TailDirection>, window, cx| {
                    this.update_text_style(*style, window, cx);
                }),
            )
    }

    fn tool_button(
        &self,
        tool: Tool,
        label: &'static str,
        icon: Icon,
        shortcut: &'static str,
        cx: &mut Context<Self>,
    ) -> Button {
        let tooltip = match tool {
            Tool::Rectangle => ToolTooltip::new(label)
                .description("Drag to draw a rectangular outline.")
                .hint("Keep square", [SHIFT]),
            Tool::RectangleFill => ToolTooltip::new(label)
                .description("Drag to fill a rectangular area with a color.")
                .hint("Keep square", [SHIFT]),
            Tool::Ellipse => ToolTooltip::new(label)
                .description("Drag to draw an oval or circle.")
                .hint("Keep circular", [SHIFT]),
            Tool::EllipseFill => ToolTooltip::new(label)
                .description("Drag to fill an oval or circle with a color.")
                .hint("Keep circular", [SHIFT]),
            Tool::Arrow => ToolTooltip::new(label)
                .description("Point to a detail. Drag the middle handle to bend the arrow."),
            Tool::Line(_) => ToolTooltip::new(label).description(
                "Draw a solid, dashed or wavy line. Snaps to horizontal or vertical when close.",
            ),
            Tool::Brush => ToolTooltip::new(label)
                .description("Draw freehand. Strokes smooth when you release."),
            Tool::Number(_) => ToolTooltip::new(label)
                .description("Click to place the next numbered label.")
                .hint("Place label", ["Enter"])
                .hint("Cancel placement", ["V", "or", "Esc"]),
            Tool::Text => ToolTooltip::new(label)
                .description("Click to add text. Double-click a text layer to edit.")
                .hint("Finish editing", ["Enter", "or", "Esc"])
                .hint("New line", [SHIFT, "Enter"]),
            Tool::Magnifier => ToolTooltip::new(label)
                .description("Drag to magnify a detail from the original image."),
            Tool::Measure(_) => ToolTooltip::new(label)
                .description(
                    "Click to measure detected edges, or drag along the chosen direction. Red guides can also be clicked or dragged. Press V to select and move measurements or guides.",
                )
                .hint("Exit tool", ["V", "or", "Esc"]),
            Tool::Redact(_) => ToolTooltip::new(label)
                .description("Cover an area using blur or mosaic."),
            Tool::Spotlight => ToolTooltip::new(label)
                .description("Keep an area bright and dim the rest of the image."),
            _ => ToolTooltip::new(label),
        };
        let tooltip = if Self::supports_scroll_adjustment(tool) {
            tooltip.hint(
                if tool == Tool::Magnifier {
                    "Adjust magnification over a lens"
                } else if tool.is_filled_shape() {
                    "Adjust opacity over a shape"
                } else if tool.has_strength() {
                    "Adjust strength over an effect"
                } else {
                    "Adjust width over a stroke"
                },
                ["Scroll"],
            )
        } else {
            tooltip
        };
        Button::icon(label, icon)
            .accessibility_label(label)
            .toggled(self.crop.is_none() && self.tool.map(Tool::family) == Some(tool.family()))
            .tool_tooltip(tooltip.shortcut([shortcut]))
            .on_click(
                cx.listener(move |this, _, window, cx| this.choose_tool(Some(tool), window, cx)),
            )
    }

    fn shape_tool_button(&self, base: Tool, cx: &mut Context<Self>) -> impl IntoElement {
        let (group_id, menu_id, trigger_id, menu_label, shortcut, variants) =
            if base == Tool::Rectangle {
                (
                    "rectangle-tools",
                    "rectangle-tool-options",
                    "rectangle-tool-options-trigger",
                    "Rectangle options",
                    "R",
                    [
                        (Tool::Rectangle, "Rectangle", Icon::Square),
                        (Tool::RectangleFill, "Filled rectangle", Icon::SquareFilled),
                    ],
                )
            } else {
                (
                    "ellipse-tools",
                    "ellipse-tool-options",
                    "ellipse-tool-options-trigger",
                    "Ellipse options",
                    "O",
                    [
                        (Tool::Ellipse, "Ellipse", Icon::Circle),
                        (Tool::EllipseFill, "Filled ellipse", Icon::CircleFilled),
                    ],
                )
            };
        let tool = self.preferences.shape_tool(base);
        let (_, label, icon) = variants[usize::from(tool.is_filled_shape())];
        let tooltip = variants.into_iter().fold(
            ToolTooltip::new(menu_label),
            |tooltip, (variant, label, _)| {
                tooltip.hint(label, (tool == variant).then_some(shortcut))
            },
        );
        ToolbarGroup::new(group_id)
            .label(menu_label)
            .gap(px(TOOLBAR_GAP))
            .child(self.tool_button(tool, label, icon, shortcut, cx))
            .child(
                Dropdown::new(
                    menu_id,
                    Button::icon(trigger_id, Icon::ChevronDown)
                        .w_5()
                        .accessibility_label(menu_label)
                        .tool_tooltip(tooltip),
                )
                .menu_label(menu_label)
                .items(variants.map(|(variant, label, icon)| {
                    Button::new(label)
                        .ghost()
                        .selected(tool == variant)
                        .child(icon)
                        .label(label)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.choose_tool(Some(variant), window, cx);
                        }))
                })),
            )
    }

    pub(super) fn tool_options(&self, tool: Tool, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = self.editor.selected();
        let is_fill = tool.is_filled_shape();
        let active_color = self.active_color(tool);
        div()
            .flex()
            .items_center()
            .gap_2()
            .when_some(self.active_measurement_option(), |this, option| {
                this.child(
                    SegmentedControl::new("measurement-option", option)
                        .accessibility_label("Measurement or guide")
                        .items(MeasurementMode::OPTIONS.map(|(mode, axis)| {
                            let (label, icon) = match (mode, axis) {
                                (MeasurementMode::Measure, MeasurementAxis::Y) =>
                                    ("Vertical measurement (Y axis)", Icon::RulerVertical),
                                (MeasurementMode::Measure, MeasurementAxis::X) =>
                                    ("Horizontal measurement (X axis)", Icon::Ruler),
                                (MeasurementMode::Guide, MeasurementAxis::Y) =>
                                    ("Vertical guide (Y axis)", Icon::GuideVertical),
                                (MeasurementMode::Guide, MeasurementAxis::X) =>
                                    ("Horizontal guide (X axis)", Icon::GuideHorizontal),
                            };
                            Segment::icon((mode, axis), icon, label).tooltip(
                                style_tooltip(ToolTooltip::new(label)
                                    .description(match mode {
                                        MeasurementMode::Measure =>
                                            "Click to measure detected edges, or drag along the chosen direction.",
                                        MeasurementMode::Guide =>
                                            "Click for a full-image guide, or drag for a shorter guide.",
                                    })
                                    .shortcut([mode.shortcut(axis)]))
                                    .hint("Place marker", ["Enter"]),
                            )
                        }))
                        .on_change(cx.listener(|this, option: &(MeasurementMode, MeasurementAxis), window, cx| {
                            this.choose_measurement_option(*option, window, cx);
                        })),
                )
            })
            .when(tool == Tool::Arrow, |this| {
                let double_headed = selected.map_or(self.preferences.arrow_double_headed.get(), |stroke| {
                    stroke.arrow_double_headed
                });
                this.child(
                    SegmentedControl::new("arrow-heads", double_headed)
                        .accessibility_label("Arrow heads")
                        .items(
                            [
                                (false, Icon::ArrowRight, "Single-headed arrow"),
                                (true, Icon::ArrowsHorizontal, "Double-headed arrow"),
                            ]
                            .into_iter()
                            .map(|(both, icon, label)| {
                                Segment::icon(both, icon, label).tooltip(style_tooltip(ToolTooltip::new(label)))
                            }),
                        )
                        .on_change(cx.listener(|this, both: &bool, window, cx| {
                            this.set_arrow_heads(*both, window, cx);
                        })),
                )
            })
            .when_some(
                match tool {
                    Tool::Line(style) => Some(style),
                    _ => None,
                },
                |this, style| {
                    this.child(
                        SegmentedControl::new("line-style", style)
                            .accessibility_label("Line style")
                            .items(
                                [
                                    (LineStyle::Solid, Icon::Line, "Solid"),
                                    (LineStyle::Dashed, Icon::LineDashed, "Dashed"),
                                    (LineStyle::Wavy, Icon::WaveSine, "Wavy"),
                                ]
                                .into_iter()
                                .map(|(style, icon, label)| {
                                    let label = format!("{label} line");
                                    Segment::icon(style, icon, label.clone())
                                        .tooltip(style_tooltip(ToolTooltip::new(label)))
                                }),
                            )
                            .on_change(cx.listener(|this, style: &LineStyle, window, cx| {
                                this.set_line_style(*style, window, cx);
                            })),
                    )
                },
            )
            .when_some(
                match tool {
                    Tool::Redact(style) => Some(style),
                    _ => None,
                },
                |this, style| {
                    this.child(
                        SegmentedControl::new("redaction-style", style)
                            .accessibility_label("Redaction style")
                            .items(
                                [
                                    (RedactionStyle::Blur, Icon::Blur, "Blur"),
                                    (RedactionStyle::Mosaic, Icon::Mosaic, "Mosaic"),
                                ]
                                .into_iter()
                                .map(|(style, icon, label)| {
                                    Segment::icon(style, icon, format!("Redact with {label}"))
                                        .tooltip(style_tooltip(ToolTooltip::new(label).description(match style {
                                            RedactionStyle::Blur => {
                                                "Soften details inside the marked area."
                                            }
                                            RedactionStyle::Mosaic => {
                                                "Pixelate the marked area into color blocks."
                                            }
                                        })))
                                }),
                            )
                            .on_change(cx.listener(|this, style: &RedactionStyle, window, cx| {
                                this.set_redaction_style(*style, window, cx);
                            })),
                    )
                },
            )
            .when(tool.has_color(), |this| {
                this.child(
                    div()
                        .id("annotation-color-options")
                        .tool_tooltip(ToolTooltip::new(if is_fill {
                            "Fill color"
                        } else if tool.is_shape() {
                            "Outline color"
                        } else {
                            "Color"
                        })
                            .hint("Previous color", [","])
                            .hint("Next color", ["."]))
                        .child(ColorPicker::new("annotation-color")
                            .selected(active_color)
                            .allow_auto(tool.has_auto_color())
                            .on_change(cx.listener(move |this, color: &Option<u32>, window, cx| {
                                this.set_tool_color(tool, *color, window, cx);
                            }))),
                )
            })
            .when(matches!(tool, Tool::Number(_)), |this| {
                let tail = selected.map_or(self.preferences.number_tail.get(), |stroke| stroke.number_tail);
                this.child(
                    SegmentedControl::new("number-tail", tail)
                        .accessibility_label("Number tail direction")
                        .items(TailDirection::ALL.into_iter().map(|direction| {
                            Segment::icon(direction, direction.number_icon(), direction.label())
                                .tooltip(
                                    style_tooltip(ToolTooltip::new(direction.label())),
                                )
                        }))
                        .on_change(cx.listener(|this, direction: &TailDirection, window, cx| {
                            this.set_number_tail(*direction, window, cx);
                        })),
                )
            })
            .when(tool == Tool::Text, |this| this.child(self.text_options(cx)))
            .when(
                Self::supports_scroll_adjustment(tool),
                |this| this.child(div()
                    .id("adjustment-options")
                    .tool_tooltip(ToolTooltip::new(if tool.is_filled_shape() {
                        format!("Opacity: {:.0}%", self.active_fill_opacity(tool) * 100.0)
                    } else if tool == Tool::Magnifier {
                        "Magnification".into()
                    } else if tool.has_strength() {
                        "Effect strength".into()
                    } else {
                        "Stroke width".into()
                    })
                        .hint("Decrease", ["["])
                        .hint("Increase", ["]"]))
                    .child(self.adjustment_slider.clone())),
            )
    }

    pub(super) fn crop_options(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap(px(2.0))
            .child(
                Button::icon("cancel-crop", Icon::X)
                    .accessibility_label("Cancel crop")
                    .tool_tooltip(ToolTooltip::new("Cancel crop").shortcut(["Esc"]))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.cancel_crop(cx);
                        this.focus.focus(window, cx);
                    })),
            )
            .child(
                Button::icon("apply-crop", Icon::Check)
                    .accessibility_label("Apply crop")
                    .tool_tooltip(ToolTooltip::new("Apply crop").shortcut(["Enter"]))
                    .disabled(!self.crop.as_ref().is_some_and(|crop| crop.can_apply()))
                    .on_click(cx.listener(|this, _, window, cx| this.apply_crop(window, cx))),
            )
    }

    fn pixel_color_indicator(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let label = ColorFormat::current(cx).format(self.sampled_color);
        let copied = self.color_copy_feedback.is_some();
        Button::new("pixel-color")
            .ghost()
            .selected(copied)
            .h(px(theme::ICON_BUTTON_SIZE))
            .flex_shrink_0()
            .accessibility_label(if copied {
                format!("Color copied. Copy color {label}")
            } else {
                format!("Copy color {label}")
            })
            .tool_tooltip(
                ToolTooltip::new("Copy color")
                    .shortcut([PRIMARY, SHIFT, "C"])
                    .description(
                        "Sample colors from the screenshot. Click to copy the last sampled color.",
                    )
                    .hint("Switch format", ["F"]),
            )
            .child(
                div()
                    .relative()
                    .text_xs()
                    .line_height(px(theme::ICON_SIZE))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .when(copied, |this| this.invisible())
                            .child(ColorSwatch::new(Some(
                                rgb(self.sampled_color & 0xffffff).into(),
                            )))
                            .child(label),
                    )
                    .when(copied, |this| {
                        this.child(
                            div()
                                .absolute()
                                .inset_0()
                                .flex()
                                .items_center()
                                .justify_center()
                                .gap_2()
                                .child(Icon::Check)
                                .child("Copied"),
                        )
                    }),
            )
            .on_click(cx.listener(|this, _, _, cx| this.copy_pixel_color(cx)))
    }

    pub(super) fn toolbar(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        Toolbar::new("toolbar")
            .aria_label("Screenshot editor")
            .flex()
            .items_center()
            .gap(px(TOOLBAR_GAP))
            .px_3()
            .when(cfg!(target_os = "windows"), |this| this.pr_0())
            .when(
                cfg!(target_os = "macos") && !window.is_fullscreen(),
                |this| {
                    #[cfg(target_os = "macos")]
                    {
                        this.pl(crate::desktop::titlebar_content_start(window))
                    }
                    #[cfg(not(target_os = "macos"))]
                    {
                        this
                    }
                },
            )
            .h(px(TOOLBAR_HEIGHT))
            .flex_shrink_0()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, _| {
                    this.toolbar_drag_pending = false;
                    if window.default_prevented() || window.is_fullscreen() {
                        return;
                    }
                    if event.click_count == 2 {
                        window.titlebar_double_click();
                    } else {
                        this.toolbar_drag_pending = true;
                    }
                }),
            )
            .on_mouse_down_out(cx.listener(|this, _, _, _| {
                this.toolbar_drag_pending = false;
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| {
                    this.toolbar_drag_pending = false;
                }),
            )
            .on_mouse_move(cx.listener(|this, event, window, cx| {
                this.move_toolbar(event, window, cx);
            }))
            .child(
                ToolbarGroup::new("export-tools")
                    .label("Export")
                    .gap(px(TOOLBAR_GAP))
                    .map(|group| {
                        #[cfg(any(target_os = "macos", target_os = "windows"))]
                        {
                            group.child(
                                Button::icon("pin-image", Icon::Pin)
                                    .accessibility_label("Pin screenshot")
                                    .tool_tooltip(
                                        ToolTooltip::new("Pin screenshot")
                                            .description("Keep this image above other windows."),
                                    )
                                    .disabled(
                                        self.importing_image
                                            || self.exporting_image
                                            || self.crop.is_some()
                                            || self.editor.is_dragging(),
                                    )
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.export_image(Destination::Pinned, window, cx);
                                    })),
                            )
                        }
                        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
                        {
                            group
                        }
                    })
                    .child(
                        Button::icon("copy-image", Icon::Copy)
                            .accessibility_label("Copy image")
                            .tool_tooltip(
                                ToolTooltip::new("Copy image")
                                    .shortcut([PRIMARY, "C", "or", "Esc"])
                                    .description("Copy the annotated image and close this window."),
                            )
                            .disabled(self.importing_image || self.exporting_image)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.export_image(Destination::Clipboard, window, cx);
                            })),
                    )
                    .child(
                        Dropdown::new(
                            "image-options",
                            Button::icon("image-options-trigger", Icon::ChevronDown)
                                .w_5()
                                .accessibility_label("Image options")
                                .tool_tooltip(
                                    ToolTooltip::new("Image options")
                                        .hint("Save as image", [PRIMARY, "S"]),
                                ),
                        )
                        .menu_label("Image options")
                        .disabled(self.importing_image || self.exporting_image)
                        .items([Button::new("save-image")
                            .ghost()
                            .child(Icon::DeviceFloppy)
                            .label("Save as image")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.export_image(Destination::File, window, cx);
                            }))]),
                    ),
            )
            .child(Divider::vertical().mx_1().h(px(22.)))
            .child(
                ToolbarGroup::new("annotation-tools")
                    .label("Annotations")
                    .gap(px(TOOLBAR_GAP))
                    .child(
                        Button::icon("select", Icon::Pointer)
                            .accessibility_label("Select, move and resize")
                            .tool_tooltip(
                                ToolTooltip::new("Select")
                                    .shortcut(["V"])
                                    .description("Select an annotation to move or resize it.")
                                    .hint("Nudge selection", ["↑", "↓", "←", "→"])
                                    .hint("Move faster", [SHIFT, "↑↓←→"])
                                    .hint("Remove selection", ["Delete"]),
                            )
                            .toggled(self.crop.is_none() && self.tool.is_none())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.choose_tool(None, window, cx)
                            })),
                    )
                    .child(self.shape_tool_button(Tool::Rectangle, cx))
                    .child(self.shape_tool_button(Tool::Ellipse, cx))
                    .child(self.tool_button(Tool::Arrow, "Arrow", Icon::ArrowUpRight, "A", cx))
                    .child(self.tool_button(
                        Tool::Line(LineStyle::Solid),
                        "Line",
                        Icon::Line,
                        "S",
                        cx,
                    ))
                    .child(self.tool_button(Tool::Brush, "Brush", Icon::Brush, "B", cx))
                    .child(self.tool_button(Tool::Text, "Text", Icon::LetterT, "T", cx))
                    .child(self.tool_button(
                        Tool::Number(0),
                        "Number",
                        Icon::NumberTailBottomLeft,
                        "N",
                        cx,
                    ))
                    .child(self.tool_button(
                        Tool::Redact(self.preferences.redaction_style.get()),
                        "Redact",
                        Icon::Eraser,
                        "E",
                        cx,
                    ))
                    .child(self.tool_button(Tool::Spotlight, "Spotlight", Icon::Spotlight, "H", cx))
                    .child(self.tool_button(Tool::Magnifier, "Magnifier", Icon::Magnifier, "L", cx))
                    .child(self.tool_button(
                        Tool::Measure(MeasurementAxis::Y),
                        "Measure & guides",
                        Icon::Ruler,
                        "1",
                        cx,
                    ))
                    .child(
                        Dropdown::new(
                            "more-tools",
                            Button::icon("more-tools-trigger", Icon::ChevronDown)
                                .w_5()
                                .accessibility_label("More tools")
                                .tool_tooltip(
                                    ToolTooltip::new("More tools")
                                        .hint("Crop image", ["C"])
                                        .hint("Paste from clipboard", [PRIMARY, "V"]),
                                ),
                        )
                        .menu_label("More tools")
                        .disabled(self.text_edit.is_some())
                        .items([
                            Button::new("crop-image")
                                .ghost()
                                .child(Icon::Crop)
                                .label("Crop image")
                                .toggled(self.crop.is_some())
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.start_crop(window, cx);
                                })),
                            Button::new("import-image")
                                .ghost()
                                .child(Icon::PhotoPlus)
                                .label("Add image")
                                .disabled(self.importing_image)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.import_image(window, cx);
                                })),
                        ]),
                    ),
            )
            .child(div().flex_1())
            .child(self.pixel_color_indicator(cx))
            .child(Divider::vertical().mx_1().h(px(22.)))
            .child(
                ToolbarGroup::new("history-tools")
                    .label("History")
                    .gap(px(TOOLBAR_GAP))
                    .child(
                        Button::icon("undo", Icon::ArrowBackUp)
                            .accessibility_label("Undo")
                            .tool_tooltip(ToolTooltip::new("Undo").shortcut([PRIMARY, "Z"]))
                            .disabled(!self.editor.can_undo() || self.text_edit.is_some())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.undo();
                                this.focus.focus(window, cx);
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::icon("redo", Icon::ArrowForwardUp)
                            .accessibility_label("Redo")
                            .tool_tooltip(ToolTooltip::new("Redo").shortcut([PRIMARY, SHIFT, "Z"]))
                            .disabled(!self.editor.can_redo() || self.text_edit.is_some())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.redo();
                                this.focus.focus(window, cx);
                                cx.notify();
                            })),
                    ),
            )
            .child(Divider::vertical().mx_1().h(px(22.)))
            .child(
                Button::icon("clear", Icon::Restore)
                    .accessibility_label("Reset to original image")
                    .tool_tooltip(
                        ToolTooltip::new("Reset image")
                            .description(
                                "Remove all annotations and restore the original, uncropped image.",
                            )
                            .hint("Undo reset", [PRIMARY, "Z"]),
                    )
                    .disabled(!self.editor.can_clear() || self.text_edit.is_some())
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.editor.clear();
                        this.crop = None;
                        this.reset_pointer();
                        this.focus.focus(window, cx);
                        cx.notify();
                    })),
            )
            .when(
                cfg!(target_os = "windows") && !window.is_fullscreen(),
                |this| this.child(window_controls(window, cx)),
            )
            .when(cfg!(target_os = "linux"), |this| {
                this.child(
                    Button::icon("close-editor", Icon::X)
                        .accessibility_label("Close editor")
                        .tool_tooltip(ToolTooltip::new("Close editor"))
                        .on_click(cx.listener(|this, _, window, cx| this.close(window, cx))),
                )
            })
    }
}

fn style_tooltip(tooltip: ToolTooltip) -> ToolTooltip {
    tooltip
        .hint("Next style", ["Space"])
        .hint("Previous style", [SHIFT, "Space"])
}
