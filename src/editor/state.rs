use crate::{
    document::{Document, Mark, Position, TextSelection, TransactionResult},
    editor::{
        error::EditorError,
        history::{History, HistoryEntry},
    },
};

pub struct EditorState {
    document: Document,
    selection: TextSelection,
    stored_marks: Option<Vec<Mark>>,
    history: History,
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

        self.selection = selection;
        self.stored_marks = None;
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
        let (document, change) = result.into_parts();

        document
            .validate_selection(selection)
            .map_err(EditorError::Document)?;
        if let Some(change) = change {
            self.history.record(HistoryEntry {
                change,
                selection_before: self.selection,
                selection_after: selection,
                stored_marks_before: self.stored_marks.clone(),
                stored_marks_after: stored_marks.clone(),
            });
        }

        self.document = document;
        self.selection = selection;
        self.stored_marks = stored_marks;

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

        self.dispatch_cursor_edit(result, selection.head(), None)
    }

    pub fn insert_text(&mut self, text: &str) -> Result<(), EditorError> {
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
        let stored_marks = self.stored_marks.clone();
        let mut transaction = self.document.transaction();

        if !self.selection.is_empty() {
            transaction
                .delete_range(from, to)
                .map_err(EditorError::Document)?;
        }

        transaction
            .insert_text_with_marks(from, text, &marks)
            .map_err(EditorError::Document)?;
        let new_offset = from.utf16_offset() + text.encode_utf16().count();
        let selection = TextSelection::cursor(Position::from_utf16_offset(new_offset));

        self.dispatch_cursor_edit(transaction.finish(), selection.head(), stored_marks)
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

        self.stored_marks = Some(marks);

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

            self.stored_marks = Some(marks);
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

            self.stored_marks = Some(marks);
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

        self.dispatch_cursor_edit(transaction.finish(), selection.head(), None)
    }

    pub fn undo(&mut self) -> Result<(), EditorError> {
        let Some(entry) = self.history.undo_entry() else {
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

        Ok(())
    }

    pub fn redo(&mut self) -> Result<(), EditorError> {
        let Some(entry) = self.history.redo_entry() else {
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
        self.delete_adjacent(false)
    }

    pub fn delete_forward(&mut self) -> Result<(), EditorError> {
        self.delete_adjacent(true)
    }

    fn delete_adjacent(&mut self, forward: bool) -> Result<(), EditorError> {
        if !self.selection.is_empty() {
            return self.delete_selection();
        }

        let current = self.selection.head();

        let adjacent = if forward {
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

        self.dispatch_cursor_edit(transaction.finish(), cursor, None)
    }

    fn dispatch_cursor_edit(
        &mut self,
        result: TransactionResult,
        cursor: Position,
        stored_marks: Option<Vec<Mark>>,
    ) -> Result<(), EditorError> {
        let cursor = result
            .document()
            .snap_cursor_forward(cursor)
            .map_err(EditorError::Document)?;

        self.dispatch_transaction(result, TextSelection::cursor(cursor), stored_marks)
    }
}

#[cfg(test)]
mod tests {
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
}
