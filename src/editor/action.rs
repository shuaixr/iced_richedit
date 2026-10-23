use crate::document::TextSelection;

/// An editing request emitted by an editor UI.
#[derive(Debug, Clone)]
pub enum EditorAction {
    InsertText(String),
    PasteText(String),
    SetSelection(TextSelection),
    SplitParagraph,
    DeleteBackward,
    DeleteForward,
    DeleteSelection,
    DeleteWordBackward,
    DeleteWordForward,
    MoveLeft,
    MoveRight,
    MoveWordLeft,
    MoveWordRight,
    ExtendWordLeft,
    ExtendWordRight,
    Undo,
    Redo,
    ExtendLeft,
    ExtendRight,
    SelectAll,
}
