use std::time::Instant;

use crate::{
    document::{Document, Mark, Position, TextSelection, TransactionResult},
    editor::{
        CompositionContext, EditorAction,
        error::EditorError,
        history::{History, HistoryEntry},
    },
};

pub struct EditorState {
    document: Document,
    selection: TextSelection,
    stored_marks: Option<Vec<Mark>>,
    history: History,
    composition_context: CompositionContext,
}
impl EditorState {
    pub fn new(document: Document) -> Result<Self, EditorError> {
        let selection = document
            .initial_selection()
            .map_err(EditorError::Document)?;

        Ok(Self {
            document,
            selection,
            stored_marks: None,
            history: History::default(),
            composition_context: CompositionContext::new(),
        })
    }
    pub fn document(&self) -> &Document {
        &self.document
    }
    pub fn selection(&self) -> TextSelection {
        self.selection
    }
    pub fn set_selection(&mut self, selection: TextSelection) -> Result<(), EditorError> {
        self.document
            .validate_selection(selection)
            .map_err(EditorError::Document)?;

        self.history.close_input_group();
        self.selection = selection;
        self.stored_marks = None;
        self.invalidate_composition_context();
        Ok(())
    }
    pub fn stored_marks(&self) -> Option<&[Mark]> {
        self.stored_marks.as_deref()
    }
    fn dispatch_transaction(
        &mut self,
        result: TransactionResult,
        selection: TextSelection,
        stored_marks: Option<Vec<Mark>>,
    ) -> Result<(), EditorError> {
        self.dispatch_with_input(result, selection, stored_marks, None)
    }

    fn dispatch_with_input(
        &mut self,
        result: TransactionResult,
        selection: TextSelection,
        stored_marks: Option<Vec<Mark>>,
        input: Option<(Vec<Mark>, Instant)>,
    ) -> Result<(), EditorError> {
        let (document, change) = result.into_parts();

        document
            .validate_selection(selection)
            .map_err(EditorError::Document)?;

        if let Some(change) = change {
            let entry = HistoryEntry {
                change,
                selection_before: self.selection,
                selection_after: selection,
                stored_marks_before: self.stored_marks.clone(),
                stored_marks_after: stored_marks.clone(),
            };

            if let Some((marks, now)) = input {
                self.history
                    .record_input(entry, marks, now)
                    .map_err(EditorError::Document)?;
            } else {
                self.history.record(entry);
            }
        }

        self.document = document;
        self.selection = selection;
        self.stored_marks = stored_marks;
        self.invalidate_composition_context();
        Ok(())
    }

    pub fn delete_selection(&mut self) -> Result<(), EditorError> {
        if self.selection.is_empty() {
            return Ok(());
        }

        let from = self.selection.from();
        let to = self.selection.to();

        let mut transaction = self.document.transaction();

        transaction
            .delete_range(from, to)
            .map_err(EditorError::Document)?;

        let result = transaction.finish();
        let selection = TextSelection::cursor(from);

        self.dispatch_cursor_edit(result, selection.head(), None, None)
    }

    pub fn insert_text(&mut self, text: &str) -> Result<(), EditorError> {
        self.insert_text_at(text, Instant::now())
    }

    fn insert_text_at(&mut self, text: &str, now: Instant) -> Result<(), EditorError> {
        if text.is_empty() {
            return Ok(());
        }

        let from = self.selection.from();
        let to = self.selection.to();
        let is_cursor_input = self.selection.is_empty();

        let marks = match &self.stored_marks {
            Some(marks) => marks.clone(),
            None => self
                .document
                .marks_at(from)
                .map_err(EditorError::Document)?,
        };

        let marks = self
            .document
            .normalize_marks(&marks)
            .map_err(EditorError::Document)?;

        let stored_marks = self.stored_marks.clone();
        let mut transaction = self.document.transaction();

        if !is_cursor_input {
            transaction
                .delete_range(from, to)
                .map_err(EditorError::Document)?;
        }

        transaction
            .insert_text_with_marks(from, text, &marks)
            .map_err(EditorError::Document)?;

        let cursor = Position::from_utf16_offset(from.utf16_offset() + text.encode_utf16().count());

        let input = if is_cursor_input {
            Some((marks, now))
        } else {
            None
        };

        self.dispatch_cursor_edit(transaction.finish(), cursor, stored_marks, input)
    }

    pub fn toggle_mark(&mut self, mark: Mark) -> Result<(), EditorError> {
        if !self.selection.is_empty() {
            let selection = self.selection;

            let remove = self
                .document
                .range_has_mark(selection, &mark)
                .map_err(EditorError::Document)?;

            let mut transaction = self.document.transaction();

            if remove {
                transaction
                    .remove_mark(selection.from(), selection.to(), mark)
                    .map_err(EditorError::Document)?;
            } else {
                transaction
                    .add_mark(selection.from(), selection.to(), mark)
                    .map_err(EditorError::Document)?;
            }

            return self.dispatch_transaction(transaction.finish(), selection, None);
        }
        let mut marks = match &self.stored_marks {
            Some(marks) => marks.clone(),
            None => self
                .document
                .marks_at(self.selection.head())
                .map_err(EditorError::Document)?,
        };

        if marks.contains(&mark) {
            marks.retain(|existing| existing != &mark);
        } else {
            marks.push(mark);
        }

        let marks = self
            .document
            .normalize_marks(&marks)
            .map_err(EditorError::Document)?;
        self.history.close_input_group();
        self.stored_marks = Some(marks);
        self.invalidate_composition_context();
        Ok(())
    }

    pub fn set_link(&mut self, href: String, title: Option<String>) -> Result<(), EditorError> {
        let mark = Mark::Link { href, title };

        if self.selection.is_empty() {
            let mut marks = match &self.stored_marks {
                Some(marks) => marks.clone(),
                None => self
                    .document
                    .marks_at(self.selection.head())
                    .map_err(EditorError::Document)?,
            };

            marks.push(mark);

            let marks = self
                .document
                .normalize_marks(&marks)
                .map_err(EditorError::Document)?;
            self.history.close_input_group();
            self.stored_marks = Some(marks);
            self.invalidate_composition_context();
            return Ok(());
        }

        let selection = self.selection;
        let mut transaction = self.document.transaction();

        transaction
            .add_mark(selection.from(), selection.to(), mark)
            .map_err(EditorError::Document)?;

        self.dispatch_transaction(transaction.finish(), selection, None)
    }

    pub fn remove_link(&mut self) -> Result<(), EditorError> {
        if self.selection.is_empty() {
            let mut marks = match &self.stored_marks {
                Some(marks) => marks.clone(),
                None => self
                    .document
                    .marks_at(self.selection.head())
                    .map_err(EditorError::Document)?,
            };

            marks.retain(|mark| !matches!(mark, Mark::Link { .. }));

            let marks = self
                .document
                .normalize_marks(&marks)
                .map_err(EditorError::Document)?;

            self.history.close_input_group();
            self.stored_marks = Some(marks);
            self.invalidate_composition_context();
            return Ok(());
        }

        let selection = self.selection;
        let mut transaction = self.document.transaction();

        transaction
            .remove_link(selection.from(), selection.to())
            .map_err(EditorError::Document)?;

        self.dispatch_transaction(transaction.finish(), selection, None)
    }

    pub fn split_paragraph(&mut self) -> Result<(), EditorError> {
        let from = self.selection.from();
        let to = self.selection.to();

        let mut transaction = self.document.transaction();

        if !self.selection.is_empty() {
            transaction
                .delete_range(from, to)
                .map_err(EditorError::Document)?;
        }

        transaction
            .split_paragraph(from)
            .map_err(EditorError::Document)?;

        let selection = TextSelection::cursor(Position::from_utf16_offset(from.utf16_offset() + 2));

        self.dispatch_cursor_edit(transaction.finish(), selection.head(), None, None)
    }

    pub fn undo(&mut self) -> Result<(), EditorError> {
        let Some(entry) = self.history.undo_entry() else {
            self.history.close_input_group();

            self.invalidate_composition_context();
            return Ok(());
        };

        let document = entry
            .change
            .apply_inverse(&self.document)
            .map_err(EditorError::Document)?;

        let selection = entry.selection_before;

        document
            .validate_selection(selection)
            .map_err(EditorError::Document)?;

        let stored_marks = entry.stored_marks_before.clone();

        self.history.complete_undo();

        self.document = document;
        self.selection = selection;
        self.stored_marks = stored_marks;
        self.invalidate_composition_context();

        Ok(())
    }

    pub fn redo(&mut self) -> Result<(), EditorError> {
        let Some(entry) = self.history.redo_entry() else {
            self.history.close_input_group();
            self.invalidate_composition_context();
            return Ok(());
        };

        let document = entry
            .change
            .apply_forward(&self.document)
            .map_err(EditorError::Document)?;

        let selection = entry.selection_after;

        document
            .validate_selection(selection)
            .map_err(EditorError::Document)?;

        let stored_marks = entry.stored_marks_after.clone();

        self.history.complete_redo();

        self.document = document;
        self.selection = selection;
        self.stored_marks = stored_marks;
        self.invalidate_composition_context();

        Ok(())
    }

    pub fn can_undo(&self) -> bool {
        self.history.undo_entry().is_some()
    }

    pub fn can_redo(&self) -> bool {
        self.history.redo_entry().is_some()
    }

    pub fn move_left(&mut self) -> Result<(), EditorError> {
        if !self.selection.is_empty() {
            return self.set_selection(TextSelection::cursor(self.selection.from()));
        }

        let position = self.selection.head();

        let previous = self
            .document
            .previous_cursor_position(position)
            .map_err(EditorError::Document)?;

        let Some(previous) = previous else {
            return Ok(());
        };

        self.set_selection(TextSelection::cursor(previous))
    }

    pub fn move_right(&mut self) -> Result<(), EditorError> {
        if !self.selection.is_empty() {
            return self.set_selection(TextSelection::cursor(self.selection.to()));
        }

        let position = self.selection.head();

        let next = self
            .document
            .next_cursor_position(position)
            .map_err(EditorError::Document)?;

        let Some(next) = next else {
            return Ok(());
        };

        self.set_selection(TextSelection::cursor(next))
    }

    pub fn delete_backward(&mut self) -> Result<(), EditorError> {
        self.delete_adjacent(false, false)
    }

    pub fn delete_forward(&mut self) -> Result<(), EditorError> {
        self.delete_adjacent(true, false)
    }

    pub fn delete_word_backward(&mut self) -> Result<(), EditorError> {
        self.delete_adjacent(false, true)
    }

    pub fn delete_word_forward(&mut self) -> Result<(), EditorError> {
        self.delete_adjacent(true, true)
    }

    fn delete_adjacent(&mut self, forward: bool, by_word: bool) -> Result<(), EditorError> {
        if !self.selection.is_empty() {
            return self.delete_selection();
        }

        let current = self.selection.head();

        let adjacent = if by_word {
            self.document.adjacent_word_position(current, forward)
        } else if forward {
            self.document.next_cursor_position(current)
        } else {
            self.document.previous_cursor_position(current)
        }
        .map_err(EditorError::Document)?;

        let Some(adjacent) = adjacent else {
            return Ok(());
        };

        let at_boundary = self
            .document
            .cursor_at_paragraph_boundary(current, forward)
            .map_err(EditorError::Document)?;

        let mut transaction = self.document.transaction();

        let cursor = if at_boundary {
            let join_offset = if forward {
                current.utf16_offset() + 1
            } else {
                current.utf16_offset() - 1
            };

            transaction
                .join_paragraphs(Position::from_utf16_offset(join_offset))
                .map_err(EditorError::Document)?;

            if forward { current } else { adjacent }
        } else {
            let (from, to) = if forward {
                (current, adjacent)
            } else {
                (adjacent, current)
            };

            transaction
                .delete_range(from, to)
                .map_err(EditorError::Document)?;

            from
        };

        self.dispatch_cursor_edit(transaction.finish(), cursor, None, None)
    }

    fn dispatch_cursor_edit(
        &mut self,
        result: TransactionResult,
        cursor: Position,
        stored_marks: Option<Vec<Mark>>,
        input: Option<(Vec<Mark>, Instant)>,
    ) -> Result<(), EditorError> {
        let cursor = result
            .document()
            .snap_cursor_forward(cursor)
            .map_err(EditorError::Document)?;

        self.dispatch_with_input(result, TextSelection::cursor(cursor), stored_marks, input)
    }

    pub fn select_all(&mut self) -> Result<(), EditorError> {
        let selection = self
            .document
            .full_selection()
            .map_err(EditorError::Document)?;

        self.set_selection(selection)
    }

    pub fn extend_left(&mut self) -> Result<(), EditorError> {
        self.extend_selection(false)
    }

    pub fn extend_right(&mut self) -> Result<(), EditorError> {
        self.extend_selection(true)
    }

    fn extend_selection(&mut self, forward: bool) -> Result<(), EditorError> {
        let head = self.selection.head();

        let adjacent = if forward {
            self.document.next_cursor_position(head)
        } else {
            self.document.previous_cursor_position(head)
        }
        .map_err(EditorError::Document)?;

        let Some(head) = adjacent else {
            return Ok(());
        };

        self.set_selection(TextSelection::new(self.selection.anchor(), head))
    }

    pub fn apply(&mut self, action: EditorAction) -> Result<(), EditorError> {
        match action {
            EditorAction::CommitCompositionInContext { context, text } => {
                self.commit_composition_in_context(&context, &text)
            }
            EditorAction::InsertText(text) => self.insert_text(&text),
            EditorAction::PasteText(text) => self.paste_text(&text),
            EditorAction::CommitComposition(text) => self.commit_composition(&text),
            EditorAction::SetSelection(selection) => self.set_selection(selection),
            EditorAction::SplitParagraph => self.split_paragraph(),
            EditorAction::DeleteBackward => self.delete_backward(),
            EditorAction::DeleteForward => self.delete_forward(),
            EditorAction::DeleteWordBackward => self.delete_word_backward(),
            EditorAction::DeleteWordForward => self.delete_word_forward(),
            EditorAction::MoveLeft => self.move_left(),
            EditorAction::MoveRight => self.move_right(),
            EditorAction::MoveWordLeft => self.move_word_left(),
            EditorAction::MoveWordRight => self.move_word_right(),
            EditorAction::ExtendWordLeft => self.extend_word_left(),
            EditorAction::ExtendWordRight => self.extend_word_right(),
            EditorAction::MoveDocumentStart => self.move_document_start(),
            EditorAction::MoveDocumentEnd => self.move_document_end(),
            EditorAction::ExtendDocumentStart => self.extend_document_start(),
            EditorAction::ExtendDocumentEnd => self.extend_document_end(),
            EditorAction::Undo => self.undo(),
            EditorAction::Redo => self.redo(),
            EditorAction::ExtendLeft => self.extend_left(),
            EditorAction::ExtendRight => self.extend_right(),
            EditorAction::SelectAll => self.select_all(),
            EditorAction::DeleteSelection => self.delete_selection(),
        }
    }

    /// Returns the selected plain text without changing editor state.
    pub fn selected_text(&self) -> Result<String, EditorError> {
        self.document
            .selection_text(self.selection)
            .map_err(EditorError::Document)
    }

    /// Replaces the selection with plain text as one history event.
    pub fn paste_text(&mut self, text: &str) -> Result<(), EditorError> {
        if text.is_empty() {
            return Ok(());
        }

        let from = self.selection.from();
        let to = self.selection.to();

        let marks = match &self.stored_marks {
            Some(marks) => marks.clone(),
            None => self
                .document
                .marks_at(from)
                .map_err(EditorError::Document)?,
        };

        let marks = self
            .document
            .normalize_marks(&marks)
            .map_err(EditorError::Document)?;

        let stored_marks = self.stored_marks.clone();
        let mut transaction = self.document.transaction();

        let cursor = transaction
            .replace_plain_text(from, to, text, &marks)
            .map_err(EditorError::Document)?;

        self.dispatch_cursor_edit(transaction.finish(), cursor, stored_marks, None)
    }

    pub fn move_word_left(&mut self) -> Result<(), EditorError> {
        self.navigate_word(false, false)
    }

    pub fn move_word_right(&mut self) -> Result<(), EditorError> {
        self.navigate_word(true, false)
    }

    pub fn extend_word_left(&mut self) -> Result<(), EditorError> {
        self.navigate_word(false, true)
    }

    pub fn extend_word_right(&mut self) -> Result<(), EditorError> {
        self.navigate_word(true, true)
    }

    fn navigate_word(&mut self, forward: bool, extend: bool) -> Result<(), EditorError> {
        // Match ordinary horizontal navigation: collapse a selection first.
        if !extend && !self.selection.is_empty() {
            let target = if forward {
                self.selection.to()
            } else {
                self.selection.from()
            };

            return self.set_selection(TextSelection::cursor(target));
        }

        let target = self
            .document
            .adjacent_word_position(self.selection.head(), forward)
            .map_err(EditorError::Document)?;

        let Some(target) = target else {
            return Ok(());
        };

        let selection = if extend {
            TextSelection::new(self.selection.anchor(), target)
        } else {
            TextSelection::cursor(target)
        };

        self.set_selection(selection)
    }

    pub fn move_document_start(&mut self) -> Result<(), EditorError> {
        self.navigate_document_boundary(false, false)
    }

    pub fn move_document_end(&mut self) -> Result<(), EditorError> {
        self.navigate_document_boundary(true, false)
    }

    pub fn extend_document_start(&mut self) -> Result<(), EditorError> {
        self.navigate_document_boundary(false, true)
    }

    pub fn extend_document_end(&mut self) -> Result<(), EditorError> {
        self.navigate_document_boundary(true, true)
    }

    fn navigate_document_boundary(&mut self, end: bool, extend: bool) -> Result<(), EditorError> {
        let full_selection = self
            .document
            .full_selection()
            .map_err(EditorError::Document)?;

        let target = if end {
            full_selection.to()
        } else {
            full_selection.from()
        };

        let selection = if extend {
            TextSelection::new(self.selection.anchor(), target)
        } else {
            TextSelection::cursor(target)
        };

        self.set_selection(selection)
    }

    /// Commits input method text as a separate history event.
    ///
    /// An empty commit preserves content, selection and stored marks,
    /// but closes the input group and invalidates the composition context.
    pub fn commit_composition(&mut self, text: &str) -> Result<(), EditorError> {
        if text.is_empty() {
            self.history.close_input_group();
            self.invalidate_composition_context();
            return Ok(());
        }

        self.paste_text(text)
    }

    /// Captures the current context for a future composition commit.
    pub fn composition_context(&self) -> CompositionContext {
        self.composition_context.clone()
    }

    fn invalidate_composition_context(&mut self) {
        self.composition_context = CompositionContext::new();
    }

    /// Commits only if no context-invalidating operation has occurred.
    pub fn commit_composition_in_context(
        &mut self,
        context: &CompositionContext,
        text: &str,
    ) -> Result<(), EditorError> {
        if !self.is_composition_context_current(context) {
            return Err(EditorError::StaleComposition);
        }

        self.commit_composition(text)
    }

    /// Returns whether a captured composition context is still current.
    pub fn is_composition_context_current(&self, context: &CompositionContext) -> bool {
        self.composition_context.matches(context)
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use crate::{
        document::{Document, DocumentError, Mark, Position, TextSelection},
        editor::error::EditorError,
    };

    use super::EditorState;

    #[test]
    fn set_selection_preserves_backward_selection() {
        let document = Document::from_json(
            r#"{
                "type": "doc",
                "content": [
                    {
                        "type": "paragraph",
                        "content": [
                            {
                                "type": "text",
                                "text": "Hello"
                            }
                        ]
                    }
                ]
            }"#,
        )
        .expect("document must be valid");

        let mut editor_state = EditorState::new(document).expect("editor state must be valid");

        let selection = TextSelection::new(
            Position::from_utf16_offset(6),
            Position::from_utf16_offset(1),
        );
        editor_state
            .set_selection(selection)
            .expect("selection must be valid");
        assert_eq!(editor_state.selection(), selection);
    }

    #[test]
    fn set_selection_rejects_block_boundary_without_changing_selection() {
        let document = Document::empty().expect("empty document must be valid");
        let mut editor_state = EditorState::new(document).expect("editor state must be valid");

        let selection = TextSelection::new(
            Position::from_utf16_offset(0),
            Position::from_utf16_offset(1),
        );

        assert!(matches!(
            editor_state.set_selection(selection),
            Err(EditorError::Document(DocumentError::InvalidPosition {
                position: 0
            }))
        ));
        assert_eq!(
            editor_state.selection(),
            TextSelection::new(
                Position::from_utf16_offset(1),
                Position::from_utf16_offset(1)
            )
        );
    }
    #[test]
    fn set_selection_clears_stored_marks() {
        let document = Document::empty().expect("empty document must be valid");
        let mut editor_state = EditorState::new(document).expect("editor state must be valid");

        editor_state.stored_marks = Some(vec![Mark::Strong]);

        let selection = TextSelection::new(
            Position::from_utf16_offset(1),
            Position::from_utf16_offset(1),
        );
        editor_state
            .set_selection(selection)
            .expect("selection must be valid");
        assert_eq!(editor_state.selection(), selection);
        assert!(editor_state.stored_marks().is_none());
    }
    #[test]
    fn set_selection_preserves_stored_marks_when_selection_is_invalid() {
        let document = Document::empty().expect("empty document must be valid");
        let mut editor_state = EditorState::new(document).expect("editor state must be valid");

        editor_state.stored_marks = Some(vec![Mark::Strong]);

        let selection = TextSelection::new(
            Position::from_utf16_offset(0),
            Position::from_utf16_offset(1),
        );
        assert!(matches!(
            editor_state.set_selection(selection),
            Err(EditorError::Document(DocumentError::InvalidPosition {
                position: 0
            }))
        ));
        assert_eq!(
            editor_state.selection(),
            TextSelection::new(
                Position::from_utf16_offset(1),
                Position::from_utf16_offset(1)
            )
        );
        assert_eq!(editor_state.stored_marks(), Some(&[Mark::Strong][..]));
    }
    #[test]
    fn delete_selection_updates_document_and_collapses_cursor() {
        let document = Document::from_json(
            r#"{
                "type": "doc",
                "content": [
                    {
                        "type": "paragraph",
                        "content": [
                            {
                                "type": "text",
                                "text": "Hello"
                            }
                        ]
                    }
                ]
            }"#,
        )
        .expect("document must be valid");

        let mut editor_state = EditorState::new(document).expect("editor state must be valid");

        let selection = TextSelection::new(
            Position::from_utf16_offset(1),
            Position::from_utf16_offset(2),
        );
        editor_state
            .set_selection(selection)
            .expect("selection must be valid");
        editor_state
            .delete_selection()
            .expect("deletion must succeed");

        assert_eq!(editor_state.document().text(), "ello");

        assert_eq!(
            editor_state.selection(),
            TextSelection::cursor(Position::from_utf16_offset(1))
        );
    }
    #[test]
    fn delete_selection_preserves_state_when_selection_is_empty() {
        let document = Document::from_json(
            r#"{
                "type": "doc",
                "content": [
                    {
                        "type": "paragraph",
                        "content": [
                            {
                                "type": "text",
                                "text": "Hello"
                            }
                        ]
                    }
                ]
            }"#,
        )
        .expect("document must be valid");

        let mut editor_state = EditorState::new(document).expect("editor state must be valid");

        let selection = TextSelection::cursor(Position::from_utf16_offset(1));
        editor_state
            .set_selection(selection)
            .expect("selection must be valid");
        editor_state.stored_marks = Some(vec![Mark::Strong]);
        editor_state
            .delete_selection()
            .expect("deletion must succeed");

        assert_eq!(editor_state.document().text(), "Hello");
        assert_eq!(editor_state.stored_marks(), Some(&[Mark::Strong][..]));
        assert_eq!(
            editor_state.selection(),
            TextSelection::cursor(Position::from_utf16_offset(1))
        );
    }
    #[test]
    fn dispatch_transaction_preserves_state_when_new_selection_is_invalid() {
        let document = Document::from_json(
            r#"{
                "type": "doc",
                "content": [
                    {
                        "type": "paragraph",
                        "content": [
                            {
                                "type": "text",
                                "text": "Hello"
                            }
                        ]
                    }
                ]
            }"#,
        )
        .expect("document must be valid");

        let mut editor_state = EditorState::new(document).expect("editor state must be valid");

        let selection = TextSelection::new(
            Position::from_utf16_offset(1),
            Position::from_utf16_offset(2),
        );
        editor_state
            .set_selection(selection)
            .expect("selection must be valid");

        editor_state.stored_marks = Some(vec![Mark::Strong]);
        let mut transaction = editor_state.document().transaction();
        transaction
            .delete_range(
                Position::from_utf16_offset(1),
                Position::from_utf16_offset(2),
            )
            .expect("deletion must succeed");
        let result = transaction.finish();

        let new_selection = TextSelection::new(
            Position::from_utf16_offset(0),
            Position::from_utf16_offset(1),
        );

        assert!(matches!(
            editor_state.dispatch_transaction(result, new_selection, Some(vec![Mark::Code])),
            Err(EditorError::Document(DocumentError::InvalidPosition {
                position: 0
            }))
        ));

        assert_eq!(editor_state.document().text(), "Hello");
        assert_eq!(editor_state.selection(), selection);
        assert_eq!(editor_state.stored_marks(), Some(&[Mark::Strong][..]));
    }
    #[test]
    fn dispatch_transaction_installs_document_selection_and_stored_marks() {
        let document = Document::from_json(
            r#"{
                "type": "doc",
                "content": [
                    {
                        "type": "paragraph",
                        "content": [
                            {
                                "type": "text",
                                "text": "Hello"
                            }
                        ]
                    }
                ]
            }"#,
        )
        .expect("document must be valid");

        let mut editor_state = EditorState::new(document).expect("editor state must be valid");

        let selection = TextSelection::new(
            Position::from_utf16_offset(1),
            Position::from_utf16_offset(2),
        );
        editor_state
            .set_selection(selection)
            .expect("selection must be valid");

        let mut transaction = editor_state.document().transaction();
        transaction
            .delete_range(
                Position::from_utf16_offset(1),
                Position::from_utf16_offset(2),
            )
            .expect("deletion must succeed");
        let result = transaction.finish();

        let new_selection = TextSelection::cursor(Position::from_utf16_offset(1));
        let new_stored_marks = Some(vec![Mark::Code]);

        editor_state
            .dispatch_transaction(result, new_selection, new_stored_marks.clone())
            .expect("dispatch must succeed");

        assert_eq!(editor_state.document().text(), "ello");
        assert_eq!(editor_state.selection(), new_selection);
        assert_eq!(editor_state.stored_marks(), new_stored_marks.as_deref());
    }

    #[test]
    fn insert_text_updates_document_and_moves_cursor() {
        let document = Document::from_json(
            r#"{
                "type": "doc",
                "content": [
                    {
                        "type": "paragraph",
                        "content": [
                            {
                                "type": "text",
                                "text": "Hello"
                            }
                        ]
                    }
                ]
            }"#,
        )
        .expect("document must be valid");

        let mut editor_state = EditorState::new(document).expect("editor state must be valid");

        let selection = TextSelection::cursor(Position::from_utf16_offset(3));
        editor_state
            .set_selection(selection)
            .expect("selection must be valid");
        editor_state
            .insert_text("y")
            .expect("insertion must succeed");

        assert_eq!(editor_state.document().text(), "Heyllo");
        assert_eq!(
            editor_state.selection(),
            TextSelection::cursor(Position::from_utf16_offset(4))
        );
    }

    #[test]
    fn insert_text_replaces_selection_in_either_direction() {
        for (anchor, head) in [(1, 3), (3, 1)] {
            let document = Document::from_json(
                r#"{
                "type": "doc",
                "content": [
                    {
                        "type": "paragraph",
                        "content": [
                            {
                                "type": "text",
                                "text": "Hello"
                            }
                        ]
                    }
                ]
            }"#,
            )
            .expect("document must be valid");

            let mut editor_state = EditorState::new(document).expect("editor state must be valid");

            // Test inserting text that replaces a selection from left to right
            let selection = TextSelection::new(
                Position::from_utf16_offset(anchor),
                Position::from_utf16_offset(head),
            );
            editor_state
                .set_selection(selection)
                .expect("selection must be valid");
            editor_state
                .insert_text("A")
                .expect("insertion must succeed");

            assert_eq!(editor_state.document().text(), "Allo");
            assert_eq!(
                editor_state.selection(),
                TextSelection::cursor(Position::from_utf16_offset(2))
            );
        }
    }

    #[test]
    fn insert_text_moves_cursor_by_utf16_length() {
        let document = Document::from_json(
            r#"{
                "type": "doc",
                "content": [
                    {
                        "type": "paragraph",
                        "content": [
                            {
                                "type": "text",
                                "text": "Hello"
                            }
                        ]
                    }
                ]
            }"#,
        )
        .expect("document must be valid");

        let mut editor_state = EditorState::new(document).expect("editor state must be valid");

        let selection = TextSelection::cursor(Position::from_utf16_offset(3));
        editor_state
            .set_selection(selection)
            .expect("selection must be valid");
        editor_state
            .insert_text("𐍈")
            .expect("insertion must succeed");

        assert_eq!(editor_state.document().text(), "He𐍈llo");
        assert_eq!(
            editor_state.selection(),
            TextSelection::cursor(Position::from_utf16_offset(5))
        );
    }
    #[test]
    fn insert_text_preserves_state_when_text_is_empty() {
        let document = Document::from_json(
            r#"{
                "type": "doc",
                "content": [
                    {
                        "type": "paragraph",
                        "content": [
                            {
                                "type": "text",
                                "text": "Hello"
                            }
                        ]
                    }
                ]
            }"#,
        )
        .expect("document must be valid");

        let mut editor_state = EditorState::new(document).expect("editor state must be valid");
        let selection = TextSelection::new(
            Position::from_utf16_offset(1),
            Position::from_utf16_offset(3),
        );
        editor_state
            .set_selection(selection)
            .expect("selection must be valid");
        editor_state.stored_marks = Some(vec![Mark::Strong]);
        editor_state
            .insert_text("")
            .expect("insertion must succeed");

        assert_eq!(editor_state.document().text(), "Hello");
        assert_eq!(editor_state.selection(), selection);
        assert_eq!(editor_state.stored_marks(), Some(&[Mark::Strong][..]));
    }

    #[test]
    fn insert_text_inherits_strong_from_cursor_context() {
        let document = Document::from_json(
            r#"{
                "type": "doc",
                "content": [
                    {
                        "type": "paragraph",
                        "content": [
                            {
                                "type": "text",
                                "text": "Hello",
                                "marks": [{ "type": "strong" }]
                            }
                        ]
                    }
                ]
            }"#,
        )
        .expect("document must be valid");

        let mut editor_state = EditorState::new(document).expect("editor state must be valid");
        let selection = TextSelection::cursor(Position::from_utf16_offset(2));
        editor_state
            .set_selection(selection)
            .expect("selection must be valid");
        editor_state
            .insert_text("y")
            .expect("insertion must succeed");
        let actual: serde_json::Value = serde_json::from_str(
            &editor_state
                .document()
                .to_json()
                .expect("document must serialize"),
        )
        .expect("output must be valid JSON");

        assert_eq!(
            actual,
            serde_json::json!({
                "type": "doc",
                "content": [{
                    "type": "paragraph",
                    "content": [{
                        "type": "text",
                        "text": "Hyello",
                        "marks": [{ "type": "strong" }]
                    }]
                }]
            })
        );
        assert_eq!(editor_state.document().text(), "Hyello");
        assert_eq!(
            editor_state.selection(),
            TextSelection::cursor(Position::from_utf16_offset(3))
        );
        assert!(editor_state.stored_marks().is_none());
    }

    #[test]
    fn insert_text_with_empty_stored_marks_does_not_inherit_strong() {
        let document = Document::from_json(
            r#"{
                "type": "doc",
                "content": [
                    {
                        "type": "paragraph",
                        "content": [
                            {
                                "type": "text",
                                "text": "Hello",
                                "marks": [{ "type": "strong" }]
                            }
                        ]
                    }
                ]
            }"#,
        )
        .expect("document must be valid");

        let mut editor_state = EditorState::new(document).expect("editor state must be valid");
        let selection = TextSelection::cursor(Position::from_utf16_offset(2));
        editor_state
            .set_selection(selection)
            .expect("selection must be valid");
        editor_state.stored_marks = Some(vec![]);
        editor_state
            .insert_text("y")
            .expect("insertion must succeed");
        let actual: serde_json::Value = serde_json::from_str(
            &editor_state
                .document()
                .to_json()
                .expect("document must serialize"),
        )
        .expect("output must be valid JSON");

        assert_eq!(
            actual,
            serde_json::json!({
                "type": "doc",
                "content": [{
                    "type": "paragraph",
                    "content": [
                        {
                            "type": "text",
                            "text": "H",
                            "marks": [{ "type": "strong" }]
                        },
                        {
                            "type": "text",
                            "text": "y"
                        },
                        {
                            "type": "text",
                            "text": "ello",
                            "marks": [{ "type": "strong" }]
                        }
                    ]
                }]
            })
        );
        assert_eq!(editor_state.document().text(), "Hyello");
        assert_eq!(
            editor_state.selection(),
            TextSelection::cursor(Position::from_utf16_offset(3))
        );
        assert_eq!(editor_state.stored_marks(), Some(&[][..]));
    }

    #[test]
    fn insert_text_preserves_explicit_strong_across_consecutive_inputs() {
        let document = Document::from_json(
            r#"{
                "type": "doc",
                "content": [
                    {
                        "type": "paragraph",
                        "content": [
                            {
                                "type": "text",
                                "text": "Hello"
                            }
                        ]
                    }
                ]
            }"#,
        )
        .expect("document must be valid");

        let mut editor_state = EditorState::new(document).expect("editor state must be valid");
        let selection = TextSelection::cursor(Position::from_utf16_offset(2));
        editor_state
            .set_selection(selection)
            .expect("selection must be valid");
        editor_state.stored_marks = Some(vec![Mark::Strong]);
        editor_state
            .insert_text("y")
            .expect("insertion must succeed");
        assert_eq!(editor_state.stored_marks(), Some(&[Mark::Strong][..]));
        editor_state
            .insert_text("!")
            .expect("insertion must succeed");
        let actual: serde_json::Value = serde_json::from_str(
            &editor_state
                .document()
                .to_json()
                .expect("document must serialize"),
        )
        .expect("output must be valid JSON");

        assert_eq!(
            actual,
            serde_json::json!({
                "type": "doc",
                "content": [{
                    "type": "paragraph",
                    "content": [
                        {
                            "type": "text",
                            "text": "H"
                        },
                        {
                            "type": "text",
                            "text": "y!",
                            "marks": [{ "type": "strong" }]
                        },
                        {
                            "type": "text",
                            "text": "ello"
                        }
                    ]
                }]
            })
        );
        assert_eq!(editor_state.document().text(), "Hy!ello");
        assert_eq!(
            editor_state.selection(),
            TextSelection::cursor(Position::from_utf16_offset(4))
        );
        assert_eq!(editor_state.stored_marks(), Some(&[Mark::Strong][..]));
    }

    #[test]
    fn toggle_mark_at_cursor_preserves_document_and_toggles_stored_marks() {
        let document = Document::empty().expect("document must be valid");
        let mut state = EditorState::new(document).expect("editor state must be valid");

        let original_json = state.document().to_json().expect("document must serialize");
        let original_selection = state.selection();

        state
            .toggle_mark(Mark::Strong)
            .expect("mark toggle must succeed");

        assert_eq!(state.stored_marks(), Some(&[Mark::Strong][..]));

        state
            .toggle_mark(Mark::Strong)
            .expect("mark toggle must succeed");

        assert_eq!(state.stored_marks(), Some(&[][..]));
        assert_eq!(state.selection(), original_selection);
        assert_eq!(
            state.document().to_json().expect("document must serialize"),
            original_json
        );
    }

    #[test]
    fn toggle_mark_adds_strong_to_range_and_preserves_direction() {
        let document = Document::from_json(
            r#"{
                "type": "doc",
                "content": [
                    {
                        "type": "paragraph",
                        "content": [
                            {
                                "type": "text",
                                "text": "Hello"
                            }
                        ]
                    }
                ]
            }"#,
        )
        .expect("document must be valid");

        let mut state = EditorState::new(document).expect("editor state must be valid");

        let selection = TextSelection::new(
            Position::from_utf16_offset(5),
            Position::from_utf16_offset(2),
        );
        state
            .set_selection(selection)
            .expect("selection must be valid");
        state
            .toggle_mark(Mark::Strong)
            .expect("mark toggle must succeed");

        let actual: serde_json::Value =
            serde_json::from_str(&state.document().to_json().expect("document must serialize"))
                .expect("output must be valid JSON");

        assert_eq!(
            actual,
            serde_json::json!({
                "type": "doc",
                "content": [{
                    "type": "paragraph",
                    "content": [
                        {
                            "type": "text",
                            "text": "H"
                        },
                        {
                            "type": "text",
                            "text": "ell",
                            "marks": [{ "type": "strong" }]
                        },
                        {
                            "type": "text",
                            "text": "o"
                        }
                    ]
                }]
            })
        );

        assert_eq!(state.selection(), selection);
        assert!(state.stored_marks().is_none());
    }

    #[test]
    fn toggle_mark_removes_strong_when_entire_range_is_marked() {
        let document = Document::from_json(
            r#"{
           "type": "doc",
           "content": [
               {
                   "type": "paragraph",
                    "content": [
                        {
                            "type": "text",
                            "text": "Hello",
                            "marks": [{ "type": "strong" }]
                        }
                ]
               }
           ]
       }"#,
        )
        .expect("document must be valid");

        let mut state = EditorState::new(document).expect("editor state must be valid");

        let selection = TextSelection::new(
            Position::from_utf16_offset(1),
            Position::from_utf16_offset(6),
        );
        state
            .set_selection(selection)
            .expect("selection must be valid");
        state
            .toggle_mark(Mark::Strong)
            .expect("mark toggle must succeed");

        let actual: serde_json::Value =
            serde_json::from_str(&state.document().to_json().expect("document must serialize"))
                .expect("output must be valid JSON");

        assert_eq!(
            actual,
            serde_json::json!({
                "type": "doc",
                "content": [{
                    "type": "paragraph",
                    "content": [{
                        "type": "text",
                        "text": "Hello"
                    }]
                }]
            })
        );

        assert_eq!(state.selection(), selection);
        assert!(state.stored_marks().is_none());
    }

    #[test]
    fn toggle_mark_adds_strong_when_range_is_partially_marked() {
        let document = Document::from_json(
            r#"{
                "type": "doc",
                "content": [{
                "type": "paragraph",
                "content": [
                    {
                    "type": "text",
                    "text": "He",
                    "marks": [{ "type": "strong" }]
                    },
                    {
                    "type": "text",
                    "text": "llo"
                    }
                ]
                }]
            }"#,
        )
        .expect("document must be valid");

        let mut state = EditorState::new(document).expect("editor state must be valid");

        let selection = TextSelection::new(
            Position::from_utf16_offset(6),
            Position::from_utf16_offset(1),
        );
        state
            .set_selection(selection)
            .expect("selection must be valid");
        state
            .toggle_mark(Mark::Strong)
            .expect("mark toggle must succeed");

        let actual: serde_json::Value =
            serde_json::from_str(&state.document().to_json().expect("document must serialize"))
                .expect("output must be valid JSON");

        assert_eq!(
            actual,
            serde_json::json!({
                "type": "doc",
                "content": [{
                    "type": "paragraph",
                    "content": [{
                        "type": "text",
                        "text": "Hello",
                        "marks": [{ "type": "strong" }]
                    }]
                }]
            })
        );

        assert_eq!(state.selection(), selection);
        assert!(state.stored_marks().is_none());
    }

    #[test]
    fn set_link_replaces_existing_link_in_selection() {
        let document = Document::from_json(
            r#"{
                "type": "doc",
                "content": [{
                    "type": "paragraph",
                    "content": [{
                        "type": "text",
                        "text": "Hello",
                        "marks": [
                            { "type": "strong" },
                            {
                                "type": "link",
                                "attrs": {
                                    "href": "https://example.com",
                                    "title": null
                                }
                            }
                        ]
                    }]
                }]
            }"#,
        )
        .expect("document must be valid");

        let mut state = EditorState::new(document).expect("editor state must be valid");

        let selection = TextSelection::new(
            Position::from_utf16_offset(6),
            Position::from_utf16_offset(1),
        );
        state
            .set_selection(selection)
            .expect("selection must be valid");
        state
            .set_link("https://new-link.com".to_owned(), None)
            .expect("link must be set");

        let actual: serde_json::Value =
            serde_json::from_str(&state.document().to_json().expect("document must serialize"))
                .expect("output must be valid JSON");

        assert_eq!(
            actual,
            serde_json::json!({
                "type": "doc",
                "content": [{
                    "type": "paragraph",
                    "content": [{
                        "type": "text",
                        "text": "Hello",
                        "marks": [
                            { "type": "strong" },
                            {
                                "type": "link",
                                "attrs": {
                                    "href": "https://new-link.com",
                                    "title": null
                                }
                            }
                        ]
                    }]
                }]
            })
        );
        assert_eq!(state.selection(), selection);
        assert!(state.stored_marks().is_none());
    }

    #[test]
    fn remove_link_removes_links_with_different_attrs() {
        let document = Document::from_json(
            r#"{
                "type": "doc",
                "content": [{
                    "type": "paragraph",
                   "content": [
                        {
                            "type": "text",
                            "text": "He",
                            "marks": [
                                { "type": "strong" },
                                {
                                    "type": "link",
                                    "attrs": {
                                        "href": "https://a.example",
                                        "title": null
                                    }
                                }
                            ]
                        },
                        {
                            "type": "text",
                            "text": "llo",
                            "marks": [{
                                "type": "link",
                                "attrs": {
                                    "href": "https://b.example",
                                    "title": "Second"
                                }
                            }]
                        }
                    ]
                }]
            }"#,
        )
        .expect("document must be valid");

        let mut state = EditorState::new(document).expect("editor state must be valid");

        let selection = TextSelection::new(
            Position::from_utf16_offset(6),
            Position::from_utf16_offset(1),
        );
        state
            .set_selection(selection)
            .expect("selection must be valid");
        state.remove_link().expect("link must be removed");

        let actual: serde_json::Value =
            serde_json::from_str(&state.document().to_json().expect("document must serialize"))
                .expect("output must be valid JSON");

        assert_eq!(
            actual,
            serde_json::json!({
                "type": "doc",
                "content": [{
                    "type": "paragraph",
                    "content": [
                        {
                            "type": "text",
                            "text": "He",
                            "marks": [{ "type": "strong" }]
                        },
                        {
                            "type": "text",
                            "text": "llo"
                        }
                    ]
                }]
            })
        );
        assert_eq!(state.selection(), selection);
        assert!(state.stored_marks().is_none());
    }

    #[test]
    fn insert_text_snaps_cursor_after_newly_combined_grapheme() {
        let document = Document::from_json(
            r#"{
               "type": "doc",
               "content": [{
                   "type": "paragraph",
                   "content": [{
                       "type": "text",
                       "text": "\u0301"
                   }]
               }]
           }"#,
        )
        .expect("document must be valid");

        let mut state = EditorState::new(document).expect("editor state must be valid");

        state
            .insert_text("e")
            .expect("input must succeed when it creates a grapheme");

        assert_eq!(state.document().text(), "e\u{301}");
        assert_eq!(
            state.selection(),
            TextSelection::cursor(Position::from_utf16_offset(3))
        );

        state.undo().expect("undo must succeed");

        assert_eq!(state.document().text(), "\u{301}");
        assert_eq!(
            state.selection(),
            TextSelection::cursor(Position::from_utf16_offset(1))
        );

        state.redo().expect("redo must succeed");

        assert_eq!(state.document().text(), "e\u{301}");
        assert_eq!(
            state.selection(),
            TextSelection::cursor(Position::from_utf16_offset(3))
        );
    }

    #[test]
    fn consecutive_inputs_share_one_undo_event() {
        let mut state = EditorState::new(Document::empty().expect("empty document must be valid"))
            .expect("editor state must be valid");

        let now = std::time::Instant::now();

        state
            .insert_text_at("H", now)
            .expect("first input must succeed");

        state
            .insert_text_at("i", now + std::time::Duration::from_millis(100))
            .expect("second input must succeed");

        assert_eq!(state.document().text(), "Hi");

        state.undo().expect("undo must succeed");

        assert_eq!(state.document().text(), "");
        assert!(!state.can_undo());
        assert!(state.can_redo());

        state.redo().expect("redo must succeed");

        assert_eq!(state.document().text(), "Hi");
        assert_eq!(
            state.selection(),
            TextSelection::cursor(Position::from_utf16_offset(3))
        );
    }

    #[test]
    fn input_grouping_respects_timeout_boundary() {
        for (delay_ms, expected_after_undo, expected_can_undo) in
            [(500, "", false), (501, "H", true)]
        {
            let document = Document::empty().expect("document must be valid");
            let mut state = EditorState::new(document).expect("state must be valid");
            let now = Instant::now();

            state
                .insert_text_at("H", now)
                .expect("first input must succeed");

            state
                .insert_text_at("i", now + Duration::from_millis(delay_ms))
                .expect("second input must succeed");

            state.undo().expect("undo must succeed");

            assert_eq!(
                state.document().text(),
                expected_after_undo,
                "unexpected grouping for delay {delay_ms} ms"
            );
            assert_eq!(
                state.can_undo(),
                expected_can_undo,
                "unexpected undo availability for delay {delay_ms} ms"
            );
        }
    }

    #[test]
    fn moving_cursor_away_and_back_starts_new_input_group() {
        let document = Document::empty().expect("document must be valid");
        let mut state = EditorState::new(document).expect("state must be valid");
        let now = Instant::now();

        state
            .insert_text_at("H", now)
            .expect("first input must succeed");

        state.move_left().expect("left movement must succeed");
        state.move_right().expect("right movement must succeed");

        state
            .insert_text_at("i", now + Duration::from_millis(100))
            .expect("second input must succeed");

        state.undo().expect("first undo must succeed");

        assert_eq!(state.document().text(), "H");
        assert_eq!(
            state.selection(),
            TextSelection::cursor(Position::from_utf16_offset(2))
        );
        assert!(state.can_undo());

        state.undo().expect("second undo must succeed");

        assert_eq!(state.document().text(), "");
        assert!(!state.can_undo());
    }

    #[test]
    fn changing_format_starts_new_input_group() {
        let document = Document::empty().expect("document must be valid");
        let mut state = EditorState::new(document).expect("state must be valid");
        let now = Instant::now();

        state
            .insert_text_at("H", now)
            .expect("first input must succeed");

        state
            .toggle_mark(Mark::Strong)
            .expect("format change must succeed");

        state
            .insert_text_at("i", now + Duration::from_millis(100))
            .expect("second input must succeed");

        state.undo().expect("first undo must succeed");

        assert_eq!(state.document().text(), "H");
        assert_eq!(state.stored_marks(), Some([Mark::Strong].as_slice()));
        assert!(state.can_undo());

        state.undo().expect("second undo must succeed");

        assert_eq!(state.document().text(), "");
        assert!(state.stored_marks().is_none());
        assert!(!state.can_undo());

        state.redo().expect("first redo must succeed");
        state.redo().expect("second redo must succeed");

        assert_eq!(state.document().text(), "Hi");
        assert_eq!(state.stored_marks(), Some([Mark::Strong].as_slice()));

        let actual: serde_json::Value = serde_json::from_str(
            &state
                .document()
                .to_json()
                .expect("serialization must succeed"),
        )
        .expect("serialized document must be valid JSON");

        assert_eq!(
            actual,
            serde_json::json!({
                "type": "doc",
                "content": [{
                    "type": "paragraph",
                    "content": [
                        {
                            "type": "text",
                            "text": "H"
                        },
                        {
                            "type": "text",
                            "text": "i",
                            "marks": [{ "type": "strong" }]
                        }
                    ]
                }]
            })
        );
    }

    #[test]
    fn selected_text_preserves_paragraphs_and_hard_breaks() {
        let document = Document::from_json(
            r#"{
               "type": "doc",
               "content": [
                   {
                       "type": "paragraph",
                       "content": [
                           {
                               "type": "text",
                               "text": "A😀"
                           },
                           {
                               "type": "hard_break"
                           },
                           {
                               "type": "text",
                               "text": "B"
                           }
                       ]
                   },
                   {
                       "type": "paragraph"
                   },
                   {
                       "type": "paragraph",
                       "content": [{
                           "type": "text",
                           "text": "CD"
                       }]
                   }
               ]
           }"#,
        )
        .expect("document must be valid");

        let mut state = EditorState::new(document).expect("state must be valid");

        for (anchor, head) in [(2, 11), (11, 2)] {
            state
                .set_selection(TextSelection::new(
                    Position::from_utf16_offset(anchor),
                    Position::from_utf16_offset(head),
                ))
                .expect("selection must be valid");

            let before = state.selection();

            assert_eq!(
                state.selected_text().expect("text extraction must succeed"),
                "😀\nB\n\nC"
            );
            assert_eq!(state.selection(), before);
            assert!(!state.can_undo());
        }

        state
            .set_selection(TextSelection::cursor(Position::from_utf16_offset(1)))
            .expect("cursor must be valid");

        assert_eq!(
            state.selected_text().expect("text extraction must succeed"),
            ""
        );
    }

    #[test]
    fn delete_selection_action_can_be_undone_and_redone() {
        let document = Document::from_json(
            r#"{
               "type": "doc",
               "content": [{
                   "type": "paragraph",
                   "content": [{
                       "type": "text",
                       "text": "A😀B"
                   }]
               }]
           }"#,
        )
        .expect("document must be valid");

        let mut state = EditorState::new(document).expect("state must be valid");

        let selection = TextSelection::new(
            Position::from_utf16_offset(4),
            Position::from_utf16_offset(2),
        );

        state
            .set_selection(selection)
            .expect("selection must be valid");

        assert_eq!(
            state.selected_text().expect("text extraction must succeed"),
            "😀"
        );

        state
            .apply(crate::editor::EditorAction::DeleteSelection)
            .expect("deletion must succeed");

        assert_eq!(state.document().text(), "AB");
        assert_eq!(
            state.selection(),
            TextSelection::cursor(Position::from_utf16_offset(2))
        );

        state.undo().expect("undo must succeed");

        assert_eq!(state.document().text(), "A😀B");
        assert_eq!(state.selection(), selection);
        assert!(!state.can_undo());

        state.redo().expect("redo must succeed");

        assert_eq!(state.document().text(), "AB");
        assert_eq!(
            state.selection(),
            TextSelection::cursor(Position::from_utf16_offset(2))
        );
    }

    #[test]
    fn paste_text_normalizes_separators_and_preserves_empty_paragraphs() {
        let cases = [
            ("A\r\nB", "A\nB", 2),
            ("A\rB", "A\nB", 2),
            ("A\n\rB", "A\n\nB", 3),
            ("A\n\n😀\n", "A\n\n😀\n", 4),
        ];

        for (input, expected, paragraph_count) in cases {
            let document = Document::empty().expect("document must be valid");
            let mut state = EditorState::new(document).expect("state must be valid");

            state
                .apply(crate::editor::EditorAction::PasteText(input.to_owned()))
                .expect("paste must succeed");

            let after = state.document().to_json().expect("document must serialize");

            let value: serde_json::Value =
                serde_json::from_str(&after).expect("JSON must be valid");

            assert_eq!(
                value["content"]
                    .as_array()
                    .expect("document must contain blocks")
                    .len(),
                paragraph_count
            );

            let expected_cursor = 1
                + expected
                    .split('\n')
                    .map(|part| part.encode_utf16().count())
                    .sum::<usize>()
                + 2 * (paragraph_count - 1);

            assert_eq!(
                state.selection(),
                TextSelection::cursor(Position::from_utf16_offset(expected_cursor))
            );

            state.select_all().expect("select all must succeed");

            assert_eq!(
                state.selected_text().expect("text extraction must succeed"),
                expected
            );

            state.undo().expect("undo must succeed");

            assert_eq!(state.document().text(), "");
            assert!(!state.can_undo());

            state.redo().expect("redo must succeed");

            assert_eq!(
                state.document().to_json().expect("document must serialize"),
                after
            );
        }
    }

    #[test]
    fn paste_text_replaces_backward_selection_in_one_history_event() {
        let document = Document::from_json(
            r#"{
               "type": "doc",
               "content": [{
                   "type": "paragraph",
                   "content": [{
                       "type": "text",
                       "text": "ABC"
                   }]
               }]
           }"#,
        )
        .expect("document must be valid");

        let mut state = EditorState::new(document).expect("state must be valid");

        let selection = TextSelection::new(
            Position::from_utf16_offset(3),
            Position::from_utf16_offset(2),
        );

        state
            .set_selection(selection)
            .expect("selection must be valid");

        let before = state.document().to_json().expect("document must serialize");

        state.paste_text("X\nY").expect("paste must succeed");

        assert_eq!(
            state.selection(),
            TextSelection::cursor(Position::from_utf16_offset(6))
        );

        let after = state.document().to_json().expect("document must serialize");

        state.select_all().expect("select all must succeed");

        assert_eq!(
            state.selected_text().expect("text extraction must succeed"),
            "AX\nYC"
        );

        state.undo().expect("undo must succeed");

        assert_eq!(
            state.document().to_json().expect("document must serialize"),
            before
        );
        assert_eq!(state.selection(), selection);
        assert!(!state.can_undo());

        state.redo().expect("redo must succeed");

        assert_eq!(
            state.document().to_json().expect("document must serialize"),
            after
        );
    }

    #[test]
    fn paste_text_is_separate_from_surrounding_input_history() {
        let document = Document::empty().expect("document must be valid");
        let mut state = EditorState::new(document).expect("state must be valid");

        state.insert_text("A").expect("input must succeed");
        state.paste_text("B").expect("paste must succeed");
        state.insert_text("C").expect("input must succeed");

        assert_eq!(state.document().text(), "ABC");

        state.undo().expect("undo must succeed");
        assert_eq!(state.document().text(), "AB");

        state.undo().expect("undo must succeed");
        assert_eq!(state.document().text(), "A");

        state.undo().expect("undo must succeed");
        assert_eq!(state.document().text(), "");
        assert!(!state.can_undo());
    }

    #[test]
    fn pasting_empty_text_preserves_selection_and_document() {
        let document = Document::from_json(
            r#"{
               "type": "doc",
               "content": [{
                   "type": "paragraph",
                   "content": [{
                       "type": "text",
                       "text": "AB"
                   }]
               }]
           }"#,
        )
        .expect("document must be valid");

        let mut state = EditorState::new(document).expect("state must be valid");

        state.select_all().expect("select all must succeed");
        let selection = state.selection();

        state.paste_text("").expect("empty paste must succeed");

        assert_eq!(state.document().text(), "AB");
        assert_eq!(state.selection(), selection);
        assert!(!state.can_undo());
    }

    #[test]
    fn word_navigation_skips_whitespace_and_preserves_selection_anchor() {
        let document = Document::from_json(
            r#"{
               "type": "doc",
               "content": [{
                   "type": "paragraph",
                   "content": [{
                       "type": "text",
                       "text": "one  two"
                   }]
               }]
           }"#,
        )
        .expect("document must be valid");

        let mut state = EditorState::new(document).expect("state must be valid");

        state
            .apply(crate::editor::EditorAction::MoveWordRight)
            .expect("navigation must succeed");

        assert_eq!(
            state.selection(),
            TextSelection::cursor(Position::from_utf16_offset(4))
        );

        state
            .apply(crate::editor::EditorAction::ExtendWordRight)
            .expect("extension must succeed");

        assert_eq!(
            state.selection(),
            TextSelection::new(
                Position::from_utf16_offset(4),
                Position::from_utf16_offset(9),
            )
        );

        state
            .apply(crate::editor::EditorAction::ExtendWordLeft)
            .expect("extension must succeed");

        assert_eq!(
            state.selection(),
            TextSelection::new(
                Position::from_utf16_offset(4),
                Position::from_utf16_offset(6),
            )
        );

        state.extend_word_left().expect("extension must succeed");

        assert_eq!(
            state.selection(),
            TextSelection::new(
                Position::from_utf16_offset(4),
                Position::from_utf16_offset(1),
            )
        );

        state.move_word_right().expect("navigation must succeed");

        assert_eq!(
            state.selection(),
            TextSelection::cursor(Position::from_utf16_offset(4))
        );
        assert!(!state.can_undo());
    }

    #[test]
    fn word_navigation_crosses_paragraph_boundaries_without_skipping_empty_blocks() {
        let document = Document::from_json(
            r#"{
               "type": "doc",
               "content": [
                   {
                       "type": "paragraph",
                       "content": [{
                           "type": "text",
                           "text": "A"
                       }]
                   },
                   {
                       "type": "paragraph"
                   },
                   {
                       "type": "paragraph",
                       "content": [{
                           "type": "text",
                           "text": "B"
                       }]
                   }
               ]
           }"#,
        )
        .expect("document must be valid");

        let mut state = EditorState::new(document).expect("state must be valid");

        for offset in [2, 4, 6, 7] {
            state.move_word_right().expect("navigation must succeed");

            assert_eq!(
                state.selection(),
                TextSelection::cursor(Position::from_utf16_offset(offset))
            );
        }

        // Moving beyond the document is a successful no-op.
        state
            .move_word_right()
            .expect("boundary navigation must succeed");

        assert_eq!(
            state.selection(),
            TextSelection::cursor(Position::from_utf16_offset(7))
        );

        for offset in [6, 4, 2, 1] {
            state.move_word_left().expect("navigation must succeed");

            assert_eq!(
                state.selection(),
                TextSelection::cursor(Position::from_utf16_offset(offset))
            );
        }

        assert!(!state.can_undo());
    }

    #[test]
    fn word_navigation_preserves_combining_character_boundaries() {
        let document = Document::from_json(
            r#"{
               "type": "doc",
               "content": [{
                   "type": "paragraph",
                   "content": [{
                       "type": "text",
                       "text": "e\u0301 x"
                   }]
               }]
           }"#,
        )
        .expect("document must be valid");

        let mut state = EditorState::new(document).expect("state must be valid");

        state.move_word_right().expect("navigation must succeed");

        assert_eq!(
            state.selection(),
            TextSelection::cursor(Position::from_utf16_offset(3))
        );

        state.move_word_right().expect("navigation must succeed");

        assert_eq!(
            state.selection(),
            TextSelection::cursor(Position::from_utf16_offset(5))
        );

        state.move_word_left().expect("navigation must succeed");

        assert_eq!(
            state.selection(),
            TextSelection::cursor(Position::from_utf16_offset(4))
        );

        state.move_word_left().expect("navigation must succeed");

        assert_eq!(
            state.selection(),
            TextSelection::cursor(Position::from_utf16_offset(1))
        );
    }

    #[test]
    fn document_boundary_navigation_preserves_anchor_when_extending() {
        let document = Document::from_json(
            r#"{
               "type": "doc",
               "content": [
                   {
                       "type": "paragraph",
                       "content": [{
                           "type": "text",
                           "text": "A"
                       }]
                   },
                   {
                       "type": "paragraph"
                   },
                   {
                       "type": "paragraph",
                       "content": [{
                           "type": "text",
                           "text": "😀"
                       }]
                   }
               ]
           }"#,
        )
        .expect("document must be valid");

        let mut state = EditorState::new(document).expect("state must be valid");

        state
            .set_selection(TextSelection::cursor(Position::from_utf16_offset(4)))
            .expect("selection must be valid");

        state
            .apply(crate::editor::EditorAction::ExtendDocumentEnd)
            .expect("extension must succeed");

        assert_eq!(
            state.selection(),
            TextSelection::new(
                Position::from_utf16_offset(4),
                Position::from_utf16_offset(8),
            )
        );

        state
            .apply(crate::editor::EditorAction::ExtendDocumentStart)
            .expect("extension must succeed");

        assert_eq!(
            state.selection(),
            TextSelection::new(
                Position::from_utf16_offset(4),
                Position::from_utf16_offset(1),
            )
        );

        state
            .apply(crate::editor::EditorAction::MoveDocumentEnd)
            .expect("navigation must succeed");

        assert_eq!(
            state.selection(),
            TextSelection::cursor(Position::from_utf16_offset(8))
        );

        state
            .apply(crate::editor::EditorAction::MoveDocumentStart)
            .expect("navigation must succeed");

        assert_eq!(
            state.selection(),
            TextSelection::cursor(Position::from_utf16_offset(1))
        );

        assert!(!state.can_undo());
    }

    #[test]
    fn document_boundary_navigation_keeps_empty_document_cursor_valid() {
        let document = Document::empty().expect("document must be valid");
        let mut state = EditorState::new(document).expect("state must be valid");

        let actions = [
            crate::editor::EditorAction::MoveDocumentStart,
            crate::editor::EditorAction::MoveDocumentEnd,
            crate::editor::EditorAction::ExtendDocumentStart,
            crate::editor::EditorAction::ExtendDocumentEnd,
        ];

        for action in actions {
            state.apply(action).expect("navigation must succeed");

            assert_eq!(
                state.selection(),
                TextSelection::cursor(Position::from_utf16_offset(1))
            );
        }

        assert!(!state.can_undo());
    }

    #[test]
    fn empty_composition_commit_preserves_selected_content() {
        let document = Document::from_json(
            r#"{
               "type": "doc",
               "content": [{
                   "type": "paragraph",
                   "content": [{
                       "type": "text",
                       "text": "ABC"
                   }]
               }]
           }"#,
        )
        .expect("document must be valid");

        let mut state = EditorState::new(document).expect("state must be valid");

        let selection = TextSelection::new(
            Position::from_utf16_offset(2),
            Position::from_utf16_offset(4),
        );

        state
            .set_selection(selection)
            .expect("selection must be valid");

        let before = state.document().to_json().expect("document must serialize");

        state
            .apply(crate::editor::EditorAction::CommitComposition(String::new()))
            .expect("empty commit must succeed");

        assert_eq!(
            state.document().to_json().expect("document must serialize"),
            before
        );
        assert_eq!(state.selection(), selection);
        assert!(!state.can_undo());
    }

    #[test]
    fn composition_commit_replaces_selection_as_one_history_event() {
        let document = Document::from_json(
            r#"{
               "type": "doc",
               "content": [{
                   "type": "paragraph",
                   "content": [{
                       "type": "text",
                       "text": "ABC"
                   }]
               }]
           }"#,
        )
        .expect("document must be valid");

        let mut state = EditorState::new(document).expect("state must be valid");

        let selection = TextSelection::new(
            Position::from_utf16_offset(4),
            Position::from_utf16_offset(2),
        );

        state
            .set_selection(selection)
            .expect("selection must be valid");

        state
            .apply(crate::editor::EditorAction::CommitComposition(
                "你好".to_owned(),
            ))
            .expect("commit must succeed");

        assert_eq!(state.document().text(), "A你好");
        assert_eq!(
            state.selection(),
            TextSelection::cursor(Position::from_utf16_offset(4))
        );

        state.undo().expect("undo must succeed");

        assert_eq!(state.document().text(), "ABC");
        assert_eq!(state.selection(), selection);
        assert!(!state.can_undo());

        state.redo().expect("redo must succeed");

        assert_eq!(state.document().text(), "A你好");
        assert_eq!(
            state.selection(),
            TextSelection::cursor(Position::from_utf16_offset(4))
        );
    }

    #[test]
    fn composition_commit_separates_surrounding_input_history() {
        let document = Document::empty().expect("document must be valid");
        let mut state = EditorState::new(document).expect("state must be valid");

        let now = Instant::now();

        state.insert_text_at("A", now).expect("input must succeed");

        state
            .commit_composition("你好")
            .expect("commit must succeed");

        state
            .insert_text_at("B", now + Duration::from_millis(1))
            .expect("input must succeed");

        assert_eq!(state.document().text(), "A你好B");

        state.undo().expect("undo must succeed");
        assert_eq!(state.document().text(), "A你好");

        state.undo().expect("undo must succeed");
        assert_eq!(state.document().text(), "A");

        state.undo().expect("undo must succeed");
        assert_eq!(state.document().text(), "");
        assert!(!state.can_undo());
    }

    #[test]
    fn empty_composition_commit_closes_input_group_and_preserves_marks() {
        let document = Document::empty().expect("document must be valid");
        let mut state = EditorState::new(document).expect("state must be valid");

        state
            .toggle_mark(Mark::Strong)
            .expect("format change must succeed");

        let now = Instant::now();

        state.insert_text_at("A", now).expect("input must succeed");

        let selection = state.selection();
        let marks = state.stored_marks().map(|marks| marks.to_vec());

        state
            .commit_composition("")
            .expect("empty commit must succeed");

        assert_eq!(state.document().text(), "A");
        assert_eq!(state.selection(), selection);
        assert_eq!(state.stored_marks(), marks.as_deref());

        state
            .insert_text_at("B", now + Duration::from_millis(1))
            .expect("input must succeed");

        state.undo().expect("undo must succeed");

        assert_eq!(state.document().text(), "A");

        state.undo().expect("undo must succeed");

        assert_eq!(state.document().text(), "");
        assert!(!state.can_undo());
    }
}
