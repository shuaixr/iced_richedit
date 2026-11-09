use iced::{
    Element, Length, Widget as _,
    widget::{button, column, container, row, text},
};
use iced_richedit::{
    document::{Document, Mark},
    editor::{EditorAction, EditorState},
    widget::RichEdit,
};

#[cfg(feature = "perf-diagnostics")]
use iced_richedit::diagnostics::{Diagnostics, Operation};

#[cfg(feature = "perf-diagnostics")]
use std::{
    error::Error,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

struct Basic {
    editor: EditorState,
    error: Option<String>,

    #[cfg(feature = "perf-diagnostics")]
    diagnostics: Diagnostics,
    #[cfg(feature = "perf-diagnostics")]
    measurement_context: Option<serde_json::Value>,
    #[cfg(feature = "perf-diagnostics")]
    performance_status: String,
}

#[cfg(feature = "perf-diagnostics")]
#[derive(Debug, Clone, Copy)]
enum PerformanceMessage {
    Start,
    Stop,
    Export,
}

#[derive(Debug, Clone)]
enum Message {
    Edit(EditorAction),
    ToggleStrong,
    ToggleEmphasis,
    ToggleUnderline,
    ToggleCode,
    SetLink,
    SetHeading(u8),
    SetParagraph,
    SetCodeBlock,
    RemoveLink,

    #[cfg(feature = "perf-diagnostics")]
    Performance(PerformanceMessage),
}
#[cfg(feature = "perf-diagnostics")]
fn performance_operation(message: &Message) -> Option<Operation> {
    Some(match message {
        Message::Performance(_) => return None,

        Message::Edit(action) => match action {
            EditorAction::InsertText(_) | EditorAction::SplitParagraph => Operation::EditInput,

            EditorAction::PasteText(_) => Operation::EditPaste,

            EditorAction::CommitComposition(_)
            | EditorAction::CommitCompositionInContext { .. } => Operation::EditImeCommit,

            EditorAction::DeleteBackward
            | EditorAction::DeleteForward
            | EditorAction::DeleteSelection
            | EditorAction::DeleteWordBackward
            | EditorAction::DeleteWordForward => Operation::EditDelete,

            EditorAction::Undo | EditorAction::Redo => Operation::EditHistory,

            EditorAction::SetSelection(_)
            | EditorAction::MoveDocumentStart
            | EditorAction::MoveDocumentEnd
            | EditorAction::MoveLeft
            | EditorAction::MoveRight
            | EditorAction::MoveWordLeft
            | EditorAction::MoveWordRight
            | EditorAction::ExtendWordLeft
            | EditorAction::ExtendWordRight
            | EditorAction::ExtendDocumentStart
            | EditorAction::ExtendDocumentEnd
            | EditorAction::ExtendLeft
            | EditorAction::ExtendRight
            | EditorAction::SelectAll => Operation::EditSelection,
        },

        Message::ToggleStrong
        | Message::ToggleEmphasis
        | Message::ToggleUnderline
        | Message::ToggleCode
        | Message::SetLink
        | Message::RemoveLink
        | Message::SetHeading(_)
        | Message::SetParagraph
        | Message::SetCodeBlock => Operation::EditFormat,
    })
}

#[cfg(feature = "perf-diagnostics")]
fn performance_metadata(variable: &str) -> Result<Option<String>, Box<dyn Error>> {
    let value = match std::env::var(variable) {
        Ok(value) => value,
        Err(std::env::VarError::NotPresent) => return Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => {
            return Err(io::Error::other(format!("{variable} must contain Unicode text.")).into());
        }
    };

    let value = value.trim();

    if value.is_empty() {
        return Ok(None);
    }

    if value.chars().count() > 128 {
        return Err(io::Error::other(format!(
            "{variable} must contain at most 128 Unicode scalar values."
        ))
        .into());
    }

    Ok(Some(value.to_owned()))
}

impl Basic {
    fn new() -> Self {
        let mut value = serde_json::json!({
            "type": "doc",
            "content": [
                {
                    "type": "heading",
                    "attrs": {
                        "level": 1
                    },
                    "content": [
                        {
                            "type": "text",
                            "text": "Rich text editor"
                        }
                    ]
                },
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
                                {
                                    "type": "strong"
                                }
                            ]
                        },
                        {
                            "type": "text",
                            "text": "Italic text. ",
                            "marks": [
                                {
                                    "type": "em"
                                }
                            ]
                        },
                        {
                            "type": "text",
                            "text": "Bold italic text. ",
                            "marks": [
                                {
                                    "type": "strong"
                                },
                                {
                                    "type": "em"
                                }
                            ]
                        },
                        {
                            "type": "text",
                            "text": "Underlined text. ",
                            "marks": [
                                {
                                    "type": "underline"
                                }
                            ]
                        },
                        {
                            "type": "text",
                            "text": "Bold underlined text. ",
                            "marks": [
                                {
                                    "type": "strong"
                                },
                                {
                                    "type": "underline"
                                }
                            ]
                        },
                        {
                            "type": "text",
                            "text": "let answer = 42; ",
                            "marks": [
                                {
                                    "type": "code"
                                }
                            ]
                        },
                        {
                            "type": "text",
                            "text": "Bold underlined code. ",
                            "marks": [
                                {
                                    "type": "code"
                                },
                                {
                                    "type": "strong"
                                },
                                {
                                    "type": "underline"
                                }
                            ]
                        },
                        {
                            "type": "text",
                            "text": "Link text. ",
                            "marks": [
                                {
                                    "type": "link",
                                    "attrs": {
                                        "href": "https://iced.rs/",
                                        "title": "iced"
                                    }
                                }
                            ]
                        },
                        {
                            "type": "text",
                            "text": "Bold underlined code link. ",
                            "marks": [
                                {
                                    "type": "strong"
                                },
                                {
                                    "type": "underline"
                                },
                                {
                                    "type": "code"
                                },
                                {
                                    "type": "link",
                                    "attrs": {
                                        "href": "https://iced.rs/",
                                        "title": "iced"
                                    }
                                }
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
        content.extend([
            serde_json::json!({
                "type": "paragraph",
                "content": [
                    {
                        "type": "text",
                        "text": "Code block: Enter inserts a newline; Backspace at a nonempty block start joins the previous block."
                    }
                ]
            }),
            serde_json::json!({
                "type": "code_block",
                "content": [
                    {
                        "type": "text",
                        "text": "fn main() {\n    let greeting = \"Hello, 😀\";\n\n    println!(\"{greeting}\");\n}\n"
                    }
                ]
            }),
            serde_json::json!({
                "type": "paragraph",
                "content": [
                    {
                        "type": "text",
                        "text": "The next block is empty code. Enter stays in code; use Body or start-of-block Backspace to exit."
                    }
                ]
            }),
            serde_json::json!({
                "type": "code_block"
            }),
            serde_json::json!({
                "type": "paragraph",
                "content": [
                    {
                        "type": "text",
                        "text": "Paragraph after code. Inline formatting in code is rejected; boundary deletion joins supported text blocks."
                    }
                ]
            }),
        ]);

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

            #[cfg(feature = "perf-diagnostics")]
            diagnostics: Diagnostics::default(),

            #[cfg(feature = "perf-diagnostics")]
            measurement_context: None,

            #[cfg(feature = "perf-diagnostics")]
            performance_status: String::from(
                "Diagnostics idle. Warm up the editor before starting.",
            ),
        }
    }

    fn update(&mut self, message: Message) {
        #[cfg(feature = "perf-diagnostics")]
        let edit_timing = performance_operation(&message)
            .and_then(|operation| self.diagnostics.operation_scope(operation));
        let result = match message {
            Message::Edit(action) => self.editor.apply(action),
            Message::ToggleStrong => self.editor.toggle_mark(Mark::Strong),
            Message::ToggleEmphasis => self.editor.toggle_mark(Mark::Emphasis),
            Message::ToggleUnderline => self.editor.toggle_mark(Mark::Underline),
            Message::ToggleCode => self.editor.toggle_mark(Mark::Code),
            Message::SetLink => self
                .editor
                .set_link("https://iced.rs/".to_owned(), Some("iced".to_owned())),
            Message::SetHeading(level) => self.editor.set_heading(level),
            Message::SetParagraph => self.editor.set_paragraph(),
            Message::SetCodeBlock => self.editor.set_code_block(),
            Message::RemoveLink => self.editor.remove_link(),

            #[cfg(feature = "perf-diagnostics")]
            Message::Performance(action) => {
                drop(edit_timing);
                self.update_performance(action);
                return;
            }
        };

        #[cfg(feature = "perf-diagnostics")]
        drop(edit_timing);

        self.error = result.err().map(|error| error.to_string());
    }

    fn view(&self) -> Element<'_, Message> {
        let editor = RichEdit::new(&self.editor)
            .width(Length::Fill)
            .height(Length::Fixed(300.0))
            .on_action(Message::Edit);

        #[cfg(feature = "perf-diagnostics")]
        let editor = editor.diagnostics(&self.diagnostics);

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
            button("Underline").on_press(Message::ToggleUnderline),
            button("Inline Code").on_press(Message::ToggleCode),
            button("Set link").on_press(Message::SetLink),
            button("Remove link").on_press(Message::RemoveLink),
        ]
        .spacing(8);
        let blocks = row![
            button("H1").on_press(Message::SetHeading(1)),
            button("H2").on_press(Message::SetHeading(2)),
            button("Code Block").on_press(Message::SetCodeBlock),
            button("Body").on_press(Message::SetParagraph),
        ]
        .spacing(8);

        #[cfg(feature = "perf-diagnostics")]
        let performance_controls = {
            let recording = self.diagnostics.is_recording();

            row![
                button("Start measurement").on_press_maybe(
                    (!recording).then_some(Message::Performance(PerformanceMessage::Start,)),
                ),
                button("Stop").on_press_maybe(
                    recording.then_some(Message::Performance(PerformanceMessage::Stop,)),
                ),
                button("Export report").on_press_maybe(
                    (!recording && self.measurement_context.is_some())
                        .then_some(Message::Performance(PerformanceMessage::Export),),
                ),
            ]
            .spacing(8)
        };

        let status = self.error.as_deref().unwrap_or(
            "Use H1, H2, Body, or Code block to change block type. \
       Inline formatting is unavailable in code blocks. \
       Try undo/redo, clipboard operations, and IME input. \
       Platform and input method coverage is limited.",
        );

        let content = column![
            text("Rich editor: action dispatch"),
            controls,
            formatting,
            blocks,
        ]
        .spacing(12);

        #[cfg(feature = "perf-diagnostics")]
        let content = content
            .push(performance_controls.boxed())
            .push(text(self.performance_status.as_str()).boxed());

        let content = content.push(editor.boxed()).push(text(status).boxed());

        container(content).padding(20).width(Length::Fill).boxed()
    }

    #[cfg(feature = "perf-diagnostics")]
    fn capture_measurement_context(&self) -> Result<serde_json::Value, Box<dyn Error>> {
        let measurement_label = performance_metadata("RICHEDIT_PERF_LABEL")?;

        let code_version = performance_metadata("RICHEDIT_PERF_CODE_VERSION")?;

        let initial_document = self.capture_document_size()?;

        Ok(serde_json::json!({
            "measurement_label": measurement_label,
            "code_version": code_version,
            "user_metadata": {
                "source": "explicit_environment_variables",
                "captured_at": "before_measurement_start",
                "maximum_unicode_scalar_values_per_field": 128,
                "privacy_responsibility": "user_supplied_do_not_include_sensitive_information"
            },
            "workload": "manual_interaction",
            "workload_semantics": "host_declared_not_automatically_classified",
            "operation_classification_source": "instrumented_operation_categories",
            "warmup": "user_controlled",
            "initial_document": initial_document,
            "final_document": null,
            "final_document_error": null,
            "requested_editor_height_logical_px": 300.0
        }))
    }

    #[cfg(feature = "perf-diagnostics")]
    fn update_performance(&mut self, message: PerformanceMessage) {
        match message {
            PerformanceMessage::Start => {
                self.diagnostics.stop();

                match self.capture_measurement_context() {
                    Ok(context) => {
                        self.measurement_context = Some(context);
                        self.performance_status =
                            "Recording. Perform one interaction scenario, then stop.".to_owned();
                        self.error = None;

                        self.diagnostics.start();
                    }
                    Err(error) => {
                        self.performance_status = "Unable to start measurement.".to_owned();
                        self.error = Some(error.to_string());
                    }
                }
            }
            PerformanceMessage::Stop => {
                if !self.diagnostics.is_recording() {
                    return;
                }

                self.diagnostics.stop();

                let final_document = self.capture_document_size();

                match final_document {
                    Ok(size) => {
                        if let Some(context) = self.measurement_context.as_mut() {
                            context["final_document"] = size;
                            context["final_document_error"] = serde_json::Value::Null;
                        }

                        self.performance_status =
                            "Measurement stopped. Export the report before starting again."
                                .to_owned();
                        self.error = None;
                    }
                    Err(error) => {
                        if let Some(context) = self.measurement_context.as_mut() {
                            context["final_document"] = serde_json::Value::Null;
                            context["final_document_error"] = serde_json::json!("capture_failed");
                        }

                        self.performance_status =
                            "Measurement stopped; final document size is unavailable.".to_owned();
                        self.error = Some(error.to_string());
                    }
                }
            }
            PerformanceMessage::Export => match self.export_performance_report() {
                Ok(path) => {
                    self.performance_status = format!("Report exported: {}", path.display());
                    self.error = None;
                }
                Err(error) => {
                    self.error = Some(error.to_string());
                }
            },
        }
    }

    #[cfg(feature = "perf-diagnostics")]
    fn export_performance_report(&self) -> Result<PathBuf, Box<dyn Error>> {
        if self.diagnostics.is_recording() {
            return Err(io::Error::other("Stop measurement before exporting a report.").into());
        }

        let context = self
            .measurement_context
            .as_ref()
            .ok_or_else(|| io::Error::other("Start a measurement before exporting a report."))?;

        let mut report = self.diagnostics.snapshot();
        report["context"] = context.clone();

        let encoded = serde_json::to_vec_pretty(&report)?;

        let directory = std::env::current_dir()?.join("target").join("perf-reports");

        fs::create_dir_all(&directory)?;

        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();

        let path = directory.join(format!("basic-{timestamp}.json",));

        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;

        file.write_all(&encoded)?;

        Ok(path)
    }

    #[cfg(feature = "perf-diagnostics")]
    fn capture_document_size(&self) -> Result<serde_json::Value, Box<dyn Error>> {
        let encoded = self.editor.document().to_json()?;
        let document: serde_json::Value = serde_json::from_str(&encoded)?;

        let top_level_blocks = document
            .get("content")
            .and_then(serde_json::Value::as_array)
            .map_or(0, Vec::len);

        let plain_text_utf16_units = self.editor.document().text().encode_utf16().count();

        Ok(serde_json::json!({
            "top_level_blocks": top_level_blocks,
            "plain_text_utf16_units": plain_text_utf16_units,
            "serialized_json_bytes": encoded.len()
        }))
    }
}

fn main() -> iced::Result {
    iced::application(Basic::new, Basic::update, Basic::view)
        .title("Rich editor — Basic")
        .run()
}
