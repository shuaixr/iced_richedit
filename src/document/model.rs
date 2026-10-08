use std::sync::Arc;

use prosemirror::{dynamic::DynamicNode, model::Node};

use crate::document::{DocumentError, schema::DocumentSchema, transaction::Transaction};

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
}
