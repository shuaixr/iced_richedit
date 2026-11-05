use iced_richedit::{
    document::{Document, Mark, Position, TextSelection},
    editor::EditorState,
};
use serde_json::{Value, json};

fn paragraph(text: &str) -> Value {
    let mut node = json!({
        "type": "paragraph"
    });

    if !text.is_empty() {
        node["content"] = json!([
            {
                "type": "text",
                "text": text
            }
        ]);
    }

    node
}

fn heading(level: u8, text: &str) -> Value {
    let mut node = paragraph(text);
    node["type"] = json!("heading");
    node["attrs"] = json!({
        "level": level
    });
    node
}

fn editor(content: Value, anchor: usize, head: usize) -> EditorState {
    let document = Document::from_json(
        &json!({
            "type": "doc",
            "content": content
        })
        .to_string(),
    )
    .expect("valid document");

    let mut editor = EditorState::new(document).expect("valid editor");

    editor
        .set_selection(TextSelection::new(
            Position::from_utf16_offset(anchor),
            Position::from_utf16_offset(head),
        ))
        .expect("valid selection");

    editor
}

fn snapshot(editor: &EditorState) -> Value {
    serde_json::from_str(&editor.document().to_json().expect("serialize document"))
        .expect("valid JSON")
}

fn assert_delete(
    mut editor: EditorState,
    forward: bool,
    by_word: bool,
    expected: Value,
    cursor: usize,
) {
    let original = snapshot(&editor);
    let selection = editor.selection();
    let marks = editor.stored_marks().map(<[Mark]>::to_vec);

    assert!(!editor.can_undo());

    match (forward, by_word) {
        (false, false) => editor.delete_backward(),
        (true, false) => editor.delete_forward(),
        (false, true) => editor.delete_word_backward(),
        (true, true) => editor.delete_word_forward(),
    }
    .expect("delete succeeds");

    let changed = snapshot(&editor);
    let cursor = TextSelection::cursor(Position::from_utf16_offset(cursor));

    assert_eq!(changed["content"], expected);
    assert_eq!(editor.selection(), cursor);
    assert_eq!(editor.stored_marks(), None);

    editor.undo().expect("undo delete");

    assert_eq!(snapshot(&editor), original);
    assert_eq!(editor.selection(), selection);
    assert_eq!(editor.stored_marks(), marks.as_deref());
    assert!(!editor.can_undo());

    editor.redo().expect("redo delete");

    assert_eq!(snapshot(&editor), changed);
    assert_eq!(editor.selection(), cursor);
    assert_eq!(editor.stored_marks(), None);
}

#[test]
fn backspace_at_first_heading_start_converts_even_without_a_previous_block() {
    for level in 1..=6 {
        for text in ["", "AB"] {
            assert_delete(
                editor(json!([heading(level, text)]), 1, 1),
                false,
                false,
                json!([paragraph(text)]),
                1,
            );
        }
    }
}

#[test]
fn backspace_at_heading_start_converts_without_joining_the_previous_block() {
    assert_delete(
        editor(json!([paragraph("L"), heading(2, "R")]), 4, 4),
        false,
        false,
        json!([paragraph("L"), paragraph("R")]),
        4,
    );
}

#[test]
fn a_second_backspace_joins_only_after_the_heading_was_converted() {
    let mut editor = editor(json!([paragraph("L"), heading(2, "R")]), 4, 4);

    let original = snapshot(&editor);

    editor.delete_backward().expect("convert heading");

    let converted = snapshot(&editor);

    assert_eq!(
        converted["content"],
        json!([paragraph("L"), paragraph("R")])
    );

    editor.delete_backward().expect("join paragraphs");

    assert_eq!(snapshot(&editor)["content"], json!([paragraph("LR")]));
    assert_eq!(
        editor.selection(),
        TextSelection::cursor(Position::from_utf16_offset(2))
    );

    editor.undo().expect("undo join");
    assert_eq!(snapshot(&editor), converted);

    editor.undo().expect("undo conversion");
    assert_eq!(snapshot(&editor), original);
    assert!(!editor.can_undo());
}

#[test]
fn delete_at_block_end_preserves_the_left_block_type_and_heading_level() {
    for level in [None, Some(2), Some(5)] {
        let left = |text| match level {
            Some(level) => heading(level, text),
            None => paragraph(text),
        };

        for right in [paragraph("CD"), heading(3, "CD")] {
            assert_delete(
                editor(json!([left("AB"), right]), 3, 3),
                true,
                false,
                json!([left("ABCD")]),
                3,
            );
        }
    }
}

#[test]
fn backspace_at_paragraph_start_preserves_the_left_block_type() {
    for left in [paragraph("AB"), heading(2, "AB"), heading(5, "AB")] {
        let mut expected = left.clone();
        expected["content"][0]["text"] = json!("ABCD");

        assert_delete(
            editor(json!([left, paragraph("CD")]), 5, 5),
            false,
            false,
            json!([expected]),
            3,
        );
    }
}

#[test]
fn delete_can_join_empty_headings_without_changing_the_left_level() {
    assert_delete(
        editor(json!([heading(2, ""), heading(3, "CD")]), 1, 1),
        true,
        false,
        json!([heading(2, "CD")]),
        1,
    );

    assert_delete(
        editor(json!([heading(2, "AB"), paragraph("")]), 3, 3),
        true,
        false,
        json!([heading(2, "AB")]),
        3,
    );

    assert_delete(
        editor(json!([heading(2, "AB"), paragraph("")]), 5, 5),
        false,
        false,
        json!([heading(2, "AB")]),
        3,
    );
}

#[test]
fn deleting_a_nonempty_selection_does_not_demote_a_heading() {
    for forward in [false, true] {
        assert_delete(
            editor(json!([heading(2, "AB")]), 2, 1),
            forward,
            false,
            json!([heading(2, "B")]),
            1,
        );
    }
}

#[test]
fn word_deletion_obeys_the_same_structural_boundary_rules() {
    assert_delete(
        editor(json!([paragraph("L"), heading(2, "R")]), 4, 4),
        false,
        true,
        json!([paragraph("L"), paragraph("R")]),
        4,
    );

    assert_delete(
        editor(json!([heading(2, "AB"), heading(3, "CD")]), 3, 3),
        true,
        true,
        json!([heading(2, "ABCD")]),
        3,
    );
}

#[test]
fn deletion_at_document_edges_does_not_add_history() {
    for (content, cursor, forward) in [
        (json!([paragraph("AB")]), 1, false),
        (json!([heading(2, "AB")]), 3, true),
    ] {
        let mut editor = editor(content, cursor, cursor);
        let original = snapshot(&editor);
        let selection = editor.selection();

        if forward {
            editor.delete_forward()
        } else {
            editor.delete_backward()
        }
        .expect("edge deletion succeeds");

        assert_eq!(snapshot(&editor), original);
        assert_eq!(editor.selection(), selection);
        assert!(!editor.can_undo());
    }
}

#[test]
fn joining_heading_and_paragraph_preserves_marks_and_utf16_cursor() {
    let left = json!({
        "type": "text",
        "text": "A😀",
        "marks": [
            { "type": "strong" }
        ]
    });

    let right = json!({
        "type": "text",
        "text": "e\u{301}",
        "marks": [
            { "type": "em" }
        ]
    });

    assert_delete(
        editor(
            json!([
                {
                    "type": "heading",
                    "attrs": { "level": 2 },
                    "content": [left]
                },
                {
                    "type": "paragraph",
                    "content": [right]
                }
            ]),
            4,
            4,
        ),
        true,
        false,
        json!([
            {
                "type": "heading",
                "attrs": { "level": 2 },
                "content": [left, right]
            }
        ]),
        4,
    );
}

#[test]
fn backspace_inside_a_heading_deletes_one_grapheme_without_demoting() {
    assert_delete(
        editor(json!([heading(2, "A😀B")]), 4, 4),
        false,
        false,
        json!([heading(2, "AB")]),
        2,
    );
}

#[test]
fn heading_demotion_clears_stored_marks_and_undo_restores_them() {
    let mut editor = editor(json!([heading(2, "")]), 1, 1);

    editor.toggle_mark(Mark::Code).expect("set stored mark");

    assert_eq!(editor.stored_marks(), Some([Mark::Code].as_slice()));

    assert_delete(editor, false, false, json!([paragraph("")]), 1);
}
