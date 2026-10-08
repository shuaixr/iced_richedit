#[derive(Debug)]
pub enum DocumentError {
    InvalidSchema(String),
    InvalidDocument(String),
    InvalidMark(String),
    InvalidPosition { position: usize },
    TransformFailed(String),
    SerializationFailed(String),
}

impl std::fmt::Display for DocumentError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSchema(message) => {
                write!(formatter, "invalid document schema: {message}")
            }
            Self::InvalidDocument(message) => {
                write!(formatter, "invalid document: {message}")
            }
            Self::InvalidMark(message) => {
                write!(formatter, "invalid mark: {message}")
            }
            Self::InvalidPosition { position } => {
                write!(formatter, "invalid document position: {position}")
            }
            Self::TransformFailed(message) => {
                write!(formatter, "document transform failed: {message}")
            }
            Self::SerializationFailed(message) => {
                write!(formatter, "document serialization failed: {message}")
            }
        }
    }
}

impl std::error::Error for DocumentError {}
