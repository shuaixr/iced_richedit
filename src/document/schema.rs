use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

use prosemirror::{
    dynamic::{DynamicMark, DynamicNode, DynamicSchema, types::Dyn},
    model::{MarkSet, Node},
};

use crate::document::{DocumentError, Mark};

pub(crate) struct DocumentSchema {
    inner: DynamicSchema,
}

impl DocumentSchema {
    pub(crate) fn mark(&self, mark: &Mark) -> Result<DynamicMark, DocumentError> {
        self.scope(|| {
            self.inner
                .mark_from_json(&mark.to_json())
                .map_err(|error| DocumentError::InvalidMark(error.to_string()))
        })
    }

    pub(crate) fn public_mark(&self, mark: &DynamicMark) -> Result<Mark, DocumentError> {
        self.scope(|| {
            let value = serde_json::to_value(mark)
                .map_err(|error| DocumentError::InvalidMark(error.to_string()))?;

            Mark::from_json(&value)
        })
    }

    pub(crate) fn scope<R>(&self, operation: impl FnOnce() -> R) -> R {
        let outcome = self
            .inner
            .with_types(|| catch_unwind(AssertUnwindSafe(operation)));

        match outcome {
            Ok(value) => value,
            Err(payload) => resume_unwind(payload),
        }
    }
    pub(crate) fn parse_node(
        &self,
        value: &serde_json::Value,
    ) -> Result<DynamicNode, DocumentError> {
        self.scope(|| {
            let node = self
                .inner
                .node_from_json(value)
                .map_err(|error| DocumentError::InvalidDocument(error.to_string()))?;
            node.check().map_err(DocumentError::InvalidDocument)?;
            Ok(node)
        })
    }
    pub(crate) fn text_node_with_marks(
        &self,
        text: &str,
        marks: &[Mark],
    ) -> Result<DynamicNode, DocumentError> {
        self.scope(|| {
            let set = self.mark_set(marks)?;
            Ok(self.inner.text(text).mark(set))
        })
    }
    pub(super) fn mark_set(&self, marks: &[Mark]) -> Result<MarkSet<Dyn>, DocumentError> {
        self.scope(|| {
            let mut set = MarkSet::<Dyn>::new();

            for mark in marks {
                let dynamic_mark = self.mark(mark)?;
                set.add(&dynamic_mark);
            }

            Ok(set)
        })
    }

    pub(crate) fn new() -> Result<Self, DocumentError> {
        let inner = DynamicSchema::from_json(&schema_spec())
            .map_err(|error| DocumentError::InvalidSchema(error.to_string()))?;

        Ok(Self { inner })
    }
}

fn schema_spec() -> serde_json::Value {
    serde_json::json!({
        "nodes": {
            "doc": { "content": "block+" },
            "paragraph": {
                "content": "inline*",
                "group": "block"
            },
            "text": {
                "group": "inline"
            },
            "hard_break": {
                "inline": true,
                "group": "inline"
            }
        },
        "marks": {
            "strong": {},
            "em": {},
            "underline": {},
            "code": {},
            "link": {
                "attrs": {
                    "href": {},
                    "title": { "default": null }
                },
                "inclusive": false
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use super::DocumentSchema;

    #[test]
    fn schema_remains_usable_after_operation_panics() {
        std::thread::spawn(|| {
            let schema = DocumentSchema::new().expect("schema must be valid");

            let result = catch_unwind(AssertUnwindSafe(|| {
                schema.scope(|| {
                    panic!("expected panic");
                });
            }));

            assert!(result.is_err());

            let value = serde_json::json!({
                "type": "doc",
                "content": [
                    { "type": "paragraph" }
                ]
            });

            schema
                .parse_node(&value)
                .expect("schema must remain usable after a panic");
        })
        .join()
        .expect("the test thread must complete");
    }
}
