use iced::{
    Element, Length, Widget as _,
    widget::{button, column, container, row, text},
};

use iced_richedit::{
    document::{Document, Mark},
    editor::{EditorAction, EditorState},
    widget::RichEdit,
};

struct Basic {
    editor: EditorState,
    error: Option<String>,
}

#[derive(Debug, Clone)]
enum Message {
    Edit(EditorAction),
    ToggleStrong,
    ToggleEmphasis,
}

impl Basic {
    fn new() -> Self {
        let mut value = serde_json::json!({
            "type": "doc",
            "content": [
                {
                    "type": "paragraph",
                    "content": [
                        {
                            "type": "text",
                            "text": "Normal text. "
                        },
                        {
                            "type": "text",
                            "text": "Bold text. ",
                            "marks": [
                                { "type": "strong" }
                            ]
                        },
                        {
                            "type": "text",
                            "text": "Italic text. ",
                            "marks": [
                                { "type": "em" }
                            ]
                        },
                        {
                            "type": "text",
                            "text": "Bold italic text. ",
                            "marks": [
                                { "type": "strong" },
                                { "type": "em" }
                            ]
                        },
                        {
                            "type": "text",
                            "text": "Resize the window to check wrapping across style boundaries."
                        }
                    ]
                },
                {
                    "type": "paragraph"
                },
                {
                    "type": "paragraph",
                    "content": [
                        {
                            "type": "text",
                            "text": "中文文本、emoji 😀、组合字符 e\u{301} 。"
                        }
                    ]
                },
                {
                    "type": "paragraph",
                    "content": [
                        {
                            "type": "text",
                            "text": "Before the hard break."
                        },
                        {
                            "type": "hard_break"
                        },
                        {
                            "type": "text",
                            "text": "After the hard break."
                        }
                    ]
                }
            ]
        });

        let content = value["content"]
            .as_array_mut()
            .expect("the built-in example must contain a content array");

        for index in 1..=20 {
            content.push(serde_json::json!({
                "type": "paragraph",
                "content": [
                    {
                        "type": "text",
                        "text": format!(
                            "Scrollable paragraph {index}. Click here after scrolling to verify document position mapping."
                        )
                    }
                ]
            }));
        }
        let document = Document::from_json(&value.to_string())
            .expect("the built-in example document must be valid");
        let editor = EditorState::new(document)
            .expect("the example document must have an initial selection");

        Self {
            editor,
            error: None,
        }
    }

    fn update(&mut self, message: Message) {
        let result = match message {
            Message::Edit(action) => self.editor.apply(action),
            Message::ToggleStrong => self.editor.toggle_mark(Mark::Strong),
            Message::ToggleEmphasis => self.editor.toggle_mark(Mark::Emphasis),
        };

        self.error = result.err().map(|error| error.to_string());
    }

    fn view(&self) -> Element<'_, Message> {
        let editor = RichEdit::new(&self.editor)
            .width(Length::Fill)
            .height(Length::Fixed(300.0))
            .on_action(Message::Edit);

        let controls = row![
            button("Insert")
                .on_press(Message::Edit(EditorAction::InsertText("Hello ".to_owned()),)),
            button("Enter").on_press(Message::Edit(EditorAction::SplitParagraph)),
            button("Backspace").on_press(Message::Edit(EditorAction::DeleteBackward)),
            button("Left").on_press(Message::Edit(EditorAction::MoveLeft)),
            button("Right").on_press(Message::Edit(EditorAction::MoveRight)),
            button("Undo").on_press_maybe(
                self.editor
                    .can_undo()
                    .then_some(Message::Edit(EditorAction::Undo)),
            ),
            button("Redo").on_press_maybe(
                self.editor
                    .can_redo()
                    .then_some(Message::Edit(EditorAction::Redo)),
            ),
        ]
        .spacing(8);
        let formatting = row![
            button("Bold").on_press(Message::ToggleStrong),
            button("Italic").on_press(Message::ToggleEmphasis),
        ]
        .spacing(8);
        let status = self.error.as_deref().unwrap_or(
            "Select text and use Bold or Italic to change formatting. \
        Type to replace the selection. Try undo/redo, clipboard operations, \
        and IME input. Platform and input method coverage is limited.",
        );

        container(
            column![
                text("Rich editor: action dispatch"),
                controls,
                formatting,
                editor,
                text(status),
            ]
            .spacing(12),
        )
        .padding(20)
        .width(Length::Fill)
        .boxed()
    }
}

fn main() -> iced::Result {
    iced::application(Basic::new, Basic::update, Basic::view)
        .title("Rich editor — Basic")
        .run()
}
