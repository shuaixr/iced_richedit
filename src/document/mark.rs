#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mark {
    Strong,
    Emphasis,
    Underline,
    Code,
    Link { href: String, title: Option<String> },
}
