use super::super::{LINE_HEIGHT, RichEdit, State, TEXT_SIZE};
use super::support::{BLUE, current_paragraph, new_tree, run_sample_layout, selected_editor};
use crate::{document::Document, editor::EditorState};
use iced::{
    Size,
    advanced::{Widget, layout},
};

#[test]
fn heading_type_and_level_changes_update_cached_metrics() {
    let renderer = iced::Renderer::new(Default::default());
    let mut editor = selected_editor();
    let mut tree = new_tree(&editor);

    run_sample_layout(&editor, &mut tree, &renderer, BLUE);

    let ordinary = current_paragraph(&tree).clone();

    editor.set_heading(1).expect("set H1");
    run_sample_layout(&editor, &mut tree, &renderer, BLUE);

    let h1 = current_paragraph(&tree).clone();

    assert!(!std::ptr::eq(ordinary.buffer(), h1.buffer()));
    assert_eq!(h1.buffer().metrics().font_size, 32.0);
    assert_eq!(h1.buffer().metrics().line_height, 48.0);

    editor.set_heading(2).expect("set H2");
    run_sample_layout(&editor, &mut tree, &renderer, BLUE);

    let h2 = current_paragraph(&tree);

    assert!(!std::ptr::eq(h1.buffer(), h2.buffer()));
    assert_eq!(h2.buffer().metrics().font_size, 28.0);
    assert_eq!(h2.buffer().metrics().line_height, 42.0);

    editor.set_paragraph().expect("set paragraph");
    run_sample_layout(&editor, &mut tree, &renderer, BLUE);

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
