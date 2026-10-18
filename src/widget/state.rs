use crate::document::{ParagraphView, Position, TextPosition, TextSelection};

use iced::{
    Point, Rectangle, Size,
    advanced::{
        graphics::text::{Paragraph, cosmic_text::Cursor},
        widget::operation::Focusable,
    },
};
use unicode_segmentation::UnicodeSegmentation;
pub(super) struct LaidOutParagraph {
    pub(super) paragraph: Paragraph,
    pub(super) origin: Point,
    pub(super) height: f32,
    pub(super) view: Option<ParagraphView>,
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
}

impl Focusable for State {
    fn is_focused(&self) -> bool {
        self.is_focused
    }

    fn focus(&mut self) {
        self.is_focused = true;
    }

    fn unfocus(&mut self) {
        self.is_focused = false;
        self.drag_anchor = None;
    }
}
impl State {
    pub(super) fn hit_test(&self, position: Point) -> Option<Position> {
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

        let cursor = item.paragraph.buffer().hit(local.x, local.y)?;

        view.document_position(TextPosition {
            line: cursor.line,
            byte: cursor.index,
        })
    }

    pub(super) fn caret_bounds(&self, position: Position) -> Option<Rectangle> {
        if self.has_error {
            return None;
        }

        self.paragraphs
            .iter()
            .find_map(|item| item.caret_bounds(position))
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
}
impl LaidOutParagraph {
    fn caret_bounds(&self, position: Position) -> Option<Rectangle> {
        let view = self.view.as_ref()?;
        let text_position = view.text_position(position)?;
        let line_text = view.line_text(text_position.line)?;

        let mut caret = None;

        for run in self.paragraph.buffer().layout_runs() {
            if run.line_i != text_position.line {
                continue;
            }

            if run.glyphs.is_empty() {
                if text_position.byte == 0 {
                    caret = Some(Rectangle {
                        x: self.origin.x,
                        y: self.origin.y + run.line_top,
                        width: 1.5,
                        height: run.line_height,
                    });
                }

                continue;
            }

            // Prefer a cluster starting at the cursor over one ending there.
            let glyph = run
                .glyphs
                .iter()
                .find(|glyph| glyph.start <= text_position.byte && text_position.byte < glyph.end)
                .or_else(|| {
                    run.glyphs
                        .iter()
                        .find(|glyph| glyph.end == text_position.byte)
                });

            let Some(glyph) = glyph else {
                continue;
            };

            let cluster = line_text.get(glyph.start..glyph.end)?;
            let prefix = line_text.get(glyph.start..text_position.byte)?;

            let grapheme_count = cluster.graphemes(true).count();

            if grapheme_count == 0 {
                continue;
            }

            let preceding = prefix.graphemes(true).count();

            let fraction = preceding as f32 / grapheme_count as f32;

            // Several glyphs can belong to the same shaping cluster.
            // Use their combined advance range instead of a single glyph.
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

            // Later matching runs win at a soft-wrap boundary.
            caret = Some(Rectangle {
                x: self.origin.x + x,
                y: self.origin.y + run.line_top,
                width: 1.5,
                height: run.line_height,
            });
        }

        caret
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

        let cursor_from = Cursor::new(text_from.line, text_from.byte);
        let cursor_to = Cursor::new(text_to.line, text_to.byte);

        let mut rectangles = Vec::new();

        for run in self.paragraph.buffer().layout_runs() {
            if from < to
                && let Some((x, width)) = run.highlight(cursor_from, cursor_to)
                && width > 0.0
            {
                rectangles.push(Rectangle {
                    x: self.origin.x + x,
                    y: self.origin.y + run.line_top,
                    width,
                    height: run.line_height,
                });
            }

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

        rectangles
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
}
