use crate::{
    capture::CapturePlacement, desktop::WindowRole, editor::Editor, image_processing::SourceImage,
};
use gpui::{AnyWindowHandle, Point, RenderImage};
use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc};

pub(crate) struct ScreenshotState {
    pub source: SourceImage,
    pub title: String,
    pub placement: Option<CapturePlacement>,
    pub window: Option<(AnyWindowHandle, WindowRole)>,
    pub(super) edits: Option<SavedEdits>,
}

pub(super) struct SavedEdits {
    pub editor: Editor,
    pub overlay_images: HashMap<u64, Arc<RenderImage>>,
    pub next_image_id: u64,
    pub image_alignment: Point<f32>,
}

impl ScreenshotState {
    pub fn new(
        source: SourceImage,
        title: String,
        placement: Option<CapturePlacement>,
    ) -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Self {
            source,
            title,
            placement,
            window: None,
            edits: None,
        }))
    }

    pub(super) fn save_edits(
        &mut self,
        editor: &Editor,
        overlay_images: &HashMap<u64, Arc<RenderImage>>,
        next_image_id: u64,
        image_alignment: Point<f32>,
    ) {
        let mut editor = editor.clone();
        editor.cancel();
        self.edits = Some(SavedEdits {
            editor,
            overlay_images: overlay_images.clone(),
            next_image_id,
            image_alignment,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        editor::{Point as ImagePoint, Tool},
        test_support::bounds,
    };
    use gpui::point;
    use image::RgbaImage;

    #[test]
    fn reopening_keeps_committed_edits_resources_and_history_but_discards_drafts() {
        let screenshot = ScreenshotState::new(
            SourceImage::from_rgba(RgbaImage::new(120, 80)).unwrap(),
            "Screenshot".into(),
            None,
        );
        let mut editor = Editor::new(120.0, 80.0);
        editor.begin(
            Tool::Rectangle,
            ImagePoint::new(20.0, 20.0),
            0xff123456,
            3.0,
        );
        assert!(editor.finish(ImagePoint::new(60.0, 50.0), 1.0));
        let original = editor.strokes().to_vec();
        let crop = bounds(10.0, 10.0, 100.0, 60.0);
        assert!(editor.crop(crop));
        let overlay = SourceImage::from_rgba(RgbaImage::new(10, 10))
            .unwrap()
            .rendered;
        assert!(editor.add_image(7, 10.0, 10.0));
        assert!(editor.undo());
        let committed = editor.strokes().to_vec();
        editor.begin(Tool::Arrow, ImagePoint::new(10.0, 10.0), 0, 3.0);
        editor.update(ImagePoint::new(50.0, 50.0), 1.0);
        screenshot.borrow_mut().save_edits(
            &editor,
            &HashMap::from([(7, overlay.clone())]),
            8,
            point(0.0, 1.0),
        );
        assert!(editor.is_dragging());
        let mut saved = screenshot.borrow_mut().edits.take().unwrap();
        assert!(!saved.editor.is_dragging());
        assert_eq!(saved.editor.strokes(), committed);
        assert_eq!(saved.editor.image_bounds(), crop);
        assert_eq!(saved.next_image_id, 8);
        assert_eq!(saved.image_alignment, point(0.0, 1.0));
        assert_eq!(saved.overlay_images[&7].as_bytes(0), overlay.as_bytes(0));
        assert!(saved.editor.redo());
        assert_eq!(saved.editor.strokes().last().unwrap().tool, Tool::Image(7));
        assert!(saved.editor.undo());
        assert!(saved.editor.undo());
        assert_eq!(saved.editor.image_bounds(), bounds(0.0, 0.0, 120.0, 80.0));
        assert_eq!(saved.editor.strokes(), original);
    }

    #[test]
    fn screenshots_keep_independent_edits_and_undo_history() {
        let source = SourceImage::from_rgba(RgbaImage::new(120, 80)).unwrap();
        let first = ScreenshotState::new(source.clone(), "First".into(), None);
        let second = ScreenshotState::new(source, "Second".into(), None);
        let mut editor = Editor::new(120.0, 80.0);
        editor.begin(
            Tool::Rectangle,
            ImagePoint::new(10.0, 10.0),
            0xff123456,
            3.0,
        );
        assert!(editor.finish(ImagePoint::new(60.0, 50.0), 1.0));
        first
            .borrow_mut()
            .save_edits(&editor, &HashMap::new(), 0, point(0.0, 0.0));
        editor.clear();
        editor.begin(Tool::Arrow, ImagePoint::new(20.0, 20.0), 0xffabcdef, 4.0);
        assert!(editor.finish(ImagePoint::new(80.0, 40.0), 1.0));
        second
            .borrow_mut()
            .save_edits(&editor, &HashMap::new(), 0, point(1.0, 1.0));

        let mut saved = first.borrow_mut().edits.take().unwrap();
        assert_eq!(saved.editor.strokes()[0].tool, Tool::Rectangle);
        assert!(saved.editor.undo());
        assert!(saved.editor.strokes().is_empty());
        let second = second.borrow();
        let other = second.edits.as_ref().unwrap();
        assert_eq!(other.editor.strokes().len(), 1);
        assert_eq!(other.editor.strokes()[0].tool, Tool::Arrow);
        assert_eq!(other.image_alignment, point(1.0, 1.0));
        assert_eq!(second.title, "Second");
        assert!(saved.editor.redo());
        assert_eq!(saved.editor.strokes()[0].tool, Tool::Rectangle);
    }
}
