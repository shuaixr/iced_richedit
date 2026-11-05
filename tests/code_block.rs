use iced_richedit::document::{Document, DocumentError};
use serde_json::{Value, json};

fn input(content: Value) -> Value {
    json!({
        "type": "doc",
        "content": [
            {
                "type": "code_block",
                "content": content
            }
        ]
    })
}

#[test]
fn code_block_import_preserves_whitespace_unicode_and_empty_content() {
    for text in ["", "  let x = 1;\n\n😀e\u{301}\n", "\tA\r\nB"] {
        let content = if text.is_empty() {
            json!([])
        } else {
            json!([
                {
                    "type": "text",
                    "text": text
                }
            ])
        };

        let document = Document::from_json(&input(content).to_string()).expect("valid code block");

        let actual: Value = serde_json::from_str(&document.to_json().expect("serialize document"))
            .expect("valid JSON");

        let expected = if text.is_empty() {
            json!({
                "type": "doc",
                "content": [
                    {
                        "type": "code_block"
                    }
                ]
            })
        } else {
            input(json!([
                {
                    "type": "text",
                    "text": text
                }
            ]))
        };

        assert_eq!(actual, expected);
    }
}

#[test]
fn code_block_import_rejects_all_builtin_inline_marks() {
    for mark in [
        json!({ "type": "strong" }),
        json!({ "type": "em" }),
        json!({ "type": "underline" }),
        json!({ "type": "code" }),
        json!({
            "type": "link",
            "attrs": {
                "href": "https://iced.rs/"
            }
        }),
    ] {
        let value = input(json!([
            {
                "type": "text",
                "text": "A",
                "marks": [mark]
            }
        ]));

        assert!(
            matches!(
                Document::from_json(&value.to_string()),
                Err(DocumentError::InvalidDocument(_))
            ),
            "accepted {mark}"
        );
    }
}

#[test]
fn code_block_import_rejects_hard_break_instead_of_flattening_it() {
    let value = input(json!([
        {
            "type": "text",
            "text": "A"
        },
        {
            "type": "hard_break"
        },
        {
            "type": "text",
            "text": "B"
        }
    ]));

    assert!(matches!(
        Document::from_json(&value.to_string()),
        Err(DocumentError::InvalidDocument(_))
    ));
}

#[test]
fn code_block_import_rejects_nested_blocks() {
    let value = input(json!([
        {
            "type": "paragraph"
        }
    ]));

    assert!(matches!(
        Document::from_json(&value.to_string()),
        Err(DocumentError::InvalidDocument(_))
    ));
}
