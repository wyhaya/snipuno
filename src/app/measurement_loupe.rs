use super::ScreenshotEditor;
use crate::{
    editor::Point as ImagePoint, image_processing::sample_image_color_in_bounds, pixel_loupe, theme,
};
use gpui::{prelude::*, *};

impl ScreenshotEditor {
    pub(super) fn render_measurement_loupe(&self) -> AnyElement {
        let preview = div()
            .id("measurement-loupe-preview")
            .w_full()
            .aspect_square()
            .flex_shrink_0()
            .overflow_hidden()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        let position = self
            .hover_position
            .unwrap_or_else(|| self.canvas_center());
        let crop = self.editor.image_bounds();
        let position = ImagePoint::new(
            position.x.floor().clamp(0.0, crop.width - 1.0),
            position.y.floor().clamp(0.0, crop.height - 1.0),
        );
        let pixels = self.source.pixels.clone();
        let center = ImagePoint::new(crop.x + position.x, crop.y + position.y);
        preview
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        pixel_loupe::paint_in_bounds(
                            bounds,
                            |dx, dy| {
                                sample_image_color_in_bounds(
                                    &pixels,
                                    ImagePoint::new(center.x + dx as f32, center.y + dy as f32),
                                    crop,
                                )
                                .unwrap_or(theme::SURFACE)
                            },
                            window,
                        );
                    },
                )
                .size_full(),
            )
            .into_any_element()
    }
}
