use iced_richedit::{
    document::{Document, Position, TextSelection},
    editor::EditorState,
};
use serde_json::{Value, json};

fn document(content: Value) -> Document {
    Document::from_json(
        &json!({
            "type": "doc",
            "content": content
        })
        .to_string(),
    )
    .expect("valid document")
}

fn snapshot(editor: &EditorState) -> Value {
    serde_json::from_str(&editor.document().to_json().expect("serialize document"))
        .expect("valid JSON")
}

#[test]
fn imports_all_heading_levels_and_preserves_inline_content() {
    for level in 1..=6 {
        let inline = json!([
            {
                "type": "text",
                "text": "A😀",
                "marks": [
                    { "type": "strong" }
                ]
            },
            {
                "type": "hard_break"
            },
            {
                "type": "text",
                "text": "B"
            }
        ]);

        let editor = EditorState::new(document(json!([
            {
                "type": "heading",
                "attrs": {
                    "level": level
                },
                "content": inline
            }
        ])))
        .expect("valid editor");

        let value = snapshot(&editor);

        assert_eq!(value["content"][0]["attrs"]["level"], level);
        assert_eq!(value["content"][0]["content"], inline);
    }
}

#[test]
fn heading_level_defaults_to_one_and_rejects_invalid_values() {
    for block in [
        json!({
            "type": "heading"
        }),
        json!({
            "type": "heading",
            "attrs": {}
        }),
        json!({
            "type": "heading",
            "attrs": null
        }),
    ] {
        let editor = EditorState::new(document(json!([block]))).expect("valid editor");

        assert_eq!(snapshot(&editor)["content"][0]["attrs"]["level"], 1);
    }

    for level in [
        json!(0),
        json!(7),
        json!(-1),
        json!(1.0),
        json!("2"),
        Value::Null,
    ] {
        let input = json!({
            "type": "doc",
            "content": [
                {
                    "type": "heading",
                    "attrs": {
                        "level": level
                    }
                }
            ]
        });

        assert!(
            Document::from_json(&input.to_string()).is_err(),
            "accepted {level}"
        );
    }
}

#[test]
fn heading_conversion_preserves_inline_content_and_round_trips_to_paragraph() {
    let mut editor = EditorState::new(document(json!([
        {
            "type": "paragraph",
            "content": [
                {
                    "type": "text",
                    "text": "A😀",
                    "marks": [
                        { "type": "em" }
                    ]
                },
                {
                    "type": "hard_break"
                },
                {
                    "type": "text",
                    "text": "B"
                }
            ]
        }
    ])))
    .expect("valid editor");

    let original = snapshot(&editor);
    let selection = editor.selection();

    editor.set_heading(2).expect("convert heading");

    let converted = snapshot(&editor);

    assert_eq!(converted["content"][0]["type"], "heading");
    assert_eq!(
        converted["content"][0]["content"],
        original["content"][0]["content"]
    );
    assert_eq!(editor.selection(), selection);

    editor.set_paragraph().expect("convert paragraph");

    assert_eq!(snapshot(&editor), original);
}

#[test]
fn multiple_block_conversion_is_one_undo_and_preserves_selection_direction() {
    let mut editor = EditorState::new(document(json!([
        {
            "type": "paragraph",
            "content": [
                {
                    "type": "text",
                    "text": "AB"
                }
            ]
        },
        {
            "type": "paragraph",
            "content": [
                {
                    "type": "text",
                    "text": "CD"
                }
            ]
        }
    ])))
    .expect("valid editor");

    let selection = TextSelection::new(
        Position::from_utf16_offset(7),
        Position::from_utf16_offset(1),
    );

    editor.set_selection(selection).expect("select blocks");

    let original = snapshot(&editor);

    editor.set_heading(3).expect("convert blocks");

    let converted = snapshot(&editor);

    assert!(
        converted["content"]
            .as_array()
            .expect("blocks")
            .iter()
            .all(|block| { block["type"] == "heading" && block["attrs"]["level"] == 3 })
    );
    assert_eq!(editor.selection(), selection);

    editor.undo().expect("undo");

    assert_eq!(snapshot(&editor), original);
    assert_eq!(editor.selection(), selection);
    assert!(!editor.can_undo());

    editor.redo().expect("redo");

    assert_eq!(snapshot(&editor), converted);
}

#[test]
fn conversion_excludes_a_block_at_the_selection_end() {
    let mut editor = EditorState::new(document(json!([
        {
            "type": "paragraph",
            "content": [
                {
                    "type": "text",
                    "text": "AB"
                }
            ]
        },
        {
            "type": "paragraph",
            "content": [
                {
                    "type": "text",
                    "text": "CD"
                }
            ]
        }
    ])))
    .expect("valid editor");

    // The second paragraph starts at position 5.
    let selection = TextSelection::new(
        Position::from_utf16_offset(5),
        Position::from_utf16_offset(2),
    );

    editor.set_selection(selection).expect("select range");
    editor.set_heading(2).expect("convert heading");

    let value = snapshot(&editor);

    assert_eq!(value["content"][0]["type"], "heading");
    assert_eq!(value["content"][1]["type"], "paragraph");
    assert_eq!(editor.selection(), selection);
}

#[test]
fn invalid_heading_conversion_leaves_document_selection_and_history_unchanged() {
    let mut editor =
        EditorState::new(Document::empty().expect("valid document")).expect("valid editor");

    let original = snapshot(&editor);
    let selection = editor.selection();

    for level in [0, 7, 255] {
        assert!(editor.set_heading(level).is_err());
        assert_eq!(snapshot(&editor), original);
        assert_eq!(editor.selection(), selection);
        assert!(!editor.can_undo());
    }
}

#[test]
fn setting_the_existing_heading_level_does_not_add_history() {
    let mut editor = EditorState::new(document(json!([
        {
            "type": "heading",
            "attrs": {
                "level": 2
            }
        }
    ])))
    .expect("valid editor");

    editor.set_heading(2).expect("already heading");

    assert!(!editor.can_undo());
}
