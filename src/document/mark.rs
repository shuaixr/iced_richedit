use serde_json::Value;

use crate::document::DocumentError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mark {
    Strong,
    Emphasis,
    Underline,
    Code,
    Link { href: String, title: Option<String> },
}

impl Mark {
    pub(super) fn to_json(&self) -> Value {
        match self {
            Self::Strong => serde_json::json!({ "type": "strong" }),
            Self::Emphasis => serde_json::json!({ "type": "em" }),
            Self::Underline => serde_json::json!({ "type": "underline" }),
            Self::Code => serde_json::json!({ "type": "code" }),
            Self::Link { href, title } => serde_json::json!({
                "type": "link",
                "attrs": {
                    "href": href,
                    "title": title
                }
            }),
        }
    }

    pub(super) fn from_json(value: &Value) -> Result<Self, DocumentError> {
        match value.get("type").and_then(Value::as_str) {
            Some("strong") => Ok(Self::Strong),
            Some("em") => Ok(Self::Emphasis),
            Some("underline") => Ok(Self::Underline),
            Some("code") => Ok(Self::Code),
            Some("link") => {
                let attrs = value
                    .get("attrs")
                    .and_then(Value::as_object)
                    .ok_or_else(|| {
                        DocumentError::InvalidMark("link attrs must be an object".to_owned())
                    })?;

                let href = attrs
                    .get("href")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        DocumentError::InvalidMark("link href must be a string".to_owned())
                    })?
                    .to_owned();

                let title = match attrs.get("title") {
                    None | Some(Value::Null) => None,
                    Some(Value::String(title)) => Some(title.clone()),
                    Some(_) => {
                        return Err(DocumentError::InvalidMark(
                            "link title must be a string or null".to_owned(),
                        ));
                    }
                };

                Ok(Self::Link { href, title })
            }
            _ => Err(DocumentError::InvalidMark("unknown mark type".to_owned())),
        }
    }
}
