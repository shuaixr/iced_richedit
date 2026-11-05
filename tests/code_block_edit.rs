use iced_richedit::{
    document::{Document, Mark, Position, TextSelection},
    editor::EditorState,
};
use serde_json::{Value, json};

fn text(value: &str, strong: bool) -> Value {
    let mut node = json!({ "type": "text", "text": value });
    if strong {
        node["marks"] = json!([{ "type": "strong" }]);
    }
    node
}

fn block(kind: &str, value: &str) -> Value {
    let mut node = json!({ "type": kind });
    if !value.is_empty() {
        node["content"] = json!([text(value, false)]);
    }
    node
}

fn editor(content: Value, anchor: usize, head: usize) -> EditorState {
    let document = Document::from_json(&json!({ "type": "doc", "content": content }).to_string())
        .expect("valid document");
    let mut state = EditorState::new(document).expect("valid editor");
    state
        .set_selection(TextSelection::new(
            Position::from_utf16_offset(anchor),
            Position::from_utf16_offset(head),
        ))
        .expect("valid selection");
    state
}

fn snapshot(state: &EditorState) -> Value {
    serde_json::from_str(&state.document().to_json().expect("serialize document"))
        .expect("valid JSON")
}

fn assert_edit(
    mut state: EditorState,
    action: impl FnOnce(&mut EditorState),
    expected: Value,
    cursor: usize,
) {
    let original = snapshot(&state);
    let selection = state.selection();
    let marks = state.stored_marks().map(<[Mark]>::to_vec);
    assert!(!state.can_undo());
    action(&mut state);
    let changed = snapshot(&state);
    let result_selection = TextSelection::cursor(Position::from_utf16_offset(cursor));
    assert_eq!(changed["content"], expected);
    assert_eq!(state.selection(), result_selection);
    Document::from_json(&changed.to_string()).expect("edited document satisfies schema");
    state.undo().expect("undo edit");
    assert_eq!(snapshot(&state), original);
    assert_eq!(state.selection(), selection);
    assert_eq!(state.stored_marks(), marks.as_deref());
    assert!(!state.can_undo());
    state.redo().expect("redo edit");
    assert_eq!(snapshot(&state), changed);
    assert_eq!(state.selection(), result_selection);
}

#[test]
fn enter_in_code_inserts_one_newline_at_start_middle_end_and_empty_positions() {
    for value in ["", "AB", "A\n"] {
        for cursor in 1..=value.len() + 1 {
            let mut expected = value.to_owned();
            expected.insert(cursor - 1, '\n');
            assert_edit(
                editor(json!([block("code_block", value)]), cursor, cursor),
                |state| state.split_paragraph().expect("Enter"),
                json!([block("code_block", &expected)]),
                cursor + 1,
            );
        }
    }
}

#[test]
fn enter_replaces_code_selection_without_exiting_or_splitting_the_block() {
    for (anchor, head, expected, cursor) in [(4, 2, "A\nD", 3), (5, 1, "\n", 2)] {
        assert_edit(
            editor(json!([block("code_block", "ABCD")]), anchor, head),
            |state| state.split_paragraph().expect("Enter"),
            json!([block("code_block", expected)]),
            cursor,
        );
    }
}

#[test]
fn paste_into_code_normalizes_separators_and_preserves_empty_and_trailing_lines() {
    for (input, normalized) in [
        ("X\r\nY", "X\nY"),
        ("X\rY", "X\nY"),
        ("X\n\rY", "X\n\nY"),
        ("\n", "\n"),
        ("😀\n\n中\n", "😀\n\n中\n"),
    ] {
        let expected = format!("A{normalized}B");
        assert_edit(
            editor(json!([block("code_block", "AB")]), 2, 2),
            |state| state.paste_text(input).expect("paste"),
            json!([block("code_block", &expected)]),
            2 + normalized.encode_utf16().count(),
        );
    }
}

#[test]
fn code_composition_commit_is_unmarked_multiline_and_rejects_a_reused_context() {
    assert_edit(
        editor(json!([block("code_block", "AB")]), 3, 2),
        |state| {
            let context = state.composition_context();
            state
                .commit_composition_in_context(&context, "中\r\n😀\n")
                .expect("commit");
            assert_eq!(state.stored_marks(), None);
            let committed = snapshot(state);
            let selection = state.selection();
            assert!(
                state
                    .commit_composition_in_context(&context, "duplicate")
                    .is_err()
            );
            assert_eq!(snapshot(state), committed);
            assert_eq!(state.selection(), selection);
        },
        json!([block("code_block", "A中\n😀\n")]),
        7,
    );
}

#[test]
fn typing_over_code_selection_uses_utf16_positions_and_inserts_no_marks() {
    assert_edit(
        editor(json!([block("code_block", "AB")]), 3, 2),
        |state| {
            state.insert_text("中😀\n").expect("type text");
            assert_eq!(state.stored_marks(), None);
        },
        json!([block("code_block", "A中😀\n")]),
        6,
    );
}

#[test]
fn cross_block_replacement_uses_the_candidate_block_for_each_input_command() {
    for left_code in [false, true] {
        for mode in 0..4 {
            let code = block("code_block", "AB");
            let paragraph = json!({ "type": "paragraph", "content": [text("AB", true)] });
            let content = if left_code {
                json!([code, { "type": "paragraph", "content": [text("CD", true)] }])
            } else {
                json!([paragraph, block("code_block", "CD")])
            };
            let input = match mode {
                0 => "x",
                1 => "X\nY",
                2 => "中\n文",
                _ => "\n",
            };
            let cursor = if left_code || mode == 0 {
                2 + input.encode_utf16().count()
            } else {
                3 + input.encode_utf16().count()
            };
            let expected = if left_code {
                json!([
                    block("code_block", &format!("A{input}")),
                    { "type": "paragraph", "content": [text("D", true)] }
                ])
            } else if mode == 0 {
                json!([{ "type": "paragraph", "content": [text("Ax", true), text("D", false)] }])
            } else if mode == 3 {
                json!([
                    { "type": "paragraph", "content": [text("A", true)] },
                    block("paragraph", "D")
                ])
            } else {
                let (first, last) = input.split_once('\n').expect("two input lines");
                json!([
                    { "type": "paragraph", "content": [text(&format!("A{first}"), true)] },
                    { "type": "paragraph", "content": [text(last, true), text("D", false)] }
                ])
            };
            assert_edit(
                editor(content, 6, 2),
                |state| {
                    match mode {
                        0 => state.insert_text(input),
                        1 => state.paste_text(input),
                        2 => state.commit_composition(input),
                        _ => state.split_paragraph(),
                    }
                    .expect("replace mixed selection");
                },
                expected,
                cursor,
            );
        }
    }
}
