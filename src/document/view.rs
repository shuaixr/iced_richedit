use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

use crate::document::{DocumentError, Mark, TextSelection};

use super::Position;

/// A renderer-independent position within a paragraph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TextPosition {
    pub(crate) line: usize,
    pub(crate) byte: usize,
}

/// A mark range within the paragraph's complete UTF-8 text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MarkedRange {
    pub(super) range: Range<usize>,
    pub(super) marks: Vec<Mark>,
}

pub(super) fn append_text_run(runs: &mut Vec<MarkedRange>, range: Range<usize>, marks: &[Mark]) {
    if range.is_empty() {
        return;
    }

    if let Some(previous) = runs.last_mut()
        && previous.range.end == range.start
        && previous.marks == marks
    {
        previous.range.end = range.end;
        return;
    }

    runs.push(MarkedRange {
        range,
        marks: marks.to_vec(),
    });
}

fn append_clipped_runs(
    target: &mut Vec<MarkedRange>,
    source: &[MarkedRange],
    source_range: Range<usize>,
    display_start: usize,
) {
    for run in source {
        let start = run.range.start.max(source_range.start);
        let end = run.range.end.min(source_range.end);

        if start >= end {
            continue;
        }

        let mapped_start = display_start + start - source_range.start;
        let mapped_end = display_start + end - source_range.start;

        append_text_run(target, mapped_start..mapped_end, &run.marks);
    }
}

struct CompositionMap {
    points: Vec<(usize, Position)>,
    preedit: Range<usize>,
    replacement_start: Position,
    head: Position,
    cursor_byte: usize,
}

/// A read-only text projection of one document paragraph.
pub(crate) struct ParagraphView {
    start: Position,
    text: String,
    runs: Vec<MarkedRange>,
    lines: Vec<Range<usize>>,
    composition: Option<CompositionMap>,
}

impl ParagraphView {
    #[cfg(test)]
    pub(super) fn new(start: Position, text: String) -> Self {
        let mut runs = Vec::new();
        append_text_run(&mut runs, 0..text.len(), &[]);

        Self::with_runs(start, text, runs)
    }

    pub(super) fn with_runs(start: Position, text: String, runs: Vec<MarkedRange>) -> Self {
        let lines = logical_lines(&text);

        Self {
            start,
            text,
            runs,
            lines,
            composition: None,
        }
    }

    pub(crate) fn start(&self) -> Position {
        self.start
    }

    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    /// Returns ordinary underline ranges, excluding text styled as links.
    pub(crate) fn underline_text_ranges(&self) -> Vec<(TextPosition, TextPosition)> {
        self.text_ranges_matching(|marks| {
            marks.contains(&Mark::Underline)
                && !marks.iter().any(|mark| matches!(mark, Mark::Link { .. }))
        })
    }

    /// Returns continuous inline-code ranges in displayed text coordinates.
    pub(crate) fn code_text_ranges(&self) -> Vec<(TextPosition, TextPosition)> {
        self.text_ranges_matching(|marks| marks.contains(&Mark::Code))
    }

    /// Returns marked fragments without exposing projection storage.
    pub(crate) fn marked_fragments(&self) -> impl Iterator<Item = (&str, &[Mark])> + '_ {
        self.runs
            .iter()
            .map(|run| (&self.text[run.range.clone()], run.marks.as_slice()))
    }

    /// Returns continuous link ranges in displayed text coordinates.
    pub(crate) fn link_text_ranges(&self) -> Vec<(TextPosition, TextPosition)> {
        self.text_ranges_matching(|marks| {
            marks.iter().any(|mark| matches!(mark, Mark::Link { .. }))
        })
    }

    /// Returns continuous ranges carrying the given mark in display coordinates.
    fn text_ranges_matching(
        &self,
        matches: impl Fn(&[Mark]) -> bool,
    ) -> Vec<(TextPosition, TextPosition)> {
        let mut ranges: Vec<Range<usize>> = Vec::new();

        for run in &self.runs {
            if !matches(&run.marks) || run.range.is_empty() {
                continue;
            }

            if let Some(previous) = ranges.last_mut()
                && previous.end == run.range.start
            {
                previous.end = run.range.end;
            } else {
                ranges.push(run.range.clone());
            }
        }

        ranges
            .into_iter()
            .map(|range| {
                let from = self
                    .display_text_position(range.start)
                    .expect("mark ranges must start at valid display boundaries");

                let to = self
                    .display_text_position(range.end)
                    .expect("mark ranges must end at valid display boundaries");

                (from, to)
            })
            .collect()
    }

    pub(crate) fn has_same_content(&self, other: &Self) -> bool {
        self.text == other.text && self.runs == other.runs
    }

    pub(crate) fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub(crate) fn line_text(&self, line: usize) -> Option<&str> {
        let range = self.lines.get(line)?;
        self.text.get(range.clone())
    }

    /// Maps an original document position into the current text projection.
    ///
    /// During composition, the active head maps to the IME cursor.
    pub(crate) fn text_position(&self, position: Position) -> Option<TextPosition> {
        let byte = if let Some(composition) = &self.composition {
            if position == composition.head {
                composition.cursor_byte
            } else {
                composition
                    .points
                    .iter()
                    .find(|(_, original)| *original == position)
                    .map(|(byte, _)| *byte)?
            }
        } else {
            let local = position
                .utf16_offset()
                .checked_sub(self.start.utf16_offset())?;

            self.byte_at_utf16_boundary(local)?
        };

        self.display_text_position(byte)
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

        if let Some(composition) = &self.composition {
            if composition.preedit.contains(&byte) {
                return Some(composition.replacement_start);
            }

            let snapped = self.snap_byte_forward(byte)?;

            return composition
                .points
                .iter()
                .find(|(display_byte, _)| *display_byte >= snapped)
                .map(|(_, original)| *original);
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

    pub(super) fn compose(
        first: &Self,
        last: &Self,
        selection: TextSelection,
        preedit: &str,
        cursor_byte: Option<usize>,
        preedit_marks: &[Mark],
    ) -> Result<Self, DocumentError> {
        let invalid_position = || {
            DocumentError::InvalidDocument(
                "composition replacement range does not match its projection".to_owned(),
            )
        };

        let from = selection
            .from()
            .utf16_offset()
            .checked_sub(first.start.utf16_offset())
            .ok_or_else(invalid_position)?;

        let to = selection
            .to()
            .utf16_offset()
            .checked_sub(last.start.utf16_offset())
            .ok_or_else(invalid_position)?;

        let prefix_end = first
            .byte_at_utf16_boundary(from)
            .ok_or_else(invalid_position)?;

        let suffix_start = last
            .byte_at_utf16_boundary(to)
            .ok_or_else(invalid_position)?;

        let cursor_byte = cursor_byte.unwrap_or(0);

        if cursor_byte > preedit.len() || !preedit.is_char_boundary(cursor_byte) {
            return Err(DocumentError::InvalidDocument(
                "composition cursor is not a valid UTF-8 boundary".to_owned(),
            ));
        }

        let prefix = &first.text[..prefix_end];
        let suffix = &last.text[suffix_start..];

        let mut text = String::with_capacity(prefix.len() + preedit.len() + suffix.len());

        text.push_str(prefix);
        text.push_str(preedit);
        text.push_str(suffix);

        let preedit_range = prefix.len()..prefix.len() + preedit.len();

        let mut points = Vec::new();

        for (byte, utf16) in text_boundaries(&first.text) {
            if byte > prefix_end {
                break;
            }

            points.push((
                byte,
                Position::from_utf16_offset(first.start.utf16_offset() + utf16),
            ));
        }

        for (byte, utf16) in text_boundaries(&last.text) {
            if byte < suffix_start {
                continue;
            }

            points.push((
                preedit_range.end + byte - suffix_start,
                Position::from_utf16_offset(last.start.utf16_offset() + utf16),
            ));
        }

        let mut runs = Vec::new();

        append_clipped_runs(&mut runs, &first.runs, 0..prefix_end, 0);

        append_text_run(
            &mut runs,
            prefix.len()..prefix.len() + preedit.len(),
            preedit_marks,
        );

        append_clipped_runs(
            &mut runs,
            &last.runs,
            suffix_start..last.text.len(),
            prefix.len() + preedit.len(),
        );

        let lines = logical_lines(&text);

        Ok(Self {
            start: first.start,
            text,
            runs,
            lines,
            composition: Some(CompositionMap {
                points,
                preedit: preedit_range,
                replacement_start: selection.from(),
                head: selection.head(),
                cursor_byte: prefix.len() + cursor_byte,
            }),
        })
    }

    fn display_text_position(&self, byte: usize) -> Option<TextPosition> {
        if byte > self.text.len() || !self.text.is_char_boundary(byte) {
            return None;
        }

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

    /// Returns the temporary composition range in displayed text coordinates.
    pub(crate) fn composition_text_range(&self) -> Option<(TextPosition, TextPosition)> {
        let composition = self.composition.as_ref()?;
        let length = composition.preedit.len();

        self.composition_selection_range(&(0..length))
    }

    /// Maps a preedit-local UTF-8 range into displayed text coordinates.
    pub(crate) fn composition_selection_range(
        &self,
        selection: &Range<usize>,
    ) -> Option<(TextPosition, TextPosition)> {
        let composition = self.composition.as_ref()?;

        if selection.start >= selection.end || selection.end > composition.preedit.len() {
            return None;
        }

        let from = composition.preedit.start.checked_add(selection.start)?;

        let to = composition.preedit.start.checked_add(selection.end)?;

        Some((
            self.display_text_position(from)?,
            self.display_text_position(to)?,
        ))
    }
}

fn text_boundaries(text: &str) -> impl Iterator<Item = (usize, usize)> + '_ {
    std::iter::once((0, 0)).chain(
        text.graphemes(true)
            .scan((0, 0), |(byte, utf16), grapheme| {
                *byte += grapheme.len();
                *utf16 += grapheme.encode_utf16().count();

                Some((*byte, *utf16))
            }),
    )
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

    #[test]
    fn underline_ranges_merge_across_font_marks_but_not_plain_text() {
        use super::append_text_run;
        use crate::document::Mark;

        let mut runs = Vec::new();

        append_text_run(&mut runs, 0..1, &[Mark::Underline]);
        append_text_run(&mut runs, 1..2, &[Mark::Strong, Mark::Underline]);
        append_text_run(&mut runs, 2..3, &[]);
        append_text_run(&mut runs, 3..4, &[Mark::Underline]);

        let view =
            ParagraphView::with_runs(Position::from_utf16_offset(1), "ABCD".to_owned(), runs);

        assert_eq!(
            view.underline_text_ranges(),
            vec![
                (
                    TextPosition { line: 0, byte: 0 },
                    TextPosition { line: 0, byte: 2 },
                ),
                (
                    TextPosition { line: 0, byte: 3 },
                    TextPosition { line: 0, byte: 4 },
                ),
            ]
        );

        let plain = ParagraphView::new(Position::from_utf16_offset(1), "Plain text".to_owned());
        assert!(plain.underline_text_ranges().is_empty());

        let empty = ParagraphView::new(Position::from_utf16_offset(1), String::new());
        assert!(empty.underline_text_ranges().is_empty());
    }

    #[test]
    fn underline_ranges_follow_composition_display_and_inherited_marks() {
        use super::append_text_run;
        use crate::document::{Mark, TextSelection};

        let mut runs = Vec::new();
        append_text_run(&mut runs, 0..4, &[Mark::Underline]);

        let original =
            ParagraphView::with_runs(Position::from_utf16_offset(1), "ABCD".to_owned(), runs);

        let selection = TextSelection::new(
            Position::from_utf16_offset(2),
            Position::from_utf16_offset(4),
        );

        let without_underline =
            ParagraphView::compose(&original, &original, selection, "中\n😀", None, &[])
                .expect("composition projection must succeed");

        assert_eq!(without_underline.text(), "A中\n😀D");

        assert_eq!(
            without_underline.underline_text_ranges(),
            vec![
                (
                    TextPosition { line: 0, byte: 0 },
                    TextPosition { line: 0, byte: 1 },
                ),
                (
                    TextPosition { line: 1, byte: 4 },
                    TextPosition { line: 1, byte: 5 },
                ),
            ]
        );

        let with_underline = ParagraphView::compose(
            &original,
            &original,
            selection,
            "中\n😀",
            None,
            &[Mark::Underline],
        )
        .expect("composition projection must succeed");

        assert_eq!(
            with_underline.underline_text_ranges(),
            vec![(
                TextPosition { line: 0, byte: 0 },
                TextPosition { line: 1, byte: 5 },
            )]
        );
    }

    #[test]
    fn code_ranges_use_display_coordinates_and_ignore_other_marks() {
        use super::append_text_run;
        use crate::document::Mark;

        let mut runs = Vec::new();

        append_text_run(&mut runs, 0..1, &[Mark::Underline]);
        append_text_run(&mut runs, 1..5, &[Mark::Code]);
        append_text_run(&mut runs, 5..6, &[Mark::Strong, Mark::Code]);
        append_text_run(&mut runs, 6..7, &[]);
        append_text_run(&mut runs, 7..8, &[Mark::Code]);

        let view =
            ParagraphView::with_runs(Position::from_utf16_offset(1), "A中\nB C".to_owned(), runs);

        assert_eq!(
            view.code_text_ranges(),
            vec![
                (
                    TextPosition { line: 0, byte: 1 },
                    TextPosition { line: 1, byte: 1 },
                ),
                (
                    TextPosition { line: 1, byte: 2 },
                    TextPosition { line: 1, byte: 3 },
                ),
            ]
        );

        let plain = ParagraphView::new(Position::from_utf16_offset(1), "Plain text".to_owned());
        assert!(plain.code_text_ranges().is_empty());
    }

    #[test]
    fn link_underlines_do_not_overlap_ordinary_underlines() {
        use super::append_text_run;
        use crate::document::Mark;

        let link = Mark::Link {
            href: "https://iced.rs/".to_owned(),
            title: None,
        };

        let mut runs = Vec::new();

        append_text_run(&mut runs, 0..1, &[Mark::Underline]);
        append_text_run(&mut runs, 1..2, &[Mark::Underline, link.clone()]);
        append_text_run(&mut runs, 2..3, &[Mark::Strong, link]);
        append_text_run(&mut runs, 3..4, &[]);

        let view =
            ParagraphView::with_runs(Position::from_utf16_offset(1), "ABCD".to_owned(), runs);

        assert_eq!(
            view.underline_text_ranges(),
            vec![(
                TextPosition { line: 0, byte: 0 },
                TextPosition { line: 0, byte: 1 },
            )]
        );

        assert_eq!(
            view.link_text_ranges(),
            vec![(
                TextPosition { line: 0, byte: 1 },
                TextPosition { line: 0, byte: 3 },
            )]
        );
    }
}
