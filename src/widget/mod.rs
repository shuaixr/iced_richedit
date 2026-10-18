mod state;

use iced::{
    Border, Color, Element, Font, Length, Pixels, Point, Rectangle, Size,
    advanced::{
        Layout, Widget,
        graphics::text::Paragraph,
        layout, mouse, renderer, text,
        widget::{Tree, tree},
    },
    alignment,
};

use iced::advanced::text::Paragraph as _;

use crate::editor::EditorState;

use self::state::{LaidOutParagraph, State};

const PADDING: f32 = 12.0;
const PARAGRAPH_SPACING: f32 = 8.0;
const TEXT_SIZE: f32 = 16.0;
const LINE_HEIGHT: f32 = 24.0;

/// A rich text editor widget.
///
/// This initial version displays document text without editing interaction.
pub struct RichEdit<'a> {
    editor: &'a EditorState,
    width: Length,
    height: Length,
}

impl<'a> RichEdit<'a> {
    pub fn new(editor: &'a EditorState) -> Self {
        Self {
            editor,
            width: Length::Fill,
            height: Length::Fixed(300.0),
        }
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }
}

fn make_paragraph(content: &str, width: f32, font: Font) -> Paragraph {
    Paragraph::with_text(text::Text {
        content,
        bounds: Size::new(width, f32::INFINITY),
        size: Pixels(TEXT_SIZE),
        line_height: text::LineHeight::Absolute(Pixels(LINE_HEIGHT)),
        font,
        align_x: text::Alignment::Left,
        align_y: alignment::Vertical::Top,
        shaping: text::Shaping::Advanced,
        wrapping: text::Wrapping::WordOrGlyph,
    })
}

impl<Message, Renderer> Widget<Message, iced::Theme, Renderer> for RichEdit<'_>
where
    Renderer: text::Renderer<Font = Font, Paragraph = Paragraph>,
{
    fn size(&self) -> Size<Length> {
        Size::new(self.width, self.height)
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let size = limits.resolve(self.width, self.height, Size::new(300.0, 300.0));

        let state = tree.state.downcast_mut::<State>();

        state.paragraphs.clear();
        state.has_error = false;

        let content_width = (size.width - PADDING * 2.0).max(0.0);

        if content_width == 0.0 {
            return layout::Node::new(size);
        }

        let font = renderer.default_font();

        match self.editor.document().paragraph_views() {
            Ok(views) => {
                let mut y = PADDING;

                for view in views {
                    let paragraph = make_paragraph(view.text(), content_width, font);

                    // Empty logical lines must retain a visible line box.
                    let minimum_height = view.line_count() as f32 * LINE_HEIGHT;

                    let height = paragraph.min_height().max(minimum_height);

                    state.paragraphs.push(LaidOutParagraph {
                        paragraph,
                        origin: Point::new(PADDING, y),
                    });

                    y += height + PARAGRAPH_SPACING;
                }
            }
            Err(error) => {
                state.has_error = true;

                let message = format!("Document display failed: {error}");
                let paragraph = make_paragraph(&message, content_width, font);

                state.paragraphs.push(LaidOutParagraph {
                    paragraph,
                    origin: Point::new(PADDING, PADDING),
                });
            }
        }

        layout::Node::new(size)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &iced::Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();

        let Some(clip) = bounds.intersection(viewport) else {
            return;
        };

        let state = tree.state.downcast_ref::<State>();
        let palette = theme.palette();

        renderer.with_layer(clip, |renderer| {
            renderer.fill_quad(
                renderer::Quad {
                    bounds,
                    border: Border {
                        color: Color {
                            a: 0.3,
                            ..palette.text
                        },
                        width: 1.0,
                        radius: 4.0.into(),
                    },
                    ..renderer::Quad::default()
                },
                palette.background,
            );

            let content_bounds = Rectangle {
                x: bounds.x + PADDING,
                y: bounds.y + PADDING,
                width: (bounds.width - PADDING * 2.0).max(0.0),
                height: (bounds.height - PADDING * 2.0).max(0.0),
            };

            let Some(text_clip) = content_bounds.intersection(&clip) else {
                return;
            };

            let color = if state.has_error {
                palette.danger
            } else {
                palette.text
            };

            for item in &state.paragraphs {
                let position = Point::new(bounds.x + item.origin.x, bounds.y + item.origin.y);

                renderer.fill_paragraph(&item.paragraph, position, color, text_clip);
            }
        });
    }
}

impl<'a, Message, Renderer> From<RichEdit<'a>> for Element<'a, Message, iced::Theme, Renderer>
where
    Message: 'a,
    Renderer: text::Renderer<Font = Font, Paragraph = Paragraph> + 'a,
{
    fn from(widget: RichEdit<'a>) -> Self {
        Element::new(widget)
    }
}
