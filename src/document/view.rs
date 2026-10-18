use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

use super::Position;

/// A renderer-independent position within a paragraph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TextPosition {
    pub(crate) line: usize,
    pub(crate) byte: usize,
}

/// A read-only text projection of one document paragraph.
pub(crate) struct ParagraphView {
    start: Position,
    text: String,
    lines: Vec<Range<usize>>,
}

impl ParagraphView {
    pub(super) fn new(start: Position, text: String) -> Self {
        let lines = logical_lines(&text);

        Self { start, text, lines }
    }

    pub(crate) fn start(&self) -> Position {
        self.start
    }

    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    pub(crate) fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub(crate) fn line_text(&self, line: usize) -> Option<&str> {
        let range = self.lines.get(line)?;
        self.text.get(range.clone())
    }

    /// Converts a valid document grapheme boundary to a logical text position.
    ///
    /// Positions inside a line separator are canonicalized to the next line.
    pub(crate) fn text_position(&self, position: Position) -> Option<TextPosition> {
        let local = position
            .utf16_offset()
            .checked_sub(self.start.utf16_offset())?;

        let byte = self.byte_at_utf16_boundary(local)?;

        for (line, range) in self.lines.iter().enumerate() {
            if byte < range.start {
                return Some(TextPosition { line, byte: 0 });
            }

            if byte <= range.end {
                return Some(TextPosition {
                    line,
                    byte: byte - range.start,
                });
            }
        }

        None
    }

    /// Converts a logical UTF-8 position to a document grapheme boundary.
    ///
    /// A position inside a grapheme is snapped forward.
    pub(crate) fn document_position(&self, position: TextPosition) -> Option<Position> {
        let range = self.lines.get(position.line)?;
        let line_len = range.end - range.start;

        if position.byte > line_len {
            return None;
        }

        let byte = range.start + position.byte;

        if !self.text.is_char_boundary(byte) {
            return None;
        }

        let snapped = self.snap_byte_forward(byte)?;

        let local_utf16 = self.text[..snapped].encode_utf16().count();
        let offset = self.start.utf16_offset().checked_add(local_utf16)?;

        Some(Position::from_utf16_offset(offset))
    }

    fn byte_at_utf16_boundary(&self, target: usize) -> Option<usize> {
        if target == 0 {
            return Some(0);
        }

        let mut utf16 = 0;

        for (byte, grapheme) in self.text.grapheme_indices(true) {
            utf16 += grapheme.encode_utf16().count();

            if utf16 == target {
                return Some(byte + grapheme.len());
            }

            if utf16 > target {
                return None;
            }
        }

        None
    }

    fn snap_byte_forward(&self, target: usize) -> Option<usize> {
        if target == 0 {
            return Some(0);
        }

        for (byte, grapheme) in self.text.grapheme_indices(true) {
            let end = byte + grapheme.len();

            if end >= target {
                return Some(end);
            }
        }

        None
    }
}

/// Mirrors cosmic-text 0.15's CR/LF separator recognition.
///
/// Unlike LineIter itself, this includes an empty trailing logical line,
/// matching the text buffer's handling of empty and newline-terminated text.
fn logical_lines(text: &str) -> Vec<Range<usize>> {
    let bytes = text.as_bytes();
    let mut lines = Vec::new();
    let mut start = 0;
    let mut index = 0;

    while index < bytes.len() {
        let current = bytes[index];

        if current != b'\r' && current != b'\n' {
            index += 1;
            continue;
        }

        lines.push(start..index);

        let paired = matches!(
            (current, bytes.get(index + 1).copied()),
            (b'\r', Some(b'\n')) | (b'\n', Some(b'\r'))
        );

        index += if paired { 2 } else { 1 };
        start = index;
    }

    lines.push(start..text.len());
    lines
}

#[cfg(test)]
mod tests {
    use super::{ParagraphView, TextPosition};
    use crate::document::Position;

    #[test]
    fn maps_utf16_positions_to_logical_utf8_positions() {
        let view = ParagraphView::new(Position::from_utf16_offset(1), "A😀\ne\u{301}".to_owned());

        assert_eq!(view.line_count(), 2);
        assert_eq!(view.line_text(0), Some("A😀"));
        assert_eq!(view.line_text(1), Some("e\u{301}"));

        for (offset, line, byte) in [(1, 0, 0), (2, 0, 1), (4, 0, 5), (5, 1, 0), (7, 1, 3)] {
            let document = Position::from_utf16_offset(offset);
            let text = TextPosition { line, byte };

            assert_eq!(view.text_position(document), Some(text));
            assert_eq!(view.document_position(text), Some(document));
        }

        // The middle of the emoji's UTF-16 surrogate pair is invalid.
        assert_eq!(view.text_position(Position::from_utf16_offset(3)), None);

        // A UTF-8 character boundary inside the combined grapheme snaps forward.
        assert_eq!(
            view.document_position(TextPosition { line: 1, byte: 1 }),
            Some(Position::from_utf16_offset(7))
        );
    }

    #[test]
    fn preserves_empty_and_trailing_lines() {
        let empty = ParagraphView::new(Position::from_utf16_offset(1), String::new());

        assert_eq!(empty.line_count(), 1);
        assert_eq!(empty.line_text(0), Some(""));
        assert_eq!(
            empty.document_position(TextPosition { line: 0, byte: 0 }),
            Some(Position::from_utf16_offset(1))
        );

        let trailing = ParagraphView::new(Position::from_utf16_offset(1), "A\n".to_owned());

        assert_eq!(trailing.line_count(), 2);
        assert_eq!(trailing.line_text(1), Some(""));
        assert_eq!(
            trailing.document_position(TextPosition { line: 1, byte: 0 }),
            Some(Position::from_utf16_offset(3))
        );
    }

    #[test]
    fn recognizes_crlf_and_lfcr_separators() {
        let view = ParagraphView::new(Position::from_utf16_offset(1), "A\r\nB\n\rC".to_owned());

        assert_eq!(view.line_count(), 3);
        assert_eq!(view.line_text(0), Some("A"));
        assert_eq!(view.line_text(1), Some("B"));
        assert_eq!(view.line_text(2), Some("C"));

        // LFCR contains a valid grapheme boundary between its characters,
        // but the renderer treats the pair as one line separator.
        assert_eq!(
            view.text_position(Position::from_utf16_offset(6)),
            Some(TextPosition { line: 2, byte: 0 })
        );
    }
}
