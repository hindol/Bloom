//! Opaque units and the single text-layout authority for the kernel.

use bloom_text::{coordinates::CharIdx, DocError, DocSnapshot};
use unicode_segmentation::UnicodeSegmentation;

/// A zero-based display column produced by layout.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DisplayCol(usize);

impl DisplayCol {
    pub const fn new(value: usize) -> Self {
        Self(value)
    }

    pub const fn get(self) -> usize {
        self.0
    }
}

/// A finite, non-negative logical-pixel measure.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct Px(f32);

impl Px {
    pub const ZERO: Self = Self(0.0);

    pub fn new(value: f32) -> Option<Self> {
        (value.is_finite() && value >= 0.0).then_some(Self(value))
    }

    pub const fn get(self) -> f32 {
        self.0
    }

    fn saturating_add(self, other: Self) -> Self {
        Self((self.0 + other.0).min(f32::MAX))
    }

    fn scaled_by(self, factor: usize) -> Self {
        Self(((self.0 as f64) * (factor as f64)).min(f32::MAX as f64) as f32)
    }
}

/// A logical-pixel measure that is strictly greater than zero.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct PositivePx(Px);

impl PositivePx {
    pub fn new(value: Px) -> Option<Self> {
        (value.get() > 0.0).then_some(Self(value))
    }

    pub const fn get(self) -> f32 {
        self.0.get()
    }

    fn as_px(self) -> Px {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Size {
    pub width: Px,
    pub height: Px,
}

impl Size {
    pub const fn new(width: Px, height: Px) -> Self {
        Self { width, height }
    }
}

impl Default for Size {
    fn default() -> Self {
        Self::new(Px(800.0), Px(600.0))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: Px,
    pub y: Px,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub origin: Point,
    pub size: Size,
}

/// The layout engine asks the injected implementation for one grapheme's
/// advance. The GUI supplies shaping-backed metrics; tests use fixed metrics.
pub trait FontMetrics: Send + Sync + std::fmt::Debug {
    fn grapheme_advance(&self, grapheme: &str) -> Px;
    fn line_height(&self) -> PositivePx;
}

#[derive(Clone, Copy, Debug)]
pub struct MonospaceFontMetrics {
    grapheme_width: Px,
    line_height: PositivePx,
}

impl MonospaceFontMetrics {
    pub const fn new(grapheme_width: Px, line_height: PositivePx) -> Self {
        Self {
            grapheme_width,
            line_height,
        }
    }
}

impl Default for MonospaceFontMetrics {
    fn default() -> Self {
        Self::new(Px(8.0), PositivePx(Px(20.0)))
    }
}

impl FontMetrics for MonospaceFontMetrics {
    fn grapheme_advance(&self, _grapheme: &str) -> Px {
        self.grapheme_width
    }

    fn line_height(&self) -> PositivePx {
        self.line_height
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PositionedGrapheme {
    pub text: String,
    pub start: CharIdx,
    pub end: CharIdx,
    pub origin: Point,
    pub advance: Px,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LayoutLine {
    pub text: String,
    pub start: CharIdx,
    pub end: CharIdx,
    pub origin: Point,
    pub width: Px,
    pub graphemes: Vec<PositionedGrapheme>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub lines: Vec<LayoutLine>,
    pub cursor: Rect,
    pub line_height: PositivePx,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LayoutError {
    Document(DocError),
}

impl Layout {
    pub fn compute(
        document: &DocSnapshot,
        cursor: CharIdx,
        viewport: Size,
        metrics: &dyn FontMetrics,
    ) -> Result<Self, LayoutError> {
        let text = document.text();
        document
            .char_offset(cursor)
            .map_err(LayoutError::Document)?;
        let line_height = metrics.line_height();
        let zero = document.char_idx(0).map_err(LayoutError::Document)?;
        let mut lines = vec![LayoutLine {
            text: String::new(),
            start: zero,
            end: zero,
            origin: Point {
                x: Px::ZERO,
                y: Px::ZERO,
            },
            width: Px::ZERO,
            graphemes: Vec::new(),
        }];
        let mut char_offset = 0;

        for grapheme in text.graphemes(true) {
            let char_count = grapheme.chars().count();
            if grapheme == "\n" || grapheme == "\r\n" {
                let end = document
                    .char_idx(char_offset)
                    .map_err(LayoutError::Document)?;
                let next_offset = char_offset + char_count;
                let start = document
                    .char_idx(next_offset)
                    .map_err(LayoutError::Document)?;
                let line_index = lines.len();
                let current = lines.last_mut().expect("layout always has one line");
                current.end = end;
                char_offset = next_offset;
                lines.push(LayoutLine {
                    text: String::new(),
                    start,
                    end: start,
                    origin: Point {
                        x: Px::ZERO,
                        y: line_height.as_px().scaled_by(line_index),
                    },
                    width: Px::ZERO,
                    graphemes: Vec::new(),
                });
                continue;
            }

            let advance = metrics.grapheme_advance(grapheme);
            let current = lines.last_mut().expect("layout always has one line");
            let next_width = current.width.saturating_add(advance);
            if !current.graphemes.is_empty() && next_width > viewport.width {
                let wrap_start = document
                    .char_idx(char_offset)
                    .map_err(LayoutError::Document)?;
                current.end = wrap_start;
                let line_index = lines.len();
                lines.push(LayoutLine {
                    text: String::new(),
                    start: wrap_start,
                    end: wrap_start,
                    origin: Point {
                        x: Px::ZERO,
                        y: line_height.as_px().scaled_by(line_index),
                    },
                    width: Px::ZERO,
                    graphemes: Vec::new(),
                });
            }

            let current = lines.last_mut().expect("layout always has one line");
            let start_offset = document
                .char_idx(char_offset)
                .map_err(LayoutError::Document)?;
            let end_offset = document
                .char_idx(char_offset + char_count)
                .map_err(LayoutError::Document)?;
            let origin = Point {
                x: current.width,
                y: current.origin.y,
            };
            current.text.push_str(grapheme);
            current.graphemes.push(PositionedGrapheme {
                text: grapheme.to_owned(),
                start: start_offset,
                end: end_offset,
                origin,
                advance,
            });
            current.width = current.width.saturating_add(advance);
            current.end = end_offset;
            char_offset += char_count;
        }

        let cursor_line = lines
            .iter()
            .enumerate()
            .rev()
            .find(|(_, line)| line.start <= cursor && cursor <= line.end)
            .expect("validated cursor must belong to a laid-out line");
        let cursor_x = cursor_line
            .1
            .graphemes
            .iter()
            .find(|grapheme| cursor <= grapheme.start || cursor < grapheme.end)
            .map_or(cursor_line.1.width, |grapheme| grapheme.origin.x);
        let cursor_rect = Rect {
            origin: Point {
                x: cursor_x,
                y: cursor_line.1.origin.y,
            },
            size: Size::new(Px(1.0), line_height.as_px()),
        };

        Ok(Self {
            lines,
            cursor: cursor_rect,
            line_height,
        })
    }
}

#[cfg(test)]
mod tests {
    use bloom_text::Doc;

    use super::{FontMetrics, Layout, MonospaceFontMetrics, PositivePx, Px, Size};

    fn test_metrics(width: f32, line_height: f32) -> MonospaceFontMetrics {
        MonospaceFontMetrics::new(
            Px::new(width).unwrap(),
            PositivePx::new(Px::new(line_height).unwrap()).unwrap(),
        )
    }

    fn layout(text: &str, cursor: usize, width: f32) -> Layout {
        let doc = Doc::new(text);
        let snapshot = doc.snapshot();
        Layout::compute(
            &snapshot,
            doc.char_idx(cursor).unwrap(),
            Size::new(Px::new(width).unwrap(), Px::new(100.0).unwrap()),
            &test_metrics(2.0, 10.0),
        )
        .unwrap()
    }

    #[test]
    fn wraps_at_grapheme_boundaries_and_places_cursor_at_wrap_start() {
        let result = layout("abcd", 2, 4.0);

        assert_eq!(
            result
                .lines
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>(),
            ["ab", "cd"]
        );
        assert_eq!(result.cursor.origin.x, Px::ZERO);
        assert_eq!(result.cursor.origin.y, Px::new(10.0).unwrap());
    }

    #[test]
    fn hard_newlines_preserve_empty_lines_and_cursor_coordinates() {
        let result = layout("a\n\nb", 3, 10.0);

        assert_eq!(
            result
                .lines
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>(),
            ["a", "", "b"]
        );
        assert_eq!(result.cursor.origin.x, Px::ZERO);
        assert_eq!(result.cursor.origin.y, Px::new(20.0).unwrap());
    }

    #[test]
    fn trailing_newline_creates_a_cursor_line() {
        let result = layout("a\n", 2, 10.0);

        assert_eq!(result.lines.len(), 2);
        assert_eq!(result.cursor.origin.x, Px::ZERO);
        assert_eq!(result.cursor.origin.y, Px::new(10.0).unwrap());
    }

    #[test]
    fn grapheme_clusters_are_not_split_by_wrapping() {
        let doc = Doc::new("e\u{301}x");
        let snapshot = doc.snapshot();
        let result = Layout::compute(
            &snapshot,
            doc.char_idx(1).unwrap(),
            Size::new(Px::new(2.0).unwrap(), Px::new(100.0).unwrap()),
            &test_metrics(2.0, 10.0),
        )
        .unwrap();

        assert_eq!(result.lines[0].graphemes[0].text, "e\u{301}");
        assert_eq!(result.lines[0].graphemes[0].start, doc.char_idx(0).unwrap());
        assert_eq!(result.lines[0].graphemes[0].end, doc.char_idx(2).unwrap());
        assert_eq!(result.lines[0].graphemes.len(), 1);
    }

    #[test]
    fn fixed_metrics_provide_positive_line_height() {
        let metrics = test_metrics(1.0, 2.0);

        assert_eq!(
            metrics.line_height(),
            PositivePx::new(Px::new(2.0).unwrap()).unwrap()
        );
    }
}
