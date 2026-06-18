#![warn(clippy::pedantic)]
use std::collections::VecDeque;
use crate::editor_core::code::{EditBatch};

/// A bounded undo/redo stack of edit batches.
pub struct History {
    index: usize,
    max_items: usize,
    edits: VecDeque<EditBatch>,
}

impl History {
    /// Create an empty history that keeps at most `max_items` edit batches.
    #[must_use] 
    pub fn new(max_items: usize) -> Self {
        Self {
            index: 0,
            max_items,
            edits: VecDeque::new(),
        }
    }

    /// Push a new edit batch, discarding any redo entries and the oldest batch if full.
    pub fn push(&mut self, batch: EditBatch) {
        while self.edits.len() > self.index {
            self.edits.pop_back();
        }

        if self.edits.len() == self.max_items {
            self.edits.pop_front();
            self.index -= 1;
        }

        self.edits.push_back(batch);
        self.index += 1;
    }

    /// Step back one batch and return it, or `None` if at the oldest state.
    pub fn undo(&mut self) -> Option<EditBatch> {
        if self.index == 0 {
            None
        } else {
            self.index -= 1;
            self.edits.get(self.index).cloned()
        }
    }

    /// Step forward one batch and return it, or `None` if at the newest state.
    pub fn redo(&mut self) -> Option<EditBatch> {
        if self.index >= self.edits.len() {
            None
        } else {
            let batch = self.edits.get(self.index).cloned();
            self.index += 1;
            batch
        }
    }
}