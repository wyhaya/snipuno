use super::*;
#[cfg(any(target_os = "macos", target_os = "windows"))]
use crate::capture::set_window_visible;
use crate::{
    canvas::prepare_measurement_label,
    clipboard,
    export::{ExportImage, text_svg},
};
use chrono::Local;

#[derive(Clone, Copy)]
pub(super) enum Destination {
    Clipboard,
    File,
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    Pinned,
}

struct PreparedImage {
    cache: ExportCache,
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    pinned: Option<Arc<RenderImage>>,
}

impl ScreenshotEditor {
    pub(super) fn export_image(
        &mut self,
        destination: Destination,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.exporting_image || self.importing_image {
            return;
        }
        self.finish_text(true, window, cx);
        let source = self.source.pixels.clone();
        self.editor.cancel();
        let scale = self
            .viewport
            .get()
            .map(|viewport| viewport.scale)
            .filter(|scale| scale.is_finite() && *scale > 0.0)
            .unwrap_or(1.0);
        let mut text = HashMap::new();
        let dimensions = self.dimension_display();
        let bounds = self.editor.image_bounds();
        let mut strokes = self.editor.strokes().to_vec();
        for (index, stroke) in strokes.iter_mut().enumerate() {
            prepare_measurement_label(
                stroke,
                dimensions,
                ImagePoint::new(bounds.width, bounds.height),
                window,
            );
            match text_svg(stroke, scale, dimensions, window, cx) {
                Ok(Some(svg)) => {
                    text.insert(index, svg);
                }
                Ok(None) => {}
                Err(error) => {
                    if matches!(destination, Destination::Clipboard) {
                        self.close(window, cx);
                    } else {
                        show_error("Unable to export image", &error, window, cx);
                    }
                    return;
                }
            }
        }
        let snapshot = ExportImage {
            source,
            bounds: self.editor.image_bounds(),
            strokes,
            overlays: self
                .editor
                .strokes()
                .iter()
                .filter_map(|stroke| {
                    let Tool::Image(id) = stroke.tool else {
                        return None;
                    };
                    self.overlay_images
                        .get(&id)
                        .map(|image| (id, image.clone()))
                })
                .collect(),
            blurred: self.blurred_images.clone(),
            mosaic: self.mosaic_ready.then(|| self.mosaic.clone()),
            text,
            scale,
        };
        let renderer = cx.svg_renderer();
        let cached = self.export_cache.clone();
        if matches!(destination, Destination::Clipboard) {
            clipboard::write_image_with(
                move || {
                    snapshot
                        .prepare(renderer, cached)
                        .map(|cache| cache.pixels())
                },
                cx,
            )
            .detach();
            self.close(window, cx);
            return;
        }
        let picker = matches!(destination, Destination::File).then(|| {
            let directory = dirs::desktop_dir().unwrap_or_default();
            let filename = Local::now()
                .format("Screenshot %Y-%m-%d at %H.%M.%S.png")
                .to_string();
            cx.prompt_for_new_path(&directory, Some(&filename))
        });
        self.exporting_image = true;
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        if matches!(destination, Destination::Pinned) {
            let _ = set_window_visible(window, false);
        }
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = async {
                let path = if let Some(picker) = picker {
                    match picker.await {
                        Ok(Ok(Some(path))) => Some(path),
                        Ok(Ok(None)) => return Ok(None),
                        Ok(Err(error)) => {
                            return Err(format!("Unable to open save dialog: {error}"));
                        }
                        Err(error) => return Err(format!("Unable to open save dialog: {error}")),
                    }
                } else {
                    None
                };
                cx.background_executor()
                    .spawn(async move {
                        let cache = snapshot.prepare(renderer, cached)?;
                        if let Some(path) = path {
                            cache.save(&path)?;
                        }
                        Ok(Some(PreparedImage {
                            #[cfg(any(target_os = "macos", target_os = "windows"))]
                            pinned: matches!(destination, Destination::Pinned).then(|| {
                                crate::image_processing::render_image((*cache.pixels()).clone())
                            }),
                            cache,
                        }))
                    })
                    .await
            }
            .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.exporting_image = false;
                match result {
                    Ok(Some(prepared)) => {
                        #[cfg(any(target_os = "macos", target_os = "windows"))]
                        if let Some(image) = prepared.pinned {
                            match super::pinned::open(
                                this.screenshot.clone(),
                                image,
                                prepared.cache.pixels(),
                                window,
                                cx,
                            ) {
                                Ok(()) => {
                                    this.close(window, cx);
                                    return;
                                }
                                Err(error) => {
                                    let _ = set_window_visible(window, true);
                                    show_error("Unable to pin screenshot", &error, window, cx)
                                }
                            }
                        }
                        this.export_cache = Some(prepared.cache);
                    }
                    Ok(None) => {}
                    Err(error) => {
                        #[cfg(any(target_os = "macos", target_os = "windows"))]
                        if matches!(destination, Destination::Pinned) {
                            let _ = set_window_visible(window, true);
                        }
                        show_error(
                            match destination {
                                #[cfg(any(target_os = "macos", target_os = "windows"))]
                                Destination::Pinned => "Unable to pin screenshot",
                                _ => "Unable to export image",
                            },
                            &error,
                            window,
                            cx,
                        );
                    }
                }
                this.focus.focus(window, cx);
                cx.notify();
            });
        })
        .detach();
    }
}
