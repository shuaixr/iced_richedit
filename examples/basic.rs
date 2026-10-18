use iced::{
    Element, Length,
    widget::{button, column, container, row, text},
};

use iced_richedit::{
    document::Document,
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
}

impl Basic {
    fn new() -> Self {
        let value = serde_json::json!({
            "type": "doc",
            "content": [
                {
                    "type": "paragraph",
                    "content": [{
                        "type": "text",
                        "text": "This is a long paragraph for checking utomatic wrapping. Resize the window to verify that the text layout follows the available editor width."
                    }]
                },
                {
                    "type": "paragraph"
                },
                {
                    "type": "paragraph",
                    "content": [{
                        "type": "text",
                        "text": "中文文本、emoji 😀、组合字符 e\u{301} 。"
                    }]
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

        let document = Document::from_json(&value.to_string())
            .expect("the built-in example document must be valid");
        let editor =
            EditorState::new(document).expect("the empty document must have an initial selection");

        Self {
            editor,
            error: None,
        }
    }

    fn update(&mut self, message: Message) {
        match message {
            Message::Edit(action) => {
                self.error = self
                    .editor
                    .apply(action)
                    .err()
                    .map(|error| error.to_string());
            }
        }
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

        let status = self.error.as_deref().unwrap_or(
            "Use the buttons to verify action dispatch. Keyboard input is not connected yet.",
        );

        container(
            column![
                text("Rich editor: action dispatch"),
                controls,
                editor,
                text(status),
            ]
            .spacing(12),
        )
        .padding(20)
        .width(Length::Fill)
        .into()
    }
}

fn main() -> iced::Result {
    iced::application(Basic::new, Basic::update, Basic::view)
        .title("Rich editor — Basic")
        .run()
}
