use std::sync::Arc;

/// Identifies the editor context in which composition started.
///
/// The token does not contain document content or selection state.
#[derive(Debug, Clone)]
pub struct CompositionContext {
    identity: Arc<()>,
}

impl CompositionContext {
    pub(super) fn new() -> Self {
        Self {
            identity: Arc::new(()),
        }
    }

    pub(super) fn matches(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.identity, &other.identity)
    }
}
