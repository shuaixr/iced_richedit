use iced_richedit::{
    document::{Document, Mark, Position, TextSelection},
    editor::EditorState,
};
use serde_json::{Value, json};

fn text(value: &str) -> Value {
    json!({ "type": "text", "text": value })
}

fn block(kind: &str, value: &str) -> Value {
    let mut node = json!({ "type": kind });
    if kind == "heading" {
        node["attrs"] = json!({ "level": 3 });
    }
    if !value.is_empty() {
        node["content"] = json!([text(value)]);
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

fn delete(state: &mut EditorState, forward: bool, by_word: bool) {
    match (forward, by_word) {
        (false, false) => state.delete_backward(),
        (true, false) => state.delete_forward(),
        (false, true) => state.delete_word_backward(),
        (true, true) => state.delete_word_forward(),
    }
    .expect("delete succeeds");
}

fn assert_edit(
    mut state: EditorState,
    forward: bool,
    by_word: bool,
    expected: Value,
    cursor: usize,
) {
    let original = snapshot(&state);
    let selection = state.selection();
    let marks = state.stored_marks().map(<[Mark]>::to_vec);
    let context = state.composition_context();
    assert!(!state.can_undo());
    delete(&mut state, forward, by_word);
    let changed = snapshot(&state);
    let result_selection = TextSelection::cursor(Position::from_utf16_offset(cursor));
    assert_eq!(changed["content"], expected);
    assert_eq!(state.selection(), result_selection);
    assert_eq!(state.stored_marks(), None);
    assert!(!state.can_redo());
    assert!(!state.is_composition_context_current(&context));
    Document::from_json(&changed.to_string()).expect("result satisfies schema");
    state.undo().expect("undo deletion");
    assert_eq!(snapshot(&state), original);
    assert_eq!(state.selection(), selection);
    assert_eq!(state.stored_marks(), marks.as_deref());
    assert!(!state.can_undo());
    state.redo().expect("redo deletion");
    assert_eq!(snapshot(&state), changed);
    assert_eq!(state.selection(), result_selection);
    assert_eq!(state.stored_marks(), None);
}

#[test]
fn backspace_at_first_code_start_converts_empty_and_multiline_blocks() {
    for by_word in [false, true] {
        for (value, expected) in [
            ("", block("paragraph", "")),
            ("AB", block("paragraph", "AB")),
            (
                "\nA\n\n",
                json!({ "type": "paragraph", "content": [
                { "type": "hard_break" }, text("A"),
                { "type": "hard_break" }, { "type": "hard_break" }
            ] }),
            ),
            (
                "😀\n",
                json!({ "type": "paragraph", "content": [text("😀"), { "type": "hard_break" }] }),
            ),
        ] {
            assert_edit(
                editor(json!([block("code_block", value)]), 1, 1),
                false,
                by_word,
                json!([expected]),
                1,
            );
        }
    }
}

#[test]
fn empty_code_start_backspace_converts_without_joining_the_previous_block() {
    for previous in ["paragraph", "heading", "code_block"] {
        for by_word in [false, true] {
            assert_edit(
                editor(json!([block(previous, "L"), block("code_block", "")]), 4, 4),
                false,
                by_word,
                json!([block(previous, "L"), block("paragraph", "")]),
                4,
            );
        }
    }
}

#[test]
fn nonempty_code_start_joins_the_previous_block_and_preserves_its_type() {
    for previous in ["paragraph", "heading", "code_block"] {
        let mut expected = block(previous, "LA\nB");
        if previous != "code_block" {
            expected["content"] = json!([text("LA"), {"type":"hard_break"}, text("B")]);
        }
        for by_word in [false, true] {
            assert_edit(
                editor(
                    json!([block(previous, "L"), block("code_block", "A\nB")]),
                    4,
                    4,
                ),
                false,
                by_word,
                json!([expected]),
                2,
            );
        }
    }
}

#[test]
fn heading_start_after_code_still_converts_before_joining() {
    let mut state = editor(
        json!([block("code_block", "L"), block("heading", "R")]),
        4,
        4,
    );
    let original = snapshot(&state);
    state.delete_backward().expect("demote heading");
    let demoted = snapshot(&state);
    assert_eq!(
        demoted["content"],
        json!([block("code_block", "L"), block("paragraph", "R")])
    );
    state.delete_backward().expect("join code");
    assert_eq!(
        snapshot(&state)["content"],
        json!([block("code_block", "LR")])
    );
    assert_eq!(
        state.selection(),
        TextSelection::cursor(Position::from_utf16_offset(2))
    );
    state.undo().expect("undo join");
    assert_eq!(snapshot(&state), demoted);
    state.undo().expect("undo demotion");
    assert_eq!(snapshot(&state), original);
    assert!(!state.can_undo());
}

#[test]
fn second_backspace_after_empty_code_exit_joins_the_previous_block() {
    for previous in ["paragraph", "code_block"] {
        for by_word in [false, true] {
            let mut state = editor(json!([block(previous, "L"), block("code_block", "")]), 4, 4);
            let original = snapshot(&state);
            delete(&mut state, false, by_word);
            let converted = snapshot(&state);
            assert_eq!(
                converted["content"],
                json!([block(previous, "L"), block("paragraph", "")])
            );
            delete(&mut state, false, by_word);
            assert_eq!(snapshot(&state)["content"], json!([block(previous, "L")]));
            assert_eq!(
                state.selection(),
                TextSelection::cursor(Position::from_utf16_offset(2))
            );
            state.undo().expect("undo join");
            assert_eq!(snapshot(&state), converted);
            state.undo().expect("undo exit");
            assert_eq!(snapshot(&state), original);
            assert!(!state.can_undo());
        }
    }
}

#[test]
fn forward_boundary_deletion_joins_code_neighbors_and_keeps_the_left_type() {
    for left in ["paragraph", "heading", "code_block"] {
        for right in ["paragraph", "heading", "code_block"] {
            if left != "code_block" && right != "code_block" {
                continue;
            }
            for (left_text, right_text) in [("AB", "CD"), ("", "CD"), ("AB", ""), ("", "")] {
                for by_word in [false, true] {
                    let cursor = 1 + left_text.encode_utf16().count();
                    assert_edit(
                        editor(
                            json!([block(left, left_text), block(right, right_text)]),
                            cursor,
                            cursor,
                        ),
                        true,
                        by_word,
                        json!([block(left, &format!("{left_text}{right_text}"))]),
                        cursor,
                    );
                }
            }
        }
    }
}

#[test]
fn backward_deletion_at_paragraph_start_joins_previous_code() {
    for value in ["", "CD"] {
        for by_word in [false, true] {
            assert_edit(
                editor(
                    json!([block("code_block", "AB"), block("paragraph", value)]),
                    5,
                    5,
                ),
                false,
                by_word,
                json!([block("code_block", &format!("AB{value}"))]),
                3,
            );
        }
    }
}

#[test]
fn joining_into_code_removes_incoming_marks_and_preserves_breaks() {
    let right = json!({"type":"paragraph", "content":[
        {"type":"text", "text":"B", "marks":[
            {"type":"strong"}, {"type":"em"}, {"type":"underline"}, {"type":"code"},
            {"type":"link", "attrs":{"href":"https://iced.rs/", "title":"iced"}}
        ]},
        {"type":"hard_break"}, {"type":"hard_break"}, text("中"), {"type":"hard_break"}
    ]});
    for forward in [false, true] {
        for by_word in [false, true] {
            let cursor = if forward { 4 } else { 6 };
            assert_edit(
                editor(json!([block("code_block", "😀\n"), right]), cursor, cursor),
                forward,
                by_word,
                json!([block("code_block", "😀\nB\n\n中\n")]),
                4,
            );
        }
    }
}

#[test]
fn joining_code_into_rich_text_preserves_left_marks_heading_level_and_surrounding_blocks() {
    for kind in ["paragraph", "heading"] {
        let mut left = block(kind, "😀");
        left["content"][0]["marks"] = json!([{"type":"strong"}, {"type":"em"}]);
        let mut merged = left.clone();
        merged["content"] = json!([
            left["content"][0], text("A"), {"type":"hard_break"}, {"type":"hard_break"},
            text("B"), {"type":"hard_break"}
        ]);
        for forward in [false, true] {
            for by_word in [false, true] {
                let cursor = if forward { 6 } else { 8 };
                assert_edit(
                    editor(
                        json!([
                            block("paragraph", "P"),
                            left,
                            block("code_block", "A\n\nB\n"),
                            block("paragraph", "Z")
                        ]),
                        cursor,
                        cursor,
                    ),
                    forward,
                    by_word,
                    json!([block("paragraph", "P"), merged, block("paragraph", "Z")]),
                    6,
                );
            }
        }
    }
}

#[test]
fn code_joining_preserves_imported_crlf_without_inserting_a_separator() {
    for forward in [false, true] {
        let cursor = if forward { 2 } else { 4 };
        assert_edit(
            editor(
                json!([block("code_block", "L"), block("code_block", "\r\nA\n")]),
                cursor,
                cursor,
            ),
            forward,
            false,
            json!([block("code_block", "L\r\nA\n")]),
            2,
        );
    }
}

#[test]
fn boundary_join_clears_stored_marks_and_replaces_existing_redo_history() {
    let mut state = editor(
        json!([block("paragraph", "A"), block("code_block", "B\n")]),
        2,
        2,
    );
    state.insert_text("x").expect("type");
    state.undo().expect("undo typing");
    assert!(state.can_redo());
    state.toggle_mark(Mark::Strong).expect("stored mark");
    assert_edit(
        state,
        true,
        true,
        json!([{
            "type":"paragraph", "content":[text("AB"), {"type":"hard_break"}]
        }]),
        2,
    );
}

#[test]
fn typing_after_a_code_join_is_unmarked_and_has_separate_history() {
    let mut state = editor(
        json!([
            block("code_block", "A"),
            {"type":"paragraph", "content":[{"type":"text", "text":"B", "marks":[{"type":"strong"}]}]}
        ]),
        4,
        4,
    );
    let original = snapshot(&state);
    state.delete_backward().expect("join");
    let joined = snapshot(&state);
    assert_eq!(joined["content"], json!([block("code_block", "AB")]));
    state.insert_text("😀").expect("type at seam");
    let typed = snapshot(&state);
    assert_eq!(typed["content"], json!([block("code_block", "A😀B")]));
    state.undo().expect("undo input");
    assert_eq!(snapshot(&state), joined);
    state.undo().expect("undo join");
    assert_eq!(snapshot(&state), original);
    state.redo().expect("redo join");
    assert_eq!(snapshot(&state), joined);
    state.redo().expect("redo input");
    assert_eq!(snapshot(&state), typed);
}

#[test]
fn forward_deletion_at_code_document_end_remains_a_noop_with_redo() {
    let mut state = editor(json!([block("code_block", "AB")]), 3, 3);
    state.insert_text("x").expect("type");
    let typed = snapshot(&state);
    state.undo().expect("undo input");
    let original = snapshot(&state);
    let context = state.composition_context();
    let selection = state.selection();
    for by_word in [false, true] {
        delete(&mut state, true, by_word);
        assert_eq!(snapshot(&state), original);
        assert_eq!(state.selection(), selection);
        assert!(state.can_redo());
        assert!(!state.can_undo());
        assert!(state.is_composition_context_current(&context));
    }
    state.redo().expect("redo input still works");
    assert_eq!(snapshot(&state), typed);
}

#[test]
fn code_internal_newlines_and_graphemes_are_deleted_without_exiting() {
    for (value, backward, forward) in [("A\nB", 3, 2), ("A😀B", 4, 2)] {
        assert_edit(
            editor(json!([block("code_block", value)]), backward, backward),
            false,
            false,
            json!([block("code_block", "AB")]),
            2,
        );
        assert_edit(
            editor(json!([block("code_block", value)]), forward, forward),
            true,
            false,
            json!([block("code_block", "AB")]),
            2,
        );
    }
}

#[test]
fn selected_text_deletion_is_not_intercepted_by_code_exit_or_boundary_rules() {
    for forward in [false, true] {
        for by_word in [false, true] {
            assert_edit(
                editor(json!([block("code_block", "AB")]), 2, 1),
                forward,
                by_word,
                json!([block("code_block", "B")]),
                1,
            );
            let suffix = json!({ "type": "paragraph", "content": [
                { "type": "text", "text": "CD", "marks": [{ "type": "strong" }] }
            ] });
            let expected = json!([
                block("code_block", "A"),
                { "type": "paragraph", "content": [
                    { "type": "text", "text": "D", "marks": [{ "type": "strong" }] }
                ] }
            ]);
            assert_edit(
                editor(json!([block("code_block", "AB"), suffix]), 6, 2),
                forward,
                by_word,
                expected,
                2,
            );
        }
    }
}
