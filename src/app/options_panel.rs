use super::ScreenshotEditor;
use crate::{editor::Tool, theme};
use gpui::{prelude::*, *};
use gpui_base::{Theme, Toolbar};

struct PanelDrag {
    pointer: Point<Pixels>,
    offset: Point<Pixels>,
}

pub(super) struct OptionsPanel {
    offset: Point<Pixels>,
    bounds: Option<Bounds<Pixels>>,
    drag: Option<PanelDrag>,
}

impl Default for OptionsPanel {
    fn default() -> Self {
        Self {
            offset: point(
                px(theme::OPTIONS_PANEL_MARGIN),
                px(theme::OPTIONS_PANEL_MARGIN),
            ),
            bounds: None,
            drag: None,
        }
    }
}

impl OptionsPanel {
    fn layout(&mut self, area: Bounds<Pixels>, size: Size<Pixels>) -> Point<Pixels> {
        let margin = px(theme::OPTIONS_PANEL_MARGIN);
        self.offset.x = self
            .offset
            .x
            .clamp(margin, (area.size.width - size.width - margin).max(margin));
        self.offset.y = self.offset.y.clamp(
            margin,
            (area.size.height - size.height - margin).max(margin),
        );
        let origin = point(
            area.right() - size.width - self.offset.x,
            area.top() + self.offset.y,
        );
        self.bounds = Some(Bounds::new(origin, size));
        origin
    }

    pub fn contains(&self, position: Point<Pixels>) -> bool {
        self.bounds.is_some_and(|bounds| bounds.contains(&position))
    }

    pub fn end_drag(&mut self) -> bool {
        self.drag.take().is_some()
    }

    pub fn cancel_drag(&mut self) -> bool {
        let Some(drag) = self.drag.take() else {
            return false;
        };
        self.offset = drag.offset;
        true
    }

    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    fn hide(&mut self) {
        self.bounds = None;
        self.end_drag();
    }
}

impl ScreenshotEditor {
    pub(super) fn render_options_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let tool = self
            .active_tool()
            .filter(|tool| !matches!(tool, Tool::Image(_) | Tool::Spotlight));
        if self.crop.is_none() && tool.is_none() {
            self.options_panel.borrow_mut().hide();
            return div().into_any_element();
        }
        let content = if self.crop.is_some() {
            self.crop_options(cx).into_any_element()
        } else {
            self.tool_options(tool.unwrap(), cx).into_any_element()
        };
        let content = if self.crop.is_none() && self.active_measurement_option().is_some() {
            div()
                .flex()
                .flex_col()
                .gap(px(theme::OPTIONS_PANEL_PADDING))
                .child(content)
                .child(self.render_measurement_loupe())
                .into_any_element()
        } else {
            content
        };
        let colors = Theme::global(cx).tokens.colors;
        let dragging = self.options_panel.borrow().is_dragging();
        let panel = Toolbar::new("tool-options-panel")
            .relative()
            .flex()
            .items_center()
            .p(px(theme::OPTIONS_PANEL_PADDING))
            .rounded(px(theme::PANEL_RADIUS))
            .border(px(theme::OPTIONS_PANEL_BORDER_WIDTH))
            .border_color(colors.border)
            .bg(colors.surface)
            .shadow_md()
            .occlude()
            .cursor(if dragging {
                CursorStyle::ClosedHand
            } else {
                CursorStyle::Arrow
            })
            .aria_label("Tool options")
            .on_any_mouse_down(cx.listener(|this, event: &MouseDownEvent, window, cx| {
                cx.stop_propagation();
                if event.button != MouseButton::Left || window.default_prevented() {
                    return;
                }
                let mut panel = this.options_panel.borrow_mut();
                if event.click_count == 2 {
                    panel.offset = point(
                        px(theme::OPTIONS_PANEL_MARGIN),
                        px(theme::OPTIONS_PANEL_MARGIN),
                    );
                    panel.end_drag();
                } else {
                    panel.drag = Some(PanelDrag {
                        pointer: event.position,
                        offset: panel.offset,
                    });
                }
                window.prevent_default();
                cx.notify();
            }))
            .child(content);
        let state = self.options_panel.clone();
        canvas(
            move |bounds, window, cx| {
                let mut panel = panel.into_any_element();
                let size = panel.layout_as_root(AvailableSpace::min_size(), window, cx);
                let origin = state.borrow_mut().layout(bounds, size);
                panel.prepaint_at(origin, window, cx);
                panel
            },
            |_, mut panel, window, cx| panel.paint(window, cx),
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .into_any_element()
    }

    pub(super) fn move_options_panel(
        &mut self,
        event: &MouseMoveEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let mut panel = self.options_panel.borrow_mut();
        if let Some(drag) = &panel.drag {
            if event.dragging() {
                let delta = event.position - drag.pointer;
                panel.offset = point(drag.offset.x - delta.x, drag.offset.y + delta.y);
            } else {
                panel.end_drag();
            }
            drop(panel);
            self.clear_hover(cx);
            cx.stop_propagation();
            cx.notify();
            return true;
        }
        if panel.contains(event.position)
            && !self.editor.is_dragging()
            && self.measurement_drag.is_none()
            && !self.crop.as_ref().is_some_and(|crop| crop.is_dragging())
        {
            drop(panel);
            self.clear_hover(cx);
            return true;
        }
        false
    }
}
