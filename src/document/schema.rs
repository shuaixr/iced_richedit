use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

use prosemirror::{
    dynamic::{
        DynamicMark, DynamicMarkType, DynamicNode, DynamicNodeType, DynamicSchema, types::Dyn,
    },
    model::{MarkSet, Node, NodeType},
};

use crate::document::{DocumentError, Mark, TextBlockKind};

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
        let mut value = value.clone();
        normalize_heading_defaults(&mut value)?;

        self.scope(|| {
            let node = self
                .inner
                .node_from_json(&value)
                .map_err(|error| DocumentError::InvalidDocument(error.to_string()))?;

            node.check().map_err(DocumentError::InvalidDocument)?;

            let mut invalid = None;

            node.nodes_between(
                0,
                node.content_size(),
                &mut |child, _, _, _| {
                    if invalid.is_some() {
                        return false;
                    }

                    if child.r#type().name() == "heading"
                        && let Err(error) = TextBlockKind::from_node(child)
                    {
                        invalid = Some(error);
                        return false;
                    }

                    true
                },
                0,
            );

            if let Some(error) = invalid {
                return Err(error);
            }

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

    pub(crate) fn link_type(&self) -> Result<DynamicMarkType, DocumentError> {
        self.scope(|| {
            self.inner.mark_type("link").ok_or_else(|| {
                DocumentError::InvalidSchema("schema must define the link mark".to_owned())
            })
        })
    }

    pub(super) fn text_block_type(
        &self,
        kind: TextBlockKind,
    ) -> Result<DynamicNodeType, DocumentError> {
        let name = match kind {
            TextBlockKind::Paragraph => "paragraph",
            TextBlockKind::Heading { .. } => "heading",
            TextBlockKind::CodeBlock => "code_block",
        };

        self.scope(|| {
            self.inner.node_type(name).ok_or_else(|| {
                DocumentError::InvalidSchema(format!("schema must define the {name} node"))
            })
        })
    }

    pub(crate) fn new() -> Result<Self, DocumentError> {
        let inner = DynamicSchema::from_json(&schema_spec())
            .map_err(|error| DocumentError::InvalidSchema(error.to_string()))?;

        Ok(Self { inner })
    }

    pub(super) fn hard_break_node(&self) -> Result<DynamicNode, DocumentError> {
        self.scope(|| {
            let node_type = self.inner.node_type("hard_break").ok_or_else(|| {
                DocumentError::InvalidSchema("schema must define the hard_break node".to_owned())
            })?;

            Ok(node_type.create_node(None, None))
        })
    }
}

fn schema_spec() -> serde_json::Value {
    serde_json::json!({
        "nodes": {
            "doc": {
                "content": "block+"
            },
            "paragraph": {
                "content": "inline*",
                "group": "block"
            },
            "heading": {
                "content": "inline*",
                "group": "block",
                "attrs": {
                    "level": {
                        "default": 1
                    }
                }
            },
            "text": {
                "group": "inline"
            },
            "hard_break": {
                "inline": true,
                "group": "inline"
            },
            "code_block": {
                "content": "text*",
                "group": "block",
                "marks": "",
                "code": true,
                "whitespace": "pre"
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
                    "title": {
                        "default": null
                    }
                },
                "inclusive": false
            }
        }
    })
}

// The pinned DynamicNode JSON parser does not materialize node attr defaults.
fn normalize_heading_defaults(value: &mut serde_json::Value) -> Result<(), DocumentError> {
    if value.get("type").and_then(serde_json::Value::as_str) == Some("heading") {
        let object = value
            .as_object_mut()
            .expect("a typed node must be an object");

        let attrs = object
            .entry("attrs")
            .or_insert_with(|| serde_json::json!({}));

        if attrs.is_null() {
            *attrs = serde_json::json!({});
        }

        let attrs = attrs.as_object_mut().ok_or_else(|| {
            DocumentError::InvalidDocument("heading attrs must be an object or null".to_owned())
        })?;

        attrs.entry("level").or_insert(serde_json::json!(1));
    }

    if let Some(content) = value
        .get_mut("content")
        .and_then(serde_json::Value::as_array_mut)
    {
        for child in content {
            normalize_heading_defaults(child)?;
        }
    }

    Ok(())
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

    fn assert_join_rejected(spec: serde_json::Value, content: serde_json::Value, position: usize) {
        let schema = DocumentSchema {
            inner: prosemirror::dynamic::DynamicSchema::from_json(&spec)
                .expect("valid test schema"),
        };

        let node = schema
            .parse_node(&serde_json::json!({
                "type": "doc",
                "content": content
            }))
            .expect("valid test document");

        let original = schema
            .scope(|| serde_json::to_value(&node))
            .expect("serialize document");

        let mut transaction = crate::document::Transaction::new(std::sync::Arc::new(schema), node);

        assert!(
            transaction
                .join_paragraphs(crate::document::Position::from_utf16_offset(position),)
                .is_err()
        );

        let result: serde_json::Value =
            serde_json::from_str(&transaction.commit().to_json().expect("serialize document"))
                .expect("valid JSON");

        assert_eq!(result, original);
    }

    #[test]
    fn joining_rejects_atomic_and_container_neighbors_without_changing_the_document() {
        let mut spec = super::schema_spec();

        spec["nodes"]["horizontal_rule"] = serde_json::json!({
            "group": "block",
            "atom": true
        });

        spec["nodes"]["blockquote"] = serde_json::json!({
            "group": "block",
            "content": "block+"
        });

        for right in [
            serde_json::json!({
                "type": "horizontal_rule"
            }),
            serde_json::json!({
                "type": "blockquote",
                "content": [
                    { "type": "paragraph" }
                ]
            }),
        ] {
            assert_join_rejected(
                spec.clone(),
                serde_json::json!([
                    {
                        "type": "paragraph",
                        "content": [
                            {
                                "type": "text",
                                "text": "A"
                            }
                        ]
                    },
                    right
                ]),
                3,
            );
        }
    }

    #[test]
    fn joining_rejects_isolating_text_blocks_on_either_side() {
        let mut spec = super::schema_spec();
        spec["nodes"]["heading"]["isolating"] = serde_json::json!(true);

        let paragraph = serde_json::json!({
            "type": "paragraph",
            "content": [
                {
                    "type": "text",
                    "text": "A"
                }
            ]
        });

        let heading = serde_json::json!({
            "type": "heading",
            "attrs": {
                "level": 2
            },
            "content": [
                {
                    "type": "text",
                    "text": "B"
                }
            ]
        });

        for content in [
            serde_json::json!([paragraph, heading]),
            serde_json::json!([heading, paragraph]),
        ] {
            assert_join_rejected(spec.clone(), content, 3);
        }
    }

    #[test]
    fn joining_rejects_nested_boundaries_without_changing_the_document() {
        let mut spec = super::schema_spec();

        spec["nodes"]["blockquote"] = serde_json::json!({
            "group": "block",
            "content": "block+"
        });

        assert_join_rejected(
            spec,
            serde_json::json!([
                {
                    "type": "blockquote",
                    "content": [
                        {
                            "type": "paragraph",
                            "content": [
                                {
                                    "type": "text",
                                    "text": "A"
                                }
                            ]
                        },
                        {
                            "type": "paragraph",
                            "content": [
                                {
                                    "type": "text",
                                    "text": "B"
                                }
                            ]
                        }
                    ]
                }
            ]),
            4,
        );
    }

    #[test]
    fn code_joining_rejects_isolating_atomic_and_unsupported_neighbors_without_changes() {
        let code = serde_json::json!({
            "type": "code_block",
            "content": [
                { "type": "text", "text": "A" }
            ]
        });

        let paragraph = serde_json::json!({
            "type": "paragraph",
            "content": [
                { "type": "text", "text": "B" }
            ]
        });

        let mut spec = super::schema_spec();

        spec["nodes"]["code_block"]["isolating"] = serde_json::json!(true);

        for content in [
            serde_json::json!([code, paragraph]),
            serde_json::json!([paragraph, code]),
        ] {
            assert_join_rejected(spec.clone(), content, 3);
        }

        spec["nodes"]["code_block"]["isolating"] = serde_json::json!(false);

        spec["nodes"]["atomic_text"] = serde_json::json!({
            "group": "block",
            "content": "text*",
            "atom": true
        });

        spec["nodes"]["custom_text"] = serde_json::json!({
            "group": "block",
            "content": "text*"
        });

        for kind in ["atomic_text", "custom_text"] {
            let neighbor = serde_json::json!({
                "type": kind,
                "content": [
                    { "type": "text", "text": "B" }
                ]
            });

            for content in [
                serde_json::json!([code, neighbor]),
                serde_json::json!([neighbor, code]),
            ] {
                assert_join_rejected(spec.clone(), content, 3);
            }
        }
    }
}
