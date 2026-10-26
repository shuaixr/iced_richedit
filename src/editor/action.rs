use crate::{document::TextSelection, editor::CompositionContext};

/// An editing request emitted by an editor UI.
#[derive(Debug, Clone)]
pub enum EditorAction {
    CommitCompositionInContext {
        context: CompositionContext,
        text: String,
    },
    InsertText(String),
    PasteText(String),
    SetSelection(TextSelection),
    CommitComposition(String),
    SplitParagraph,
    DeleteBackward,
    DeleteForward,
    DeleteSelection,
    DeleteWordBackward,
    DeleteWordForward,
    MoveDocumentStart,
    MoveDocumentEnd,
    MoveLeft,
    MoveRight,
    MoveWordLeft,
    MoveWordRight,
    ExtendWordLeft,
    ExtendWordRight,
    ExtendDocumentStart,
    ExtendDocumentEnd,
    Undo,
    Redo,
    ExtendLeft,
    ExtendRight,
    SelectAll,
}
