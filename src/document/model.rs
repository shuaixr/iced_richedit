use crate::document::{
    DocumentError, Mark, Position, TextSelection, schema::DocumentSchema, transaction::Transaction,
};
use prosemirror::{
    dynamic::{DynamicNode, types::Dyn},
    model::{Mark as ProseMirrorMark, Node, NodeType, ResolvedPos},
};
use std::sync::Arc;
use unicode_segmentation::UnicodeSegmentation;

pub struct Document {
    schema: Arc<DocumentSchema>,
    node: DynamicNode,
}

impl Document {
    pub fn empty() -> Result<Self, DocumentError> {
        let schema = Arc::new(DocumentSchema::new()?);
        let value = serde_json::json!({
            "type": "doc",
            "content": [
                { "type": "paragraph" }
            ]
        });
        let node: DynamicNode = schema.parse_node(&value)?;
        Ok(Self { schema, node })
    }

    pub fn text(&self) -> String {
        self.schema.scope(|| self.node.text_content())
    }

    pub fn from_json(json: &str) -> Result<Self, DocumentError> {
        let value: serde_json::Value = serde_json::from_str(json)
            .map_err(|error| DocumentError::InvalidDocument(error.to_string()))?;

        if value.get("type").and_then(serde_json::Value::as_str) != Some("doc") {
            return Err(DocumentError::InvalidDocument(
                "the root node must be doc".to_owned(),
            ));
        }

        let schema = Arc::new(DocumentSchema::new()?);
        let node = schema.parse_node(&value)?;

        Ok(Self { schema, node })
    }

    pub fn to_json(&self) -> Result<String, DocumentError> {
        self.schema.scope(|| {
            serde_json::to_string(&self.node)
                .map_err(|error| DocumentError::SerializationFailed(error.to_string()))
        })
    }

    pub fn transaction(&self) -> Transaction {
        Transaction::new(Arc::clone(&self.schema), self.node.clone())
    }
    pub(crate) fn from_parts(schema: Arc<DocumentSchema>, node: DynamicNode) -> Self {
        Self { schema, node }
    }
    pub(crate) fn initial_selection(&self) -> Result<TextSelection, DocumentError> {
        self.schema.scope(|| {
            let first_child = self.node.maybe_child(0).ok_or_else(|| {
                DocumentError::InvalidDocument("document contains no text block".to_owned())
            })?;

            if !first_child.inline_content() {
                return Err(DocumentError::InvalidDocument(
                    "the first block does not support a text cursor".to_owned(),
                ));
            }

            Ok(TextSelection::cursor(Position::from_utf16_offset(1)))
        })
    }

    pub(crate) fn validate_selection(&self, selection: TextSelection) -> Result<(), DocumentError> {
        self.schema.scope(|| {
            for position in [selection.anchor(), selection.head()] {
                let offset = position.utf16_offset();

                let resolved = ResolvedPos::<Dyn>::resolve(&self.node, offset)
                    .map_err(|_| DocumentError::InvalidPosition { position: offset })?;

                if !resolved.parent().inline_content() {
                    return Err(DocumentError::InvalidPosition { position: offset });
                }

                let content_start = resolved.start(resolved.depth);
                let local_offset = offset - content_start;

                let text = Self::navigation_text(resolved.parent())?;

                let mut boundary = 0;
                let mut valid = local_offset == 0;

                for grapheme in text.graphemes(true) {
                    boundary += grapheme.encode_utf16().count();

                    if boundary == local_offset {
                        valid = true;
                        break;
                    }

                    if boundary > local_offset {
                        break;
                    }
                }

                if !valid {
                    return Err(DocumentError::InvalidPosition { position: offset });
                }
            }

            Ok(())
        })
    }
    pub(crate) fn marks_at(&self, position: Position) -> Result<Vec<Mark>, DocumentError> {
        self.schema.scope(|| {
            let offset = position.utf16_offset();

            let resolved = ResolvedPos::<Dyn>::resolve(&self.node, offset)
                .map_err(|_| DocumentError::InvalidPosition { position: offset })?;

            if !resolved.parent().inline_content() {
                return Err(DocumentError::InvalidPosition { position: offset });
            }

            let marks = resolved.marks();

            marks
                .iter()
                .map(|mark| self.schema.public_mark(mark))
                .collect()
        })
    }

    pub(crate) fn normalize_marks(&self, marks: &[Mark]) -> Result<Vec<Mark>, DocumentError> {
        self.schema.scope(|| {
            let set = self.schema.mark_set(marks)?;

            set.iter()
                .map(|mark| self.schema.public_mark(mark))
                .collect()
        })
    }

    pub(crate) fn range_has_mark(
        &self,
        selection: TextSelection,
        mark: &Mark,
    ) -> Result<bool, DocumentError> {
        self.validate_selection(selection)?;

        if selection.is_empty() {
            return Ok(false);
        }

        self.schema.scope(|| {
            let dynamic_mark = self.schema.mark(mark)?;
            let mark_type = dynamic_mark.r#type();

            let from = selection.from().utf16_offset();
            let to = selection.to().utf16_offset();

            let mut found_inline = false;
            let mut all_marked = true;

            self.node.nodes_between(
                from,
                to,
                &mut |node, position, parent, _index| {
                    if !all_marked {
                        return false;
                    }

                    if !node.is_inline() {
                        return true;
                    }

                    // Only inspect inline nodes that overlap the selected range.
                    if position >= to || position + node.node_size() <= from {
                        return false;
                    }

                    let Some(parent) = parent else {
                        return false;
                    };

                    if !parent.r#type().allows_mark_type(mark_type) {
                        return false;
                    }

                    found_inline = true;

                    if !node
                        .marks()
                        .is_some_and(|marks| marks.contains(&dynamic_mark))
                    {
                        all_marked = false;
                    }

                    false
                },
                0,
            );

            Ok(found_inline && all_marked)
        })
    }

    pub(crate) fn previous_cursor_position(
        &self,
        position: Position,
    ) -> Result<Option<Position>, DocumentError> {
        self.adjacent_cursor_position(position, false)
    }

    pub(crate) fn next_cursor_position(
        &self,
        position: Position,
    ) -> Result<Option<Position>, DocumentError> {
        self.adjacent_cursor_position(position, true)
    }

    fn adjacent_cursor_position(
        &self,
        position: Position,
        forward: bool,
    ) -> Result<Option<Position>, DocumentError> {
        self.validate_selection(TextSelection::cursor(position))?;

        let current = position.utf16_offset();

        self.schema.scope(|| {
            let mut adjacent: Option<usize> = None;

            let mut consider = |candidate: usize| {
                if forward {
                    if candidate > current && adjacent.is_none_or(|previous| candidate < previous) {
                        adjacent = Some(candidate);
                    }
                } else if candidate < current
                    && adjacent.is_none_or(|previous| candidate > previous)
                {
                    adjacent = Some(candidate);
                }
            };

            let mut block_start = 0;

            for block_index in 0..self.node.child_count() {
                let block = self.node.maybe_child(block_index).ok_or_else(|| {
                    DocumentError::InvalidDocument("document block is missing".to_owned())
                })?;

                if !block.is_textblock() {
                    return Err(DocumentError::InvalidDocument(
                        "cursor navigation requires top-level text blocks".to_owned(),
                    ));
                }

                let text = Self::navigation_text(block)?;

                let content_start = block_start + 1;
                let mut utf16_offset = 0;

                // An empty paragraph still has one valid cursor position.
                consider(content_start);

                for grapheme in text.graphemes(true) {
                    utf16_offset += grapheme.encode_utf16().count();
                    consider(content_start + utf16_offset);
                }

                block_start += block.node_size();
            }

            Ok(adjacent.map(Position::from_utf16_offset))
        })
    }

    pub(crate) fn cursor_at_paragraph_boundary(
        &self,
        position: Position,
        forward: bool,
    ) -> Result<bool, DocumentError> {
        self.validate_selection(TextSelection::cursor(position))?;

        let offset = position.utf16_offset();

        self.schema.scope(|| {
            let resolved = ResolvedPos::<Dyn>::resolve(&self.node, offset)
                .map_err(|_| DocumentError::InvalidPosition { position: offset })?;

            if resolved.depth != 1 || !resolved.parent().is_textblock() {
                return Err(DocumentError::InvalidDocument(
                    "paragraph deletion requires a top-level text block".to_owned(),
                ));
            }

            let boundary = if forward {
                resolved.end(resolved.depth)
            } else {
                resolved.start(resolved.depth)
            };

            Ok(offset == boundary)
        })
    }

    fn navigation_text(block: &DynamicNode) -> Result<String, DocumentError> {
        let mut text = String::new();

        for index in 0..block.child_count() {
            let inline = block.maybe_child(index).ok_or_else(|| {
                DocumentError::InvalidDocument("text block child is missing".to_owned())
            })?;

            if inline.is_text() {
                text.push_str(&inline.text_content());
            } else if inline.is_leaf() && inline.is_inline() && inline.node_size() == 1 {
                // The current schema's only inline leaf is hard_break.
                text.push('\n');
            } else {
                return Err(DocumentError::InvalidDocument(
                    "cursor navigation encountered unsupported inline content".to_owned(),
                ));
            }
        }

        Ok(text)
    }

    pub(crate) fn snap_cursor_forward(
        &self,
        position: Position,
    ) -> Result<Position, DocumentError> {
        let offset = position.utf16_offset();

        self.schema.scope(|| {
            let resolved = ResolvedPos::<Dyn>::resolve(&self.node, offset)
                .map_err(|_| DocumentError::InvalidPosition { position: offset })?;

            if !resolved.parent().inline_content() {
                return Err(DocumentError::InvalidPosition { position: offset });
            }

            let content_start = resolved.start(resolved.depth);
            let local_offset = offset - content_start;

            if local_offset == 0 {
                return Ok(position);
            }

            let text = Self::navigation_text(resolved.parent())?;
            let mut boundary = 0;

            for grapheme in text.graphemes(true) {
                boundary += grapheme.encode_utf16().count();

                if boundary >= local_offset {
                    return Ok(Position::from_utf16_offset(content_start + boundary));
                }
            }

            Err(DocumentError::InvalidPosition { position: offset })
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::document::{DocumentError, Mark, Position};

    use super::Document;

    #[test]
    fn empty_document_starts_with_cursor_in_first_paragraph() {
        let document = Document::empty().expect("empty document must be valid");
        let selection = document
            .initial_selection()
            .expect("empty document must have an initial selection");
        assert_eq!(selection.anchor(), super::Position::from_utf16_offset(1));
        assert_eq!(selection.head(), super::Position::from_utf16_offset(1));
    }
    #[test]
    fn initial_selection_stays_in_first_empty_paragraph() {
        let document = Document::from_json(
            r#"{
                "type": "doc",
                "content": [
                    { "type": "paragraph" },
                    {
                        "type": "paragraph",
                        "content": [{
                            "type": "text",
                            "text": "Hello"
                        }]
                    }
                ]
            }"#,
        )
        .expect("document must be valid");

        let selection = document
            .initial_selection()
            .expect("document must have an initial selection");
        assert_eq!(selection.anchor(), super::Position::from_utf16_offset(1));
        assert_eq!(selection.head(), super::Position::from_utf16_offset(1));
    }
    #[test]
    fn validate_selection_rejects_block_boundary() {
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

        let selection = super::TextSelection::new(
            super::Position::from_utf16_offset(0),
            super::Position::from_utf16_offset(5),
        );

        assert!(matches!(
            document.validate_selection(selection),
            Err(DocumentError::InvalidPosition { position: 0 })
        ));
    }
    #[test]
    fn validate_selection_rejects_out_of_bounds_head() {
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

        let selection = super::TextSelection::new(
            super::Position::from_utf16_offset(1),
            super::Position::from_utf16_offset(10),
        );

        assert!(matches!(
            document.validate_selection(selection),
            Err(DocumentError::InvalidPosition { position: 10 })
        ));
    }

    #[test]
    fn marks_at_returns_strong_inside_marked_text() {
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

        let marks = document
            .marks_at(Position::from_utf16_offset(2))
            .expect("marks_at must succeed");

        assert_eq!(marks, vec![Mark::Strong]);
    }

    #[test]
    fn marks_at_preserves_link_attrs_and_excludes_non_inclusive_end() {
        let document = Document::from_json(
            r#"{
                "type": "doc",
                "content": [{
                    "type": "paragraph",
                    "content": [{
                        "type": "text",
                        "text": "Hello",
                        "marks": [{
                            "type": "link",
                            "attrs": { "href": "https://example.com", "title": null }
                        }]
                    }]
                }]
            }"#,
        )
        .expect("document must be valid");

        let marks = document
            .marks_at(Position::from_utf16_offset(5))
            .expect("marks_at must succeed");
        assert!(
            document
                .marks_at(Position::from_utf16_offset(6))
                .expect("link end position must be valid")
                .is_empty()
        );
        assert_eq!(
            marks,
            vec![Mark::Link {
                href: "https://example.com".to_owned(),
                title: None
            }]
        );
    }

    #[test]
    fn validate_selection_rejects_positions_inside_graphemes() {
        for text in ["😀", "e\u{301}"] {
            let json = serde_json::json!({
                "type": "doc",
                "content": [{
                    "type": "paragraph",
                    "content": [{
                        "type": "text",
                        "text": text
                    }]
                }]
            });

            let document = Document::from_json(&json.to_string()).expect("document must be valid");

            let start = super::Position::from_utf16_offset(1);
            let inside = super::Position::from_utf16_offset(2);
            let end = super::Position::from_utf16_offset(3);

            document
                .validate_selection(super::TextSelection::cursor(start))
                .expect("grapheme start must be valid");

            document
                .validate_selection(super::TextSelection::cursor(end))
                .expect("grapheme end must be valid");

            for selection in [
                super::TextSelection::cursor(inside),
                super::TextSelection::new(start, inside),
                super::TextSelection::new(inside, end),
            ] {
                assert!(matches!(
                    document.validate_selection(selection),
                    Err(DocumentError::InvalidPosition { position: 2 })
                ));
            }
        }
    }
}
