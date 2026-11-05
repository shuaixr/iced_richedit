use iced_richedit::{
    document::{Document, Mark, Position, TextSelection},
    editor::EditorState,
};
use serde_json::{Value, json};

fn block(kind: &str, level: Option<u8>, text: &str) -> Value {
    let mut value = json!({
        "type": kind
    });

    if let Some(level) = level {
        value["attrs"] = json!({
            "level": level
        });
    }

    if !text.is_empty() {
        value["content"] = json!([
            {
                "type": "text",
                "text": text
            }
        ]);
    }

    value
}

fn heading(level: u8, text: &str) -> Value {
    block("heading", Some(level), text)
}

fn paragraph(text: &str) -> Value {
    block("paragraph", None, text)
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

fn assert_enter(mut editor: EditorState, expected: Value, cursor: usize) {
    let original = snapshot(&editor);
    let original_selection = editor.selection();
    let original_marks = editor.stored_marks().map(<[Mark]>::to_vec);

    assert!(!editor.can_undo());

    editor.split_paragraph().expect("Enter succeeds");

    let result = snapshot(&editor);
    let result_selection = TextSelection::cursor(Position::from_utf16_offset(cursor));

    assert_eq!(result["content"], expected);
    assert_eq!(editor.selection(), result_selection);
    assert_eq!(editor.stored_marks(), None);

    editor.undo().expect("undo Enter");

    assert_eq!(snapshot(&editor), original);
    assert_eq!(editor.selection(), original_selection);
    assert_eq!(editor.stored_marks(), original_marks.as_deref());
    assert!(!editor.can_undo());

    editor.redo().expect("redo Enter");

    assert_eq!(snapshot(&editor), result);
    assert_eq!(editor.selection(), result_selection);
    assert_eq!(editor.stored_marks(), None);
}

#[test]
fn enter_inside_heading_creates_two_headings_of_the_same_level() {
    for level in 1..=6 {
        assert_enter(
            editor(json!([heading(level, "AB")]), 2, 2),
            json!([heading(level, "A"), heading(level, "B")]),
            4,
        );
    }
}

#[test]
fn enter_at_heading_start_inserts_a_paragraph_before_it() {
    for level in 1..=6 {
        assert_enter(
            editor(json!([heading(level, "AB")]), 1, 1),
            json!([paragraph(""), heading(level, "AB")]),
            3,
        );
    }
}

#[test]
fn enter_at_heading_end_creates_a_paragraph_after_it() {
    for level in 1..=6 {
        assert_enter(
            editor(json!([heading(level, "AB")]), 3, 3),
            json!([heading(level, "AB"), paragraph("")]),
            5,
        );
    }
}

#[test]
fn enter_in_an_empty_heading_converts_it_without_adding_a_block() {
    for level in 1..=6 {
        assert_enter(
            editor(json!([heading(level, "")]), 1, 1),
            json!([paragraph("")]),
            1,
        );
    }
}

#[test]
fn heading_enter_preserves_marks_and_uses_utf16_positions() {
    let left = json!({
        "type": "text",
        "text": "A😀",
        "marks": [
            {
                "type": "strong"
            }
        ]
    });

    let right = json!({
        "type": "text",
        "text": "BC",
        "marks": [
            {
                "type": "em"
            }
        ]
    });

    assert_enter(
        editor(
            json!([
                {
                    "type": "heading",
                    "attrs": {
                        "level": 4
                    },
                    "content": [left, right]
                }
            ]),
            4,
            4,
        ),
        json!([
            {
                "type": "heading",
                "attrs": {
                    "level": 4
                },
                "content": [left]
            },
            {
                "type": "heading",
                "attrs": {
                    "level": 4
                },
                "content": [right]
            }
        ]),
        6,
    );
}

#[test]
fn heading_enter_deletes_a_reverse_selection_before_splitting() {
    assert_enter(
        editor(json!([heading(2, "ABCD")]), 4, 2),
        json!([heading(2, "A"), heading(2, "D")]),
        4,
    );
}

#[test]
fn heading_enter_uses_the_end_rule_after_deleting_selected_trailing_text() {
    assert_enter(
        editor(json!([heading(2, "ABCD")]), 3, 5),
        json!([heading(2, "AB"), paragraph("")]),
        5,
    );
}

#[test]
fn heading_enter_replacing_all_text_produces_one_empty_paragraph() {
    assert_enter(
        editor(json!([heading(2, "ABCD")]), 1, 5),
        json!([paragraph("")]),
        1,
    );
}

#[test]
fn heading_enter_handles_a_cross_block_selection_as_one_history_event() {
    assert_enter(
        editor(json!([heading(2, "HAB"), paragraph("CD")]), 7, 2),
        json!([heading(2, "H"), heading(2, "D")]),
        4,
    );
}

#[test]
fn empty_heading_enter_clears_stored_marks_and_undo_restores_them() {
    let mut editor = editor(json!([heading(2, "")]), 1, 1);

    editor.toggle_mark(Mark::Strong).expect("set stored mark");

    assert_eq!(editor.stored_marks(), Some([Mark::Strong].as_slice()));

    assert_enter(editor, json!([paragraph("")]), 1);
}
