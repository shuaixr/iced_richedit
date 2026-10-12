use crate::document::{DocumentChange, Mark, TextSelection};

pub(super) struct HistoryEntry {
    pub(super) change: DocumentChange,
    pub(super) selection_before: TextSelection,
    pub(super) selection_after: TextSelection,
    pub(super) stored_marks_before: Option<Vec<Mark>>,
    pub(super) stored_marks_after: Option<Vec<Mark>>,
}

#[derive(Default)]
pub(super) struct History {
    undo_stack: Vec<HistoryEntry>,
    redo_stack: Vec<HistoryEntry>,
}

impl History {
    pub(super) fn record(&mut self, entry: HistoryEntry) {
        self.undo_stack.push(entry);
        self.redo_stack.clear();
    }
    pub(super) fn undo_entry(&self) -> Option<&HistoryEntry> {
        self.undo_stack.last()
    }

    pub(super) fn redo_entry(&self) -> Option<&HistoryEntry> {
        self.redo_stack.last()
    }

    pub(super) fn complete_undo(&mut self) {
        let entry = self
            .undo_stack
            .pop()
            .expect("undo completion requires an undo entry");

        self.redo_stack.push(entry);
    }

    pub(super) fn complete_redo(&mut self) {
        let entry = self
            .redo_stack
            .pop()
            .expect("redo completion requires a redo entry");

        self.undo_stack.push(entry);
    }
}
