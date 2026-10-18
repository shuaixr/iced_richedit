use iced::{Point, advanced::graphics::text::Paragraph};

pub(super) struct LaidOutParagraph {
    pub(super) paragraph: Paragraph,
    pub(super) origin: Point,
}

#[derive(Default)]
pub(super) struct State {
    pub(super) paragraphs: Vec<LaidOutParagraph>,
    pub(super) has_error: bool,
}
