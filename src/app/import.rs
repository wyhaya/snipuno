use super::*;
use crate::{clipboard, image_processing::load_overlay};
use std::path::PathBuf;

impl ScreenshotEditor {
    pub(super) fn begin_import(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.importing_image {
            return false;
        }
        self.finish_text(true, window, cx);
        self.crop = None;
        self.editor.cancel();
        self.importing_image = true;
        true
    }

    pub(super) fn drop_images(
        &mut self,
        paths: &ExternalPaths,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if paths.paths().is_empty() || !self.begin_import(window, cx) {
            return;
        }
        let center = self
            .viewport
            .get()
            .filter(|viewport| viewport.scale > 0.0)
            .map(|viewport| viewport.to_image(window.mouse_position()));
        self.load_import_paths(paths.paths().to_vec(), center, window, cx);
    }

    pub(super) fn import_image(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.begin_import(window, cx) {
            return;
        }
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Add image".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = match paths.await {
                Ok(Ok(Some(paths))) => Ok(Some(paths)),
                Ok(Ok(None)) => Ok(None),
                Ok(Err(error)) => Err(format!("Unable to open file picker: {error}")),
                Err(error) => Err(format!("Unable to open file picker: {error}")),
            };
            let _ = this.update_in(cx, |this, window, cx| {
                this.focus.focus(window, cx);
                match result {
                    Ok(Some(paths)) if !paths.is_empty() => {
                        this.load_import_paths(paths, None, window, cx);
                        return;
                    }
                    Err(error) => show_error("Unable to add image", &error, window, cx),
                    _ => {}
                }
                this.importing_image = false;
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn load_import_paths(
        &mut self,
        paths: Vec<PathBuf>,
        center: Option<ImagePoint>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.spawn_in(window, async move |this, cx| {
            let mut errors = Vec::new();
            for path in paths {
                let result = cx
                    .background_executor()
                    .spawn(async move { load_overlay(path) })
                    .await;
                if this
                    .update_in(cx, |this, _, cx| {
                        match result {
                            Ok((image, _)) => this.add_overlay(image, center),
                            Err(error) => errors.push(error),
                        }
                        cx.notify();
                    })
                    .is_err()
                {
                    return;
                }
            }
            let _ = this.update_in(cx, |this, window, cx| {
                this.importing_image = false;
                this.focus.focus(window, cx);
                if !errors.is_empty() {
                    show_error("Unable to add images", &errors.join("\n\n"), window, cx);
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn add_overlay(&mut self, image: Arc<RenderImage>, center: Option<ImagePoint>) {
        let dimensions = image.size(0);
        let id = self.next_image_id;
        let added = if let Some(center) = center {
            self.editor.add_image_at(
                id,
                dimensions.width.0 as f32,
                dimensions.height.0 as f32,
                center,
            )
        } else {
            self.editor
                .add_image(id, dimensions.width.0 as f32, dimensions.height.0 as f32)
        };
        if added {
            self.next_image_id += 1;
            self.overlay_images.insert(id, image);
            self.tool = None;
            self.hovered_handle = None;
        }
    }

    pub(super) fn paste_image(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.importing_image {
            return;
        }
        self.crop = None;
        self.importing_image = true;
        let center = self
            .viewport
            .get()
            .filter(|viewport| viewport.scale > 0.0)
            .map(|viewport| viewport.to_image(window.mouse_position()));
        let clipboard = clipboard::read_image(cx);
        cx.spawn_in(window, async move |this, cx| {
            let result = clipboard.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.importing_image = false;
                this.focus.focus(window, cx);
                match result {
                    Ok(Some(image)) => this.add_overlay(image, center),
                    Ok(None) => {}
                    Err(error) => show_error("Unable to paste image", &error, window, cx),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
