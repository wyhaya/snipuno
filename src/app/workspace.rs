use super::*;
use crate::canvas::{
    paint_crop, paint_hover, paint_magnifier, paint_selection, paint_spotlights, paint_stroke,
    prepare_measurement_label, rounded_corners, stroke_path,
};
use crate::selection;
use gpui_base::Theme;

impl ScreenshotEditor {
    pub(super) fn workspace(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let colors = Theme::global(cx).tokens.colors;
        let image = self.source.rendered.clone();
        let dimensions = self.source.pixels.dimensions();
        let image_alignment = self.image_alignment;
        let dimension_display = self.dimension_display();
        let fill_previews = self.fill_previews.clone();
        let image_bounds = self.editor.image_bounds();
        let source_offset = ImagePoint::new(image_bounds.x, image_bounds.y);
        let crop = self.crop.clone();
        let viewport_cell = self.viewport.clone();
        let editor = cx.weak_entity();
        let marks: Vec<_> = self
            .editor
            .strokes()
            .iter()
            .chain(self.editor.draft())
            .filter(|stroke| {
                !self.text_edit.as_ref().is_some_and(|edit| {
                    edit.existing
                        && self
                            .editor
                            .selected()
                            .is_some_and(|selected| std::ptr::eq(*stroke, selected))
                })
            })
            .cloned()
            .map(|mut stroke| {
                prepare_measurement_label(
                    &mut stroke,
                    dimension_display,
                    ImagePoint::new(image_bounds.width, image_bounds.height),
                    window,
                );
                stroke
            })
            .collect();
        let selected = self
            .editor
            .selected()
            .cloned()
            .filter(|_| self.text_edit.is_none() && self.crop.is_none());
        let preview = self
            .measurement_preview(window)
            .or_else(|| self.stamp_preview())
            .map(|mut stroke| {
                prepare_measurement_label(
                    &mut stroke,
                    dimension_display,
                    ImagePoint::new(image_bounds.width, image_bounds.height),
                    window,
                );
                stroke
            });
        let alignment_preview = self.measurement_alignment_preview();
        let dimension_target_is_draft = self.editor.draft().is_some();
        let dimension_target = self
            .editor
            .draft()
            .cloned()
            .or_else(|| preview.clone())
            .or_else(|| selected.clone());
        let hovered = if self.tool.is_none()
            && !self.editor.is_dragging()
            && self.text_edit.is_none()
            && self.crop.is_none()
        {
            self.viewport
                .get()
                .and_then(|v| {
                    self.hover_position
                        .and_then(|p| self.editor.hovered_layer(p, v.scale))
                })
                .cloned()
        } else {
            None
        };
        let show_hovered_dimensions = hovered
            .as_ref()
            .is_some_and(|stroke| self.editor.selected() != Some(stroke));
        let over_layer = self.viewport.get().is_some_and(|v| {
            self.hover_position.is_some_and(|p| {
                self.edits_at(p, v.scale) && self.editor.hovered_layer(p, v.scale).is_some()
            })
        });
        let mosaic = self.mosaic.clone();
        let mosaic_ready = self.mosaic_ready;
        let blurred_images = self.blurred_images.clone();
        let overlay_images = self.overlay_images.clone();
        let flipped_images = self.flipped_images.clone();
        let magnifier_previews = self.magnifier_previews.clone();
        let mouse_move = cx.listener(Self::pointer_move);
        let mouse_up = cx.listener(Self::pointer_up);
        let mouse_exit = cx.listener(|this, _: &MouseExitEvent, _, cx| {
            this.sampling_position = None;
            this.clear_hover(cx);
        });

        div()
            .id("image-workspace")
            .relative()
            .flex_1()
            .min_h_0()
            .w_full()
            .cursor(
                if self.options_panel.borrow().is_dragging() {
                    CursorStyle::ClosedHand
                } else if let Some(handle) = self.editor.active_resize_handle().or(self
                    .hovered_handle
                    .filter(|_| (selected.is_some() || crop.is_some()) && !self.editor.is_dragging()))
                {
                    match handle {
                        ResizeHandle::TopLeft | ResizeHandle::BottomRight => {
                            CursorStyle::ResizeUpLeftDownRight
                        }
                        ResizeHandle::TopRight | ResizeHandle::BottomLeft => {
                            CursorStyle::ResizeUpRightDownLeft
                        }
                        ResizeHandle::Top | ResizeHandle::Bottom => CursorStyle::ResizeUpDown,
                        ResizeHandle::Left | ResizeHandle::Right => CursorStyle::ResizeLeftRight,
                        ResizeHandle::ArrowStart
                        | ResizeHandle::ArrowEnd
                        | ResizeHandle::ArrowMiddle => CursorStyle::Crosshair,
                    }
                } else if crop.as_ref().is_some_and(CropSelection::is_dragging) {
                    CursorStyle::Crosshair
                } else if let Some(crop) = &crop {
                    if crop.can_apply() && self.hover_position.is_some_and(|p| crop.contains(p)) {
                        CursorStyle::OpenHand
                    } else {
                        CursorStyle::Crosshair
                    }
                } else if self.editor.is_dragging() && self.editor.draft().is_none() {
                    CursorStyle::ClosedHand
                } else if over_layer {
                    CursorStyle::OpenHand
                } else if self.tool == Some(Tool::Text) {
                    CursorStyle::IBeam
                } else if self.tool.is_some() {
                    CursorStyle::Crosshair
                } else {
                    CursorStyle::Arrow
                },
            )
            .when(matches!(self.tool, Some(Tool::Measure(_))), |this| {
                this.hover_listener_mode(HoverListenerMode::InputModalityIndependent)
            })
            .on_hover(cx.listener(|this, hovered: &bool, window, cx| {
                if !hovered
                    && !(matches!(this.tool, Some(Tool::Measure(_)))
                        && this
                            .canvas_position(window.mouse_position())
                            .is_some())
                {
                    this.clear_hover(cx);
                }
            }))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::pointer_down))
            .on_scroll_wheel(cx.listener(Self::pointer_scroll))
            .child(
                gpui::canvas(
                    move |bounds, window, cx| {
                        let viewport = ImageViewport::fit(
                            image_bounds.width,
                            image_bounds.height,
                            bounds,
                            window.scale_factor(),
                        )
                        .aligned(bounds, image_alignment);
                        let previous = viewport_cell.replace(Some(viewport));
                        if previous.is_none_or(|previous| {
                            previous.scale != viewport.scale || previous.bounds != viewport.bounds
                        }) {
                            let editor = editor.clone();
                            cx.defer(move |cx| {
                                let _ = editor.update(cx, |_, cx| cx.notify());
                            });
                        }
                        viewport
                    },
                    move |_, viewport, window, cx| {
                        let source_bounds = viewport.source_bounds(image_bounds, dimensions);
                        window.paint_quad(fill(viewport.bounds, colors.background));
                        window.paint_quad(fill(
                            viewport.bounds,
                            checkerboard(
                                colors.muted,
                                theme::CHECKERBOARD_SIZE * window.scale_factor(),
                            ),
                        ));
                        if let Err(error) = window.paint_image(
                            viewport.bounds,
                            source_bounds,
                            Corners::default(),
                            image.clone(),
                            0,
                            false,
                        ) {
                            eprintln!("Failed to display image: {error}");
                        }
                        window.with_content_mask(
                            Some(ContentMask {
                                bounds: viewport.bounds,
                            }),
                            |window| {
                                paint_spotlights(&marks, viewport, window);
                                for mark in &marks {
                                    if (mark.tool == Tool::Redact(RedactionStyle::Blur)
                                        && !blurred_images.contains_key(&mark.effect_strength))
                                        || (mark.tool == Tool::Redact(RedactionStyle::Mosaic) && !mosaic_ready)
                                        || matches!(mark.tool, Tool::Image(id)
                                            if (mark.end.x < mark.start.x || mark.end.y < mark.start.y)
                                                && !flipped_images.contains_key(&(id, mark.end.x < mark.start.x, mark.end.y < mark.start.y)))
                                    {
                                        continue;
                                    }
                                    if mark.is_filled_shape() && mark.fill_color.is_none() {
                                        if let Some(color) = fill_previews.borrow_mut().color(mark.bounds(), source_offset)
                                            && let Some(path) = stroke_path(mark, viewport)
                                        {
                                            let mut color = rgb(color);
                                            color.a = mark.paint_opacity();
                                            window.paint_path(path, color);
                                        }
                                    } else if mark.tool == Tool::Redact(RedactionStyle::Blur) {
                                        if let Some(blurred) =
                                            blurred_images.get(&mark.effect_strength)
                                        {
                                            let rect = mark.bounds();
                                            let bounds = Bounds {
                                                origin: viewport
                                                    .to_window(ImagePoint::new(rect.x, rect.y)),
                                                size: size(
                                                    px(rect.width * viewport.scale),
                                                    px(rect.height * viewport.scale),
                                                ),
                                            };
                                            if let Err(error) = window.paint_image(
                                                bounds,
                                                source_bounds,
                                                rounded_corners(bounds),
                                                blurred.clone(),
                                                0,
                                                false,
                                            ) {
                                                eprintln!("Failed to display blur: {error}");
                                            }
                                        }
                                    } else if mark.magnifier.is_some() {
                                        paint_magnifier(mark, viewport, &image, source_offset, &mut magnifier_previews.borrow_mut(), window);
                                    } else if let Tool::Image(id) = mark.tool {
                                        if let Some(image) = flipped_images
                                            .get(&(
                                                id,
                                                mark.end.x < mark.start.x,
                                                mark.end.y < mark.start.y,
                                            ))
                                            .or_else(|| overlay_images.get(&id))
                                        {
                                            let rect = mark.bounds();
                                            let bounds = Bounds {
                                                origin: viewport
                                                    .to_window(ImagePoint::new(rect.x, rect.y)),
                                                size: size(
                                                    px(rect.width * viewport.scale),
                                                    px(rect.height * viewport.scale),
                                                ),
                                            };
                                            if let Err(error) = window.paint_image(
                                                bounds,
                                                bounds,
                                                Corners::default(),
                                                image.clone(),
                                                0,
                                                false,
                                            ) {
                                                eprintln!("Failed to display image layer: {error}");
                                            }
                                        }
                                    } else if mark.tool != Tool::Spotlight {
                                        paint_stroke(mark, viewport, dimension_display, window, &mosaic, source_offset, cx);
                                    }
                                }
                            },
                        );
                        window.with_content_mask(Some(ContentMask { bounds: viewport.bounds }), |window| {
                        if let Some(hovered) = &hovered {
                            paint_hover(hovered, viewport, window);
                        }
                        if let Some(preview) = &preview {
                            paint_stroke(preview, viewport, dimension_display, window, &mosaic, source_offset, cx);
                        }
                        if let Some(alignment) = &alignment_preview {
                            paint_stroke(alignment, viewport, dimension_display, window, &mosaic, source_offset, cx);
                        }
                        });
                        if let Some(selected) = &selected {
                            paint_selection(selected, viewport, window);
                        }
                        if show_hovered_dimensions && let Some(hovered) = &hovered {
                            selection::paint_stroke_label(hovered, viewport, dimension_display, window, cx);
                        }
                        if let Some(stroke) = &dimension_target
                            && (!dimension_target_is_draft || stroke.is_large_enough(viewport.scale))
                        {
                            selection::paint_stroke_label(stroke, viewport, dimension_display, window, cx);
                        }
                        if let Some(crop) = &crop {
                            paint_crop(crop, viewport, dimension_display, window, cx);
                        }
                        magnifier_previews.borrow_mut().finish_frame(window, cx);
                        fill_previews.borrow_mut().finish_frame();
                        // Window-level listeners retain a drag when the pointer leaves the image.
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                            if phase == DispatchPhase::Bubble {
                                mouse_move(event, window, cx);
                            }
                        });
                        window.on_mouse_event(move |event: &MouseExitEvent, phase, window, cx| {
                            if phase == DispatchPhase::Bubble { mouse_exit(event, window, cx); }
                        });
                        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                            if phase == DispatchPhase::Bubble {
                                mouse_up(event, window, cx);
                            }
                        });
                    },
                )
                .size_full(),
            )
            .when(self.text_edit.is_some(), |this| {
                this.child(self.inline_text_input(cx))
            })
            .child(self.render_options_panel(cx))
            .into_any_element()
    }
}
