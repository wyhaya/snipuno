use super::DEFAULT_STROKE_WIDTH;
use crate::{
    editor::{EffectStrength, LineStyle, RedactionStyle, TailDirection, Tool},
    magnifier::Magnifier,
    measurement::{MeasurementAxis, MeasurementMode},
    theme::PALETTE,
};
use gpui::{App, Global};
use std::{cell::Cell, rc::Rc};

#[derive(Default)]
struct SessionPreferences(Rc<ToolPreferences>);

impl Global for SessionPreferences {}

pub(super) struct ToolPreferences {
    // Temporary selection mode after Escape or finishing text is not a new tool choice.
    selected_tool: Cell<Option<Tool>>,
    pub measurement_mode: Cell<MeasurementMode>,
    pub measurement_axis: Cell<MeasurementAxis>,
    pub line_style: Cell<LineStyle>,
    pub redaction_style: Cell<RedactionStyle>,
    rectangle_tool: Cell<Tool>,
    ellipse_tool: Cell<Tool>,
    pub magnifier_zoom: Cell<f32>,
    pub arrow_double_headed: Cell<bool>,
    pub color: Cell<u32>,
    pub brush_auto_color: Cell<bool>,
    rectangle_fill_color: Cell<Option<u32>>,
    ellipse_fill_color: Cell<Option<u32>>,
    rectangle_fill_opacity: Cell<f32>,
    ellipse_fill_opacity: Cell<f32>,
    pub stroke_width: Cell<f32>,
    pub number_tail: Cell<TailDirection>,
    pub text_bubble: Cell<bool>,
    pub text_tail: Cell<TailDirection>,
    pub effect_strength: Cell<EffectStrength>,
}

impl Default for ToolPreferences {
    fn default() -> Self {
        Self {
            selected_tool: Cell::new(Some(Tool::Rectangle)),
            measurement_mode: Cell::default(),
            measurement_axis: Cell::new(MeasurementAxis::Y),
            line_style: Cell::default(),
            redaction_style: Cell::default(),
            rectangle_tool: Cell::new(Tool::Rectangle),
            ellipse_tool: Cell::new(Tool::Ellipse),
            magnifier_zoom: Cell::new(Magnifier::default().zoom),
            arrow_double_headed: Cell::new(false),
            color: Cell::new(PALETTE[0].0),
            brush_auto_color: Cell::new(false),
            rectangle_fill_color: Cell::new(None),
            ellipse_fill_color: Cell::new(None),
            rectangle_fill_opacity: Cell::new(1.0),
            ellipse_fill_opacity: Cell::new(1.0),
            stroke_width: Cell::new(DEFAULT_STROKE_WIDTH),
            number_tail: Cell::default(),
            text_bubble: Cell::new(false),
            text_tail: Cell::default(),
            effect_strength: Cell::default(),
        }
    }
}

impl ToolPreferences {
    pub fn fill_opacity(&self, tool: Tool) -> f32 {
        match tool.family() {
            Tool::Rectangle => self.rectangle_fill_opacity.get(),
            Tool::Ellipse => self.ellipse_fill_opacity.get(),
            _ => 1.0,
        }
    }

    pub fn set_fill_opacity(&self, tool: Tool, opacity: f32) {
        match tool.family() {
            Tool::Rectangle => self.rectangle_fill_opacity.set(opacity),
            Tool::Ellipse => self.ellipse_fill_opacity.set(opacity),
            _ => {}
        }
    }

    pub fn fill_color(&self, tool: Tool) -> Option<u32> {
        match tool.family() {
            Tool::Rectangle => self.rectangle_fill_color.get(),
            Tool::Ellipse => self.ellipse_fill_color.get(),
            _ => None,
        }
    }

    pub fn set_fill_color(&self, tool: Tool, color: Option<u32>) {
        match tool.family() {
            Tool::Rectangle => self.rectangle_fill_color.set(color),
            Tool::Ellipse => self.ellipse_fill_color.set(color),
            _ => {}
        }
    }

    pub fn shape_tool(&self, tool: Tool) -> Tool {
        match tool.family() {
            Tool::Rectangle => self.rectangle_tool.get(),
            Tool::Ellipse => self.ellipse_tool.get(),
            _ => tool,
        }
    }

    fn set_shape_tool(&self, tool: Tool) {
        match tool.family() {
            Tool::Rectangle => self.rectangle_tool.set(tool),
            Tool::Ellipse => self.ellipse_tool.set(tool),
            _ => {}
        }
    }

    pub fn shared(cx: &mut App) -> Rc<Self> {
        cx.default_global::<SessionPreferences>().0.clone()
    }

    pub fn selected_tool(&self) -> Option<Tool> {
        self.selected_tool.get().map(|tool| match tool {
            Tool::Rectangle | Tool::RectangleFill | Tool::Ellipse | Tool::EllipseFill => {
                self.shape_tool(tool)
            }
            Tool::Line(_) => Tool::Line(self.line_style.get()),
            Tool::Redact(_) => Tool::Redact(self.redaction_style.get()),
            Tool::Measure(_) => Tool::Measure(self.measurement_axis.get()),
            _ => tool,
        })
    }

    pub fn select_tool(&self, tool: Option<Tool>) -> Option<Tool> {
        if let Some(tool) = tool.filter(|tool| tool.is_shape()) {
            self.set_shape_tool(tool);
        }
        self.selected_tool.set(tool);
        self.selected_tool()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_tools_default_to_opaque_and_remember_opacity_independently() {
        let preferences = ToolPreferences::default();
        assert_eq!(preferences.fill_opacity(Tool::RectangleFill), 1.0);
        assert_eq!(preferences.fill_opacity(Tool::EllipseFill), 1.0);
        preferences.set_fill_opacity(Tool::RectangleFill, 0.2);
        preferences.select_tool(Some(Tool::EllipseFill));
        assert_eq!(preferences.fill_opacity(Tool::EllipseFill), 1.0);
        preferences.set_fill_opacity(Tool::EllipseFill, 0.5);
        preferences.select_tool(Some(Tool::RectangleFill));
        assert_eq!(preferences.fill_opacity(Tool::RectangleFill), 0.2);
        assert_eq!(preferences.fill_opacity(Tool::EllipseFill), 0.5);
    }

    #[test]
    fn fill_tools_default_to_auto_and_remember_colors_independently() {
        let preferences = ToolPreferences::default();
        assert_eq!(preferences.fill_color(Tool::RectangleFill), None);
        assert_eq!(preferences.fill_color(Tool::EllipseFill), None);
        preferences.set_fill_color(Tool::RectangleFill, Some(0xff123456));
        preferences.select_tool(Some(Tool::EllipseFill));
        assert_eq!(preferences.fill_color(Tool::EllipseFill), None);
        assert_eq!(
            preferences.fill_color(Tool::RectangleFill),
            Some(0xff123456)
        );
        preferences.set_fill_color(Tool::EllipseFill, Some(0xffabcdef));
        preferences.set_fill_color(Tool::RectangleFill, None);
        assert_eq!(preferences.fill_color(Tool::RectangleFill), None);
        assert_eq!(preferences.fill_color(Tool::EllipseFill), Some(0xffabcdef));
    }

    #[test]
    fn shape_groups_remember_their_tools_independently() {
        let preferences = ToolPreferences::default();
        assert_eq!(preferences.shape_tool(Tool::Rectangle), Tool::Rectangle);
        assert_eq!(preferences.shape_tool(Tool::Ellipse), Tool::Ellipse);
        assert_eq!(
            preferences.select_tool(Some(Tool::RectangleFill)),
            Some(Tool::RectangleFill)
        );
        assert_eq!(preferences.shape_tool(Tool::Rectangle), Tool::RectangleFill);
        assert_eq!(preferences.shape_tool(Tool::Ellipse), Tool::Ellipse);
        preferences.select_tool(Some(Tool::EllipseFill));
        assert_eq!(preferences.shape_tool(Tool::Rectangle), Tool::RectangleFill);
        assert_eq!(preferences.shape_tool(Tool::Ellipse), Tool::EllipseFill);
        preferences.select_tool(Some(Tool::Rectangle));
        assert_eq!(preferences.shape_tool(Tool::Rectangle), Tool::Rectangle);
        assert_eq!(preferences.shape_tool(Tool::Ellipse), Tool::EllipseFill);
        preferences.select_tool(Some(Tool::Arrow));
        assert_eq!(
            preferences.select_tool(Some(preferences.shape_tool(Tool::Ellipse))),
            Some(Tool::EllipseFill)
        );
    }

    #[test]
    fn reselecting_a_tool_uses_its_latest_options() {
        let preferences = ToolPreferences::default();
        preferences.line_style.set(LineStyle::Wavy);
        preferences.redaction_style.set(RedactionStyle::Mosaic);
        preferences.measurement_axis.set(MeasurementAxis::X);
        for (requested, expected) in [
            (Tool::Line(LineStyle::Solid), Tool::Line(LineStyle::Wavy)),
            (
                Tool::Redact(RedactionStyle::Blur),
                Tool::Redact(RedactionStyle::Mosaic),
            ),
            (
                Tool::Measure(MeasurementAxis::Y),
                Tool::Measure(MeasurementAxis::X),
            ),
            (Tool::Text, Tool::Text),
        ] {
            preferences.select_tool(Some(Tool::Rectangle));
            assert_eq!(preferences.select_tool(Some(requested)), Some(expected));
            assert_eq!(preferences.selected_tool(), Some(expected));
        }
        preferences.select_tool(None);
        assert_eq!(preferences.selected_tool(), None);
    }
}
