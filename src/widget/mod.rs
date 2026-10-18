mod state;

use iced::{
    Border, Color, Element, Event, Font, Length, Pixels, Point, Rectangle, Size,
    advanced::{
        Clipboard, Layout, Shell, Widget,
        graphics::text::Paragraph,
        layout, mouse, renderer, text,
        widget::{Operation, Tree, tree},
    },
    alignment, keyboard,
    widget::Id,
    window,
};

use iced::advanced::text::Paragraph as _;

use crate::editor::{EditorAction, EditorState};

use self::state::{LaidOutParagraph, State};

const PADDING: f32 = 12.0;
const PARAGRAPH_SPACING: f32 = 8.0;
const TEXT_SIZE: f32 = 16.0;
const LINE_HEIGHT: f32 = 24.0;
pub struct RichEdit<'a, Message> {
    editor: &'a EditorState,
    width: Length,
    height: Length,
    id: Option<Id>,
    on_action: Option<Box<dyn Fn(EditorAction) -> Message + 'a>>,
}

impl<'a, Message> RichEdit<'a, Message> {
    pub fn new(editor: &'a EditorState) -> Self {
        Self {
            editor,
            width: Length::Fill,
            height: Length::Fixed(300.0),
            id: None,
            on_action: None,
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

    pub fn id(mut self, id: Id) -> Self {
        self.id = Some(id);
        self
    }

    pub fn on_action(mut self, on_action: impl Fn(EditorAction) -> Message + 'a) -> Self {
        self.on_action = Some(Box::new(on_action));
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

fn keyboard_action(
    key: &keyboard::Key,
    physical_key: keyboard::key::Physical,
    modifiers: keyboard::Modifiers,
    committed_text: Option<&str>,
) -> Option<EditorAction> {
    use keyboard::key::Named;

    if modifiers.command() && !modifiers.alt() {
        return match key.to_latin(physical_key) {
            Some('z') if modifiers.shift() => Some(EditorAction::Redo),
            Some('z') => Some(EditorAction::Undo),
            Some('y') if !cfg!(target_os = "macos") && !modifiers.shift() => {
                Some(EditorAction::Redo)
            }
            _ => None,
        };
    }

    // Modified navigation and AltGr input are outside this initial keymap.
    if modifiers.control() || modifiers.alt() || modifiers.logo() {
        return None;
    }

    let action = match key {
        keyboard::Key::Named(Named::Enter) if !modifiers.shift() => {
            Some(EditorAction::SplitParagraph)
        }
        keyboard::Key::Named(Named::Backspace) if !modifiers.shift() => {
            Some(EditorAction::DeleteBackward)
        }
        keyboard::Key::Named(Named::Delete) if !modifiers.shift() => {
            Some(EditorAction::DeleteForward)
        }
        keyboard::Key::Named(Named::ArrowLeft) if !modifiers.shift() => {
            Some(EditorAction::MoveLeft)
        }
        keyboard::Key::Named(Named::ArrowRight) if !modifiers.shift() => {
            Some(EditorAction::MoveRight)
        }
        keyboard::Key::Named(Named::Space) => None,
        keyboard::Key::Named(_) => return None,
        _ => None,
    };

    if action.is_some() {
        return action;
    }

    let content = committed_text?;

    if content.is_empty() || content.chars().any(char::is_control) {
        return None;
    }

    Some(EditorAction::InsertText(content.to_owned()))
}

impl<Message, Renderer> Widget<Message, iced::Theme, Renderer> for RichEdit<'_, Message>
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
                        color: if state.is_focused && !state.window_unfocused {
                            palette.primary
                        } else {
                            Color {
                                a: 0.3,
                                ..palette.text
                            }
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

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        _renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        if self.on_action.is_none() {
            return;
        }

        let state = tree.state.downcast_mut::<State>();

        operation.focusable(self.id.as_ref(), layout.bounds(), state);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();

        match event {
            Event::Window(window::Event::Unfocused) => {
                state.window_unfocused = true;
                shell.request_redraw();
            }
            Event::Window(window::Event::Focused) => {
                state.window_unfocused = false;
                shell.request_redraw();
            }

            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let inside = layout
                    .bounds()
                    .intersection(viewport)
                    .is_some_and(|visible| cursor.is_over(visible));

                let was_focused = state.is_focused;

                if self.on_action.is_some() {
                    state.is_focused = inside && !shell.is_event_captured();
                } else {
                    state.is_focused = false;
                }

                if state.is_focused != was_focused {
                    shell.request_redraw();
                }

                if state.is_focused {
                    shell.capture_event();
                }
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                physical_key,
                modifiers,
                text,
                ..
            }) => {
                if !state.is_focused
                    || state.window_unfocused
                    || state.has_error
                    || shell.is_event_captured()
                {
                    return;
                }

                let Some(on_action) = &self.on_action else {
                    return;
                };

                if let Some(action) =
                    keyboard_action(key, *physical_key, *modifiers, text.as_deref())
                {
                    shell.publish(on_action(action));
                    shell.capture_event();
                }
            }
            _ => {}
        }
    }
}

impl<'a, Message, Renderer> From<RichEdit<'a, Message>>
    for Element<'a, Message, iced::Theme, Renderer>
where
    Message: 'a,
    Renderer: text::Renderer<Font = Font, Paragraph = Paragraph> + 'a,
{
    fn from(widget: RichEdit<'a, Message>) -> Self {
        Element::new(widget)
    }
}
