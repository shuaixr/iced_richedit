use crate::document::{
    DocumentError, Mark, ParagraphView, Position, TextSelection,
    schema::DocumentSchema,
    text_block::text_blocks,
    transaction::Transaction,
    view::{MarkedRange, append_text_run},
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
            let blocks = text_blocks(&self.node)?;

            let first = blocks.first().ok_or_else(|| {
                DocumentError::InvalidDocument("document contains no text block".to_owned())
            })?;

            Ok(TextSelection::cursor(Position::from_utf16_offset(
                first.content_start(),
            )))
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
            for source in text_blocks(&self.node)? {
                if source.depth != 1 {
                    return Err(DocumentError::InvalidDocument(
                        "cursor navigation requires top-level text blocks".to_owned(),
                    ));
                }

                let text = Self::navigation_text(source.node)?;
                let content_start = source.content_start();
                let mut utf16_offset = 0;

                // An empty text block still has one valid cursor position.
                consider(content_start);

                for grapheme in text.graphemes(true) {
                    utf16_offset += grapheme.encode_utf16().count();
                    consider(content_start + utf16_offset);
                }
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

    fn paragraph_text_runs(
        &self,
        block: &DynamicNode,
    ) -> Result<(String, Vec<MarkedRange>), DocumentError> {
        let mut text = String::new();
        let mut runs = Vec::new();

        for index in 0..block.child_count() {
            let inline = block.maybe_child(index).ok_or_else(|| {
                DocumentError::InvalidDocument("text block child is missing".to_owned())
            })?;

            let start = text.len();

            if inline.is_text() {
                text.push_str(&inline.text_content());
            } else if inline.is_leaf() && inline.is_inline() && inline.node_size() == 1 {
                // The current schema's only inline leaf is hard_break.
                text.push('\n');
            } else {
                return Err(DocumentError::InvalidDocument(
                    "paragraph projection encountered unsupported inline content".to_owned(),
                ));
            }

            let marks = inline
                .marks()
                .into_iter()
                .flat_map(|marks| marks.iter())
                .map(|mark| self.schema.public_mark(mark))
                .collect::<Result<Vec<_>, DocumentError>>()?;

            append_text_run(&mut runs, start..text.len(), &marks);
        }

        Ok((text, runs))
    }
    pub(crate) fn paragraph_views(&self) -> Result<Vec<ParagraphView>, DocumentError> {
        self.schema.scope(|| {
            let blocks = text_blocks(&self.node)?;
            let mut views = Vec::with_capacity(blocks.len());

            for source in blocks {
                if source.depth != 1 {
                    return Err(DocumentError::InvalidDocument(
                        "paragraph projection requires top-level text blocks".to_owned(),
                    ));
                }

                let (text, runs) = self.paragraph_text_runs(source.node)?;

                views.push(ParagraphView::with_runs(
                    Position::from_utf16_offset(source.content_start()),
                    text,
                    runs,
                ));
            }

            Ok(views)
        })
    }

    pub(crate) fn full_selection(&self) -> Result<TextSelection, DocumentError> {
        let selection = self.schema.scope(|| {
            let blocks = text_blocks(&self.node)?;

            let first = blocks.first().ok_or_else(|| {
                DocumentError::InvalidDocument("document contains no selectable content".to_owned())
            })?;

            let last = blocks
                .last()
                .expect("a first text block must also have a last block");

            Ok(TextSelection::new(
                Position::from_utf16_offset(first.content_start()),
                Position::from_utf16_offset(last.content_end()),
            ))
        })?;

        self.validate_selection(selection)?;

        Ok(selection)
    }

    pub(crate) fn selection_text(&self, selection: TextSelection) -> Result<String, DocumentError> {
        self.validate_selection(selection)?;

        if selection.is_empty() {
            return Ok(String::new());
        }

        let from = selection.from().utf16_offset();
        let to = selection.to().utf16_offset();

        let mut parts = Vec::new();

        for view in self.paragraph_views()? {
            let start = view.start().utf16_offset();
            let length = view.text().encode_utf16().count();
            let end = start + length;

            if to < start || from > end {
                continue;
            }

            let local_from = from.max(start) - start;
            let local_to = to.min(end) - start;

            let units: Vec<u16> = view
                .text()
                .encode_utf16()
                .skip(local_from)
                .take(local_to - local_from)
                .collect();

            let text = String::from_utf16(&units).map_err(|_| {
                DocumentError::InvalidDocument(
                    "selected text contains an invalid UTF-16 boundary".to_owned(),
                )
            })?;

            parts.push(text);
        }

        Ok(parts.join("\n"))
    }

    pub(crate) fn adjacent_word_position(
        &self,
        position: Position,
        forward: bool,
    ) -> Result<Option<Position>, DocumentError> {
        if self.cursor_at_paragraph_boundary(position, forward)? {
            return if forward {
                self.next_cursor_position(position)
            } else {
                self.previous_cursor_position(position)
            };
        }

        self.schema.scope(|| {
            let offset = position.utf16_offset();

            let resolved = ResolvedPos::<Dyn>::resolve(&self.node, offset)
                .map_err(|_| DocumentError::InvalidPosition { position: offset })?;

            let content_start = resolved.start(resolved.depth);
            let local_offset = offset - content_start;
            let text = Self::navigation_text(resolved.parent())?;

            let mut byte = 0;
            let mut utf16_offset = 0;

            for character in text.chars() {
                if utf16_offset >= local_offset {
                    break;
                }

                byte += character.len_utf8();
                utf16_offset += character.len_utf16();
            }

            let target_byte = if forward {
                text[byte..]
                    .split_word_bound_indices()
                    .find(|(_, segment)| !segment.trim().is_empty())
                    .map(|(index, segment)| byte + index + segment.len())
                    .unwrap_or(text.len())
            } else {
                text[..byte]
                    .split_word_bound_indices()
                    .rfind(|(_, segment)| !segment.trim().is_empty())
                    .map(|(index, _)| index)
                    .unwrap_or(0)
            };

            // Word boundaries must not produce a cursor inside a grapheme.
            let mut boundaries = text
                .grapheme_indices(true)
                .map(|(index, _)| index)
                .chain(std::iter::once(text.len()));

            let target_byte = if forward {
                boundaries.find(|&index| index >= target_byte)
            } else {
                boundaries.take_while(|&index| index <= target_byte).last()
            }
            .ok_or_else(|| {
                DocumentError::InvalidDocument(
                    "word navigation found no valid grapheme boundary".to_owned(),
                )
            })?;

            let target_offset = text[..target_byte].encode_utf16().count();

            Ok(Some(Position::from_utf16_offset(
                content_start + target_offset,
            )))
        })
    }

    pub(crate) fn composition_paragraph_views(
        &self,
        selection: TextSelection,
        preedit: &str,
        cursor_byte: Option<usize>,
        stored_marks: Option<&[Mark]>,
    ) -> Result<Vec<ParagraphView>, DocumentError> {
        self.validate_selection(selection)?;

        let mut views = self.paragraph_views()?;

        let contains = |view: &ParagraphView, position: Position| {
            let start = view.start().utf16_offset();
            let end = start + view.text().encode_utf16().count();
            let offset = position.utf16_offset();

            start <= offset && offset <= end
        };

        let first = views
            .iter()
            .position(|view| contains(view, selection.from()))
            .ok_or_else(|| {
                DocumentError::InvalidDocument("composition start paragraph is missing".to_owned())
            })?;

        let last = views
            .iter()
            .position(|view| contains(view, selection.to()))
            .ok_or_else(|| {
                DocumentError::InvalidDocument("composition end paragraph is missing".to_owned())
            })?;

        let inherited_marks;

        let preedit_marks = match stored_marks {
            Some(marks) => marks,
            None => {
                inherited_marks = self.marks_at(selection.from())?;
                &inherited_marks
            }
        };

        let preedit_marks = self.normalize_marks(preedit_marks)?;

        let composed = ParagraphView::compose(
            &views[first],
            &views[last],
            selection,
            preedit,
            cursor_byte,
            &preedit_marks,
        )?;
        views.splice(first..=last, std::iter::once(composed));

        Ok(views)
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
