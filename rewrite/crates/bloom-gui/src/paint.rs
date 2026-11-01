use bloom_kernel::{
    geometry::{Point, Rect},
    update::Frame,
};

use crate::font::FontSpec;

#[derive(Clone, Debug, PartialEq)]
pub struct PaintedGrapheme {
    pub text: String,
    pub origin: Point,
    pub font: FontSpec,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PaintSnapshot {
    pub graphemes: Vec<PaintedGrapheme>,
    pub cursor: Rect,
}

pub trait PaintSink {
    fn draw_grapheme(&mut self, grapheme: &PaintedGrapheme);
    fn draw_cursor(&mut self, cursor: Rect);
}

/// Paints only positions supplied by the kernel's layout authority.
pub fn paint(frame: &Frame, font: FontSpec, sink: &mut impl PaintSink) -> PaintSnapshot {
    let graphemes = frame
        .layout
        .lines
        .iter()
        .flat_map(|line| {
            line.graphemes.iter().map(move |grapheme| PaintedGrapheme {
                text: line.grapheme_text(grapheme).to_owned(),
                origin: grapheme.origin,
                font,
            })
        })
        .collect::<Vec<_>>();
    let cursor = frame.layout.cursor;

    for grapheme in &graphemes {
        sink.draw_grapheme(grapheme);
    }
    sink.draw_cursor(cursor);

    PaintSnapshot { graphemes, cursor }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bloom_kernel::{
        geometry::{FontMetrics, Px, Size},
        update::{update, Event, Model},
    };

    use crate::{
        font::CosmicFontMetrics,
        paint::{paint, PaintSink, PaintedGrapheme},
    };

    #[derive(Default)]
    struct RecordingSink {
        graphemes: Vec<PaintedGrapheme>,
        cursor: Option<bloom_kernel::geometry::Rect>,
    }

    impl PaintSink for RecordingSink {
        fn draw_grapheme(&mut self, grapheme: &PaintedGrapheme) {
            self.graphemes.push(grapheme.clone());
        }

        fn draw_cursor(&mut self, cursor: bloom_kernel::geometry::Rect) {
            self.cursor = Some(cursor);
        }
    }

    #[test]
    fn painted_snapshot_uses_exact_kernel_grapheme_and_cursor_positions() {
        let metrics = Arc::new(CosmicFontMetrics::default());
        let font = metrics.spec();
        let two_graphemes = metrics.grapheme_advance("a").get() * 2.0;
        let mut model = Model::with_font_metrics(
            "abcd",
            Size::new(Px::new(two_graphemes).unwrap(), Px::new(100.0).unwrap()),
            metrics,
        );
        update(
            &mut model,
            Event::Resize(Size::new(
                Px::new(two_graphemes).unwrap(),
                Px::new(100.0).unwrap(),
            )),
            std::time::Duration::ZERO,
        )
        .unwrap();
        let frame = bloom_kernel::update::view(&model);
        let mut sink = RecordingSink::default();

        let snapshot = paint(&frame, font, &mut sink);

        assert_eq!(snapshot.cursor, frame.layout.cursor);
        assert_eq!(sink.cursor, Some(frame.layout.cursor));
        assert_eq!(
            snapshot
                .graphemes
                .iter()
                .map(|grapheme| (grapheme.text.as_str(), grapheme.origin))
                .collect::<Vec<_>>(),
            frame
                .layout
                .lines
                .iter()
                .flat_map(|line| line
                    .graphemes
                    .iter()
                    .map(move |grapheme| { (line.grapheme_text(grapheme), grapheme.origin) }))
                .collect::<Vec<_>>()
        );
        assert_eq!(sink.graphemes, snapshot.graphemes);
        assert!(frame.layout.lines.len() > 1);
        assert_eq!(font.line_height(), frame.layout.line_height);
    }
}
