use crate::document::DocumentError;

#[derive(Debug)]
pub enum EditorError {
    Document(DocumentError),
}
impl std::fmt::Display for EditorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Document(error) => write!(f, "editor document error: {error}"),
        }
    }
}

impl std::error::Error for EditorError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Document(error) => Some(error),
        }
    }
}
