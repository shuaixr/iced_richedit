use iced_richedit::document::{Document, DocumentError};

#[test]
fn empty_document_contains_one_empty_paragraph() {
    let document = Document::empty().expect("empty document must be valid");
    let json: serde_json::Value =
        serde_json::from_str(&document.to_json().expect("document must serialize"))
            .expect("serialized document must be valid JSON");

    assert_eq!(
        json,
        serde_json::json!({
            "type": "doc",
            "content": [
                { "type": "paragraph" }
            ]
        })
    );
}
#[test]
fn empty_document_has_no_text() {
    let document = Document::empty().expect("empty document must be valid");

    assert_eq!(document.text(), "");
}

#[test]
fn document_parses_prosemirror_json() {
    let json = r#"{"type":"doc","content":[{"type":"paragraph"}]}"#;
    let document = Document::from_json(json).expect("document must be valid");

    assert_eq!(document.text(), "");
}

#[test]
fn document_serializes_to_prosemirror_json() {
    let document = Document::empty().expect("empty document must be valid");
    let json = document.to_json().expect("serialization must succeed");

    let actual: serde_json::Value = serde_json::from_str(&json).expect("output must be valid JSON");

    let expected = serde_json::json!({
        "type": "doc",
        "content": [
            { "type": "paragraph" }
        ]
    });

    assert_eq!(actual, expected);
}

#[test]
fn document_json_round_trips() {
    let input = serde_json::json!({
        "type": "doc",
        "content": [{
            "type": "paragraph",
            "content": [{
                "type": "text",
                "text": "Hello, 世界!"
            }]
        }]
    });

    let document = Document::from_json(&input.to_string()).expect("document must be valid");

    let output: serde_json::Value =
        serde_json::from_str(&document.to_json().expect("serialization must succeed"))
            .expect("output must be valid JSON");

    assert_eq!(output, input);
}

#[test]
fn document_preserves_unicode_text() {
    let json = r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Hello, 世界!"}]}]}"#;
    let document = Document::from_json(json).expect("document must be valid");

    assert_eq!(document.text(), "Hello, 世界!");
}

#[test]
fn document_rejects_invalid_json() {
    let json =
        r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Hello!""#;
    let result = Document::from_json(json);

    assert!(result.is_err());
}

#[test]
fn document_rejects_non_document_root() {
    let json = r#"{"type":"paragraph"}"#;
    let result = Document::from_json(json);
    assert!(result.is_err());
}

#[test]
fn document_rejects_unknown_node_type() {
    let json = r#"{"type":"doc","content":[{"type":"unknown"}]}"#;
    let result = Document::from_json(json);
    assert!(result.is_err());
}

#[test]
fn document_rejects_unknown_mark_type() {
    let json = r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Hello, 世界!","marks":[{"type":"unknown"}]}]}]}"#;
    let result = Document::from_json(json);
    assert!(result.is_err());
}

#[test]
fn document_rejects_invalid_nesting() {
    let result = Document::from_json(
        r#"{
               "type": "doc",
               "content": [{
                   "type": "text",
                   "text": "Hello"
               }]
           }"#,
    );

    assert!(matches!(result, Err(DocumentError::InvalidDocument(_))));
}
#[test]
fn document_imports_and_exports_link_json() {
    let input = serde_json::json!({
        "type": "doc",
        "content": [{
            "type": "paragraph",
            "content": [{
                "type": "text",
                "text": "Hello, ",
                "marks": [{
                    "type": "link",
                    "attrs": {
                        "href": "https://example.com",
                        "title": "Example"
                    }
                }]
            }]
        }]
    });

    let document = Document::from_json(&input.to_string()).expect("document must be valid");

    let output: serde_json::Value =
        serde_json::from_str(&document.to_json().expect("serialization must succeed"))
            .expect("output must be valid JSON");

    assert_eq!(output, input);
}
