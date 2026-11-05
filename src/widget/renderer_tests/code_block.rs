use super::super::{LINE_HEIGHT, PADDING, RichEdit, State, TEXT_SIZE};
use super::support::{
    BLUE, SAMPLE, current_paragraph, new_tree, run_sample_layout, selected_editor,
};
use crate::{
    document::{Document, Position, TextSelection},
    editor::EditorState,
};
use iced::{
    Rectangle, Size,
    advanced::{Widget, graphics::text::cosmic_text::Family, layout},
};

#[test]
fn code_block_changes_rebuild_layout_and_use_full_width_background() {
    let renderer = iced::Renderer::new(Default::default());
    let ordinary = selected_editor();

    let code = EditorState::new(
        Document::from_json(
            &serde_json::json!({
                "type": "doc",
                "content": [
                    {
                        "type": "code_block",
                        "content": [
                            {
                                "type": "text",
                                "text": SAMPLE
                            }
                        ]
                    }
                ]
            })
            .to_string(),
        )
        .expect("valid code document"),
    )
    .expect("valid editor");

    let width = 400.0 - PADDING * 2.0;
    let mut tree = new_tree(&ordinary);

    run_sample_layout(&ordinary, &mut tree, &renderer, BLUE);

    let previous = current_paragraph(&tree).clone();

    assert!(
        tree.state
            .downcast_ref::<State>()
            .code_block_background_bounds(width)
            .is_empty()
    );

    run_sample_layout(&code, &mut tree, &renderer, BLUE);

    let paragraph = current_paragraph(&tree).clone();

    assert!(!std::ptr::eq(previous.buffer(), paragraph.buffer()));
    assert_eq!(
        paragraph.buffer().lines[0].attrs_list().get_span(0).family,
        Family::Monospace
    );
    assert_eq!(paragraph.buffer().metrics().font_size, TEXT_SIZE);

    let state = tree.state.downcast_ref::<State>();

    assert_eq!(
        state.code_block_background_bounds(width),
        vec![Rectangle {
            x: state.paragraphs[0].origin.x,
            y: state.paragraphs[0].origin.y,
            width,
            height: state.paragraphs[0].height,
        }]
    );
    assert!(state.code_background_bounds().is_empty());

    run_sample_layout(&code, &mut tree, &renderer, BLUE);

    assert!(std::ptr::eq(
        paragraph.buffer(),
        current_paragraph(&tree).buffer()
    ));

    run_sample_layout(&ordinary, &mut tree, &renderer, BLUE);

    assert!(!std::ptr::eq(
        paragraph.buffer(),
        current_paragraph(&tree).buffer()
    ));
    assert!(
        tree.state
            .downcast_ref::<State>()
            .code_block_background_bounds(width)
            .is_empty()
    );
}
#[test]
fn code_block_empty_trailing_and_wrapped_lines_keep_background_and_caret_geometry() {
    let renderer = iced::Renderer::new(Default::default());

    for text in [String::new(), "A\n\nB\n".to_owned(), "W".repeat(200)] {
        let content = if text.is_empty() {
            serde_json::json!([])
        } else {
            serde_json::json!([
                {
                    "type": "text",
                    "text": text
                }
            ])
        };

        let mut editor = EditorState::new(
            Document::from_json(
                &serde_json::json!({
                    "type": "doc",
                    "content": [
                        {
                            "type": "code_block",
                            "content": content
                        }
                    ]
                })
                .to_string(),
            )
            .expect("valid code document"),
        )
        .expect("valid editor");

        let end = Position::from_utf16_offset(1 + text.encode_utf16().count());

        editor
            .set_selection(TextSelection::cursor(end))
            .expect("valid end cursor");

        let mut tree = new_tree(&editor);
        let mut widget = RichEdit::<()>::new(&editor);

        <RichEdit<'_, ()> as Widget<(), iced::Theme, iced::Renderer>>::layout(
            &mut widget,
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, Size::new(400.0, 200.0)),
        );

        let state = tree.state.downcast_ref::<State>();

        assert!(!state.has_error);

        let backgrounds = state.code_block_background_bounds(400.0 - PADDING * 2.0);

        assert_eq!(backgrounds.len(), 1);

        let background = backgrounds[0];

        assert!(background.height >= text.split('\n').count() as f32 * LINE_HEIGHT);

        let caret = state
            .caret_bounds(end)
            .unwrap_or_else(|| panic!("missing end caret for {text:?}"));

        assert_eq!(caret.height, LINE_HEIGHT);
        assert!(caret.y >= background.y);
        assert!(caret.y + caret.height <= background.y + background.height + 0.01);

        if text.len() == 200 {
            assert!(background.height > LINE_HEIGHT);
        }
    }
}
