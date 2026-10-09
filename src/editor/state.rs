use crate::{
    document::{Document, Mark, TextSelection},
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
}
