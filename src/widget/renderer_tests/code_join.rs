use super::super::{LINE_HEIGHT, PADDING, PARAGRAPH_SPACING, RichEdit, State};
use super::support::new_tree;
use crate::{
    document::{Document, Position, TextSelection},
    editor::EditorState,
};
use iced::{
    Length, Size,
    advanced::{Widget, layout},
};

#[test]
fn code_boundary_join_removes_the_block_gap_and_history_restores_backgrounds() {
    let document = Document::from_json(
        &serde_json::json!({
            "type": "doc",
            "content": [
                {"type": "code_block", "content": [{"type": "text", "text": "A\n"}]},
                {"type": "code_block", "content": [{"type": "text", "text": "B\n"}]}
            ]
        })
        .to_string(),
    )
    .expect("valid document");
    let mut editor = EditorState::new(document).expect("valid editor");
    editor
        .set_selection(TextSelection::cursor(Position::from_utf16_offset(5)))
        .expect("second code start");
    let renderer = iced::Renderer::new(Default::default());
    let mut tree = new_tree(&editor);

    for stage in 0..4 {
        match stage {
            1 => editor.delete_backward().expect("join"),
            2 => editor.undo().expect("undo join"),
            3 => editor.redo().expect("redo join"),
            _ => {}
        }
        let mut widget = RichEdit::<()>::new(&editor)
            .width(Length::Fixed(400.0))
            .height(Length::Fixed(200.0));
        <RichEdit<'_, ()> as Widget<(), iced::Theme, iced::Renderer>>::layout(
            &mut widget,
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, Size::new(400.0, 200.0)),
        );
        let state = tree.state.downcast_ref::<State>();
        assert!(!state.has_error);
        let backgrounds = state.code_block_background_bounds(400.0 - 2.0 * PADDING);
        if stage == 1 || stage == 3 {
            assert_eq!(state.paragraphs.len(), 1);
            assert_eq!(backgrounds.len(), 1);
            assert_eq!(backgrounds[0].height, 3.0 * LINE_HEIGHT);
            assert_eq!(
                state.paragraphs[0]
                    .view
                    .as_ref()
                    .expect("merged view")
                    .text(),
                "A\nB\n"
            );
            let caret = state
                .caret_bounds(editor.selection().head())
                .expect("join seam caret");
            assert_eq!(editor.selection().head(), Position::from_utf16_offset(3));
            assert_eq!(caret.y, PADDING + LINE_HEIGHT);
        } else {
            assert_eq!(state.paragraphs.len(), 2);
            assert_eq!(backgrounds.len(), 2);
            assert_eq!(backgrounds[0].height, 2.0 * LINE_HEIGHT);
            assert_eq!(backgrounds[1].height, 2.0 * LINE_HEIGHT);
            assert_eq!(
                backgrounds[1].y,
                backgrounds[0].y + backgrounds[0].height + PARAGRAPH_SPACING
            );
        }
    }
}
