use std::collections::BTreeSet;

use iced::{
    Color, Font, Pixels, Size,
    advanced::{
        graphics::text::Paragraph,
        text::{self, Paragraph as _},
    },
    alignment,
    font::{Style, Weight},
};

fn descriptor(width: f32) -> text::Text<()> {
    text::Text {
        content: (),
        bounds: Size::new(width, f32::INFINITY),
        size: Pixels(16.0),
        line_height: text::LineHeight::Absolute(Pixels(24.0)),
        font: Font::DEFAULT,
        align_x: text::Alignment::Left,
        align_y: alignment::Vertical::Top,
        shaping: text::Shaping::Advanced,
        wrapping: text::Wrapping::WordOrGlyph,
    }
}

fn main() {
    let normal = Font::DEFAULT;

    let bold = Font {
        weight: Weight::Bold,
        ..normal
    };

    let italic = Font {
        style: Style::Italic,
        ..normal
    };

    let bold_italic = Font {
        weight: Weight::Bold,
        style: Style::Italic,
        ..normal
    };

    let spans: Vec<text::Span<'static>> = vec![
        text::Span::new("Normal text ").font(normal),
        text::Span::new("bold text ").font(bold),
        text::Span::new("italic text ").font(italic),
        text::Span::new("bold italic text ").font(bold_italic),
        text::Span::new("monospace text")
            .font(Font::MONOSPACE)
            .color(Color::from_rgb(0.2, 0.4, 0.8)),
    ];

    let source = spans
        .iter()
        .map(|span| span.text.as_ref())
        .collect::<String>();

    let mut paragraph = Paragraph::with_spans(descriptor(120.0).with_content(spans.as_slice()));

    let mut seen_spans = BTreeSet::new();
    let mut run_count = 0;

    for run in paragraph.buffer().layout_runs() {
        run_count += 1;

        println!(
            "run={run_count} logical_line={} top={} height={}",
            run.line_i, run.line_top, run.line_height,
        );

        for glyph in run.glyphs {
            assert!(
                glyph.metadata < spans.len(),
                "glyph metadata must identify a source span"
            );

            seen_spans.insert(glyph.metadata);

            println!(
                "  span={} bytes={}..{} x={} width={}",
                glyph.metadata, glyph.start, glyph.end, glyph.x, glyph.w,
            );
        }
    }

    assert!(
        run_count > 1,
        "narrow rich text must wrap into multiple visual runs"
    );

    assert_eq!(
        seen_spans.len(),
        spans.len(),
        "every nonempty span must contribute shaped glyphs"
    );

    // Span geometry comes from the same paragraph used for shaping.
    for index in 0..spans.len() {
        let rectangles = paragraph.span_bounds(index);

        assert!(
            !rectangles.is_empty(),
            "each visible span must expose geometry"
        );

        for bounds in rectangles {
            assert!(bounds.width > 0.0);
            assert!(bounds.height > 0.0);

            println!("span={index} bounds={bounds:?}");
        }
    }

    // Hit testing uses the same buffer, with logical-line byte indices.
    let run = paragraph
        .buffer()
        .layout_runs()
        .next()
        .expect("a visual run must exist");

    let glyph = run
        .glyphs
        .first()
        .expect("the first run must contain glyphs");

    let hit = paragraph
        .buffer()
        .hit(
            glyph.x + glyph.w * 0.5,
            run.line_top + run.line_height * 0.5,
        )
        .expect("a point inside the first glyph must hit text");

    assert_eq!(hit.line, 0);
    assert!(hit.index <= source.len());
    assert!(source.is_char_boundary(hit.index));

    println!("hit={hit:?}");

    // Compare only checks the paragraph descriptor, not its span data.
    assert_eq!(paragraph.compare(descriptor(120.0)), text::Difference::None);

    assert_eq!(
        paragraph.compare(descriptor(260.0)),
        text::Difference::Bounds
    );

    paragraph.resize(Size::new(260.0, f32::INFINITY));

    assert_eq!(paragraph.compare(descriptor(260.0)), text::Difference::None);

    println!(
        "resized visual runs={}",
        paragraph.buffer().layout_runs().count()
    );

    println!("Rich paragraph layout probe completed.");
}
