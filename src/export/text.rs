use crate::{
    canvas::{layout_measurement, layout_text},
    dimensions::DimensionDisplay,
    editor::{Stroke, Tool},
    theme,
};
use gpui::*;
use gpui_base::Theme;
use std::slice;

pub(crate) fn text_svg(
    stroke: &Stroke,
    scale: f32,
    dimensions: DimensionDisplay,
    window: &Window,
    cx: &App,
) -> Result<Option<String>, String> {
    let mut svg = String::new();
    if matches!(stroke.tool, Tool::Measure(_)) {
        let label = layout_measurement(stroke, scale, dimensions, window)
            .ok_or("A measurement label is incomplete")?;
        return Ok(Some(text_element(
            &label.line.text,
            &window.text_style().font().family,
            label.font_size,
            label.origin.x,
            label.origin.y
                + (label.line_height + f32::from(label.line.ascent - label.line.descent) / scale)
                    / 2.0,
            0xff000000 | theme::TEXT,
            false,
        )));
    }
    if let Some(text) = &stroke.text {
        let lines = layout_text(text, stroke.color, scale, window, cx)?;
        let family = &Theme::global(cx).tokens.typography.sans;
        let offset = text.content_offset();
        let x = stroke.start.x + offset.x;
        let mut y = stroke.start.y + offset.y;
        let height = text.line_height();
        for line in lines {
            let layout = &line.unwrapped_layout;
            let baseline = (height + f32::from(layout.ascent - layout.descent) / scale) / 2.0;
            let boundaries = line
                .wrap_boundaries
                .iter()
                .map(|boundary| layout.runs[boundary.run_ix].glyphs[boundary.glyph_ix].index);
            let mut start = 0;
            for end in boundaries.chain(std::iter::once(line.text.len())) {
                svg.push_str(&text_element(
                    &line.text[start..end],
                    family,
                    text.font_size,
                    x,
                    y + baseline,
                    text.foreground_color(stroke.color),
                    false,
                ));
                start = end;
                y += height;
            }
        }
    } else if let Tool::Number(number) = stroke.tool {
        let bounds = stroke.bounds();
        let diameter = bounds.width.min(bounds.height);
        let mut font = window.text_style().font();
        font.family = Theme::global(cx).tokens.typography.mono;
        font.weight = FontWeight::BOLD;
        let family = font.family.clone();
        let text: SharedString = number.to_string().into();
        let run = TextRun {
            len: text.len(),
            font,
            color: rgb(theme::TEXT).into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let mut font_size = px(diameter * 0.55 * scale);
        let mut line =
            window
                .text_system()
                .shape_line(text.clone(), font_size, slice::from_ref(&run), None);
        let max_width = px(diameter * 0.76 * scale);
        if line.width() > max_width {
            font_size *= max_width / line.width();
            line = window
                .text_system()
                .shape_line(text.clone(), font_size, &[run], None);
        }
        let color = theme::contrasting_text(stroke.color);
        let x = bounds.x + (bounds.width - f32::from(line.width()) / scale) / 2.0;
        let y =
            bounds.y + bounds.height / 2.0 + f32::from(line.ascent - line.descent) / scale / 2.0;
        svg = text_element(
            &text,
            &family,
            f32::from(font_size) / scale,
            x,
            y,
            0xff000000 | color,
            true,
        );
    } else {
        return Ok(None);
    }
    Ok(Some(svg))
}

pub(super) fn text_element(
    text: &str,
    family: &str,
    size: f32,
    x: f32,
    y: f32,
    color: u32,
    bold: bool,
) -> String {
    let family = if cfg!(target_os = "macos") {
        font_name_with_fallbacks(family, ".AppleSystemUIFont")
    } else {
        family
    };
    format!(
        "<text xml:space=\"preserve\" x=\"{x}\" y=\"{y}\" font-family=\"{}\" font-size=\"{size}\" font-weight=\"{}\" fill=\"#{:06x}\" fill-opacity=\"{}\">{}</text>",
        escape_xml(family),
        if bold { 700 } else { 400 },
        color & 0xffffff,
        (color >> 24) as f32 / 255.0,
        escape_xml(text)
    )
}

fn escape_xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
