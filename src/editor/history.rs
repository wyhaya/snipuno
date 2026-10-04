use super::{Bounds, Editor, Stroke, Tool};
use std::collections::HashSet;

#[derive(Clone, Debug)]
pub(super) enum Edit {
    Add(Stroke),
    Clear {
        strokes: Vec<Stroke>,
        bounds: Bounds,
    },
    Raise {
        index: usize,
    },
    Delete {
        index: usize,
        stroke: Stroke,
    },
    Crop {
        before: Bounds,
        after: Bounds,
    },
    Transform {
        index: usize,
        before: Stroke,
        after: Stroke,
    },
}

impl Editor {
    pub fn history_revision(&self) -> u64 {
        self.history_revision
    }

    pub(super) fn record_edit(&mut self, edit: Edit) {
        self.past.push(edit);
        self.future.clear();
        self.history_revision = self.history_revision.wrapping_add(1);
    }

    pub fn referenced_image_ids(&self) -> HashSet<u64> {
        let mut ids = HashSet::new();
        let mut include = |stroke: &Stroke| {
            if let Tool::Image(id) = stroke.tool {
                ids.insert(id);
            }
        };
        for stroke in self
            .strokes
            .iter()
            .chain(self.draft.iter())
            .chain(self.transforming.iter().map(|draft| &draft.before))
        {
            include(stroke);
        }
        for edit in self.past.iter().chain(&self.future) {
            match edit {
                Edit::Add(stroke) | Edit::Delete { stroke, .. } => include(stroke),
                Edit::Clear { strokes, .. } => {
                    for stroke in strokes {
                        include(stroke);
                    }
                }
                Edit::Transform { before, after, .. } => {
                    include(before);
                    include(after);
                }
                Edit::Raise { .. } | Edit::Crop { .. } => {}
            }
        }
        ids
    }

    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }

    /// Cancels an in-progress drag, then reverses the latest committed edit.
    pub fn undo(&mut self) -> bool {
        self.deselect();
        let Some(edit) = self.past.pop() else {
            return false;
        };
        match &edit {
            Edit::Crop { before, .. } => self.apply_crop(*before),
            Edit::Add(_) => {
                self.strokes.pop();
            }
            Edit::Raise { index } => {
                let stroke = self.strokes.pop().unwrap();
                self.strokes.insert(*index, stroke);
                self.selected = Some(*index);
            }
            Edit::Clear { strokes, bounds } => {
                self.apply_crop(*bounds);
                self.strokes.clone_from(strokes);
            }
            Edit::Delete { index, stroke } => {
                self.strokes.insert(*index, stroke.clone());
                self.selected = Some(*index);
            }
            Edit::Transform { index, before, .. } => {
                self.strokes[*index] = before.clone();
                self.selected = Some(*index);
            }
        }
        self.future.push(edit);
        self.history_revision = self.history_revision.wrapping_add(1);
        true
    }

    pub fn redo(&mut self) -> bool {
        self.deselect();
        let Some(edit) = self.future.pop() else {
            return false;
        };
        match &edit {
            Edit::Crop { after, .. } => self.apply_crop(*after),
            Edit::Add(stroke) => self.strokes.push(stroke.clone()),
            Edit::Raise { index } => {
                let stroke = self.strokes.remove(*index);
                self.strokes.push(stroke);
                self.selected = Some(self.strokes.len() - 1);
            }
            Edit::Clear { .. } => {
                self.strokes.clear();
                self.apply_crop(self.original_bounds);
            }
            Edit::Delete { index, .. } => {
                self.strokes.remove(*index);
            }
            Edit::Transform { index, after, .. } => {
                self.strokes[*index] = after.clone();
                self.selected = Some(*index);
            }
        }
        self.past.push(edit);
        self.history_revision = self.history_revision.wrapping_add(1);
        true
    }

    pub fn delete_selected(&mut self) -> bool {
        let Some(index) = self.selected else {
            return false;
        };
        self.cancel();
        self.selected = None;
        let stroke = self.strokes.remove(index);
        self.record_edit(Edit::Delete { index, stroke });
        true
    }

    /// Clearing the image is one reversible history entry.
    pub fn clear(&mut self) -> bool {
        self.deselect();
        if !self.can_clear() {
            return false;
        }
        let strokes = std::mem::take(&mut self.strokes);
        self.record_edit(Edit::Clear {
            strokes,
            bounds: self.source_bounds,
        });
        self.apply_crop(self.original_bounds);
        true
    }

    pub fn can_clear(&self) -> bool {
        self.annotation_count() > 0 || self.source_bounds != self.original_bounds
    }
}
