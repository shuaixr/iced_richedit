use super::Position;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextSelection {
    anchor: Position,
    head: Position,
}

impl TextSelection {
    pub const fn cursor(position: Position) -> Self {
        Self {
            anchor: position,
            head: position,
        }
    }
    pub const fn new(anchor: Position, head: Position) -> Self {
        Self { anchor, head }
    }
    pub const fn anchor(self) -> Position {
        self.anchor
    }
    pub const fn head(self) -> Position {
        self.head
    }
    pub fn from(self) -> Position {
        self.anchor.min(self.head)
    }
    pub fn to(self) -> Position {
        self.anchor.max(self.head)
    }
    pub fn is_empty(self) -> bool {
        self.anchor == self.head
    }
}

#[cfg(test)]
mod tests {
    use super::TextSelection;
    use crate::document::Position;

    #[test]
    fn cursor_has_equal_endpoints() {
        let position = Position::from_utf16_offset(5);
        let selection = TextSelection::cursor(position);

        assert_eq!(selection.anchor(), position);
        assert_eq!(selection.head(), position);
        assert!(selection.is_empty());
    }

    #[test]
    fn forward_selection_preserves_direction() {
        let anchor = Position::from_utf16_offset(3);
        let head = Position::from_utf16_offset(7);
        let selection = TextSelection::new(anchor, head);

        assert_eq!(selection.anchor(), anchor);
        assert_eq!(selection.head(), head);
        assert_eq!(selection.from(), anchor);
        assert_eq!(selection.to(), head);
        assert!(!selection.is_empty());
    }

    #[test]
    fn backward_selection_returns_sorted_bounds() {
        let anchor = Position::from_utf16_offset(7);
        let head = Position::from_utf16_offset(3);
        let selection = TextSelection::new(anchor, head);

        assert_eq!(selection.anchor(), anchor);
        assert_eq!(selection.head(), head);
        assert_eq!(selection.from(), Position::from_utf16_offset(3));
        assert_eq!(selection.to(), Position::from_utf16_offset(7));
        assert!(!selection.is_empty());
    }
}
