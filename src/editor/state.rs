use crate::{
    document::{Document, Mark, TextSelection, TransactionResult},
    editor::error::EditorError,
};

pub struct EditorState {
    document: Document,
    selection: TextSelection,
    stored_marks: Option<Vec<Mark>>,
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
    fn apply_transaction(
        &mut self,
        result: TransactionResult,
        selection: TextSelection,
    ) -> Result<(), EditorError> {
        let document = result.into_document();

        document
            .validate_selection(selection)
            .map_err(EditorError::Document)?;

        self.document = document;
        self.selection = selection;
        self.stored_marks = None;

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

        self.apply_transaction(result, selection)
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
    fn apply_transaction_preserves_state_when_new_selection_is_invalid() {
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
            editor_state.apply_transaction(result, new_selection),
            Err(EditorError::Document(DocumentError::InvalidPosition {
                position: 0
            }))
        ));

        assert_eq!(editor_state.document().text(), "Hello");
        assert_eq!(editor_state.selection(), selection);
        assert_eq!(editor_state.stored_marks(), Some(&[Mark::Strong][..]));
    }
}
