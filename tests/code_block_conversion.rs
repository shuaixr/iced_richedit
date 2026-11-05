use iced_richedit::{
    document::{Document, Mark, Position, TextSelection},
    editor::EditorState,
};
use serde_json::{Value, json};

fn block(kind: &str, text: &str) -> Value {
    let mut node = json!({ "type": kind });
    if !text.is_empty() {
        node["content"] = json!([{ "type": "text", "text": text }]);
    }
    node
}

fn editor(content: Value, anchor: usize, head: usize) -> EditorState {
    let document = Document::from_json(&json!({ "type": "doc", "content": content }).to_string())
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

fn assert_conversion(
    mut editor: EditorState,
    action: impl FnOnce(&mut EditorState),
    expected: Value,
) {
    let original = snapshot(&editor);
    let selection = editor.selection();
    let marks = editor.stored_marks().map(<[Mark]>::to_vec);
    assert!(!editor.can_undo());
    action(&mut editor);
    let converted = snapshot(&editor);
    let converted_marks = editor.stored_marks().map(<[Mark]>::to_vec);
    assert_eq!(converted["content"], expected);
    assert_eq!(editor.selection(), selection);
    Document::from_json(&converted.to_string()).expect("converted document satisfies schema");
    editor.undo().expect("undo conversion");
    assert_eq!(snapshot(&editor), original);
    assert_eq!(editor.selection(), selection);
    assert_eq!(editor.stored_marks(), marks.as_deref());
    assert!(!editor.can_undo());
    editor.redo().expect("redo conversion");
    assert_eq!(snapshot(&editor), converted);
    assert_eq!(editor.selection(), selection);
    assert_eq!(editor.stored_marks(), converted_marks.as_deref());
}

#[test]
fn conversion_to_code_removes_all_marks_and_preserves_break_positions() {
    let content = json!([{
        "type": "paragraph", "content": [
            { "type": "text", "text": "A😀", "marks": [
                { "type": "strong" }, { "type": "em" }, { "type": "underline" },
                { "type": "code" }, { "type": "link", "attrs": { "href": "https://iced.rs/" } }
            ] },
            { "type": "hard_break" }, { "type": "hard_break" },
            { "type": "text", "text": "B" }, { "type": "hard_break" }
        ]
    }]);
    assert_conversion(
        editor(content, 8, 2),
        |editor| editor.set_code_block().expect("convert to code"),
        json!([block("code_block", "A😀\n\nB\n")]),
    );
}

#[test]
fn code_converts_to_paragraph_or_heading_without_splitting_into_multiple_blocks() {
    let inline = json!([
        { "type": "hard_break" }, { "type": "text", "text": "A😀" },
        { "type": "hard_break" }, { "type": "hard_break" },
        { "type": "text", "text": "B" }, { "type": "hard_break" }
    ]);
    for heading in [false, true] {
        let mut expected = json!({ "type": "paragraph", "content": inline });
        if heading {
            expected["type"] = json!("heading");
            expected["attrs"] = json!({ "level": 2 });
        }
        assert_conversion(
            editor(json!([block("code_block", "\nA😀\n\nB\n")]), 9, 2),
            |editor| {
                if heading {
                    editor.set_heading(2)
                } else {
                    editor.set_paragraph()
                }
                .expect("convert from code");
            },
            json!([expected]),
        );
    }
}

#[test]
fn multi_block_conversion_keeps_blocks_separate_and_excludes_the_terminal_start() {
    assert_conversion(
        editor(
            json!([
                block("paragraph", "A"),
                { "type": "heading", "attrs": { "level": 4 }, "content": [{ "type": "text", "text": "B" }] },
                block("paragraph", "C")
            ]),
            7,
            1,
        ),
        |editor| editor.set_code_block().expect("convert blocks"),
        json!([
            block("code_block", "A"),
            block("code_block", "B"),
            block("paragraph", "C")
        ]),
    );
}

#[test]
fn multi_block_conversion_accepts_an_existing_code_block_in_the_selection() {
    let mut editor = editor(
        json!([
            block("paragraph", "A"),
            block("code_block", "B"),
            block("paragraph", "C")
        ]),
        1,
        1,
    );
    editor.select_all().expect("select all blocks");
    assert_conversion(
        editor,
        |editor| editor.set_code_block().expect("convert blocks"),
        json!([
            block("code_block", "A"),
            block("code_block", "B"),
            block("code_block", "C")
        ]),
    );
}

#[test]
fn empty_block_conversion_clears_stored_marks_and_undo_restores_them() {
    let mut state = editor(json!([block("paragraph", "")]), 1, 1);
    state.toggle_mark(Mark::Strong).expect("store strong");
    state
        .set_link("https://iced.rs/".to_owned(), None)
        .expect("store link");
    assert_conversion(
        state,
        |editor| {
            editor.set_code_block().expect("convert empty block");
            assert_eq!(editor.stored_marks(), None);
        },
        json!([block("code_block", "")]),
    );
    assert_conversion(
        editor(json!([block("code_block", "")]), 1, 1),
        |editor| editor.set_paragraph().expect("convert empty code"),
        json!([block("paragraph", "")]),
    );
}

#[test]
fn converting_an_existing_code_block_does_not_add_history() {
    let mut editor = editor(json!([block("code_block", "A\n")]), 1, 1);
    let original = snapshot(&editor);
    editor.set_code_block().expect("already code");
    assert_eq!(snapshot(&editor), original);
    assert!(!editor.can_undo());
}

#[test]
fn code_conversion_preserves_crlf_without_normalizing_imported_text() {
    let mut editor = editor(json!([block("code_block", "\r\n")]), 1, 1);
    let original = snapshot(&editor);
    editor.set_paragraph().expect("convert to paragraph");
    assert_eq!(
        snapshot(&editor)["content"],
        json!([{
            "type": "paragraph", "content": [{ "type": "text", "text": "\r" }, { "type": "hard_break" }]
        }])
    );
    editor.set_code_block().expect("convert back to code");
    assert_eq!(snapshot(&editor), original);
}

#[test]
fn formatting_rejects_code_cursors_ranges_and_mixed_ranges_without_changes() {
    for (anchor, head) in [(1, 1), (1, 3), (6, 1)] {
        let mut editor = editor(
            json!([
                block("code_block", "AB"),
                { "type": "paragraph", "content": [{ "type": "text", "text": "C", "marks": [{ "type": "strong" }] }] }
            ]),
            anchor,
            head,
        );
        let original = snapshot(&editor);
        let selection = editor.selection();
        for mark in [
            Mark::Strong,
            Mark::Emphasis,
            Mark::Underline,
            Mark::Code,
            Mark::Link {
                href: "https://iced.rs/".to_owned(),
                title: None,
            },
        ] {
            assert!(editor.toggle_mark(mark).is_err());
            assert_eq!(snapshot(&editor), original);
            assert_eq!(editor.selection(), selection);
            assert_eq!(editor.stored_marks(), None);
            assert!(!editor.can_undo());
        }
        assert!(
            editor
                .set_link("https://iced.rs/".to_owned(), None)
                .is_err()
        );
        assert!(editor.remove_link().is_err());
        assert_eq!(snapshot(&editor), original);
        assert_eq!(editor.selection(), selection);
        assert_eq!(editor.stored_marks(), None);
        assert!(!editor.can_undo());
    }
}

#[test]
fn formatting_excludes_a_code_block_at_the_selection_end() {
    let mut editor = editor(
        json!([block("paragraph", "AB"), block("code_block", "CD")]),
        5,
        1,
    );
    editor
        .toggle_mark(Mark::Strong)
        .expect("format only paragraph");
    let actual = snapshot(&editor);
    assert_eq!(
        actual["content"][0]["content"][0]["marks"],
        json!([{ "type": "strong" }])
    );
    assert_eq!(actual["content"][1], block("code_block", "CD"));
}

#[test]
fn transaction_mark_commands_reject_mixed_code_ranges_before_changing_any_block() {
    let editor = editor(
        json!([block("paragraph", "AB"), block("code_block", "CD")]),
        1,
        1,
    );
    let original = snapshot(&editor);
    let mut transaction = editor.document().transaction();
    let from = Position::from_utf16_offset(1);
    let to = Position::from_utf16_offset(7);
    assert!(transaction.add_mark(from, to, Mark::Strong).is_err());
    assert!(transaction.remove_mark(from, to, Mark::Strong).is_err());
    let cursor = Position::from_utf16_offset(5);
    assert!(transaction.add_mark(cursor, cursor, Mark::Strong).is_err());
    let actual: Value = serde_json::from_str(&transaction.commit().to_json().expect("serialize"))
        .expect("valid JSON");
    assert_eq!(actual, original);
}

#[test]
fn rejected_code_formatting_preserves_existing_redo_history() {
    let mut editor = editor(
        json!([block("paragraph", "AB"), block("code_block", "CD")]),
        1,
        2,
    );
    editor.toggle_mark(Mark::Strong).expect("format paragraph");
    let formatted = snapshot(&editor);
    editor.undo().expect("undo formatting");
    editor
        .set_selection(TextSelection::cursor(Position::from_utf16_offset(5)))
        .expect("code cursor");
    assert!(editor.toggle_mark(Mark::Strong).is_err());
    assert!(editor.can_redo());
    editor.redo().expect("redo original formatting");
    assert_eq!(snapshot(&editor), formatted);
}

#[test]
fn typing_after_conversion_does_not_reuse_preconversion_stored_marks() {
    let mut editor = editor(json!([block("paragraph", "")]), 1, 1);
    editor.toggle_mark(Mark::Strong).expect("store strong");
    editor.set_code_block().expect("convert to code");
    assert_eq!(editor.stored_marks(), None);
    editor.insert_text("x").expect("insert plain text");
    let actual = snapshot(&editor);
    assert_eq!(actual["content"], json!([block("code_block", "x")]));
    Document::from_json(&actual.to_string()).expect("result remains valid");
}
