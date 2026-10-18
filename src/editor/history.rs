use std::time::{Duration, Instant};

use crate::document::{DocumentChange, DocumentError, Mark, TextSelection};

const INPUT_GROUP_TIMEOUT: Duration = Duration::from_millis(500);

pub(super) struct HistoryEntry {
    pub(super) change: DocumentChange,
    pub(super) selection_before: TextSelection,
    pub(super) selection_after: TextSelection,
    pub(super) stored_marks_before: Option<Vec<Mark>>,
    pub(super) stored_marks_after: Option<Vec<Mark>>,
}

struct InputGroup {
    marks: Vec<Mark>,
    last_input_at: Instant,
}

#[derive(Default)]
pub(super) struct History {
    undo_stack: Vec<HistoryEntry>,
    redo_stack: Vec<HistoryEntry>,
    input_group: Option<InputGroup>,
}

impl History {
    pub(super) fn record(&mut self, entry: HistoryEntry) {
        self.undo_stack.push(entry);
        self.redo_stack.clear();
        self.close_input_group();
    }

    pub(super) fn record_input(
        &mut self,
        entry: HistoryEntry,
        marks: Vec<Mark>,
        now: Instant,
    ) -> Result<(), DocumentError> {
        let can_merge = self.can_merge_input(&entry, &marks, now);

        if can_merge {
            let previous = self
                .undo_stack
                .last_mut()
                .expect("input merging requires an undo entry");

            let HistoryEntry {
                change,
                selection_after,
                stored_marks_after,
                ..
            } = entry;

            previous.change.append(change)?;

            previous.selection_after = selection_after;
            previous.stored_marks_after = stored_marks_after;
        } else {
            self.undo_stack.push(entry);
        }

        self.redo_stack.clear();

        self.input_group = Some(InputGroup {
            marks,
            last_input_at: now,
        });

        Ok(())
    }

    fn can_merge_input(&self, entry: &HistoryEntry, marks: &[Mark], now: Instant) -> bool {
        let Some(group) = &self.input_group else {
            return false;
        };

        let Some(previous) = self.undo_stack.last() else {
            return false;
        };

        let Some(elapsed) = now.checked_duration_since(group.last_input_at) else {
            return false;
        };

        previous.selection_after.is_empty()
            && entry.selection_before.is_empty()
            && entry.selection_after.is_empty()
            && previous.selection_after == entry.selection_before
            && group.marks.as_slice() == marks
            && elapsed <= INPUT_GROUP_TIMEOUT
    }

    pub(super) fn close_input_group(&mut self) {
        self.input_group = None;
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
        self.close_input_group();
    }

    pub(super) fn complete_redo(&mut self) {
        let entry = self
            .redo_stack
            .pop()
            .expect("redo completion requires a redo entry");

        self.undo_stack.push(entry);
        self.close_input_group();
    }
}
