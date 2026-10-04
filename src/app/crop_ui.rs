use super::*;

impl ScreenshotEditor {
    pub(super) fn start_crop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.finish_text(true, window, cx);
        self.editor.deselect();
        self.tool = None;
        self.measurement_drag = None;
        self.hover_position = None;
        self.hovered_handle = None;
        self.preview_position = None;
        let bounds = self.editor.image_bounds();
        let mut crop = CropSelection::new(bounds.width, bounds.height);
        crop.set_grid(self.dimension_display().grid());
        self.crop = Some(crop);
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn apply_crop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(crop) = self.crop.as_ref().filter(|crop| crop.can_apply()) else {
            return;
        };
        if self.editor.crop(crop.bounds()) {
            self.crop = None;
            self.reset_pointer();
            self.focus.focus(window, cx);
            cx.notify();
        }
    }

    pub(super) fn cancel_crop(&mut self, cx: &mut Context<Self>) {
        self.crop = None;
        self.reset_pointer();
        cx.notify();
    }

    pub(super) fn reset_pointer(&mut self) {
        self.viewport.set(None);
        self.adjustment_scroll = None;
        self.measurement_drag = None;
        self.hover_position = None;
        self.hovered_handle = None;
        self.preview_position = None;
    }
}
