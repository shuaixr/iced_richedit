use crate::document::TextSelection;

/// An editing request emitted by an editor UI.
#[derive(Debug, Clone)]
pub enum EditorAction {
    InsertText(String),
    SetSelection(TextSelection),
    SplitParagraph,
    DeleteBackward,
    DeleteForward,
    MoveLeft,
    MoveRight,
    Undo,
    Redo,
}
