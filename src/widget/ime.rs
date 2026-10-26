use iced::{
    Pixels,
    advanced::input_method::{self, Preedit},
};

use crate::editor::{CompositionContext, EditorAction, EditorState};

use super::TEXT_SIZE;

#[derive(Default)]
enum Phase {
    #[default]
    Closed,
    Open,
    Composing {
        context: CompositionContext,
        preedit: Preedit,
    },
    Submitted {
        context: CompositionContext,
    },
    Discarding,
}

#[derive(Default)]
pub(super) struct ImeState {
    phase: Phase,
}

impl ImeState {
    /// Cancels the local session without changing committed content.
    ///
    /// Remaining events are discarded until the input method closes.
    pub(super) fn cancel(&mut self) {
        if matches!(
            &self.phase,
            Phase::Composing { .. } | Phase::Submitted { .. }
        ) {
            self.phase = Phase::Discarding;
        }
    }

    pub(super) fn close(&mut self) {
        self.phase = Phase::Closed;
    }

    /// Reconciles temporary input state with application-owned editor state.
    pub(super) fn synchronize(&mut self, editor: &EditorState) {
        let next = match &self.phase {
            Phase::Composing { context, .. } if !editor.is_composition_context_current(context) => {
                Some(Phase::Discarding)
            }
            Phase::Submitted { context } if !editor.is_composition_context_current(context) => {
                Some(Phase::Open)
            }
            _ => None,
        };

        if let Some(next) = next {
            self.phase = next;
        }
    }

    pub(super) fn blocks_keyboard(&self) -> bool {
        matches!(
            &self.phase,
            Phase::Composing { .. } | Phase::Submitted { .. } | Phase::Discarding
        )
    }

    pub(super) fn preedit(&self) -> Option<Preedit<&str>> {
        match &self.phase {
            Phase::Composing { preedit, .. } => Some(preedit.as_ref()),
            _ => None,
        }
    }

    pub(super) fn handle(
        &mut self,
        event: &input_method::Event,
        editor: &EditorState,
    ) -> Option<EditorAction> {
        self.synchronize(editor);

        match event {
            input_method::Event::Opened => {
                if matches!(&self.phase, Phase::Closed) {
                    self.phase = Phase::Open;
                }
            }
            input_method::Event::Preedit(content, selection) => {
                let context = match &self.phase {
                    Phase::Open => editor.composition_context(),
                    Phase::Composing { context, .. } => context.clone(),
                    _ => return None,
                };

                if let Some(range) = selection
                    && (range.start > range.end
                        || range.end > content.len()
                        || !content.is_char_boundary(range.start)
                        || !content.is_char_boundary(range.end))
                {
                    eprintln!("Rejected invalid IME preedit byte range");
                    self.phase = Phase::Discarding;
                    return None;
                }

                // An empty preedit clears its display, not the session.
                self.phase = Phase::Composing {
                    context,
                    preedit: Preedit {
                        content: content.clone(),
                        selection: selection.clone(),
                        text_size: Some(Pixels(TEXT_SIZE)),
                    },
                };
            }
            input_method::Event::Commit(text) => {
                let context = match &self.phase {
                    Phase::Open => editor.composition_context(),
                    Phase::Composing { context, .. } => context.clone(),
                    _ => return None,
                };

                if !editor.is_composition_context_current(&context) {
                    self.phase = Phase::Discarding;
                    return None;
                }

                // Do not bind another commit to editor state that has not
                // yet incorporated the action we are publishing.
                self.phase = Phase::Submitted {
                    context: context.clone(),
                };

                return Some(EditorAction::CommitCompositionInContext {
                    context,
                    text: text.clone(),
                });
            }
            input_method::Event::Closed => {
                self.close();
            }
        }

        None
    }
}
