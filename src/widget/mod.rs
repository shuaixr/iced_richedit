mod ime;
mod paste;
#[cfg(all(test, feature = "renderer-tests"))]
mod renderer_tests;
mod state;

use self::state::{LaidOutParagraph, State};
use crate::{
    document::{Mark, ParagraphView, TextBlockKind, TextSelection},
    editor::{EditorAction, EditorState},
    widget::state::ObservedPreedit,
};
use iced::advanced::text::Paragraph as _;
use iced::time::Instant;
use iced::{
    Border, Color, Element, Event, Font, Length, Pixels, Point, Rectangle, Size,
    advanced::{
        InputMethod, Layout, Shell, Widget, clipboard,
        graphics::text::Paragraph,
        input_method::{self, Purpose},
        layout, mouse, renderer, text,
        widget::{Operation, Tree, tree},
    },
    alignment,
    font::{Style, Weight},
    keyboard,
    widget::Id,
    window,
};

#[cfg(feature = "perf-diagnostics")]
use crate::diagnostics::{Counter, Diagnostics, Operation as DiagnosticOperation, Timing};

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
    link_color: Color,

    #[cfg(feature = "perf-diagnostics")]
    diagnostics: Option<&'a Diagnostics>,
}

impl<'a, Message> RichEdit<'a, Message> {
    pub fn new(editor: &'a EditorState) -> Self {
        Self {
            editor,
            width: Length::Fill,
            height: Length::Fixed(300.0),
            id: None,
            on_action: None,
            link_color: Color::from_rgb8(0, 102, 204),

            #[cfg(feature = "perf-diagnostics")]
            diagnostics: None,
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

    pub fn link_color(mut self, color: Color) -> Self {
        self.link_color = color;
        self
    }

    /// Attaches an opt-in recorder without changing editor state.
    #[cfg(feature = "perf-diagnostics")]
    pub fn diagnostics(mut self, diagnostics: &'a Diagnostics) -> Self {
        self.diagnostics = Some(diagnostics);
        self
    }

    #[cfg(feature = "perf-diagnostics")]
    fn record_cache_decision(&self, counter: Counter) {
        if let Some(diagnostics) = self.diagnostics {
            diagnostics.increment(counter);
        }
    }
}

fn paragraph_text(content: &str, width: f32, font: Font) -> text::Text<&str> {
    text::Text {
        content,
        bounds: Size::new(width, f32::INFINITY),
        size: Pixels(TEXT_SIZE),
        line_height: text::LineHeight::Absolute(Pixels(LINE_HEIGHT)),
        font,
        align_x: text::Alignment::Left,
        align_y: alignment::Vertical::Top,
        shaping: text::Shaping::Advanced,
        wrapping: text::Wrapping::WordOrGlyph,
        ellipsis: text::Ellipsis::None,
        hint_factor: None,
    }
}

fn rich_paragraph_text(view: &ParagraphView, width: f32, font: Font) -> text::Text<&str> {
    let mut descriptor = paragraph_text(view.text(), width, font);
    if view.kind() == TextBlockKind::CodeBlock {
        descriptor.font.family = Font::MONOSPACE.family;
    }
    if let TextBlockKind::Heading { level } = view.kind() {
        let size = match level {
            1 => 32.0,
            2 => 28.0,
            3 => 24.0,
            4 => 20.0,
            5 => 18.0,
            6 => 16.0,
            _ => unreachable!("heading levels are validated by the document"),
        };

        descriptor.font.weight = Weight::Bold;
        descriptor.size = Pixels(size);
        descriptor.line_height = text::LineHeight::Absolute(Pixels(size * 1.5));
    }

    descriptor
}

fn make_paragraph(content: &str, width: f32, font: Font) -> Paragraph {
    Paragraph::with_text(paragraph_text(content, width, font))
}

fn font_for_marks(base: Font, marks: &[Mark]) -> Font {
    let mut font = base;

    for mark in marks {
        match mark {
            Mark::Strong => font.weight = Weight::Bold,
            Mark::Emphasis => font.style = Style::Italic,
            Mark::Code => font.family = Font::MONOSPACE.family,
            Mark::Underline | Mark::Link { .. } => {}
        }
    }

    font
}

fn span_for_marks<'a>(
    fragment: &'a str,
    marks: &[Mark],
    base_font: Font,
    link_color: Color,
) -> text::Span<'a> {
    let span = text::Span::new(fragment).font(font_for_marks(base_font, marks));

    if marks.iter().any(|mark| matches!(mark, Mark::Link { .. })) {
        span.color(link_color)
    } else {
        span
    }
}

fn make_rich_paragraph(
    view: &ParagraphView,
    width: f32,
    base_font: Font,
    link_color: Color,
) -> Paragraph {
    let descriptor = rich_paragraph_text(view, width, base_font);
    if view.kind() == TextBlockKind::CodeBlock {
        return Paragraph::with_text(descriptor);
    }
    let spans: Vec<text::Span<'_>> = view
        .marked_fragments()
        .map(|(fragment, marks)| span_for_marks(fragment, marks, descriptor.font, link_color))
        .collect();

    Paragraph::with_spans(descriptor.with_content(spans.as_slice()))
}

fn visual_line_boundary_key(key: &keyboard::Key, modifiers: keyboard::Modifiers) -> Option<bool> {
    use keyboard::key::Named;

    if cfg!(target_os = "macos") {
        if !modifiers.macos_command() || modifiers.control() || modifiers.alt() {
            return None;
        }

        match key {
            keyboard::Key::Named(Named::ArrowLeft) => Some(false),
            keyboard::Key::Named(Named::ArrowRight) => Some(true),
            _ => None,
        }
    } else {
        if modifiers.control() || modifiers.alt() || modifiers.logo() {
            return None;
        }

        match key {
            keyboard::Key::Named(Named::Home) => Some(false),
            keyboard::Key::Named(Named::End) => Some(true),
            _ => None,
        }
    }
}

fn vertical_navigation_key(key: &keyboard::Key, modifiers: keyboard::Modifiers) -> Option<bool> {
    use keyboard::key::Named;

    if modifiers.control() || modifiers.alt() || modifiers.logo() {
        return None;
    }

    match key {
        keyboard::Key::Named(Named::ArrowUp) => Some(false),
        keyboard::Key::Named(Named::ArrowDown) => Some(true),
        _ => None,
    }
}

fn keyboard_action(
    key: &keyboard::Key,
    physical_key: keyboard::key::Physical,
    modifiers: keyboard::Modifiers,
    committed_text: Option<&str>,
) -> Option<EditorAction> {
    use keyboard::key::Named;
    let document_boundary =
        if modifiers.command() && !modifiers.alt() && !(modifiers.control() && modifiers.logo()) {
            match key {
                keyboard::Key::Named(Named::Home) if !cfg!(target_os = "macos") => Some(false),
                keyboard::Key::Named(Named::End) if !cfg!(target_os = "macos") => Some(true),
                keyboard::Key::Named(Named::ArrowUp) if cfg!(target_os = "macos") => Some(false),
                keyboard::Key::Named(Named::ArrowDown) if cfg!(target_os = "macos") => Some(true),
                _ => None,
            }
        } else {
            None
        };

    if let Some(end) = document_boundary {
        return Some(match (end, modifiers.shift()) {
            (false, false) => EditorAction::MoveDocumentStart,
            (true, false) => EditorAction::MoveDocumentEnd,
            (false, true) => EditorAction::ExtendDocumentStart,
            (true, true) => EditorAction::ExtendDocumentEnd,
        });
    }
    let is_word_command =
        modifiers.jump() && !modifiers.logo() && !(modifiers.control() && modifiers.alt());

    if is_word_command {
        match key {
            keyboard::Key::Named(Named::ArrowLeft) => {
                return Some(if modifiers.shift() {
                    EditorAction::ExtendWordLeft
                } else {
                    EditorAction::MoveWordLeft
                });
            }
            keyboard::Key::Named(Named::ArrowRight) => {
                return Some(if modifiers.shift() {
                    EditorAction::ExtendWordRight
                } else {
                    EditorAction::MoveWordRight
                });
            }
            keyboard::Key::Named(Named::Backspace) if !modifiers.shift() => {
                return Some(EditorAction::DeleteWordBackward);
            }
            keyboard::Key::Named(Named::Delete) if !modifiers.shift() => {
                return Some(EditorAction::DeleteWordForward);
            }
            _ => {}
        }
    }
    if modifiers.command() && !modifiers.alt() {
        return match key.to_latin(physical_key) {
            Some('a') if !modifiers.shift() => Some(EditorAction::SelectAll),
            Some('z') if modifiers.shift() => Some(EditorAction::Redo),
            Some('z') => Some(EditorAction::Undo),
            Some('y') if !cfg!(target_os = "macos") && !modifiers.shift() => {
                Some(EditorAction::Redo)
            }
            _ => None,
        };
    }

    // Other modified navigation and AltGr text input are not handled yet.
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
        keyboard::Key::Named(Named::ArrowLeft) => Some(if modifiers.shift() {
            EditorAction::ExtendLeft
        } else {
            EditorAction::MoveLeft
        }),
        keyboard::Key::Named(Named::ArrowRight) => Some(if modifiers.shift() {
            EditorAction::ExtendRight
        } else {
            EditorAction::MoveRight
        }),
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

impl<Message> iced::advanced::widget::Meta for RichEdit<'_, Message> {}

impl<Message, Renderer> Widget<Message, iced::Theme, Renderer> for RichEdit<'_, Message>
where
    Renderer: text::Renderer<Paragraph = Paragraph>,
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

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        #[cfg(feature = "perf-diagnostics")]
        let _layout_timing = self
            .diagnostics
            .and_then(|diagnostics| diagnostics.scope(Timing::Layout));
        let size = limits.resolve(self.width, self.height, Size::new(300.0, 300.0));
        tree.size = size;

        let state = tree.state.downcast_mut::<State>();
        state.ime.synchronize(self.editor);
        let selection = self.editor.selection();

        let views = {
            #[cfg(feature = "perf-diagnostics")]
            let _projection_timing = self.diagnostics.and_then(|diagnostics| {
                let timing = if state.ime.preedit().is_some() {
                    Timing::CompositionProjection
                } else {
                    Timing::Projection
                };

                diagnostics.scope(timing)
            });

            match state.ime.preedit() {
                Some(preedit) => self.editor.document().composition_paragraph_views(
                    selection,
                    preedit.content,
                    preedit.selection.as_ref().map(|range| range.end),
                    self.editor.stored_marks(),
                ),
                None => self.editor.document().paragraph_views(),
            }
        };
        #[cfg(feature = "perf-diagnostics")]
        if let Some(diagnostics) = self.diagnostics {
            diagnostics.observe_layout(size.width, size.height, views.as_ref().ok().map(Vec::len));
        }
        // Compare against the old projection before taking the layout entries.
        let content_changed = match &views {
            Ok(views) => {
                state.has_error
                    || views.len() != state.paragraphs.len()
                    || views.iter().zip(&state.paragraphs).any(|(view, previous)| {
                        previous.view.as_ref().is_none_or(|previous| {
                            previous.start() != view.start() || !previous.has_same_content(view)
                        })
                    })
            }
            Err(_) => false,
        };

        let selection_changed = state.observed_selection != Some(selection);
        let preedit_changed = match (state.ime.preedit(), state.observed_preedit.as_ref()) {
            (None, None) => false,
            (Some(current), Some(previous)) => {
                current.content != previous.content.as_str()
                    || current.selection.as_ref() != previous.selection.as_ref()
            }
            _ => true,
        };
        if (content_changed || selection_changed || preedit_changed)
            && state.is_focused
            && !state.window_unfocused
        {
            state.caret_blink.reset(Instant::now());
        }

        let viewport_changed = state.observed_viewport != Some(size);
        let hint_matches_head = state
            .caret_hint
            .is_some_and(|(position, _)| position == selection.head());

        if content_changed || viewport_changed || (selection_changed && !hint_matches_head) {
            state.caret_hint = None;
            state.preferred_x = None;
        }
        let mut previous = std::mem::take(&mut state.paragraphs).into_iter();

        state.has_error = false;
        state.content_height = 0.0;

        let content_width = (size.width - PADDING * 2.0).max(0.0);

        if content_width == 0.0 {
            state.clamp_scroll(size.height);
            return;
        }

        let font = renderer.font();
        let link_color_changed = state.observed_link_color != Some(self.link_color);
        match views {
            Ok(views) => {
                state.paragraphs.reserve(views.len());

                let mut y = PADDING;

                for view in views {
                    let previous_item = previous.next();

                    // Document offsets do not affect shaping, but text and marks do.
                    let reusable = previous_item.filter(|item| {
                        !link_color_changed
                            && item
                                .view
                                .as_ref()
                                .is_some_and(|old| old.has_same_content(&view))
                    });

                    let desired = rich_paragraph_text(&view, content_width, font).with_content(());

                    let line_height = desired.line_height.to_absolute(desired.size).0;

                    let paragraph = match reusable {
                        Some(mut item) => match item.paragraph.compare(desired) {
                            text::Difference::None => {
                                #[cfg(feature = "perf-diagnostics")]
                                self.record_cache_decision(Counter::ParagraphReused);

                                item.paragraph
                            }
                            text::Difference::Bounds => {
                                #[cfg(feature = "perf-diagnostics")]
                                self.record_cache_decision(Counter::ParagraphResized);

                                item.paragraph.resize(desired.bounds);
                                item.paragraph
                            }
                            text::Difference::Shape => {
                                #[cfg(feature = "perf-diagnostics")]
                                self.record_cache_decision(Counter::ParagraphRebuilt);

                                make_rich_paragraph(&view, content_width, font, self.link_color)
                            }
                        },
                        None => {
                            #[cfg(feature = "perf-diagnostics")]
                            self.record_cache_decision(Counter::ParagraphRebuilt);

                            make_rich_paragraph(&view, content_width, font, self.link_color)
                        }
                    };
                    let minimum_height = view.line_count() as f32 * line_height;

                    let trailing_height = state::trailing_empty_line(&paragraph, &view)
                        .map_or(0.0, |(_, bounds)| bounds.y + bounds.height);

                    let height = paragraph
                        .min_height()
                        .max(minimum_height)
                        .max(trailing_height);
                    state.paragraphs.push(LaidOutParagraph {
                        paragraph,
                        origin: Point::new(PADDING, y),
                        height,
                        view: Some(view),
                    });

                    y += height + PARAGRAPH_SPACING;
                }
            }
            Err(error) => {
                state.has_error = true;

                let message = format!("Document display failed: {error}");
                let paragraph = make_paragraph(&message, content_width, font);

                let height = paragraph.min_height().max(LINE_HEIGHT);

                state.paragraphs.push(LaidOutParagraph {
                    paragraph,
                    origin: Point::new(PADDING, PADDING),
                    height,
                    view: None,
                });
            }
        }

        state.content_height = state
            .paragraphs
            .last()
            .map(|item| item.origin.y + item.height + PADDING)
            .unwrap_or(PADDING * 2.0);

        state.clamp_scroll(size.height);

        if !state.has_error
            && (content_changed || selection_changed || viewport_changed || preedit_changed)
        {
            state.reveal_head(selection.head(), size.height, PADDING);
        }
        if preedit_changed {
            state.observed_preedit = state.ime.preedit().map(|preedit| ObservedPreedit {
                content: preedit.content.to_owned(),
                selection: preedit.selection,
            });
        }

        state.observed_selection = Some(selection);
        state.observed_viewport = Some(size);
        state.observed_link_color = Some(self.link_color);
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &iced::Theme,
        _style: &renderer::Style,
        layout: Layout,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        #[cfg(feature = "perf-diagnostics")]
        let _draw_timing = self
            .diagnostics
            .and_then(|diagnostics| diagnostics.scope(Timing::Draw));
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
                            palette.primary.base.color
                        } else {
                            Color {
                                a: 0.3,
                                ..palette.background.base.text
                            }
                        },
                        width: 1.0,
                        radius: 4.0.into(),
                    },
                    ..renderer::Quad::default()
                },
                palette.background.base.color,
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
                palette.danger.base.color
            } else {
                palette.background.base.text
            };
            let selection = self.editor.selection();

            let selection_color = Color {
                a: if state.is_focused && !state.window_unfocused {
                    0.25
                } else {
                    0.12
                },
                ..palette.primary.base.color
            };

            let code_background = Color {
                a: 0.08,
                ..palette.background.base.text
            };

            for rectangle in state
                .code_block_background_bounds(content_bounds.width)
                .into_iter()
                .chain(state.code_background_bounds())
            {
                let background = Rectangle {
                    x: bounds.x + rectangle.x,
                    y: bounds.y + rectangle.y - state.scroll_y,
                    width: rectangle.width,
                    height: rectangle.height,
                };

                if let Some(background) = background.intersection(&text_clip) {
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: background,
                            ..renderer::Quad::default()
                        },
                        code_background,
                    );
                }
            }

            let selection_bounds = if state.ime.preedit().is_some() {
                state.composition_selection_bounds()
            } else {
                state.selection_bounds(selection)
            };

            for rectangle in selection_bounds {
                let bounds = Rectangle {
                    x: bounds.x + rectangle.x,
                    y: bounds.y + rectangle.y - state.scroll_y,
                    width: rectangle.width,
                    height: rectangle.height,
                };

                if let Some(bounds) = bounds.intersection(&text_clip) {
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds,
                            ..renderer::Quad::default()
                        },
                        selection_color,
                    );
                }
            }
            for item in &state.paragraphs {
                let position = Point::new(
                    bounds.x + item.origin.x,
                    bounds.y + item.origin.y - state.scroll_y,
                );

                renderer.fill_paragraph(&item.paragraph, position, color, text_clip);
            }

            for (rectangle, is_link) in state.underline_bounds() {
                let underline = Rectangle {
                    x: bounds.x + rectangle.x,
                    y: bounds.y + rectangle.y - state.scroll_y,
                    width: rectangle.width,
                    height: rectangle.height,
                };

                let underline_color = if is_link { self.link_color } else { color };

                if let Some(underline) = underline.intersection(&text_clip) {
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: underline,
                            ..renderer::Quad::default()
                        },
                        underline_color,
                    );
                }
            }

            if state.is_focused && !state.window_unfocused && state.ime.preedit().is_some() {
                for rectangle in state.composition_bounds() {
                    let thickness = rectangle.height.min(1.0);

                    let underline = Rectangle {
                        x: bounds.x + rectangle.x,
                        y: bounds.y + rectangle.y + rectangle.height - state.scroll_y - thickness,
                        width: rectangle.width,
                        height: thickness,
                    };

                    if let Some(underline) = underline.intersection(&text_clip) {
                        renderer.fill_quad(
                            renderer::Quad {
                                bounds: underline,
                                ..renderer::Quad::default()
                            },
                            palette.background.base.text,
                        );
                    }
                }
            }
            let selection = self.editor.selection();

            if state.is_focused
                && !state.window_unfocused
                && !state.has_error
                && self.on_action.is_some()
                && state.caret_blink.is_visible()
                && (selection.is_empty() || state.ime.preedit().is_some())
                && state
                    .ime
                    .preedit()
                    .is_none_or(|preedit| preedit.selection.is_some())
                && let Some(caret) = state.caret_bounds(selection.head())
            {
                let caret = Rectangle {
                    x: bounds.x + caret.x,
                    y: bounds.y + caret.y - state.scroll_y,
                    width: caret.width,
                    height: caret.height,
                };
                if let Some(caret) = caret.intersection(&text_clip) {
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: caret,
                            ..renderer::Quad::default()
                        },
                        palette.background.base.text,
                    );
                }
            }
        });
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        _viewport: &Rectangle,
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
        layout: Layout,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        #[cfg(feature = "perf-diagnostics")]
        let _event_timing = self.diagnostics.and_then(|diagnostics| {
            let timing = match event {
                Event::Keyboard(_) => Timing::KeyboardEvent,
                Event::Mouse(_) => Timing::PointerEvent,
                Event::InputMethod(_) => Timing::ImeEvent,

                Event::Window(window::Event::RedrawRequested(_)) => Timing::RedrawEvent,
                _ => Timing::OtherEvent,
            };

            diagnostics.scope(timing)
        });

        let state = tree.state.downcast_mut::<State>();
        state.ime.synchronize(self.editor);
        match event {
            Event::Clipboard(clipboard::Event::Read(result)) => {
                if !state.is_focused
                    || state.window_unfocused
                    || state.ime.blocks_keyboard()
                    || self.on_action.is_none()
                {
                    state.paste.cancel();
                }
                if !state.paste.complete(self.editor) {
                    return;
                }
                if let Ok(content) = result
                    && let clipboard::Content::Text(text) = content.as_ref()
                    && !text.is_empty()
                    && let Some(on_action) = &self.on_action
                {
                    state.drag_anchor = None;
                    state.preferred_x = None;
                    state.caret_hint = None;
                    shell.publish(on_action(EditorAction::PasteText(text.clone())));
                }
                shell.capture_event();
            }
            Event::Window(window::Event::Unfocused) => {
                state.paste.cancel();
                state.window_unfocused = true;
                state.caret_blink.stop();
                state.preferred_x = None;
                state.drag_anchor = None;
                state.caret_hint = None;
                state.ime.cancel();
                shell.invalidate_layout();
                shell.request_redraw();
            }
            Event::Window(window::Event::Focused) => {
                state.window_unfocused = false;

                if state.is_focused {
                    state.caret_blink.reset(Instant::now());
                }

                shell.request_redraw();
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                state.paste.cancel();
                state.ime.cancel();
                shell.invalidate_layout();
                state.drag_anchor = None;

                let bounds = layout.bounds();

                let inside = bounds
                    .intersection(viewport)
                    .is_some_and(|visible| cursor.is_over(visible));

                let was_focused = state.is_focused;

                state.is_focused = self.on_action.is_some() && inside && !shell.is_event_captured();

                if state.is_focused != was_focused {
                    shell.request_redraw();
                }

                if !state.is_focused {
                    state.caret_blink.stop();
                    return;
                }

                state.caret_blink.reset(Instant::now());
                shell.request_redraw();

                shell.capture_event();
                let Some(on_action) = &self.on_action else {
                    return;
                };

                let Some(mouse_position) = cursor.position() else {
                    return;
                };

                let local = state.content_position(mouse_position, bounds.position());

                let position = {
                    #[cfg(feature = "perf-diagnostics")]
                    let _hit_test_timing = self.diagnostics.and_then(|diagnostics| {
                        diagnostics.operation_scope(DiagnosticOperation::HitTestClick)
                    });

                    state.hit_test(local)
                };

                if let Some(position) = position {
                    state.drag_anchor = Some(position);
                    shell.request_redraw();

                    shell.publish(on_action(EditorAction::SetSelection(
                        TextSelection::cursor(position),
                    )));
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                let Some(anchor) = state.drag_anchor else {
                    return;
                };

                if !state.is_focused || state.window_unfocused || state.has_error {
                    state.drag_anchor = None;
                    return;
                }

                let Some(on_action) = &self.on_action else {
                    state.drag_anchor = None;
                    return;
                };

                let bounds = layout.bounds();

                let Some(visible) = bounds.intersection(viewport) else {
                    state.drag_anchor = None;
                    return;
                };

                if visible.width <= 0.0 || visible.height <= 0.0 {
                    state.drag_anchor = None;
                    return;
                }

                let clamped = Point::new(
                    position.x.clamp(visible.x, visible.x + visible.width),
                    position.y.clamp(visible.y, visible.y + visible.height),
                );
                let local = state.content_position(clamped, bounds.position());

                let head = {
                    #[cfg(feature = "perf-diagnostics")]
                    let _hit_test_timing = self.diagnostics.and_then(|diagnostics| {
                        diagnostics.operation_scope(DiagnosticOperation::HitTestDrag)
                    });

                    state.hit_test(local)
                };

                if let Some(head) = head {
                    let selection = TextSelection::new(anchor, head);
                    shell.request_redraw();

                    if selection != self.editor.selection() {
                        shell.publish(on_action(EditorAction::SetSelection(selection)));
                    }
                }

                shell.capture_event();
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                if shell.is_event_captured() {
                    return;
                }

                let bounds = layout.bounds();

                let inside = bounds
                    .intersection(viewport)
                    .is_some_and(|visible| cursor.is_over(visible));

                if !inside {
                    return;
                }

                let delta_y = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => -*y * LINE_HEIGHT * 3.0,
                    mouse::ScrollDelta::Pixels { y, .. } => -*y,
                };

                if !delta_y.is_finite() || delta_y == 0.0 {
                    return;
                }

                if state.scroll_by(delta_y, bounds.height) {
                    shell.capture_event();
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.drag_anchor.take().is_some() {
                    shell.capture_event();
                }
            }

            Event::Mouse(mouse::Event::CursorLeft) => {
                state.drag_anchor = None;
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

                if matches!(key, keyboard::Key::Named(keyboard::key::Named::Process))
                    || state.ime.blocks_keyboard()
                {
                    state.paste.cancel();
                    shell.capture_event();
                    return;
                }
                if self.on_action.is_some() {
                    state.caret_blink.reset(Instant::now());
                    shell.request_redraw();
                }

                let clipboard_shortcut =
                    if modifiers.command() && !modifiers.alt() && !modifiers.shift() {
                        key.to_latin(*physical_key)
                    } else {
                        None
                    };

                if matches!(clipboard_shortcut, Some('c' | 'x')) {
                    let is_cut = clipboard_shortcut == Some('x');

                    // Cutting requires an editing callback.
                    if is_cut && self.on_action.is_none() {
                        return;
                    }

                    if self.editor.selection().is_empty() {
                        shell.capture_event();
                        return;
                    }

                    match self.editor.selected_text() {
                        Ok(text) => {
                            shell.write_clipboard(text);

                            if is_cut && let Some(on_action) = &self.on_action {
                                state.paste.cancel();
                                state.drag_anchor = None;
                                state.preferred_x = None;
                                state.caret_hint = None;
                                shell.publish(on_action(EditorAction::DeleteSelection));
                            }
                        }
                        Err(error) => {
                            eprintln!("Failed to extract selected text for clipboard: {error}");
                        }
                    }

                    shell.capture_event();
                    return;
                }

                let Some(on_action) = &self.on_action else {
                    return;
                };

                if clipboard_shortcut != Some('v') {
                    state.paste.cancel();
                }

                if let Some(down) = vertical_navigation_key(key, *modifiers) {
                    let selection = self.editor.selection();

                    let target = {
                        #[cfg(feature = "perf-diagnostics")]
                        let _navigation_timing = self.diagnostics.and_then(|diagnostics| {
                            diagnostics.operation_scope(DiagnosticOperation::NavigationVertical)
                        });

                        let preferred_x = state.preferred_x.or_else(|| {
                            state.caret_bounds(selection.head()).map(|bounds| bounds.x)
                        });

                        preferred_x.and_then(|preferred_x| {
                            state
                                .vertical_target(selection.head(), down, preferred_x)
                                .map(|(position, affinity)| (preferred_x, position, affinity))
                        })
                    };

                    if let Some((preferred_x, position, affinity)) = target {
                        let selection = if modifiers.shift() {
                            TextSelection::new(selection.anchor(), position)
                        } else {
                            TextSelection::cursor(position)
                        };

                        state.drag_anchor = None;
                        state.preferred_x = Some(preferred_x);
                        state.caret_hint = Some((position, affinity));

                        state.reveal_head(position, layout.bounds().height, PADDING);

                        shell.publish(on_action(EditorAction::SetSelection(selection)));
                        shell.request_redraw();
                    }

                    shell.capture_event();
                    return;
                }

                if let Some(end) = visual_line_boundary_key(key, *modifiers) {
                    let selection = self.editor.selection();

                    let target = {
                        #[cfg(feature = "perf-diagnostics")]
                        let _navigation_timing = self.diagnostics.and_then(|diagnostics| {
                            diagnostics.operation_scope(DiagnosticOperation::NavigationLineBoundary)
                        });

                        state.visual_line_boundary(selection.head(), end)
                    };

                    if let Some((position, affinity)) = target {
                        let selection = if modifiers.shift() {
                            TextSelection::new(selection.anchor(), position)
                        } else {
                            TextSelection::cursor(position)
                        };

                        state.drag_anchor = None;
                        state.preferred_x = None;
                        state.caret_hint = Some((position, affinity));

                        // A visual-side change may leave the document position unchanged.
                        state.reveal_head(position, layout.bounds().height, PADDING);

                        shell.publish(on_action(EditorAction::SetSelection(selection)));
                        shell.request_redraw();
                    }

                    shell.capture_event();
                    return;
                }

                if clipboard_shortcut == Some('v') {
                    if state.paste.request(self.editor) {
                        shell.read_clipboard(clipboard::Kind::Text);
                    }

                    shell.capture_event();
                    return;
                }

                if let Some(action) =
                    keyboard_action(key, *physical_key, *modifiers, text.as_deref())
                {
                    state.drag_anchor = None;
                    state.caret_hint = None;
                    state.preferred_x = None;
                    shell.publish(on_action(action));
                    shell.capture_event();
                }
            }
            Event::Window(window::Event::RedrawRequested(_)) => {
                let composing = state.ime.preedit().is_some();

                let visible_viewport = layout
                    .bounds()
                    .intersection(viewport)
                    .is_some_and(|visible| visible.width > 0.0 && visible.height > 0.0);

                let active = state.is_focused
                    && !state.window_unfocused
                    && !state.has_error
                    && self.on_action.is_some()
                    && visible_viewport
                    && (self.editor.selection().is_empty() || composing);

                if let Some(deadline) =
                    state
                        .caret_blink
                        .next_redraw(Instant::now(), active, composing)
                {
                    shell.request_redraw_at(deadline);
                }

                let input_method = if state.is_focused
                    && !state.window_unfocused
                    && !state.has_error
                    && self.on_action.is_some()
                {
                    state
                        .caret_bounds(self.editor.selection().head())
                        .map(|caret| {
                            let bounds = layout.bounds();

                            InputMethod::Enabled {
                                cursor: Rectangle {
                                    x: bounds.x + caret.x,
                                    y: bounds.y + caret.y - state.scroll_y,
                                    width: caret.width,
                                    height: caret.height,
                                },
                                purpose: Purpose::Normal,
                                preedit: None::<input_method::Preedit<&str>>,
                            }
                        })
                        .unwrap_or(InputMethod::Disabled)
                } else {
                    InputMethod::Disabled
                };

                shell.request_input_method(&input_method);
            }
            Event::InputMethod(event) => {
                if matches!(
                    event,
                    input_method::Event::Preedit(..) | input_method::Event::Commit(_)
                ) {
                    state.paste.cancel();
                }
                if !state.is_focused
                    || state.window_unfocused
                    || state.has_error
                    || shell.is_event_captured()
                {
                    state.ime.cancel();

                    if matches!(event, input_method::Event::Closed) {
                        state.ime.close();
                    }

                    shell.invalidate_layout();
                    shell.request_redraw();
                    return;
                }

                let Some(on_action) = &self.on_action else {
                    state.ime.cancel();
                    shell.invalidate_layout();
                    shell.request_redraw();
                    return;
                };

                if let Some(action) = state.ime.handle(event, self.editor) {
                    state.drag_anchor = None;
                    state.caret_hint = None;
                    state.preferred_x = None;

                    shell.publish(on_action(action));
                }

                shell.capture_event();
                shell.invalidate_layout();
                shell.request_redraw();
            }
            _ => {}
        }
    }
}

impl<'a, Message, Renderer> From<RichEdit<'a, Message>>
    for Element<'a, Message, iced::Theme, Renderer>
where
    Message: 'a,
    Renderer: text::Renderer<Paragraph = Paragraph> + 'a,
{
    fn from(widget: RichEdit<'a, Message>) -> Self {
        widget._boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unmarked_text_preserves_the_base_font() {
        let base = Font {
            weight: Weight::Light,
            style: Style::Italic,
            ..Font::MONOSPACE
        };

        assert_eq!(font_for_marks(base, &[]), base);
    }

    #[test]
    fn strong_and_emphasis_combine_independently_of_order() {
        let base = Font::MONOSPACE;

        let expected = Font {
            weight: Weight::Bold,
            style: Style::Italic,
            ..base
        };

        assert_eq!(
            font_for_marks(base, &[Mark::Strong, Mark::Emphasis]),
            expected,
        );

        assert_eq!(
            font_for_marks(base, &[Mark::Emphasis, Mark::Strong]),
            expected,
        );
    }

    #[test]
    fn code_changes_only_the_font_family() {
        let base = Font {
            weight: Weight::Light,
            style: Style::Italic,
            stretch: iced::font::Stretch::Expanded,
            ..Font::DEFAULT
        };

        let expected = Font {
            family: Font::MONOSPACE.family,
            ..base
        };

        assert_eq!(font_for_marks(base, &[Mark::Code]), expected);
    }

    #[test]
    fn code_strong_and_emphasis_combine_independently_of_order() {
        let expected = Font {
            weight: Weight::Bold,
            style: Style::Italic,
            ..Font::MONOSPACE
        };

        for marks in [
            [Mark::Code, Mark::Strong, Mark::Emphasis],
            [Mark::Emphasis, Mark::Code, Mark::Strong],
            [Mark::Strong, Mark::Emphasis, Mark::Code],
        ] {
            assert_eq!(font_for_marks(Font::DEFAULT, &marks), expected);
        }
    }

    #[test]
    fn link_spans_use_the_supplied_color_without_resetting_font_marks() {
        let marks = [
            Mark::Code,
            Mark::Strong,
            Mark::Emphasis,
            Mark::Link {
                href: "https://iced.rs/".to_owned(),
                title: None,
            },
        ];

        for color in [
            Color::from_rgb8(0, 102, 204),
            Color::from_rgb8(120, 190, 255),
        ] {
            let span = span_for_marks("link", &marks, Font::DEFAULT, color);

            assert_eq!(span.color, Some(color));
            assert_eq!(
                span.font,
                Some(Font {
                    weight: Weight::Bold,
                    style: Style::Italic,
                    ..Font::MONOSPACE
                })
            );
        }

        let plain = span_for_marks("plain", &[], Font::DEFAULT, Color::from_rgb8(0, 102, 204));

        assert_eq!(plain.color, None);
    }
}
