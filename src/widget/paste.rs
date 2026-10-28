use crate::editor::{CompositionContext, EditorState};

/// Tracks one asynchronous clipboard read without retaining editable content.
#[derive(Default)]
pub(super) enum PasteState {
    #[default]
    Idle,
    Waiting(CompositionContext),
    Discarding,
}

impl PasteState {
    pub(super) fn request(&mut self, editor: &EditorState) -> bool {
        if !matches!(self, Self::Idle) {
            return false;
        }
        *self = Self::Waiting(editor.composition_context());
        true
    }

    pub(super) fn cancel(&mut self) {
        if matches!(self, Self::Waiting(_)) {
            *self = Self::Discarding;
        }
    }

    /// Drains a response even after cancellation before allowing another read.
    pub(super) fn complete(&mut self, editor: &EditorState) -> bool {
        match std::mem::take(self) {
            Self::Waiting(context) => editor.is_composition_context_current(&context),
            Self::Idle | Self::Discarding => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PasteState;
    use crate::{document::Document, editor::EditorState};

    fn editor() -> EditorState {
        EditorState::new(Document::empty().expect("valid document")).expect("valid selection")
    }

    #[test]
    fn accepts_one_response_for_an_unchanged_context() {
        let editor = editor();
        let mut state = PasteState::default();
        assert!(state.request(&editor));
        assert!(!state.request(&editor));
        assert!(state.complete(&editor));
        assert!(!state.complete(&editor));
        assert!(state.request(&editor));
    }

    #[test]
    fn rejects_a_response_after_document_changes() {
        let mut editor = editor();
        let mut state = PasteState::default();
        assert!(state.request(&editor));
        editor.insert_text("new").expect("valid input");
        assert!(!state.complete(&editor));
        assert!(state.request(&editor));
    }

    #[test]
    fn cancellation_waits_for_the_old_response_before_another_read() {
        let editor = editor();
        let mut state = PasteState::default();
        assert!(state.request(&editor));
        state.cancel();
        state.cancel();
        assert!(!state.request(&editor));
        assert!(!state.complete(&editor));
        assert!(state.request(&editor));
        assert!(state.complete(&editor));
    }

    #[test]
    fn rejects_a_response_after_selection_is_reinstalled() {
        let mut editor = editor();
        let mut state = PasteState::default();
        assert!(state.request(&editor));
        editor
            .set_selection(editor.selection())
            .expect("valid selection");
        assert!(!state.complete(&editor));
    }

    #[test]
    fn rejects_a_response_for_a_different_editor() {
        let first = editor();
        let second = editor();
        let mut state = PasteState::default();
        assert!(state.request(&first));
        assert!(!state.complete(&second));
    }
}
