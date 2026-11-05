use super::super::{LINE_HEIGHT, PADDING, PARAGRAPH_SPACING, RichEdit, State};
use super::support::{current_paragraph, new_tree};
use crate::{
    document::{Document, Position, TextSelection},
    editor::EditorState,
};
use iced::{
    Length, Point, Size,
    advanced::{
        Widget, graphics::text::cosmic_text::Weight as GlyphWeight, input_method, layout,
        text::Paragraph as _, widget::Tree,
    },
};
use serde_json::json;

fn trailing_editor(
    kind: &str,
    level: u8,
    text: &str,
    endings: usize,
    following: bool,
) -> EditorState {
    let mut content = Vec::new();
    if !text.is_empty() {
        content.push(json!({"type":"text", "text":text,
            "marks":[{"type":"strong"}, {"type":"em"}]}));
    }
    content.extend((0..endings).map(|_| json!({"type":"hard_break"})));
    let mut node = json!({"type":kind, "content":content});
    if kind == "heading" {
        node["attrs"] = json!({"level":level});
    }
    let mut nodes = vec![node];
    if following {
        nodes.push(json!({"type":"paragraph", "content":[{"type":"text", "text":"B"}]}));
    }
    let document = Document::from_json(&json!({"type":"doc", "content":nodes}).to_string())
        .expect("valid document");
    let mut editor = EditorState::new(document).expect("valid editor");
    editor
        .set_selection(TextSelection::cursor(Position::from_utf16_offset(
            1 + text.encode_utf16().count() + endings,
        )))
        .expect("valid end cursor");
    editor
}

fn layout_trailing(
    editor: &EditorState,
    tree: &mut Tree,
    renderer: &iced::Renderer,
    width: f32,
    height: f32,
) {
    let mut widget = RichEdit::<()>::new(editor)
        .width(Length::Fixed(width))
        .height(Length::Fixed(height));
    <RichEdit<'_, ()> as Widget<(), iced::Theme, iced::Renderer>>::layout(
        &mut widget,
        tree,
        renderer,
        &layout::Limits::new(Size::ZERO, Size::new(width, height)),
    );
    assert!(!tree.state.downcast_ref::<State>().has_error);
}

#[test]
fn trailing_hard_break_rows_support_carets_hit_testing_and_navigation() {
    let renderer = iced::Renderer::new(Default::default());
    for (kind, level, line_height) in [
        ("paragraph", 0, 24.0),
        ("heading", 1, 48.0),
        ("heading", 2, 42.0),
        ("heading", 6, 24.0),
    ] {
        for value in ["", "A"] {
            for endings in 1..=3 {
                let editor = trailing_editor(kind, level, value, endings, false);
                let original = editor.document().to_json().expect("serialize");
                let end = editor.selection().head();
                let mut tree = new_tree(&editor);
                layout_trailing(&editor, &mut tree, &renderer, 400.0, 500.0);
                let cached = current_paragraph(&tree).clone();
                layout_trailing(&editor, &mut tree, &renderer, 400.0, 500.0);
                assert!(std::ptr::eq(
                    cached.buffer(),
                    current_paragraph(&tree).buffer()
                ));
                let state = tree.state.downcast_mut::<State>();
                let item = &state.paragraphs[0];
                let caret = state.caret_bounds(end).expect("final empty row caret");
                assert_eq!(caret.y, item.origin.y + endings as f32 * line_height);
                assert_eq!(caret.height, line_height);
                assert!(item.height >= (endings + 1) as f32 * line_height);
                if !value.is_empty() {
                    assert_eq!(
                        item.paragraph.buffer().lines[0]
                            .attrs_list()
                            .get_span(0)
                            .weight,
                        GlyphWeight::BOLD
                    );
                }
                let previous = Position::from_utf16_offset(end.utf16_offset() - 1);
                let previous_caret = state.caret_bounds(previous).expect("preceding row caret");
                assert_eq!(
                    state
                        .vertical_target(previous, true, caret.x)
                        .expect("down")
                        .0,
                    end
                );
                let up = state.vertical_target(end, false, caret.x).expect("up").0;
                assert_eq!(
                    state.caret_bounds(up).expect("up caret").y,
                    previous_caret.y
                );
                for boundary_end in [false, true] {
                    assert_eq!(
                        state
                            .visual_line_boundary(end, boundary_end)
                            .expect("empty row boundary")
                            .0,
                        end
                    );
                }
                assert_eq!(
                    state.hit_test(Point::new(caret.x + 80.0, caret.y + caret.height / 2.0)),
                    Some(end)
                );
                assert_eq!(editor.document().to_json().expect("serialize"), original);
            }
        }
    }
}

#[test]
fn wrapped_text_reserves_the_final_empty_row_after_resize_and_reveals_it() {
    let renderer = iced::Renderer::new(Default::default());
    for (kind, level, line_height) in [("paragraph", 0, 24.0), ("heading", 2, 42.0)] {
        let editor = trailing_editor(kind, level, &"MMMM iii ".repeat(12), 2, true);
        let end = editor.selection().head();
        let mut tree = new_tree(&editor);
        let mut previous_height = f32::INFINITY;
        for width in [100.0, 220.0] {
            layout_trailing(&editor, &mut tree, &renderer, width, 120.0);
            let state = tree.state.downcast_mut::<State>();
            let first = &state.paragraphs[0];
            let caret = state.caret_bounds(end).expect("wrapped final row");
            assert!(first.paragraph.buffer().layout_runs().count() > 3);
            assert!(first.height >= first.paragraph.min_height() + line_height);
            assert!(first.height < previous_height);
            previous_height = first.height;
            assert!(caret.y + caret.height <= first.origin.y + first.height);
            assert_eq!(
                state.paragraphs[1].origin.y,
                first.origin.y + first.height + PARAGRAPH_SPACING
            );
            assert_eq!(
                state.hit_test(Point::new(caret.x + 20.0, caret.y + caret.height / 2.0)),
                Some(end)
            );
            state.scroll_y = 0.0;
            state.reveal_head(end, 120.0, PADDING);
            assert!(state.scroll_y > 0.0);
            assert!(caret.y - state.scroll_y >= PADDING);
            assert!(caret.y + caret.height - state.scroll_y <= 120.0 - PADDING);
        }
    }
}

#[test]
fn selection_marks_the_empty_paragraph_ending_but_not_an_unselected_tail() {
    let renderer = iced::Renderer::new(Default::default());
    let editor = trailing_editor("paragraph", 0, "A", 1, true);
    let mut tree = new_tree(&editor);
    layout_trailing(&editor, &mut tree, &renderer, 400.0, 200.0);
    let state = tree.state.downcast_ref::<State>();
    let end = editor.selection().head();
    let caret = state.caret_bounds(end).expect("empty row");
    let next_start = state.paragraphs[1]
        .view
        .as_ref()
        .expect("next view")
        .start();
    let across = state.selection_bounds(TextSelection::new(
        Position::from_utf16_offset(1),
        next_start,
    ));
    assert!(
        across.iter().any(|bounds| bounds.y == caret.y
            && bounds.width == 3.0
            && bounds.height == caret.height)
    );
    let newline_only =
        state.selection_bounds(TextSelection::new(Position::from_utf16_offset(2), end));
    assert!(
        newline_only
            .iter()
            .any(|bounds| bounds.width == 3.0 && bounds.y < caret.y)
    );
    assert!(newline_only.iter().all(|bounds| bounds.y != caret.y));
}

#[test]
fn complete_plain_layouts_and_empty_blocks_do_not_gain_an_extra_row() {
    let renderer = iced::Renderer::new(Default::default());
    for (node, cursor, expected_height) in [
        (json!({"type":"paragraph"}), 1, 24.0),
        (json!({"type":"heading", "attrs":{"level":1}}), 1, 48.0),
        (json!({"type":"code_block"}), 1, 24.0),
        (
            json!({"type":"code_block", "content":[{"type":"text", "text":"A\n\n"}]}),
            4,
            72.0,
        ),
    ] {
        let document = Document::from_json(&json!({"type":"doc", "content":[node]}).to_string())
            .expect("valid document");
        let mut editor = EditorState::new(document).expect("valid editor");
        editor
            .set_selection(TextSelection::cursor(Position::from_utf16_offset(cursor)))
            .expect("valid cursor");
        let mut tree = new_tree(&editor);
        layout_trailing(&editor, &mut tree, &renderer, 400.0, 200.0);
        let state = tree.state.downcast_ref::<State>();
        assert_eq!(state.paragraphs[0].height, expected_height);
        assert!(state.caret_bounds(editor.selection().head()).is_some());
    }
}

#[test]
fn trailing_preedit_newline_has_a_caret_and_closing_preedit_removes_the_row() {
    let renderer = iced::Renderer::new(Default::default());
    let editor = trailing_editor("paragraph", 0, "A", 0, false);
    let original = editor.document().to_json().expect("serialize");
    let mut tree = new_tree(&editor);
    {
        let state = tree.state.downcast_mut::<State>();
        state.ime.handle(&input_method::Event::Opened, &editor);
        state.ime.handle(
            &input_method::Event::Preedit("中\n".to_owned(), Some(4..4)),
            &editor,
        );
    }
    layout_trailing(&editor, &mut tree, &renderer, 400.0, 200.0);
    {
        let state = tree.state.downcast_mut::<State>();
        let first = &state.paragraphs[0];
        assert_eq!(first.view.as_ref().expect("composed view").text(), "A中\n");
        let caret = state
            .caret_bounds(editor.selection().head())
            .expect("preedit tail caret");
        assert_eq!(caret.y, first.origin.y + LINE_HEIGHT);
        assert_eq!(first.height, LINE_HEIGHT * 2.0);
        state.ime.close();
    }
    layout_trailing(&editor, &mut tree, &renderer, 400.0, 200.0);
    let state = tree.state.downcast_ref::<State>();
    let first = &state.paragraphs[0];
    assert_eq!(first.view.as_ref().expect("committed view").text(), "A");
    assert_eq!(first.height, LINE_HEIGHT);
    assert_eq!(
        state
            .caret_bounds(editor.selection().head())
            .expect("ordinary caret")
            .y,
        first.origin.y
    );
    assert_eq!(editor.document().to_json().expect("serialize"), original);
    assert!(!editor.can_undo());
}

#[test]
fn code_conversion_and_history_keep_tail_geometry_in_sync() {
    let renderer = iced::Renderer::new(Default::default());
    for heading in [false, true] {
        let document = Document::from_json(&json!({
            "type":"doc", "content":[{"type":"code_block", "content":[{"type":"text", "text":"A\n\n"}]}]
        }).to_string()).expect("valid code");
        let mut editor = EditorState::new(document).expect("valid editor");
        let end = Position::from_utf16_offset(4);
        editor
            .set_selection(TextSelection::cursor(end))
            .expect("end cursor");
        let original = editor.document().to_json().expect("serialize");
        let mut tree = new_tree(&editor);
        layout_trailing(&editor, &mut tree, &renderer, 400.0, 200.0);
        if heading {
            editor.set_heading(2).expect("convert to heading");
        } else {
            editor.set_paragraph().expect("exit code");
        }
        let line_height = if heading { 42.0 } else { LINE_HEIGHT };
        layout_trailing(&editor, &mut tree, &renderer, 400.0, 200.0);
        {
            let state = tree.state.downcast_ref::<State>();
            let caret = state.caret_bounds(end).expect("converted tail");
            assert_eq!(state.paragraphs[0].height, 3.0 * line_height);
            assert_eq!(caret.y, PADDING + 2.0 * line_height);
        }
        editor.undo().expect("undo conversion");
        assert_eq!(editor.document().to_json().expect("serialize"), original);
        layout_trailing(&editor, &mut tree, &renderer, 400.0, 200.0);
        assert_eq!(
            tree.state.downcast_ref::<State>().paragraphs[0].height,
            3.0 * LINE_HEIGHT
        );
        editor.redo().expect("redo conversion");
        layout_trailing(&editor, &mut tree, &renderer, 400.0, 200.0);
        {
            let state = tree.state.downcast_ref::<State>();
            assert_eq!(
                state.caret_bounds(end).expect("redone tail").height,
                line_height
            );
            assert_eq!(state.paragraphs[0].height, 3.0 * line_height);
        }
        editor.set_code_block().expect("convert back to code");
        layout_trailing(&editor, &mut tree, &renderer, 400.0, 200.0);
        assert_eq!(editor.document().to_json().expect("serialize"), original);
        assert_eq!(
            tree.state.downcast_ref::<State>().paragraphs[0].height,
            3.0 * LINE_HEIGHT
        );
    }
}
