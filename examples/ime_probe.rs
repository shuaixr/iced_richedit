use iced::{
    Element, Event, Length, Subscription, event,
    widget::{column, container, text, text_input},
    window,
};

#[derive(Default)]
struct ImeProbe {
    primary: String,
    secondary: String,
    event_sequence: u64,
}

#[derive(Debug, Clone)]
enum Message {
    PrimaryChanged(String),
    SecondaryChanged(String),
    Observed(Event, event::Status),
}

impl ImeProbe {
    fn update(&mut self, message: Message) {
        match message {
            Message::PrimaryChanged(value) => {
                eprintln!("[primary value] {value:?}");
                self.primary = value;
            }
            Message::SecondaryChanged(value) => {
                eprintln!("[secondary value] {value:?}");
                self.secondary = value;
            }
            Message::Observed(event, status) => {
                self.event_sequence += 1;

                eprintln!(
                    "[event {}] status={status:?} {event:?}",
                    self.event_sequence,
                );
            }
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        event::listen_with(observe_input)
    }

    fn view(&self) -> Element<'_, Message> {
        let content = column![
            text("Native input event probe").size(24),
            text(
                "Use synthetic text only. Input events and field values \
                    are printed to the terminal."
            ),
            text("Primary field"),
            text_input("Type here", &self.primary)
                .on_input(Message::PrimaryChanged)
                .padding(12),
            text("Secondary field"),
            text_input("Click here during composition", &self.secondary)
                .on_input(Message::SecondaryChanged)
                .padding(12),
            text(
                "Test ordinary typing, IME commit/cancel, selection \
                    replacement, focus changes, AltGr and dead keys."
            ),
        ]
        .spacing(12);

        container(content).padding(24).width(Length::Fill).into()
    }
}

fn observe_input(event: Event, status: event::Status, _window: window::Id) -> Option<Message> {
    let relevant = matches!(
        &event,
        Event::Keyboard(_)
            | Event::InputMethod(_)
            | Event::Window(window::Event::Focused)
            | Event::Window(window::Event::Unfocused)
    );

    relevant.then_some(Message::Observed(event, status))
}

fn main() -> iced::Result {
    iced::application(ImeProbe::default, ImeProbe::update, ImeProbe::view)
        .title("IME Event Probe")
        .subscription(ImeProbe::subscription)
        .run()
}
