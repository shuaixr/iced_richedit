use prosemirror::{
    dynamic::DynamicNode,
    model::{Node, NodeType},
};

use super::DocumentError;

/// Built-in text block semantics currently supported by the projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TextBlockKind {
    Paragraph,
    Heading { level: u8 },
    CodeBlock,
}

impl TextBlockKind {
    pub(crate) fn heading(level: u8) -> Result<Self, DocumentError> {
        if !(1..=6).contains(&level) {
            return Err(DocumentError::InvalidDocument(
                "heading level must be an integer from 1 to 6".to_owned(),
            ));
        }

        Ok(Self::Heading { level })
    }

    pub(super) fn from_node(node: &DynamicNode) -> Result<Self, DocumentError> {
        match node.r#type().name() {
            "paragraph" => Ok(Self::Paragraph),
            "code_block" => Ok(Self::CodeBlock),
            "heading" => {
                let level = node
                    .attrs_json()
                    .get("level")
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|value| u8::try_from(value).ok())
                    .ok_or_else(|| {
                        DocumentError::InvalidDocument(
                            "heading level must be an integer from 1 to 6".to_owned(),
                        )
                    })?;

                Self::heading(level)
            }
            _ => Err(DocumentError::InvalidDocument(
                "projection encountered an unsupported text block type".to_owned(),
            )),
        }
    }
}

pub(super) fn ensure_inline_formatting_allowed(
    root: &DynamicNode,
    from: usize,
    to: usize,
) -> Result<(), DocumentError> {
    for source in text_blocks(root)? {
        if source.is_touched_by(from, to)
            && TextBlockKind::from_node(source.node)? == TextBlockKind::CodeBlock
        {
            return Err(DocumentError::TransformFailed(
                "inline formatting is not allowed in code blocks".to_owned(),
            ));
        }
    }

    Ok(())
}

/// A borrowed text block and its actual location in the document tree.
///
/// Create and consume these records inside the document's schema scope.
pub(super) struct TextBlockSource<'a> {
    pub(super) node: &'a DynamicNode,
    pub(super) depth: usize,
    before: usize,
}

impl TextBlockSource<'_> {
    pub(super) fn content_start(&self) -> usize {
        self.before + 1
    }

    pub(super) fn content_end(&self) -> usize {
        self.content_start() + self.node.content_size()
    }

    pub(super) fn before(&self) -> usize {
        self.before
    }

    /// Includes a cursor, but excludes a block starting at a nonempty range's end.
    pub(super) fn is_touched_by(&self, from: usize, to: usize) -> bool {
        if from == to {
            self.content_start() <= from && from <= self.content_end()
        } else {
            self.content_end() >= from && self.content_start() < to
        }
    }
}

/// Visits text blocks in document order without flattening their positions.
pub(super) fn text_blocks(root: &DynamicNode) -> Result<Vec<TextBlockSource<'_>>, DocumentError> {
    let mut blocks = Vec::new();
    collect_children(root, 0, 1, &mut blocks)?;
    Ok(blocks)
}

fn collect_children<'a>(
    parent: &'a DynamicNode,
    content_start: usize,
    depth: usize,
    blocks: &mut Vec<TextBlockSource<'a>>,
) -> Result<(), DocumentError> {
    let mut before = content_start;

    for index in 0..parent.child_count() {
        let node = parent.maybe_child(index).ok_or_else(|| {
            DocumentError::InvalidDocument("document block is missing".to_owned())
        })?;

        if !node.is_block() || node.is_atom() {
            return Err(DocumentError::InvalidDocument(
                "text block discovery encountered unsupported structural content".to_owned(),
            ));
        }

        if node.is_textblock() {
            blocks.push(TextBlockSource {
                node,
                depth,
                before,
            });
        } else {
            collect_children(node, before + 1, depth + 1, blocks)?;
        }

        before += node.node_size();
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

    use prosemirror::dynamic::DynamicSchema;

    use super::*;

    fn with_document(value: serde_json::Value, inspect: impl FnOnce(&DynamicNode)) {
        let schema = DynamicSchema::from_json(&serde_json::json!({
            "nodes": {
                "doc": {
                    "content": "block+"
                },
                "paragraph": {
                    "content": "text*",
                    "group": "block"
                },
                "blockquote": {
                    "content": "block+",
                    "group": "block"
                },
                "horizontal_rule": {
                    "group": "block"
                },
                "text": {}
            },
            "marks": {}
        }))
        .expect("valid test schema");

        let outcome = schema.with_types(|| {
            catch_unwind(AssertUnwindSafe(|| {
                let node = schema.node_from_json(&value).expect("valid test document");

                node.check().expect("document must satisfy its schema");

                inspect(&node);
            }))
        });

        if let Err(payload) = outcome {
            resume_unwind(payload);
        }
    }

    #[test]
    fn discovers_top_level_text_blocks_including_empty_blocks() {
        with_document(
            serde_json::json!({
                "type": "doc",
                "content": [
                    {
                        "type": "paragraph"
                    },
                    {
                        "type": "paragraph",
                        "content": [
                            {
                                "type": "text",
                                "text": "A😀"
                            }
                        ]
                    }
                ]
            }),
            |node| {
                let blocks = text_blocks(node).expect("text blocks must be discoverable");

                let positions: Vec<_> = blocks
                    .iter()
                    .map(|block| (block.content_start(), block.content_end(), block.depth))
                    .collect();

                assert_eq!(positions, vec![(1, 1, 1), (3, 6, 1)]);
                assert_eq!(blocks[1].node.text_content(), "A😀");
            },
        );
    }

    #[test]
    fn discovers_nested_text_blocks_without_losing_structural_offsets() {
        with_document(
            serde_json::json!({
                "type": "doc",
                "content": [
                    {
                        "type": "blockquote",
                        "content": [
                            {
                                "type": "paragraph",
                                "content": [
                                    {
                                        "type": "text",
                                        "text": "A😀"
                                    }
                                ]
                            },
                            {
                                "type": "blockquote",
                                "content": [
                                    {
                                        "type": "paragraph"
                                    }
                                ]
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
            }),
            |node| {
                let blocks = text_blocks(node).expect("text blocks must be discoverable");

                let positions: Vec<_> = blocks
                    .iter()
                    .map(|block| (block.content_start(), block.content_end(), block.depth))
                    .collect();

                assert_eq!(positions, vec![(2, 5, 2), (8, 8, 3), (12, 13, 1)],);
            },
        );
    }

    #[test]
    fn rejects_unsupported_block_atoms_instead_of_skipping_them() {
        with_document(
            serde_json::json!({
                "type": "doc",
                "content": [
                    {
                        "type": "paragraph"
                    },
                    {
                        "type": "horizontal_rule"
                    }
                ]
            }),
            |node| {
                assert!(matches!(
                    text_blocks(node),
                    Err(DocumentError::InvalidDocument(_))
                ));
            },
        );
    }
}
