use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

use prosemirror::{
    dynamic::{DynamicMark, DynamicNode, DynamicSchema},
    model::Node,
};

use crate::document::{DocumentError, Mark};

pub(crate) struct DocumentSchema {
    inner: DynamicSchema,
}

impl DocumentSchema {
    pub(crate) fn mark(&self, mark: &Mark) -> Result<DynamicMark, DocumentError> {
        self.scope(|| {
            let value = match mark {
                Mark::Strong => serde_json::json!({ "type": "strong" }),
                Mark::Emphasis => serde_json::json!({ "type": "em" }),
                Mark::Underline => serde_json::json!({ "type": "underline" }),
                Mark::Code => serde_json::json!({ "type": "code" }),
                Mark::Link { href, title } => serde_json::json!({
                    "type": "link",
                    "attrs": {
                        "href": href,
                        "title": title
                    }
                }),
            };

            self.inner
                .mark_from_json(&value)
                .map_err(|error| DocumentError::InvalidMark(error.to_string()))
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
    pub(crate) fn new() -> Result<Self, DocumentError> {
        let inner = DynamicSchema::from_json(&schema_spec())
            .map_err(|error| DocumentError::InvalidSchema(error.to_string()))?;

        Ok(Self { inner })
    }
    pub(crate) fn text_node(&self, text: &str) -> DynamicNode {
        self.scope(|| self.inner.text(text))
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
