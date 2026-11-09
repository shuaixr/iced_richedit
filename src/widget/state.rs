use crate::{
    document::{ParagraphView, Position, TextBlockKind, TextPosition, TextSelection},
    widget::{ime::ImeState, paste::PasteState},
};

use iced::{
    Color, Point, Rectangle, Size,
    advanced::{
        graphics::text::{Paragraph, cosmic_text::Cursor},
        widget::operation::Focusable,
    },
};
use unicode_segmentation::UnicodeSegmentation;

const CARET_WIDTH: f32 = 1.5;

/// Supplies the final empty row omitted by the pinned rich-text shaping path.
/// Its geometry comes from the preceding actual layout run, without extra text.
pub(super) fn trailing_empty_line(
    paragraph: &Paragraph,
    view: &ParagraphView,
) -> Option<(usize, Rectangle)> {
    let line = view.line_count().checked_sub(1)?;

    if !view.line_text(line)?.is_empty() {
        return None;
    }

    let previous = paragraph.buffer().layout_runs().last()?;

    if previous.line_i.checked_add(1)? != line {
        return None;
    }

    Some((
        line,
        Rectangle {
            x: 0.0,
            y: previous.line_top + previous.line_height,
            width: CARET_WIDTH,
            height: previous.line_height,
        },
    ))
}

pub(super) struct LaidOutParagraph {
    pub(super) paragraph: Paragraph,
    pub(super) origin: Point,
    pub(super) height: f32,
    pub(super) view: Option<ParagraphView>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CaretAffinity {
    Upstream,
    Downstream,
}

pub(super) struct ObservedPreedit {
    pub(super) content: String,
    pub(super) selection: Option<std::ops::Range<usize>>,
}

#[derive(Default)]
pub(super) struct State {
    pub(super) paragraphs: Vec<LaidOutParagraph>,
    pub(super) has_error: bool,
    pub(super) is_focused: bool,
    pub(super) window_unfocused: bool,
    pub(super) drag_anchor: Option<Position>,
    pub(super) scroll_y: f32,
    pub(super) content_height: f32,
    pub(super) observed_selection: Option<TextSelection>,
    pub(super) observed_viewport: Option<Size>,
    pub(super) caret_hint: Option<(Position, CaretAffinity)>,
    pub(super) ime: ImeState,
    pub(super) preferred_x: Option<f32>,
    pub(super) observed_preedit: Option<ObservedPreedit>,
    pub(super) observed_link_color: Option<Color>,
    pub(super) paste: PasteState,
}

impl Focusable for State {
    fn is_focused(&self) -> bool {
        self.is_focused
    }

    fn focus(&mut self) {
        self.is_focused = true;
    }

    fn unfocus(&mut self) {
        self.paste.cancel();
        self.is_focused = false;
        self.drag_anchor = None;
        self.caret_hint = None;
        self.preferred_x = None;
        self.ime.cancel();
    }
}
impl State {
    pub(super) fn hit_test(&mut self, point: Point) -> Option<Position> {
        let position = self.hit_test_position(point)?;

        let affinity = self.paragraphs.iter().find_map(|item| {
            let mut candidates = item.caret_candidates(position);

            let upstream = candidates.next()?;
            let downstream = candidates.last().unwrap_or(upstream);

            let upstream_center = upstream.y + upstream.height * 0.5;
            let downstream_center = downstream.y + downstream.height * 0.5;

            let upstream_distance = (point.y - upstream_center).abs();
            let downstream_distance = (point.y - downstream_center).abs();

            Some(if upstream_distance < downstream_distance {
                CaretAffinity::Upstream
            } else {
                CaretAffinity::Downstream
            })
        })?;

        self.caret_hint = Some((position, affinity));
        self.preferred_x = None;
        Some(position)
    }

    fn hit_test_position(&self, position: Point) -> Option<Position> {
        if self.has_error {
            return None;
        }

        // Paragraph gaps belong to the following paragraph.
        // Space below all paragraphs belongs to the last paragraph.
        let item = self
            .paragraphs
            .iter()
            .find(|item| position.y < item.origin.y + item.height)
            .or_else(|| self.paragraphs.last())?;

        let view = item.view.as_ref()?;

        if view.text().is_empty() {
            return Some(view.start());
        }

        let local = Point::new(position.x - item.origin.x, position.y - item.origin.y);

        if let Some((line, bounds)) = item.trailing_empty_line()
            && position.y >= bounds.y
        {
            return view.document_position(TextPosition { line, byte: 0 });
        }

        let cursor = item.paragraph.buffer().hit(local.x, local.y)?;
        view.document_position(TextPosition {
            line: cursor.line,
            byte: cursor.index,
        })
    }

    pub(super) fn caret_bounds(&self, position: Position) -> Option<Rectangle> {
        self.caret_location(position).map(|(_, bounds)| bounds)
    }

    fn caret_location(&self, position: Position) -> Option<(usize, Rectangle)> {
        if self.has_error {
            return None;
        }

        let affinity = self
            .caret_hint
            .filter(|(hint_position, _)| *hint_position == position)
            .map(|(_, affinity)| affinity)
            .unwrap_or(CaretAffinity::Downstream);

        self.paragraphs
            .iter()
            .enumerate()
            .find_map(|(index, item)| {
                let bounds = match affinity {
                    CaretAffinity::Upstream => item.caret_candidates(position).next(),
                    CaretAffinity::Downstream => item.caret_bounds(position),
                };

                bounds.map(|bounds| (index, bounds))
            })
    }
    pub(super) fn selection_bounds(&self, selection: TextSelection) -> Vec<Rectangle> {
        if self.has_error || selection.is_empty() {
            return Vec::new();
        }

        self.paragraphs
            .iter()
            .flat_map(|item| item.selection_bounds(selection))
            .collect()
    }

    pub(super) fn clamp_scroll(&mut self, viewport_height: f32) {
        let maximum = (self.content_height - viewport_height).max(0.0);
        self.scroll_y = self.scroll_y.clamp(0.0, maximum);
    }

    pub(super) fn scroll_by(&mut self, delta: f32, viewport_height: f32) -> bool {
        let previous = self.scroll_y;

        self.scroll_y += delta;
        self.clamp_scroll(viewport_height);

        self.scroll_y != previous
    }

    pub(super) fn content_position(&self, window_position: Point, widget_origin: Point) -> Point {
        Point::new(
            window_position.x - widget_origin.x,
            window_position.y - widget_origin.y + self.scroll_y,
        )
    }

    pub(super) fn reveal_head(&mut self, position: Position, viewport_height: f32, padding: f32) {
        let Some(bounds) = self.caret_bounds(position) else {
            return;
        };

        self.reveal_bounds(bounds, viewport_height, padding);
    }

    fn reveal_bounds(&mut self, bounds: Rectangle, viewport_height: f32, padding: f32) {
        let available_height = viewport_height - padding * 2.0;

        if available_height <= 0.0 {
            self.clamp_scroll(viewport_height);
            return;
        }

        let visible_top = self.scroll_y + padding;
        let visible_bottom = self.scroll_y + viewport_height - padding;

        if bounds.height > available_height || bounds.y < visible_top {
            self.scroll_y = bounds.y - padding;
        } else if bounds.y + bounds.height > visible_bottom {
            self.scroll_y = bounds.y + bounds.height - viewport_height + padding;
        }

        self.clamp_scroll(viewport_height);
    }

    pub(super) fn visual_line_boundary(
        &self,
        position: Position,
        end: bool,
    ) -> Option<(Position, CaretAffinity)> {
        let current = self.caret_bounds(position)?;

        let item = self.paragraphs.iter().find(|item| {
            item.view
                .as_ref()
                .is_some_and(|view| view.text_position(position).is_some())
        })?;

        let mut target: Option<(Position, CaretAffinity)> = None;

        for (candidate, affinity, bounds) in item.visual_carets() {
            if bounds.y != current.y {
                continue;
            }

            let replace = target.is_none_or(|(previous, _)| {
                if end {
                    candidate > previous
                } else {
                    candidate < previous
                }
            });

            if replace {
                target = Some((candidate, affinity));
            }
        }

        target
    }

    pub(super) fn vertical_target(
        &self,
        position: Position,
        down: bool,
        preferred_x: f32,
    ) -> Option<(Position, CaretAffinity)> {
        let (current_index, current) = self.caret_location(position)?;

        if let Some(target) =
            self.paragraphs[current_index].vertical_target(current.y, down, preferred_x)
        {
            return Some(target);
        }

        // Layout places paragraphs in non-overlapping vertical order.
        // The first paragraph with a candidate contains the nearest row.
        if down {
            self.paragraphs[current_index + 1..]
                .iter()
                .find_map(|item| item.vertical_target(current.y, down, preferred_x))
        } else {
            self.paragraphs[..current_index]
                .iter()
                .rev()
                .find_map(|item| item.vertical_target(current.y, down, preferred_x))
        }
    }
    pub(super) fn code_block_background_bounds(&self, content_width: f32) -> Vec<Rectangle> {
        if self.has_error {
            return Vec::new();
        }

        self.paragraphs
            .iter()
            .filter_map(|item| {
                let view = item.view.as_ref()?;

                (view.kind() == TextBlockKind::CodeBlock).then_some(Rectangle {
                    x: item.origin.x,
                    y: item.origin.y,
                    width: content_width,
                    height: item.height,
                })
            })
            .collect()
    }

    pub(super) fn code_background_bounds(&self) -> Vec<Rectangle> {
        if self.has_error {
            return Vec::new();
        }

        self.paragraphs
            .iter()
            .flat_map(|item| item.code_background_bounds())
            .collect()
    }

    pub(super) fn underline_bounds(&self) -> Vec<(Rectangle, bool)> {
        if self.has_error {
            return Vec::new();
        }

        self.paragraphs
            .iter()
            .flat_map(|item| item.underline_bounds())
            .collect()
    }

    pub(super) fn composition_bounds(&self) -> Vec<Rectangle> {
        if self.has_error {
            return Vec::new();
        }

        self.paragraphs
            .iter()
            .flat_map(|item| item.composition_bounds())
            .collect()
    }

    pub(super) fn composition_selection_bounds(&self) -> Vec<Rectangle> {
        if self.has_error {
            return Vec::new();
        }

        let Some(selection) = self.ime.preedit().and_then(|preedit| preedit.selection) else {
            return Vec::new();
        };

        if selection.is_empty() {
            return Vec::new();
        }

        self.paragraphs
            .iter()
            .flat_map(|item| item.composition_selection_bounds(&selection))
            .collect()
    }
}

impl LaidOutParagraph {
    fn vertical_target(
        &self,
        current_y: f32,
        down: bool,
        preferred_x: f32,
    ) -> Option<(Position, CaretAffinity)> {
        let mut target: Option<(Position, CaretAffinity, Rectangle)> = None;

        for (candidate, affinity, bounds) in self.visual_carets() {
            let in_direction = if down {
                bounds.y > current_y
            } else {
                bounds.y < current_y
            };

            if !in_direction {
                continue;
            }

            let replace = match target {
                None => true,
                Some((_, _, previous)) => {
                    let nearer_row = if down {
                        bounds.y < previous.y
                    } else {
                        bounds.y > previous.y
                    };

                    let nearer_x = bounds.y == previous.y
                        && (bounds.x - preferred_x).abs() < (previous.x - preferred_x).abs();

                    nearer_row || nearer_x
                }
            };

            if replace {
                target = Some((candidate, affinity, bounds));
            }
        }

        target.map(|(position, affinity, _)| (position, affinity))
    }

    fn caret_bounds(&self, position: Position) -> Option<Rectangle> {
        // Preserve the existing downstream preference at soft wraps.
        self.caret_candidates(position).last()
    }

    fn caret_candidates(&self, position: Position) -> impl Iterator<Item = Rectangle> + '_ {
        let view = self.view.as_ref();

        let text_position = view.and_then(|view| view.text_position(position));

        self.paragraph
            .buffer()
            .layout_runs()
            .filter_map(move |run| {
                let view = view?;
                let text_position = text_position?;

                if run.line_i != text_position.line {
                    return None;
                }

                let line_text = view.line_text(text_position.line)?;

                if run.glyphs.is_empty() {
                    return (text_position.byte == 0).then_some(Rectangle {
                        x: self.origin.x,
                        y: self.origin.y + run.line_top,
                        width: CARET_WIDTH,
                        height: run.line_height,
                    });
                }

                // Prefer a cluster starting at the cursor over one ending there.
                let glyph = run
                    .glyphs
                    .iter()
                    .find(|glyph| {
                        glyph.start <= text_position.byte && text_position.byte < glyph.end
                    })
                    .or_else(|| {
                        run.glyphs
                            .iter()
                            .find(|glyph| glyph.end == text_position.byte)
                    })?;

                let cluster = line_text.get(glyph.start..glyph.end)?;
                let prefix = line_text.get(glyph.start..text_position.byte)?;

                let grapheme_count = cluster.graphemes(true).count();

                if grapheme_count == 0 {
                    return None;
                }

                let preceding = prefix.graphemes(true).count();
                let fraction = preceding as f32 / grapheme_count as f32;

                // Several glyphs can belong to the same shaping cluster.
                let mut left = f32::INFINITY;
                let mut right = f32::NEG_INFINITY;

                for part in run
                    .glyphs
                    .iter()
                    .filter(|part| part.start == glyph.start && part.end == glyph.end)
                {
                    left = left.min(part.x);
                    right = right.max(part.x + part.w);
                }

                let x = if glyph.level.is_rtl() {
                    right - (right - left) * fraction
                } else {
                    left + (right - left) * fraction
                };

                Some(Rectangle {
                    x: self.origin.x + x,
                    y: self.origin.y + run.line_top,
                    width: CARET_WIDTH,
                    height: run.line_height,
                })
            })
            .chain(self.trailing_empty_line().and_then(|(line, bounds)| {
                (text_position == Some(TextPosition { line, byte: 0 })).then_some(bounds)
            }))
    }
    fn selection_bounds(&self, selection: TextSelection) -> Vec<Rectangle> {
        let Some(view) = &self.view else {
            return Vec::new();
        };

        let start = view.start();
        let end =
            Position::from_utf16_offset(start.utf16_offset() + view.text().encode_utf16().count());

        if selection.to() < start || selection.from() > end {
            return Vec::new();
        }

        let from = selection.from().max(start);
        let to = selection.to().min(end);

        let Some(text_from) = view.text_position(from) else {
            return Vec::new();
        };

        let Some(text_to) = view.text_position(to) else {
            return Vec::new();
        };

        let mut rectangles = self.text_range_bounds(text_from, text_to);

        for run in self.paragraph.buffer().layout_runs() {
            let Some(line) = view.line_text(run.line_i) else {
                continue;
            };

            let run_end = run.glyphs.iter().map(|glyph| glyph.end).max().unwrap_or(0);

            // Only the last visual run of a logical line owns its line ending.
            if run_end != line.len() {
                continue;
            }

            let Some(line_end) = view.document_position(TextPosition {
                line: run.line_i,
                byte: line.len(),
            }) else {
                continue;
            };

            // A small marker makes selected line/paragraph endings and empty
            // paragraphs visible without filling the entire remaining viewport.
            if selection.from() <= line_end && selection.to() > line_end {
                let x = run
                    .glyphs
                    .last()
                    .map(|glyph| {
                        if glyph.level.is_rtl() {
                            glyph.x
                        } else {
                            glyph.x + glyph.w
                        }
                    })
                    .unwrap_or(0.0);

                rectangles.push(Rectangle {
                    x: self.origin.x + x,
                    y: self.origin.y + run.line_top,
                    width: 3.0,
                    height: run.line_height,
                });
            }
        }
        if let Some((line, mut bounds)) = self.trailing_empty_line()
            && view
                .document_position(TextPosition { line, byte: 0 })
                .is_some_and(|line_end| selection.from() <= line_end && selection.to() > line_end)
        {
            bounds.width = 3.0;
            rectangles.push(bounds);
        }

        rectangles
    }

    fn visual_carets(&self) -> impl Iterator<Item = (Position, CaretAffinity, Rectangle)> + '_ {
        self.view.iter().flat_map(move |view| {
            let start = view.start().utf16_offset();

            let offsets = std::iter::once(0).chain(view.text().graphemes(true).scan(
                0,
                |offset, grapheme| {
                    *offset += grapheme.encode_utf16().count();
                    Some(*offset)
                },
            ));

            offsets.flat_map(move |offset| {
                let position = Position::from_utf16_offset(start + offset);

                self.caret_candidates(position)
                    .enumerate()
                    .map(move |(index, bounds)| {
                        let affinity = if index == 0 {
                            CaretAffinity::Upstream
                        } else {
                            CaretAffinity::Downstream
                        };

                        (position, affinity, bounds)
                    })
            })
        })
    }

    fn text_range_bounds(&self, from: TextPosition, to: TextPosition) -> Vec<Rectangle> {
        if from == to {
            return Vec::new();
        }

        let cursor_from = Cursor::new(from.line, from.byte);
        let cursor_to = Cursor::new(to.line, to.byte);

        self.paragraph
            .buffer()
            .layout_runs()
            .flat_map(|run| {
                let y = self.origin.y + run.line_top;
                let height = run.line_height;
                run.highlight(cursor_from, cursor_to)
                    .filter_map(move |(x, width)| {
                        (width > 0.0).then_some(Rectangle {
                            x: self.origin.x + x,
                            y,
                            width,
                            height,
                        })
                    })
            })
            .collect()
    }

    fn code_background_bounds(&self) -> Vec<Rectangle> {
        let Some(view) = &self.view else {
            return Vec::new();
        };

        view.code_text_ranges()
            .into_iter()
            .flat_map(|(from, to)| self.text_range_bounds(from, to))
            .collect()
    }

    fn underline_bounds(&self) -> Vec<(Rectangle, bool)> {
        let Some(view) = &self.view else {
            return Vec::new();
        };

        let mut bounds = Vec::new();

        for (ranges, is_link) in [
            (view.underline_text_ranges(), false),
            (view.link_text_ranges(), true),
        ] {
            for (from, to) in ranges {
                bounds.extend(
                    self.underline_range_bounds(from, to)
                        .into_iter()
                        .map(|rectangle| (rectangle, is_link)),
                );
            }
        }

        bounds
    }

    fn underline_range_bounds(&self, from: TextPosition, to: TextPosition) -> Vec<Rectangle> {
        let cursor_from = Cursor::new(from.line, from.byte);
        let cursor_to = Cursor::new(to.line, to.byte);

        self.paragraph
            .buffer()
            .layout_runs()
            .flat_map(|run| {
                let thickness = run.line_height.min(1.0);

                // Use the actual visual baseline with a small fixed offset.
                // This is not the font's underline metric or skip-ink shaping.
                let top = run.line_top;
                let bottom = top + run.line_height - thickness;
                let y = (run.line_y + 2.0).min(bottom).max(top);

                run.highlight(cursor_from, cursor_to)
                    .filter_map(move |(x, width)| {
                        (width > 0.0 && thickness > 0.0).then_some(Rectangle {
                            x: self.origin.x + x,
                            y: self.origin.y + y,
                            width,
                            height: thickness,
                        })
                    })
            })
            .collect()
    }

    fn composition_bounds(&self) -> Vec<Rectangle> {
        let Some((from, to)) = self
            .view
            .as_ref()
            .and_then(|view| view.composition_text_range())
        else {
            return Vec::new();
        };

        self.text_range_bounds(from, to)
    }

    fn composition_selection_bounds(&self, selection: &std::ops::Range<usize>) -> Vec<Rectangle> {
        let Some((from, to)) = self
            .view
            .as_ref()
            .and_then(|view| view.composition_selection_range(selection))
        else {
            return Vec::new();
        };

        self.text_range_bounds(from, to)
    }

    fn trailing_empty_line(&self) -> Option<(usize, Rectangle)> {
        let (line, mut bounds) = trailing_empty_line(&self.paragraph, self.view.as_ref()?)?;

        bounds.x += self.origin.x;
        bounds.y += self.origin.y;

        Some((line, bounds))
    }
}

#[cfg(test)]
mod tests {
    use super::State;

    #[test]
    fn scrolling_clamps_to_content_and_viewport_bounds() {
        let mut state = State {
            content_height: 800.0,
            ..State::default()
        };

        assert!(state.scroll_by(100.0, 300.0));
        assert_eq!(state.scroll_y, 100.0);

        assert!(state.scroll_by(1000.0, 300.0));
        assert_eq!(state.scroll_y, 500.0);

        assert!(!state.scroll_by(1.0, 300.0));

        assert!(state.scroll_by(-1000.0, 300.0));
        assert_eq!(state.scroll_y, 0.0);

        assert!(!state.scroll_by(-1.0, 300.0));

        state.scroll_by(400.0, 300.0);
        state.content_height = 100.0;
        state.clamp_scroll(300.0);

        assert_eq!(state.scroll_y, 0.0);
    }

    #[test]
    fn soft_wrap_exposes_both_caret_candidates_and_defaults_downstream() {
        use crate::document::{Document, Position};
        use iced::{Font, Point};

        use super::LaidOutParagraph;

        let text = "abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz";

        let document = Document::from_json(
            &serde_json::json!({
                "type": "doc",
                "content": [{
                    "type": "paragraph",
                    "content": [{
                        "type": "text",
                        "text": text
                    }]
                }]
            })
            .to_string(),
        )
        .expect("document must be valid");

        let view = document
            .paragraph_views()
            .expect("projection must succeed")
            .into_iter()
            .next()
            .expect("paragraph must exist");

        let start = view.start().utf16_offset();

        let item = LaidOutParagraph {
            paragraph: super::super::make_paragraph(text, 48.0, Font::DEFAULT),
            origin: Point::new(12.0, 20.0),
            height: 500.0,
            view: Some(view),
        };

        let position = (0..=text.len())
            .map(|offset| Position::from_utf16_offset(start + offset))
            .find(|&position| item.caret_candidates(position).count() > 1)
            .expect("wrapped text must have a shared caret position");

        let candidates: Vec<_> = item.caret_candidates(position).collect();

        let upstream = candidates.first().expect("upstream candidate must exist");

        let downstream = candidates.last().expect("downstream candidate must exist");

        assert!(upstream.y < downstream.y);
        assert!(upstream.x > downstream.x);

        assert_eq!(item.caret_bounds(position), Some(*downstream));

        let upstream = *upstream;
        let downstream = *downstream;

        let mut state = State {
            paragraphs: vec![item],
            ..State::default()
        };

        assert_eq!(state.caret_bounds(position), Some(downstream));

        state.caret_hint = Some((position, super::CaretAffinity::Upstream));

        assert_eq!(state.caret_bounds(position), Some(upstream));

        state.caret_hint = Some((position, super::CaretAffinity::Downstream));

        assert_eq!(state.caret_bounds(position), Some(downstream));

        // A hint for another position must not affect this caret.
        state.caret_hint = Some((
            Position::from_utf16_offset(start),
            super::CaretAffinity::Upstream,
        ));

        assert_eq!(state.caret_bounds(position), Some(downstream));

        state.caret_hint = Some((position, super::CaretAffinity::Upstream));

        let (end_position, end_affinity) = state
            .visual_line_boundary(position, true)
            .expect("visual line end must exist");

        assert_eq!(end_position, position);

        state.caret_hint = Some((end_position, end_affinity));

        assert_eq!(state.caret_bounds(end_position), Some(upstream));

        // Home on the downstream row must preserve its visual side.
        state.caret_hint = Some((position, super::CaretAffinity::Downstream));

        let (start_position, start_affinity) = state
            .visual_line_boundary(position, false)
            .expect("visual line start must exist");

        assert_eq!(start_position, position);

        state.caret_hint = Some((start_position, start_affinity));

        assert_eq!(state.caret_bounds(start_position), Some(downstream));

        // Traversing the row from Home to End must stay on that row.
        let (row_end, row_end_affinity) = state
            .visual_line_boundary(start_position, true)
            .expect("visual line end must exist");

        assert!(row_end > start_position);

        state.caret_hint = Some((row_end, row_end_affinity));

        let row_end_bounds = state.caret_bounds(row_end).expect("caret must exist");

        assert_eq!(row_end_bounds.y, downstream.y);
    }

    #[test]
    fn vertical_navigation_preserves_horizontal_target_across_short_paragraph() {
        use crate::document::{Document, Position};
        use iced::{Font, Point};

        use super::LaidOutParagraph;

        let document = Document::from_json(
            &serde_json::json!({
                "type": "doc",
                "content": [
                    {
                        "type": "paragraph",
                        "content": [{
                            "type": "text",
                            "text": "MMMMMMMM"
                        }]
                    },
                    {
                        "type": "paragraph",
                        "content": [{
                            "type": "text",
                            "text": "x"
                        }]
                    },
                    {
                        "type": "paragraph",
                        "content": [{
                            "type": "text",
                            "text": "MMMMMMMM"
                        }]
                    }
                ]
            })
            .to_string(),
        )
        .expect("document must be valid");

        let views = document.paragraph_views().expect("projection must succeed");

        let mut state = State::default();

        for (index, view) in views.into_iter().enumerate() {
            let paragraph = super::super::make_paragraph(view.text(), 400.0, Font::DEFAULT);

            state.paragraphs.push(LaidOutParagraph {
                paragraph,
                origin: Point::new(12.0, 12.0 + index as f32 * 40.0),
                height: 24.0,
                view: Some(view),
            });
        }

        let initial = Position::from_utf16_offset(7);

        let preferred_x = state
            .caret_bounds(initial)
            .expect("initial caret must exist")
            .x;

        let (middle, affinity) = state
            .vertical_target(initial, true, preferred_x)
            .expect("middle row must exist");

        assert_eq!(middle, Position::from_utf16_offset(12));

        state.caret_hint = Some((middle, affinity));

        let (last, affinity) = state
            .vertical_target(middle, true, preferred_x)
            .expect("last row must exist");

        assert_eq!(last, Position::from_utf16_offset(20));

        state.caret_hint = Some((last, affinity));

        let last_bounds = state.caret_bounds(last).expect("last caret must exist");

        assert!((last_bounds.x - preferred_x).abs() < 0.01);

        assert!(state.vertical_target(last, true, preferred_x).is_none());

        let (middle, affinity) = state
            .vertical_target(last, false, preferred_x)
            .expect("middle row must exist");

        state.caret_hint = Some((middle, affinity));

        let (first, _) = state
            .vertical_target(middle, false, preferred_x)
            .expect("first row must exist");

        assert_eq!(first, initial);
    }
}
