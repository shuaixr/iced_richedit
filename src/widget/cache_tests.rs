use super::*;
use crate::document::{Document, Position};
use iced::advanced::graphics::text::cosmic_text::{
    Color as GlyphColor, Family, Weight as GlyphWeight,
};

const SAMPLE: &str = "MMMM iii";
const BLUE: Color = Color::from_rgb8(0, 102, 204);
const RED: Color = Color::from_rgb8(204, 51, 51);

fn selected_editor() -> EditorState {
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

fn new_tree(editor: &EditorState) -> Tree {
    let widget = RichEdit::<()>::new(editor);

    Tree::new::<(), iced::Theme, iced::Renderer>(&widget)
}

fn run_layout(editor: &EditorState, tree: &mut Tree, renderer: &iced::Renderer, color: Color) {
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

fn current_paragraph(tree: &Tree) -> &Paragraph {
    &tree.state.downcast_ref::<State>().paragraphs[0].paragraph
}

#[test]
fn unchanged_content_and_style_reuse_the_paragraph() {
    let renderer = iced::Renderer::new(Default::default());
    let editor = selected_editor();
    let mut tree = new_tree(&editor);

    run_layout(&editor, &mut tree, &renderer, BLUE);
    let previous = current_paragraph(&tree).clone();

    run_layout(&editor, &mut tree, &renderer, BLUE);

    assert!(std::ptr::eq(
        previous.buffer(),
        current_paragraph(&tree).buffer(),
    ));
}

#[test]
fn strong_changes_rebuild_layout_without_changing_text() {
    let renderer = iced::Renderer::new(Default::default());
    let mut editor = selected_editor();
    let mut tree = new_tree(&editor);

    run_layout(&editor, &mut tree, &renderer, BLUE);
    let previous = current_paragraph(&tree).clone();

    editor.toggle_mark(Mark::Strong).expect("apply strong");
    run_layout(&editor, &mut tree, &renderer, BLUE);

    let paragraph = current_paragraph(&tree);

    assert!(!std::ptr::eq(previous.buffer(), paragraph.buffer()));
    assert_eq!(
        paragraph.buffer().lines[0].attrs_list().get_span(0).weight,
        GlyphWeight::BOLD,
    );
}

#[test]
fn code_changes_update_font_and_background_ranges() {
    let renderer = iced::Renderer::new(Default::default());
    let mut editor = selected_editor();
    let mut tree = new_tree(&editor);

    run_layout(&editor, &mut tree, &renderer, BLUE);
    let previous = current_paragraph(&tree).clone();

    assert!(
        tree.state
            .downcast_ref::<State>()
            .code_background_bounds()
            .is_empty()
    );

    editor.toggle_mark(Mark::Code).expect("apply code");
    run_layout(&editor, &mut tree, &renderer, BLUE);

    let paragraph = current_paragraph(&tree);

    assert!(!std::ptr::eq(previous.buffer(), paragraph.buffer()));
    assert_eq!(
        paragraph.buffer().lines[0].attrs_list().get_span(0).family,
        Family::Monospace,
    );
    assert!(
        !tree
            .state
            .downcast_ref::<State>()
            .code_background_bounds()
            .is_empty()
    );

    editor.toggle_mark(Mark::Code).expect("remove code");
    run_layout(&editor, &mut tree, &renderer, BLUE);

    assert!(
        tree.state
            .downcast_ref::<State>()
            .code_background_bounds()
            .is_empty()
    );
    assert_ne!(
        current_paragraph(&tree).buffer().lines[0]
            .attrs_list()
            .get_span(0)
            .family,
        Family::Monospace,
    );
}

#[test]
fn link_changes_update_color_and_underline_ranges() {
    let renderer = iced::Renderer::new(Default::default());
    let mut editor = selected_editor();
    let mut tree = new_tree(&editor);

    run_layout(&editor, &mut tree, &renderer, BLUE);
    let previous = current_paragraph(&tree).clone();

    assert!(
        tree.state
            .downcast_ref::<State>()
            .underline_bounds()
            .is_empty()
    );

    editor
        .set_link("https://iced.rs/".to_owned(), None)
        .expect("apply link");

    run_layout(&editor, &mut tree, &renderer, BLUE);

    let paragraph = current_paragraph(&tree);

    assert!(!std::ptr::eq(previous.buffer(), paragraph.buffer()));
    assert_eq!(
        paragraph.buffer().lines[0]
            .attrs_list()
            .get_span(0)
            .color_opt,
        Some(GlyphColor::rgb(0, 102, 204)),
    );

    let underlines = tree.state.downcast_ref::<State>().underline_bounds();

    assert!(!underlines.is_empty());
    assert!(underlines.iter().all(|(_, is_link)| *is_link));

    editor.remove_link().expect("remove link");
    run_layout(&editor, &mut tree, &renderer, BLUE);

    assert_eq!(
        current_paragraph(&tree).buffer().lines[0]
            .attrs_list()
            .get_span(0)
            .color_opt,
        None,
    );
    assert!(
        tree.state
            .downcast_ref::<State>()
            .underline_bounds()
            .is_empty()
    );
}

#[test]
fn link_color_changes_rebuild_layout_without_document_changes() {
    let renderer = iced::Renderer::new(Default::default());
    let mut editor = selected_editor();

    editor
        .set_link("https://iced.rs/".to_owned(), None)
        .expect("apply link");

    let mut tree = new_tree(&editor);

    run_layout(&editor, &mut tree, &renderer, BLUE);
    let previous = current_paragraph(&tree).clone();

    assert_eq!(
        previous.buffer().lines[0]
            .attrs_list()
            .get_span(0)
            .color_opt,
        Some(GlyphColor::rgb(0, 102, 204)),
    );

    run_layout(&editor, &mut tree, &renderer, RED);

    let paragraph = current_paragraph(&tree);

    assert!(!std::ptr::eq(previous.buffer(), paragraph.buffer()));
    assert_eq!(
        paragraph.buffer().lines[0]
            .attrs_list()
            .get_span(0)
            .color_opt,
        Some(GlyphColor::rgb(204, 51, 51)),
    );
    assert_eq!(
        tree.state.downcast_ref::<State>().observed_link_color,
        Some(RED),
    );
}
#[test]
fn heading_type_and_level_changes_update_cached_metrics() {
    let renderer = iced::Renderer::new(Default::default());
    let mut editor = selected_editor();
    let mut tree = new_tree(&editor);

    run_layout(&editor, &mut tree, &renderer, BLUE);

    let ordinary = current_paragraph(&tree).clone();

    editor.set_heading(1).expect("set H1");
    run_layout(&editor, &mut tree, &renderer, BLUE);

    let h1 = current_paragraph(&tree).clone();

    assert!(!std::ptr::eq(ordinary.buffer(), h1.buffer()));
    assert_eq!(h1.buffer().metrics().font_size, 32.0);
    assert_eq!(h1.buffer().metrics().line_height, 48.0);

    editor.set_heading(2).expect("set H2");
    run_layout(&editor, &mut tree, &renderer, BLUE);

    let h2 = current_paragraph(&tree);

    assert!(!std::ptr::eq(h1.buffer(), h2.buffer()));
    assert_eq!(h2.buffer().metrics().font_size, 28.0);
    assert_eq!(h2.buffer().metrics().line_height, 42.0);

    editor.set_paragraph().expect("set paragraph");
    run_layout(&editor, &mut tree, &renderer, BLUE);

    assert_eq!(
        current_paragraph(&tree).buffer().metrics().font_size,
        TEXT_SIZE
    );
    assert_eq!(
        current_paragraph(&tree).buffer().metrics().line_height,
        LINE_HEIGHT
    );
}

#[test]
fn empty_heading_uses_its_own_line_height_and_caret_geometry() {
    let renderer = iced::Renderer::new(Default::default());

    let mut editor =
        EditorState::new(Document::empty().expect("valid document")).expect("valid editor");

    editor.set_heading(1).expect("set H1");

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
    assert_eq!(state.paragraphs.len(), 1);
    assert!(state.paragraphs[0].height >= 48.0);
    assert_eq!(
        state
            .caret_bounds(editor.selection().head())
            .expect("caret")
            .height,
        48.0
    );
}
