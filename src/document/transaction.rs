use std::sync::Arc;

use prosemirror::{
    dynamic::{DynamicNode, types::Dyn},
    model::{Fragment, ResolvedPos},
    transform::{MarkOrType, Transform},
};

use crate::document::{Document, DocumentError, Mark, Position, schema::DocumentSchema};

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
    pub fn insert_text(
        &mut self,
        position: Position,
        text: &str,
    ) -> Result<&mut Self, DocumentError> {
        if text.is_empty() {
            return Ok(self);
        }

        let offset = self.validate_position(position)?;
        let text_node = self.schema.text_node(text);
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
        let document = Document::from_parts(self.schema, self.transform.doc);

        TransactionResult { document }
    }
}
pub(crate) struct TransactionResult {
    document: Document,
}

impl TransactionResult {
    pub(crate) fn into_document(self) -> Document {
        self.document
    }
}
