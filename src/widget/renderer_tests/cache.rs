use super::super::State;
use super::support::{BLUE, current_paragraph, new_tree, run_sample_layout, selected_editor};
use crate::document::Mark;
use iced::{
    Color,
    advanced::graphics::text::cosmic_text::{Color as GlyphColor, Family, Weight as GlyphWeight},
};

const RED: Color = Color::from_rgb8(204, 51, 51);

#[test]
fn unchanged_content_and_style_reuse_the_paragraph() {
    let renderer = iced::Renderer::new(Default::default());
    let editor = selected_editor();
    let mut tree = new_tree(&editor);

    run_sample_layout(&editor, &mut tree, &renderer, BLUE);
    let previous = current_paragraph(&tree).clone();

    run_sample_layout(&editor, &mut tree, &renderer, BLUE);

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

    run_sample_layout(&editor, &mut tree, &renderer, BLUE);
    let previous = current_paragraph(&tree).clone();

    editor.toggle_mark(Mark::Strong).expect("apply strong");
    run_sample_layout(&editor, &mut tree, &renderer, BLUE);

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

    run_sample_layout(&editor, &mut tree, &renderer, BLUE);
    let previous = current_paragraph(&tree).clone();

    assert!(
        tree.state
            .downcast_ref::<State>()
            .code_background_bounds()
            .is_empty()
    );

    editor.toggle_mark(Mark::Code).expect("apply code");
    run_sample_layout(&editor, &mut tree, &renderer, BLUE);

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
    run_sample_layout(&editor, &mut tree, &renderer, BLUE);

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

    run_sample_layout(&editor, &mut tree, &renderer, BLUE);
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

    run_sample_layout(&editor, &mut tree, &renderer, BLUE);

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
    run_sample_layout(&editor, &mut tree, &renderer, BLUE);

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

    run_sample_layout(&editor, &mut tree, &renderer, BLUE);
    let previous = current_paragraph(&tree).clone();

    assert_eq!(
        previous.buffer().lines[0]
            .attrs_list()
            .get_span(0)
            .color_opt,
        Some(GlyphColor::rgb(0, 102, 204)),
    );

    run_sample_layout(&editor, &mut tree, &renderer, RED);

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
