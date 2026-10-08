use iced_richedit::document::{Document, DocumentError, Mark, Position};

#[test]
fn empty_transaction_preserves_document() {
    let document = Document::from_json(
        r#"{
            "type": "doc",
            "content": [{
                "type": "paragraph",
                "content": [{
                    "type": "text",
                    "text": "Hello"
                }]
            }]
        }"#,
    )
    .expect("document must be valid");

    let updated = document.transaction().commit();

    assert_eq!(updated.text(), "Hello");
    assert_eq!(document.text(), "Hello");
}

#[test]
fn transaction_inserts_text() {
    let document = Document::empty().expect("empty document must be valid");

    let mut transaction = document.transaction();

    transaction
        .insert_text(Position::from_utf16_offset(1), "Hello")
        .expect("text insertion must succeed");

    let updated = transaction.commit();

    assert_eq!(updated.text(), "Hello");
    assert_eq!(document.text(), "");
}

#[test]
fn transaction_deletes_text() {
    let document = Document::from_json(
        r#"{
            "type": "doc",
            "content": [{
                "type": "paragraph",
                "content": [{
                    "type": "text",
                    "text": "Hello"
                }]
            }]
        }"#,
    )
    .expect("document must be valid");

    let mut transaction = document.transaction();

    transaction
        .delete_range(
            Position::from_utf16_offset(2),
            Position::from_utf16_offset(4),
        )
        .expect("deletion must succeed");

    let updated = transaction.commit();

    assert_eq!(updated.text(), "Hlo");
}
#[test]
fn transaction_adds_strong_mark() {
    let document = Document::from_json(
           r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Hello"}]}]}"#,
       )
       .expect("document must be valid");

    let mut transaction = document.transaction();

    transaction
        .add_mark(
            Position::from_utf16_offset(1),
            Position::from_utf16_offset(6),
            Mark::Strong,
        )
        .expect("mark must be added");

    let updated = transaction.commit();
    let json: serde_json::Value =
        serde_json::from_str(&updated.to_json().expect("document must serialize"))
            .expect("output must be valid JSON");

    assert_eq!(
        json["content"][0]["content"][0]["marks"],
        serde_json::json!([{ "type": "strong" }])
    );
}
#[test]
fn transaction_adds_link_mark() {
    let document = Document::from_json(
           r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Hello"}]}]}"#,
       )
       .expect("document must be valid");

    let mut transaction = document.transaction();

    transaction
        .add_mark(
            Position::from_utf16_offset(1),
            Position::from_utf16_offset(6),
            Mark::Link {
                href: "https://example.com".to_owned(),
                title: Some("Example".to_owned()),
            },
        )
        .expect("link must be added");

    let updated = transaction.commit();
    let json: serde_json::Value =
        serde_json::from_str(&updated.to_json().expect("document must serialize"))
            .expect("output must be valid JSON");

    assert_eq!(
        json["content"][0]["content"][0]["marks"],
        serde_json::json!([{
            "type": "link",
            "attrs": {
                "href": "https://example.com",
                "title": "Example"
            }
        }])
    );
}

#[test]
fn transaction_removes_strong_mark() {
    let document = Document::from_json(
        r#"{
            "type": "doc",
            "content": [{
                "type": "paragraph",
                "content": [{
                    "type": "text",
                    "text": "Hello",
                    "marks": [{ "type": "strong" }]
                }]
            }]
        }"#,
    )
    .expect("document must be valid");

    let mut transaction = document.transaction();

    transaction
        .remove_mark(
            Position::from_utf16_offset(1),
            Position::from_utf16_offset(6),
            Mark::Strong,
        )
        .expect("mark must be removed");

    let updated = transaction.commit();
    let json: serde_json::Value =
        serde_json::from_str(&updated.to_json().expect("document must serialize"))
            .expect("output must be valid JSON");

    assert!(json["content"][0]["content"][0].get("marks").is_none());
}
#[test]
fn transaction_add_strong_mark() {
    let document = Document::from_json(
        r#"{
            "type": "doc",
            "content": [{
                "type": "paragraph",
                "content": [{
                    "type": "text",
                    "text": "Hello"
                }]
            }]
        }"#,
    )
    .expect("document must be valid");

    let mut transaction = document.transaction();

    transaction
        .add_mark(
            Position::from_utf16_offset(1),
            Position::from_utf16_offset(6),
            Mark::Strong,
        )
        .expect("mark must be added");

    let updated = transaction.commit();
    let json: serde_json::Value =
        serde_json::from_str(&updated.to_json().expect("document must serialize"))
            .expect("output must be valid JSON");

    assert_eq!(
        json["content"][0]["content"][0]["marks"],
        serde_json::json!([{ "type": "strong" }])
    );
}
#[test]
fn transaction_invalid_position() {
    let document = Document::empty().expect("empty document must be valid");

    let mut transaction = document.transaction();

    let result = transaction.insert_text(Position::from_utf16_offset(100), "Hello");

    assert!(matches!(
        result,
        Err(DocumentError::InvalidPosition { position: 100 })
    ));
}
#[test]
fn transaction_delete_emoji() {
    let document = Document::from_json(
        r#"{
            "type": "doc",
            "content": [{
                "type": "paragraph",
                "content": [{
                    "type": "text",
                    "text": "a😊b"
                }]
            }]
        }"#,
    )
    .expect("document must be valid");

    let mut transaction = document.transaction();

    transaction
        .delete_range(
            Position::from_utf16_offset(2),
            Position::from_utf16_offset(4),
        )
        .expect("deletion must succeed");

    let updated = transaction.commit();

    assert_eq!(updated.text(), "ab");
}
