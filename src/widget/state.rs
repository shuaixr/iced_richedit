use iced::{
    Point,
    advanced::{graphics::text::Paragraph, widget::operation::Focusable},
};

pub(super) struct LaidOutParagraph {
    pub(super) paragraph: Paragraph,
    pub(super) origin: Point,
}

#[derive(Default)]
pub(super) struct State {
    pub(super) paragraphs: Vec<LaidOutParagraph>,
    pub(super) has_error: bool,
    pub(super) is_focused: bool,
    pub(super) window_unfocused: bool,
}

impl Focusable for State {
    fn is_focused(&self) -> bool {
        self.is_focused
    }

    fn focus(&mut self) {
        self.is_focused = true;
    }

    fn unfocus(&mut self) {
        self.is_focused = false;
    }
}
