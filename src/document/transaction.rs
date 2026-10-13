use std::sync::Arc;

use prosemirror::transform::Step;
use prosemirror::transform::structure::can_join;
use prosemirror::transform::transform::StepError as TransformStepError;
use prosemirror::{
    dynamic::{DynamicNode, types::Dyn},
    model::{Fragment, Node, ResolvedPos},
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

    pub(crate) fn join_paragraphs(
        &mut self,
        position: Position,
    ) -> Result<&mut Self, DocumentError> {
        let offset = position.utf16_offset();
        let previous_step_count = self.transform.steps.len();

        self.schema.scope(|| {
            if can_join::<Dyn>(&self.transform.doc, offset) != Some(true) {
                return Err(DocumentError::TransformFailed(format!(
                    "paragraphs cannot be joined at position {offset}"
                )));
            }
            self.transform.join(offset, Some(1));

            if self.transform.steps.len() == previous_step_count {
                return Err(DocumentError::TransformFailed(format!(
                    "paragraphs could not be joined at position {offset}"
                )));
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
