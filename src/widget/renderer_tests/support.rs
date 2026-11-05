use super::super::{RichEdit, State};
use crate::{
    document::{Document, Position, TextSelection},
    editor::EditorState,
};
use iced::{
    Color, Length, Size,
    advanced::{Widget, graphics::text::Paragraph, layout, widget::Tree},
};

pub(super) const SAMPLE: &str = "MMMM iii";
pub(super) const BLUE: Color = Color::from_rgb8(0, 102, 204);

pub(super) fn selected_editor() -> EditorState {
    let document = Document::from_json(
        &serde_json::json!({
            "type": "doc",
            "content": [{
                "type": "paragraph",
                "content": [{
                    "type": "text",
                    "text": SAMPLE
                }]
            }]
        })
        .to_string(),
    )
    .expect("valid document");

    let mut editor = EditorState::new(document).expect("valid editor");

    editor
        .set_selection(TextSelection::new(
            Position::from_utf16_offset(1),
            Position::from_utf16_offset(1 + SAMPLE.len()),
        ))
        .expect("valid selection");

    editor
}

pub(super) fn new_tree(editor: &EditorState) -> Tree {
    let widget = RichEdit::<()>::new(editor);

    Tree::new::<(), iced::Theme, iced::Renderer>(&widget)
}

pub(super) fn run_sample_layout(
    editor: &EditorState,
    tree: &mut Tree,
    renderer: &iced::Renderer,
    color: Color,
) {
    let mut widget = RichEdit::<()>::new(editor)
        .width(Length::Fixed(400.0))
        .height(Length::Fixed(200.0))
        .link_color(color);

    <RichEdit<'_, ()> as Widget<(), iced::Theme, iced::Renderer>>::layout(
        &mut widget,
        tree,
        renderer,
        &layout::Limits::new(Size::ZERO, Size::new(400.0, 200.0)),
    );

    let state = tree.state.downcast_ref::<State>();

    assert!(!state.has_error);
    assert_eq!(state.paragraphs.len(), 1);
    assert_eq!(
        state.paragraphs[0]
            .view
            .as_ref()
            .expect("paragraph view")
            .text(),
        SAMPLE,
    );
}

pub(super) fn current_paragraph(tree: &Tree) -> &Paragraph {
    &tree.state.downcast_ref::<State>().paragraphs[0].paragraph
}
