use super::*;
use crate::image_processing::{blurred_image, flip_image};
use std::collections::HashSet;

impl ScreenshotEditor {
    fn requested_blurs(&self) -> [bool; EffectStrength::ALL.len()] {
        let mut requested = [false; EffectStrength::ALL.len()];
        for (tool, strength) in self
            .editor
            .strokes()
            .iter()
            .chain(self.editor.draft())
            .map(|stroke| (stroke.tool, stroke.effect_strength))
            .chain(
                self.tool
                    .map(|tool| (tool, self.preferences.effect_strength.get())),
            )
        {
            if tool == Tool::Redact(RedactionStyle::Blur) {
                requested[strength.index()] = true;
            }
        }
        requested
    }

    pub(super) fn prepare_effects(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let source = self.source.pixels.clone();
        let requested = self.requested_blurs();
        self.blurred_images.retain(|strength, image| {
            if requested[strength.index()] {
                return true;
            }
            cx.drop_image(image.clone(), Some(window));
            false
        });
        if self.preparing_blur.is_none() {
            let next = self
                .tool
                .filter(|tool| *tool == Tool::Redact(RedactionStyle::Blur))
                .map(|_| self.preferences.effect_strength.get())
                .into_iter()
                .chain(EffectStrength::ALL)
                .find(|strength| {
                    requested[strength.index()] && !self.blurred_images.contains_key(strength)
                });
            if let Some(strength) = next {
                self.preparing_blur = Some(strength);
                let source = source.clone();
                cx.spawn(async move |this, cx| {
                    let image = cx
                        .background_executor()
                        .spawn(async move { blurred_image(&source, strength) })
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        this.preparing_blur = None;
                        if this.requested_blurs()[strength.index()] {
                            this.blurred_images.insert(strength, image);
                        }
                        cx.notify();
                    });
                })
                .detach();
            }
        }
        let mosaic_requested = self.tool == Some(Tool::Redact(RedactionStyle::Mosaic))
            || self
                .editor
                .strokes()
                .iter()
                .chain(self.editor.draft())
                .any(|stroke| stroke.tool == Tool::Redact(RedactionStyle::Mosaic));
        if mosaic_requested && !self.mosaic_ready && !self.preparing_mosaic {
            self.preparing_mosaic = true;
            cx.spawn(async move |this, cx| {
                let mosaic = cx
                    .background_executor()
                    .spawn(async move { Arc::new(Mosaic::new(&source)) })
                    .await;
                let _ = this.update(cx, |this, cx| {
                    this.preparing_mosaic = false;
                    this.mosaic_ready = true;
                    this.mosaic = mosaic;
                    cx.notify();
                });
            })
            .detach();
        }
    }

    fn requested_flips(&self) -> HashSet<(u64, bool, bool)> {
        self.editor
            .strokes()
            .iter()
            .filter_map(|stroke| {
                let Tool::Image(id) = stroke.tool else {
                    return None;
                };
                let x = stroke.end.x < stroke.start.x;
                let y = stroke.end.y < stroke.start.y;
                (x || y).then_some((id, x, y))
            })
            .collect()
    }

    pub(super) fn prepare_image_layers(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let revision = self.editor.history_revision();
        if self.image_history_revision != revision {
            if !self.overlay_images.is_empty() {
                let referenced = self.editor.referenced_image_ids();
                self.overlay_images.retain(|id, image| {
                    if referenced.contains(id) {
                        return true;
                    }
                    cx.drop_image(image.clone(), Some(window));
                    false
                });
            }
            self.image_history_revision = revision;
        }
        let requested = self.requested_flips();
        self.flipped_images.retain(|key, image| {
            if requested.contains(key) {
                return true;
            }
            cx.drop_image(image.clone(), Some(window));
            false
        });
        if self.preparing_flips.is_some() {
            return;
        }
        let Some(key @ (id, x, y)) = requested
            .into_iter()
            .find(|key| !self.flipped_images.contains_key(key))
        else {
            return;
        };
        let Some(source) = self.overlay_images.get(&id).cloned() else {
            return;
        };
        self.preparing_flips = Some(key);
        cx.spawn(async move |this, cx| {
            let image = cx
                .background_executor()
                .spawn(async move { flip_image(&source, x, y) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.preparing_flips = None;
                if this.requested_flips().contains(&key) {
                    this.flipped_images.insert(key, image);
                }
                cx.notify();
            });
        })
        .detach();
    }
}
