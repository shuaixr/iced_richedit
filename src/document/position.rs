#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Position(usize);

impl Position {
    pub const fn from_utf16_offset(utf16_offset: usize) -> Self {
        Self(utf16_offset)
    }

    pub const fn utf16_offset(self) -> usize {
        self.0
    }
}
