use iced::{
    Element, Length,
    widget::{column, container, text},
};

use iced_richedit::{document::Document, editor::EditorState, widget::RichEdit};

struct Basic {
    editor: EditorState,
}

#[derive(Debug, Clone)]
enum Message {}

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

        Self { editor }
    }

    fn update(&mut self, message: Message) {
        match message {}
    }

    fn view(&self) -> Element<'_, Message> {
        let editor = RichEdit::new(&self.editor)
            .width(Length::Fill)
            .height(Length::Fixed(300.0));

        container(
            column![
                text("Rich editor: static widget skeleton"),
                editor,
                text("Text layout and editing are not connected yet."),
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
