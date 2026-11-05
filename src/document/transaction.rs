use std::sync::Arc;

use prosemirror::transform::Step;
use prosemirror::transform::structure::can_join;
use prosemirror::transform::transform::StepError as TransformStepError;
use prosemirror::{
    dynamic::{DynamicNode, types::Dyn},
    model::{Fragment, Node, NodeType, ResolvedPos},
    transform::{MarkOrType, Transform},
};

use crate::document::text_block::text_blocks;
use crate::document::{Document, DocumentError, Mark, Position, schema::DocumentSchema};
use crate::document::{TextBlockKind, TextSelection};

pub struct Transaction {
    schema: Arc<DocumentSchema>,
    transform: Transform<Dyn>,
}

impl Transaction {
    pub(super) fn new(schema: Arc<DocumentSchema>, document: DynamicNode) -> Self {
        let transform = schema.scope(|| Transform::new(document));

        Self { schema, transform }
    }

    pub fn commit(self) -> Document {
        self.finish().into_document()
    }

    fn validate_position(&self, position: Position) -> Result<usize, DocumentError> {
        let offset = position.utf16_offset();

        self.schema.scope(|| {
            ResolvedPos::<Dyn>::resolve(&self.transform.doc, offset)
                .map(|_| offset)
                .map_err(|_| DocumentError::InvalidPosition { position: offset })
        })
    }

    pub(crate) fn is_code_block_at(&self, position: Position) -> Result<bool, DocumentError> {
        let offset = position.utf16_offset();

        self.schema.scope(|| {
            let resolved = ResolvedPos::<Dyn>::resolve(&self.transform.doc, offset)
                .map_err(|_| DocumentError::InvalidPosition { position: offset })?;

            if resolved.depth != 1 || !resolved.parent().is_textblock() {
                return Err(DocumentError::InvalidPosition { position: offset });
            }

            Ok(TextBlockKind::from_node(resolved.parent())? == TextBlockKind::CodeBlock)
        })
    }

    pub fn insert_text(
        &mut self,
        position: Position,
        text: &str,
    ) -> Result<&mut Self, DocumentError> {
        self.insert_text_with_marks(position, text, &[])
    }

    pub(crate) fn insert_text_with_marks(
        &mut self,
        position: Position,
        text: &str,
        marks: &[Mark],
    ) -> Result<&mut Self, DocumentError> {
        if text.is_empty() {
            return Ok(self);
        }

        let offset = self.validate_position(position)?;

        if !marks.is_empty() && self.is_code_block_at(position)? {
            return Err(DocumentError::TransformFailed(
                "marked text cannot be inserted into a code block".to_owned(),
            ));
        }

        let text_node = self.schema.text_node_with_marks(text, marks)?;
        let content = Fragment::from(vec![text_node]);
        let previous_step_count = self.transform.steps.len();

        self.schema.scope(|| {
            self.transform.insert(offset, content);
        });

        if self.transform.steps.len() == previous_step_count {
            return Err(DocumentError::TransformFailed(format!(
                "text could not be inserted at position {offset}"
            )));
        }

        Ok(self)
    }

    pub fn delete_range(
        &mut self,
        from: Position,
        to: Position,
    ) -> Result<&mut Self, DocumentError> {
        let from = self.validate_position(from)?;
        let to = self.validate_position(to)?;

        if from > to {
            return Err(DocumentError::TransformFailed(format!(
                "range start {from} is after range end {to}"
            )));
        }

        if from == to {
            return Ok(self);
        }

        let previous_step_count = self.transform.steps.len();

        self.schema.scope(|| {
            self.transform.delete(from, to);
        });

        if self.transform.steps.len() == previous_step_count {
            return Err(DocumentError::TransformFailed(format!(
                "range {from}..{to} could not be deleted"
            )));
        }

        Ok(self)
    }

    fn ensure_inline_formatting_allowed(
        &self,
        from: usize,
        to: usize,
    ) -> Result<(), DocumentError> {
        self.schema.scope(|| {
            super::text_block::ensure_inline_formatting_allowed(&self.transform.doc, from, to)
        })
    }

    pub fn add_mark(
        &mut self,
        from: Position,
        to: Position,
        mark: Mark,
    ) -> Result<&mut Self, DocumentError> {
        let from = self.validate_position(from)?;
        let to = self.validate_position(to)?;

        if from > to {
            return Err(DocumentError::TransformFailed(format!(
                "range start {from} is after range end {to}"
            )));
        }

        self.ensure_inline_formatting_allowed(from, to)?;

        if from == to {
            return Ok(self);
        }

        let dynamic_mark = self.schema.mark(&mark)?;

        self.schema.scope(|| {
            self.transform.add_mark(from, to, dynamic_mark);
        });

        Ok(self)
    }

    pub fn remove_mark(
        &mut self,
        from: Position,
        to: Position,
        mark: Mark,
    ) -> Result<&mut Self, DocumentError> {
        let from = self.validate_position(from)?;
        let to = self.validate_position(to)?;

        if from > to {
            return Err(DocumentError::TransformFailed(format!(
                "range start {from} is after range end {to}"
            )));
        }

        self.ensure_inline_formatting_allowed(from, to)?;

        if from == to {
            return Ok(self);
        }

        let dynamic_mark = self.schema.mark(&mark)?;

        self.schema.scope(|| {
            self.transform
                .remove_mark(from, to, Some(MarkOrType::Mark(dynamic_mark)));
        });

        Ok(self)
    }

    pub(crate) fn finish(self) -> TransactionResult {
        let change = if self.transform.steps.is_empty() {
            None
        } else {
            let inverse = self.schema.scope(|| {
                self.transform
                    .steps
                    .iter()
                    .zip(self.transform.docs.iter())
                    .rev()
                    .map(|(step, document_before_step)| step.invert(document_before_step))
                    .collect()
            });

            Some(DocumentChange {
                schema: Arc::clone(&self.schema),
                forward: self.transform.steps,
                inverse,
            })
        };

        let document = Document::from_parts(self.schema, self.transform.doc);

        TransactionResult { document, change }
    }

    pub(crate) fn remove_link(
        &mut self,
        from: Position,
        to: Position,
    ) -> Result<&mut Self, DocumentError> {
        let from = self.validate_position(from)?;
        let to = self.validate_position(to)?;

        if from > to {
            return Err(DocumentError::TransformFailed(format!(
                "range start {from} is after range end {to}"
            )));
        }
        self.ensure_inline_formatting_allowed(from, to)?;

        if from == to {
            return Ok(self);
        }

        let link_type = self.schema.link_type()?;

        self.schema.scope(|| {
            self.transform
                .remove_mark(from, to, Some(MarkOrType::MarkType(link_type)));
        });

        Ok(self)
    }

    pub(crate) fn split_paragraph(
        &mut self,
        position: Position,
    ) -> Result<&mut Self, DocumentError> {
        let offset = self.validate_position(position)?;

        self.schema.scope(|| {
            let resolved = ResolvedPos::<Dyn>::resolve(&self.transform.doc, offset)
                .map_err(|_| DocumentError::InvalidPosition { position: offset })?;

            if !resolved.parent().is_textblock() {
                return Err(DocumentError::InvalidPosition { position: offset });
            }

            if TextBlockKind::from_node(resolved.parent())? == TextBlockKind::CodeBlock {
                return Err(DocumentError::TransformFailed(
                    "code blocks cannot be split into separate blocks".to_owned(),
                ));
            }

            self.transform
                .split(offset, Some(1), None)
                .map_err(|error| match error {
                    TransformStepError::ApplyFailed(message) => {
                        DocumentError::TransformFailed(message)
                    }
                })?;

            Ok(())
        })?;

        Ok(self)
    }

    pub(crate) fn split_text_block_for_enter(
        &mut self,
        position: Position,
    ) -> Result<Position, DocumentError> {
        let offset = self.validate_position(position)?;

        let (kind, start, end) = self.schema.scope(|| {
            let resolved = ResolvedPos::<Dyn>::resolve(&self.transform.doc, offset)
                .map_err(|_| DocumentError::InvalidPosition { position: offset })?;

            if resolved.depth != 1 || !resolved.parent().is_textblock() {
                return Err(DocumentError::InvalidDocument(
                    "Enter currently requires a top-level text block".to_owned(),
                ));
            }

            Ok((
                TextBlockKind::from_node(resolved.parent())?,
                resolved.start(resolved.depth),
                resolved.end(resolved.depth),
            ))
        })?;
        if kind == TextBlockKind::CodeBlock {
            self.insert_text_with_marks(position, "\n", &[])?;

            return Ok(Position::from_utf16_offset(offset + 1));
        }
        if matches!(kind, TextBlockKind::Heading { .. }) && start == end {
            self.set_text_block_kind(TextSelection::cursor(position), TextBlockKind::Paragraph)?;

            return Ok(position);
        }

        self.split_paragraph(position)?;

        let cursor = Position::from_utf16_offset(offset + 2);

        if matches!(kind, TextBlockKind::Heading { .. }) {
            let paragraph_cursor = if offset == start {
                Some(position)
            } else if offset == end {
                Some(cursor)
            } else {
                None
            };

            if let Some(paragraph_cursor) = paragraph_cursor {
                self.set_text_block_kind(
                    TextSelection::cursor(paragraph_cursor),
                    TextBlockKind::Paragraph,
                )?;
            }
        }

        Ok(cursor)
    }

    fn apply_steps(&mut self, steps: &[Step<Dyn>]) -> Result<(), DocumentError> {
        self.schema.scope(|| {
            for step in steps {
                self.transform
                    .step(step.clone())
                    .map_err(|error| match error {
                        TransformStepError::ApplyFailed(message) => {
                            DocumentError::TransformFailed(message)
                        }
                    })?;
            }

            Ok(())
        })
    }

    pub(crate) fn reset_text_block_at_start(
        &mut self,
        position: Position,
    ) -> Result<bool, DocumentError> {
        let offset = self.validate_position(position)?;

        let should_convert = self.schema.scope(|| {
            let resolved = ResolvedPos::<Dyn>::resolve(&self.transform.doc, offset)
                .map_err(|_| DocumentError::InvalidPosition { position: offset })?;

            if resolved.depth != 1 || !resolved.parent().is_textblock() {
                return Err(DocumentError::InvalidDocument(
                    "backward deletion currently requires a top-level text block".to_owned(),
                ));
            }

            let at_start = offset == resolved.start(resolved.depth);

            let should_reset = match TextBlockKind::from_node(resolved.parent())? {
                TextBlockKind::Heading { .. } => true,
                TextBlockKind::CodeBlock => offset == 1 || resolved.parent().content_size() == 0,
                TextBlockKind::Paragraph => false,
            };

            Ok(at_start && should_reset)
        })?;

        if !should_convert {
            return Ok(false);
        }

        self.set_text_block_kind(TextSelection::cursor(position), TextBlockKind::Paragraph)?;

        Ok(true)
    }

    pub(crate) fn join_paragraphs(
        &mut self,
        position: Position,
    ) -> Result<&mut Self, DocumentError> {
        let offset = position.utf16_offset();
        let previous_step_count = self.transform.steps.len();

        self.schema.scope(|| {
            let resolved = ResolvedPos::<Dyn>::resolve(&self.transform.doc, offset)
                .map_err(|_| DocumentError::InvalidPosition { position: offset })?;

            if resolved.depth != 0 {
                return Err(DocumentError::TransformFailed(
                    "joining currently requires a boundary between top-level text blocks"
                        .to_owned(),
                ));
            }

            let index = resolved.index(0);

            let left = index
                .checked_sub(1)
                .and_then(|index| self.transform.doc.maybe_child(index));

            let right = self.transform.doc.maybe_child(index);

            let (Some(left), Some(right)) = (left, right) else {
                return Err(DocumentError::TransformFailed(
                    "joining requires text blocks on both sides".to_owned(),
                ));
            };

            if !left.is_textblock()
                || !right.is_textblock()
                || left.is_atom()
                || right.is_atom()
                || left.r#type().is_isolating()
                || right.r#type().is_isolating()
            {
                return Err(DocumentError::TransformFailed(
                    "joining requires non-atomic, non-isolating text blocks".to_owned(),
                ));
            }

            let left_kind = TextBlockKind::from_node(left)?;
            let right_kind = TextBlockKind::from_node(right)?;

            if left_kind == TextBlockKind::CodeBlock || right_kind == TextBlockKind::CodeBlock {
                left.check().map_err(DocumentError::InvalidDocument)?;
                right.check().map_err(DocumentError::InvalidDocument)?;

                let right_content = Self::converted_text_content(&self.schema, right, left_kind)?;

                if right_content.size() != right.content_size() {
                    return Err(DocumentError::TransformFailed(
                        "joining must preserve converted content positions".to_owned(),
                    ));
                }

                let replacement = left.copy(|content| content.clone().append(right_content));

                replacement
                    .check()
                    .map_err(DocumentError::InvalidDocument)?;

                let from = offset - left.node_size();
                let to = offset + right.node_size();

                self.transform
                    .replace_with(from, to, Fragment::from(vec![replacement]));
            } else {
                if can_join::<Dyn>(&self.transform.doc, offset) != Some(true) {
                    return Err(DocumentError::TransformFailed(format!(
                        "paragraphs cannot be joined at position {offset}"
                    )));
                }

                self.transform.join(offset, Some(1));
            }

            if self.transform.steps.len() == previous_step_count {
                return Err(DocumentError::TransformFailed(format!(
                    "paragraphs could not be joined at position {offset}"
                )));
            }

            Ok(())
        })?;

        Ok(self)
    }

    pub(crate) fn replace_plain_text(
        &mut self,
        from: Position,
        to: Position,
        text: &str,
        marks: &[Mark],
    ) -> Result<Position, DocumentError> {
        if text.is_empty() {
            return Ok(from);
        }

        // CRLF is one separator; a remaining CR is another separator.
        let normalized = text.replace("\r\n", "\n").replace('\r', "\n");

        self.delete_range(from, to)?;

        if self.is_code_block_at(from)? {
            self.insert_text_with_marks(from, &normalized, &[])?;

            return Ok(Position::from_utf16_offset(
                from.utf16_offset() + normalized.encode_utf16().count(),
            ));
        }

        let mut cursor = from;

        for (index, part) in normalized.split('\n').enumerate() {
            if index > 0 {
                self.split_paragraph(cursor)?;

                cursor = Position::from_utf16_offset(cursor.utf16_offset() + 2);
            }

            self.insert_text_with_marks(cursor, part, marks)?;

            cursor =
                Position::from_utf16_offset(cursor.utf16_offset() + part.encode_utf16().count());
        }

        Ok(cursor)
    }

    fn converted_text_content(
        schema: &DocumentSchema,
        node: &DynamicNode,
        kind: TextBlockKind,
    ) -> Result<Fragment<Dyn>, DocumentError> {
        let mut text = String::new();

        for index in 0..node.child_count() {
            let child = node.maybe_child(index).ok_or_else(|| {
                DocumentError::InvalidDocument("text block child is missing".to_owned())
            })?;

            if child.is_text() {
                text.push_str(&child.text_content());
            } else if child.r#type().name() == "hard_break" {
                text.push('\n');
            } else {
                return Err(DocumentError::InvalidDocument(
                    "block conversion encountered unsupported inline content".to_owned(),
                ));
            }
        }

        let mut children = Vec::new();

        if kind == TextBlockKind::CodeBlock {
            if !text.is_empty() {
                children.push(schema.text_node_with_marks(&text, &[])?);
            }
        } else {
            for (index, part) in text.split('\n').enumerate() {
                if index > 0 {
                    children.push(schema.hard_break_node()?);
                }

                if !part.is_empty() {
                    children.push(schema.text_node_with_marks(part, &[])?);
                }
            }
        }

        Ok(Fragment::from(children))
    }

    pub(crate) fn set_text_block_kind(
        &mut self,
        selection: TextSelection,
        kind: TextBlockKind,
    ) -> Result<&mut Self, DocumentError> {
        let from = self.validate_position(selection.from())?;
        let to = self.validate_position(selection.to())?;

        let attrs = match kind {
            TextBlockKind::Paragraph | TextBlockKind::CodeBlock => {
                serde_json::json!({})
            }
            TextBlockKind::Heading { level } => {
                TextBlockKind::heading(level)?;
                serde_json::json!({
                    "level": level
                })
            }
        };

        self.schema.scope(|| {
            let target_type = self.schema.text_block_type(kind)?;
            let mut targets = Vec::new();
            let mut selected = false;

            for source in text_blocks(&self.transform.doc)? {
                if !source.is_touched_by(from, to) {
                    continue;
                }

                if source.depth != 1 {
                    return Err(DocumentError::TransformFailed(
                        "block conversion currently requires top-level text blocks".to_owned(),
                    ));
                }

                selected = true;

                let source_kind = TextBlockKind::from_node(source.node)?;

                source
                    .node
                    .check()
                    .map_err(DocumentError::InvalidDocument)?;

                if source_kind == kind {
                    continue;
                }

                let replacement = if source_kind == TextBlockKind::CodeBlock
                    || kind == TextBlockKind::CodeBlock
                {
                    let content = Self::converted_text_content(&self.schema, source.node, kind)?;

                    let node =
                        target_type.create(attrs.clone(), Some(&content), source.node.marks());

                    node.check().map_err(DocumentError::InvalidDocument)?;

                    if node.node_size() != source.node.node_size() {
                        return Err(DocumentError::TransformFailed(
                            "block conversion must preserve content positions".to_owned(),
                        ));
                    }

                    Some(node)
                } else {
                    None
                };

                targets.push((source.before(), source.node.node_size(), replacement));
            }

            if !selected {
                return Err(DocumentError::InvalidPosition { position: from });
            }

            for (before, size, replacement) in targets {
                let previous = self.transform.steps.len();

                if let Some(node) = replacement {
                    self.transform
                        .replace_with(before, before + size, Fragment::from(vec![node]));
                } else {
                    self.transform.set_node_markup(
                        before,
                        Some(target_type),
                        Some(attrs.clone()),
                        None,
                    );
                }

                if self.transform.steps.len() == previous {
                    return Err(DocumentError::TransformFailed(format!(
                        "text block could not be converted at position {before}"
                    )));
                }
            }

            Ok(())
        })?;

        Ok(self)
    }
}
pub(crate) struct TransactionResult {
    document: Document,
    change: Option<DocumentChange>,
}

impl TransactionResult {
    pub(crate) fn into_document(self) -> Document {
        self.document
    }
    pub(crate) fn into_parts(self) -> (Document, Option<DocumentChange>) {
        (self.document, self.change)
    }
    pub(crate) fn document(&self) -> &Document {
        &self.document
    }
}

pub(crate) struct DocumentChange {
    schema: Arc<DocumentSchema>,
    forward: Vec<Step<Dyn>>,
    inverse: Vec<Step<Dyn>>,
}

impl DocumentChange {
    pub(crate) fn append(&mut self, next: DocumentChange) -> Result<(), DocumentError> {
        if !Arc::ptr_eq(&self.schema, &next.schema) {
            return Err(DocumentError::TransformFailed(
                "cannot merge changes from different document schemas".to_owned(),
            ));
        }

        let DocumentChange {
            forward,
            mut inverse,
            ..
        } = next;

        self.forward.extend(forward);

        inverse.append(&mut self.inverse);
        self.inverse = inverse;

        Ok(())
    }

    fn apply_steps(
        &self,
        document: &Document,
        steps: &[Step<Dyn>],
    ) -> Result<Document, DocumentError> {
        let mut transaction = document.transaction();
        if !Arc::ptr_eq(&self.schema, &transaction.schema) {
            return Err(DocumentError::TransformFailed(
                "change belongs to a different document schema".to_owned(),
            ));
        }
        transaction.apply_steps(steps)?;

        Ok(transaction.commit())
    }

    pub(crate) fn apply_forward(&self, document: &Document) -> Result<Document, DocumentError> {
        self.apply_steps(document, &self.forward)
    }

    pub(crate) fn apply_inverse(&self, document: &Document) -> Result<Document, DocumentError> {
        self.apply_steps(document, &self.inverse)
    }
}

#[cfg(test)]
mod tests {
    use crate::document::{Document, Mark, Position};

    fn empty_code() -> Document {
        Document::from_json(
            r#"{
                   "type": "doc",
                   "content": [
                       {
                           "type": "code_block"
                       }
                   ]
               }"#,
        )
        .expect("valid code document")
    }

    #[test]
    fn marked_text_insertion_into_code_is_rejected_without_steps() {
        let document = empty_code();

        let original = document.to_json().expect("serialize document");

        let mut transaction = document.transaction();

        assert!(
            transaction
                .insert_text_with_marks(Position::from_utf16_offset(1), "x", &[Mark::Strong],)
                .is_err()
        );

        let result = transaction.finish();

        assert!(result.change.is_none());

        assert_eq!(
            result.document.to_json().expect("serialize document"),
            original
        );
    }

    #[test]
    fn plain_text_replacement_in_code_ignores_requested_marks() {
        let document = empty_code();
        let mut transaction = document.transaction();

        let cursor = transaction
            .replace_plain_text(
                Position::from_utf16_offset(1),
                Position::from_utf16_offset(1),
                "A\r\nB",
                &[Mark::Strong, Mark::Emphasis],
            )
            .expect("replace code text");

        assert_eq!(cursor, Position::from_utf16_offset(4));

        let result = transaction.commit().to_json().expect("serialize document");

        let actual: serde_json::Value = serde_json::from_str(&result).expect("valid JSON");

        assert_eq!(
            actual,
            serde_json::json!({
                "type": "doc",
                "content": [
                    {
                        "type": "code_block",
                        "content": [
                            {
                                "type": "text",
                                "text": "A\nB"
                            }
                        ]
                    }
                ]
            })
        );

        Document::from_json(&result).expect("result satisfies schema");
    }
}
