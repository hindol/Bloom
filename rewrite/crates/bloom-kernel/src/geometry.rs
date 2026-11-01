//! Opaque units and the single text-layout authority for the kernel.

use std::ops::Range;

use bloom_text::{coordinates::RawCharIdx, DocError, DocSnapshot, EditDelta};
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
    pub byte_range: Range<usize>,
    pub start: RawCharIdx,
    pub end: RawCharIdx,
    pub origin: Point,
    pub advance: Px,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LayoutLine {
    pub text: String,
    pub start: RawCharIdx,
    pub end: RawCharIdx,
    pub origin: Point,
    pub width: Px,
    pub graphemes: Vec<PositionedGrapheme>,
}

impl LayoutLine {
    pub fn grapheme_text(&self, grapheme: &PositionedGrapheme) -> &str {
        &self.text[grapheme.byte_range.clone()]
    }
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
    CacheInconsistent,
}

impl Layout {
    pub fn compute(
        document: &DocSnapshot,
        cursor: RawCharIdx,
        viewport: Size,
        metrics: &dyn FontMetrics,
    ) -> Result<Self, LayoutError> {
        let text = document.text();
        document
            .raw_char_offset(cursor)
            .map_err(LayoutError::Document)?;
        let line_height = metrics.line_height();
        let zero = document.raw_char_idx(0).map_err(LayoutError::Document)?;
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
        let mut raw_char_offset = 0;

        for grapheme in text.graphemes(true) {
            let char_count = grapheme.chars().count();
            if grapheme == "\n" || grapheme == "\r\n" {
                let end = document
                    .raw_char_idx(raw_char_offset)
                    .map_err(LayoutError::Document)?;
                let next_offset = raw_char_offset + char_count;
                let start = document
                    .raw_char_idx(next_offset)
                    .map_err(LayoutError::Document)?;
                let line_index = lines.len();
                let current = lines.last_mut().expect("layout always has one line");
                current.end = end;
                raw_char_offset = next_offset;
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
                    .raw_char_idx(raw_char_offset)
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
                .raw_char_idx(raw_char_offset)
                .map_err(LayoutError::Document)?;
            let end_offset = document
                .raw_char_idx(raw_char_offset + char_count)
                .map_err(LayoutError::Document)?;
            let origin = Point {
                x: current.width,
                y: current.origin.y,
            };
            let byte_start = current.text.len();
            current.text.push_str(grapheme);
            current.graphemes.push(PositionedGrapheme {
                byte_range: byte_start..current.text.len(),
                start: start_offset,
                end: end_offset,
                origin,
                advance,
            });
            current.width = current.width.saturating_add(advance);
            current.end = end_offset;
            raw_char_offset += char_count;
        }

        let cursor = lines
            .windows(2)
            .find(|pair| pair[0].end < cursor && cursor < pair[1].start)
            .map_or(cursor, |pair| pair[0].end);
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

#[derive(Clone, Debug)]
pub(crate) struct LayoutCache {
    physical_lines: Vec<CachedPhysicalLine>,
    wrap_counts: Fenwick,
    char_counts: Fenwick,
    width: Px,
    line_height: PositivePx,
    scroll_top: usize,
    #[cfg(test)]
    reshaped_lines: usize,
}

#[derive(Clone, Debug)]
struct CachedPhysicalLine {
    char_len: usize,
    content_char_len: usize,
    visual_lines: Vec<CachedVisualLine>,
}

#[derive(Clone, Debug)]
struct CachedVisualLine {
    text: String,
    start: usize,
    end: usize,
    width: Px,
    graphemes: Vec<CachedGrapheme>,
}

#[derive(Clone, Debug)]
struct CachedGrapheme {
    byte_range: Range<usize>,
    start: usize,
    end: usize,
    origin: Point,
    advance: Px,
}

#[derive(Clone, Debug)]
struct Fenwick {
    values: Vec<usize>,
    tree: Vec<usize>,
}

impl LayoutCache {
    pub(crate) fn new(
        document: &DocSnapshot,
        viewport: Size,
        metrics: &dyn FontMetrics,
    ) -> Result<Self, LayoutError> {
        let line_height = metrics.line_height();
        let mut cache = Self {
            physical_lines: Vec::new(),
            wrap_counts: Fenwick::default(),
            char_counts: Fenwick::default(),
            width: viewport.width,
            line_height,
            scroll_top: 0,
            #[cfg(test)]
            reshaped_lines: 0,
        };
        cache.rebuild(document, metrics)?;
        Ok(cache)
    }

    pub(crate) fn apply_edit(
        &mut self,
        document: &DocSnapshot,
        delta: EditDelta,
        metrics: &dyn FontMetrics,
    ) -> Result<(), LayoutError> {
        let old_end = delta
            .start_line
            .checked_add(delta.old_line_count)
            .ok_or(LayoutError::CacheInconsistent)?;
        if delta.old_line_count == 0
            || delta.new_line_count == 0
            || old_end > self.physical_lines.len()
        {
            return Err(LayoutError::CacheInconsistent);
        }

        let new_end = delta
            .start_line
            .checked_add(delta.new_line_count)
            .ok_or(LayoutError::CacheInconsistent)?;
        if new_end > document.line_count() {
            return Err(LayoutError::CacheInconsistent);
        }
        let replacements = (delta.start_line..new_end)
            .map(|line| self.layout_physical_line(document, line, metrics))
            .collect::<Result<Vec<_>, _>>()?;
        if delta.old_line_count == delta.new_line_count {
            let end = delta.start_line + delta.new_line_count;
            for (line, replacement) in (delta.start_line..end).zip(replacements) {
                self.physical_lines[line] = replacement;
                let cached = &self.physical_lines[line];
                self.wrap_counts.set(line, cached.visual_lines.len());
                self.char_counts.set(line, cached.char_len);
            }
        } else {
            self.physical_lines
                .splice(delta.start_line..old_end, replacements);
            self.rebuild_prefix_counts();
        }
        if self.physical_lines.len() != document.line_count() {
            return Err(LayoutError::CacheInconsistent);
        }
        Ok(())
    }

    pub(crate) fn resize(
        &mut self,
        document: &DocSnapshot,
        viewport: Size,
        metrics: &dyn FontMetrics,
    ) -> Result<(), LayoutError> {
        if self.width != viewport.width {
            self.width = viewport.width;
            self.rebuild(document, metrics)?;
        }
        Ok(())
    }

    pub(crate) fn frame(
        &mut self,
        document: &DocSnapshot,
        cursor: RawCharIdx,
        viewport: Size,
    ) -> Result<Layout, LayoutError> {
        if self.physical_lines.len() != document.line_count() {
            return Err(LayoutError::CacheInconsistent);
        }
        let cursor_line = document
            .line_for_raw_char(cursor)
            .map_err(LayoutError::Document)?;
        let cursor_offset = document
            .raw_char_offset(cursor)
            .map_err(LayoutError::Document)?;
        let cursor_line_start = document
            .line_start(cursor_line)
            .ok_or(LayoutError::CacheInconsistent)?;
        let cursor_local = cursor_offset
            - document
                .raw_char_offset(cursor_line_start)
                .map_err(LayoutError::Document)?;
        let cached_cursor_line = self
            .physical_lines
            .get(cursor_line)
            .ok_or(LayoutError::CacheInconsistent)?;
        let cursor_local = cursor_local.min(cached_cursor_line.content_char_len);
        let cursor_visual_line = cached_cursor_line
            .visual_lines
            .iter()
            .enumerate()
            .rev()
            .find(|(_, line)| line.start <= cursor_local && cursor_local <= line.end)
            .ok_or(LayoutError::CacheInconsistent)?;
        let local_x = cursor_visual_line
            .1
            .graphemes
            .iter()
            .find(|grapheme| cursor_local <= grapheme.start || cursor_local < grapheme.end)
            .map_or(cursor_visual_line.1.width, |grapheme| grapheme.origin.x);
        let cursor_row = self.wrap_counts.prefix_sum(cursor_line) + cursor_visual_line.0;
        let rows_in_view =
            ((viewport.height.get() / self.line_height.get()).floor() as usize).max(1);
        let max_scroll = self.wrap_counts.total().saturating_sub(rows_in_view);
        self.scroll_top = self.scroll_top.min(max_scroll);
        if cursor_row < self.scroll_top {
            self.scroll_top = cursor_row;
        } else if cursor_row >= self.scroll_top.saturating_add(rows_in_view) {
            self.scroll_top = cursor_row.saturating_sub(rows_in_view - 1);
        }

        let visible_end = self
            .scroll_top
            .saturating_add(rows_in_view)
            .min(self.wrap_counts.total());
        let mut lines = Vec::with_capacity(visible_end.saturating_sub(self.scroll_top));
        for visual_row in self.scroll_top..visible_end {
            let physical_line = self
                .wrap_counts
                .index_for_order(visual_row)
                .ok_or(LayoutError::CacheInconsistent)?;
            let first_visual_row = self.wrap_counts.prefix_sum(physical_line);
            let row = self.physical_lines[physical_line]
                .visual_lines
                .get(visual_row - first_visual_row)
                .ok_or(LayoutError::CacheInconsistent)?;
            let char_start = self.char_counts.prefix_sum(physical_line);
            let y = self
                .line_height
                .as_px()
                .scaled_by(visual_row - self.scroll_top);
            lines.push(LayoutLine {
                text: row.text.clone(),
                start: document
                    .raw_char_idx(char_start + row.start)
                    .map_err(LayoutError::Document)?,
                end: document
                    .raw_char_idx(char_start + row.end)
                    .map_err(LayoutError::Document)?,
                origin: Point { x: Px::ZERO, y },
                width: row.width,
                graphemes: row
                    .graphemes
                    .iter()
                    .map(|grapheme| {
                        Ok(PositionedGrapheme {
                            byte_range: grapheme.byte_range.clone(),
                            start: document
                                .raw_char_idx(char_start + grapheme.start)
                                .map_err(LayoutError::Document)?,
                            end: document
                                .raw_char_idx(char_start + grapheme.end)
                                .map_err(LayoutError::Document)?,
                            origin: Point {
                                x: grapheme.origin.x,
                                y,
                            },
                            advance: grapheme.advance,
                        })
                    })
                    .collect::<Result<Vec<_>, LayoutError>>()?,
            });
        }
        let cursor_y = self
            .line_height
            .as_px()
            .scaled_by(cursor_row.saturating_sub(self.scroll_top));

        Ok(Layout {
            lines,
            cursor: Rect {
                origin: Point {
                    x: local_x,
                    y: cursor_y,
                },
                size: Size::new(Px(1.0), self.line_height.as_px()),
            },
            line_height: self.line_height,
        })
    }

    #[cfg(test)]
    fn reshaped_lines(&self) -> usize {
        self.reshaped_lines
    }

    #[cfg(test)]
    fn reset_reshaped_lines(&mut self) {
        self.reshaped_lines = 0;
    }

    fn rebuild(
        &mut self,
        document: &DocSnapshot,
        metrics: &dyn FontMetrics,
    ) -> Result<(), LayoutError> {
        self.physical_lines = (0..document.line_count())
            .map(|line| self.layout_physical_line(document, line, metrics))
            .collect::<Result<Vec<_>, _>>()?;
        self.rebuild_prefix_counts();
        Ok(())
    }

    fn rebuild_prefix_counts(&mut self) {
        self.wrap_counts = Fenwick::new(
            self.physical_lines
                .iter()
                .map(|line| line.visual_lines.len())
                .collect(),
        );
        self.char_counts = Fenwick::new(
            self.physical_lines
                .iter()
                .map(|line| line.char_len)
                .collect(),
        );
    }

    fn layout_physical_line(
        &mut self,
        document: &DocSnapshot,
        line: usize,
        metrics: &dyn FontMetrics,
    ) -> Result<CachedPhysicalLine, LayoutError> {
        let text = document
            .line_text(line)
            .ok_or(LayoutError::CacheInconsistent)?;
        let char_len = document
            .line_char_len(line)
            .ok_or(LayoutError::CacheInconsistent)?;
        #[cfg(test)]
        {
            self.reshaped_lines = self.reshaped_lines.saturating_add(1);
        }
        Ok(shape_physical_line(&text, char_len, self.width, metrics))
    }
}

impl Default for Fenwick {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl Fenwick {
    fn new(values: Vec<usize>) -> Self {
        let mut tree = vec![0usize; values.len() + 1];
        for (index, value) in values.iter().copied().enumerate() {
            let node = index + 1;
            tree[node] = tree[node].saturating_add(value);
            let parent = node + node.isolate_lowest_one();
            if parent < tree.len() {
                tree[parent] = tree[parent].saturating_add(tree[node]);
            }
        }
        Self { values, tree }
    }

    fn set(&mut self, index: usize, value: usize) {
        let old = self.values[index];
        self.values[index] = value;
        let mut node = index + 1;
        while node < self.tree.len() {
            if value >= old {
                self.tree[node] = self.tree[node].saturating_add(value - old);
            } else {
                self.tree[node] -= old - value;
            }
            node += node.isolate_lowest_one();
        }
    }

    fn prefix_sum(&self, end: usize) -> usize {
        let mut node = end;
        let mut sum = 0usize;
        while node > 0 {
            sum = sum.saturating_add(self.tree[node]);
            node -= node.isolate_lowest_one();
        }
        sum
    }

    fn total(&self) -> usize {
        self.prefix_sum(self.values.len())
    }

    fn index_for_order(&self, order: usize) -> Option<usize> {
        if order >= self.total() {
            return None;
        }
        let mut index = 0usize;
        let mut prefix = 0usize;
        let mut step = 1usize;
        while step <= self.values.len() / 2 {
            step *= 2;
        }
        while step > 0 {
            let next = index + step;
            if next < self.tree.len() && prefix.saturating_add(self.tree[next]) <= order {
                index = next;
                prefix = prefix.saturating_add(self.tree[next]);
            }
            step /= 2;
        }
        Some(index)
    }
}

fn shape_physical_line(
    text: &str,
    char_len: usize,
    viewport_width: Px,
    metrics: &dyn FontMetrics,
) -> CachedPhysicalLine {
    let mut visual_lines = vec![CachedVisualLine {
        text: String::new(),
        start: 0,
        end: 0,
        width: Px::ZERO,
        graphemes: Vec::new(),
    }];
    let mut raw_char_offset = 0usize;
    for grapheme in text.graphemes(true) {
        let char_count = grapheme.chars().count();
        let advance = metrics.grapheme_advance(grapheme);
        let current = visual_lines
            .last_mut()
            .expect("a physical line has a visual row");
        let next_width = current.width.saturating_add(advance);
        if !current.graphemes.is_empty() && next_width > viewport_width {
            current.end = raw_char_offset;
            visual_lines.push(CachedVisualLine {
                text: String::new(),
                start: raw_char_offset,
                end: raw_char_offset,
                width: Px::ZERO,
                graphemes: Vec::new(),
            });
        }
        let current = visual_lines
            .last_mut()
            .expect("a physical line has a visual row");
        let byte_start = current.text.len();
        current.text.push_str(grapheme);
        let origin = Point {
            x: current.width,
            y: Px::ZERO,
        };
        current.graphemes.push(CachedGrapheme {
            byte_range: byte_start..current.text.len(),
            start: raw_char_offset,
            end: raw_char_offset + char_count,
            origin,
            advance,
        });
        current.width = current.width.saturating_add(advance);
        current.end = raw_char_offset + char_count;
        raw_char_offset += char_count;
    }
    CachedPhysicalLine {
        char_len,
        content_char_len: text.chars().count(),
        visual_lines,
    }
}

#[cfg(test)]
mod tests {
    use bloom_text::Doc;

    use super::{FontMetrics, Layout, LayoutCache, MonospaceFontMetrics, PositivePx, Px, Size};

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
            doc.raw_char_idx(cursor).unwrap(),
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
            doc.raw_char_idx(1).unwrap(),
            Size::new(Px::new(2.0).unwrap(), Px::new(100.0).unwrap()),
            &test_metrics(2.0, 10.0),
        )
        .unwrap();

        assert_eq!(
            result.lines[0].grapheme_text(&result.lines[0].graphemes[0]),
            "e\u{301}"
        );
        assert_eq!(
            result.lines[0].graphemes[0].start,
            doc.raw_char_idx(0).unwrap()
        );
        assert_eq!(
            result.lines[0].graphemes[0].end,
            doc.raw_char_idx(2).unwrap()
        );
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

    #[test]
    fn incremental_cache_matches_full_layout_after_line_splits_and_merges() {
        let mut doc = Doc::new("ab\ncd\nef");
        let viewport = Size::new(Px::new(8.0).unwrap(), Px::new(100.0).unwrap());
        let metrics = test_metrics(2.0, 10.0);
        let mut cache = LayoutCache::new(&doc.snapshot(), viewport, &metrics).unwrap();
        cache.reset_reshaped_lines();

        let (cursor, delta) = doc
            .insert_with_delta(doc.raw_char_idx(1).unwrap(), "\nX\n")
            .unwrap();
        cache
            .apply_edit(&doc.snapshot(), delta.unwrap(), &metrics)
            .unwrap();
        assert_eq!(cache.reshaped_lines(), 3);
        assert_eq!(
            cache.frame(&doc.snapshot(), cursor, viewport).unwrap(),
            Layout::compute(&doc.snapshot(), cursor, viewport, &metrics).unwrap()
        );

        cache.reset_reshaped_lines();
        let (cursor, delta) = doc.delete_previous_char_with_delta(cursor).unwrap();
        cache
            .apply_edit(&doc.snapshot(), delta.unwrap(), &metrics)
            .unwrap();
        assert_eq!(cache.reshaped_lines(), 1);
        assert_eq!(
            cache.frame(&doc.snapshot(), cursor, viewport).unwrap(),
            Layout::compute(&doc.snapshot(), cursor, viewport, &metrics).unwrap()
        );
    }

    #[test]
    fn ten_thousand_line_edit_reshapes_one_line_and_frames_only_the_viewport() {
        let text = (0..10_000)
            .map(|line| format!("line {line}"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut doc = Doc::new(&text);
        let viewport = Size::new(Px::new(800.0).unwrap(), Px::new(20.0).unwrap());
        let metrics = test_metrics(2.0, 10.0);
        let mut cache = LayoutCache::new(&doc.snapshot(), viewport, &metrics).unwrap();
        cache.reset_reshaped_lines();

        let at = doc.snapshot().line_start(9_999).unwrap();
        let (cursor, delta) = doc.insert_with_delta(at, "x").unwrap();
        cache
            .apply_edit(&doc.snapshot(), delta.unwrap(), &metrics)
            .unwrap();
        let frame = cache.frame(&doc.snapshot(), cursor, viewport).unwrap();

        assert_eq!(cache.reshaped_lines(), 1);
        assert_eq!(frame.lines.len(), 2);
        assert_eq!(frame.lines[0].text, "line 9998");
        assert_eq!(frame.lines[1].text, "xline 9999");
        assert_eq!(frame.cursor.origin.y, Px::new(10.0).unwrap());
    }

    #[test]
    fn cursor_inside_crlf_is_placed_at_the_physical_line_end() {
        let doc = Doc::new("a\r\nb");
        let snapshot = doc.snapshot();
        let cursor = snapshot.raw_char_idx(2).unwrap();
        let viewport = Size::new(Px::new(20.0).unwrap(), Px::new(100.0).unwrap());
        let metrics = test_metrics(2.0, 10.0);
        let mut cache = LayoutCache::new(&snapshot, viewport, &metrics).unwrap();

        assert_eq!(
            cache.frame(&snapshot, cursor, viewport).unwrap(),
            Layout::compute(&snapshot, cursor, viewport, &metrics).unwrap()
        );
    }

    #[test]
    fn cached_layout_preserves_unicode_grapheme_boundaries_and_byte_ranges() {
        let doc = Doc::new("e\u{301}🙂\n中");
        let snapshot = doc.snapshot();
        let cursor = doc.raw_char_idx(1).unwrap();
        let viewport = Size::new(Px::new(2.0).unwrap(), Px::new(100.0).unwrap());
        let metrics = test_metrics(2.0, 10.0);
        let mut cache = LayoutCache::new(&snapshot, viewport, &metrics).unwrap();

        let cached = cache.frame(&snapshot, cursor, viewport).unwrap();
        let full = Layout::compute(&snapshot, cursor, viewport, &metrics).unwrap();

        assert_eq!(cached, full);
        assert_eq!(
            cached.lines[0].grapheme_text(&cached.lines[0].graphemes[0]),
            "e\u{301}"
        );
        assert_eq!(
            cached.lines[0].graphemes[0].end,
            doc.raw_char_idx(2).unwrap()
        );
    }

    #[test]
    fn width_resize_reflows_cached_lines_but_height_resize_does_not() {
        let doc = Doc::new("abcd");
        let snapshot = doc.snapshot();
        let metrics = test_metrics(2.0, 10.0);
        let mut cache = LayoutCache::new(
            &snapshot,
            Size::new(Px::new(8.0).unwrap(), Px::new(100.0).unwrap()),
            &metrics,
        )
        .unwrap();
        cache.reset_reshaped_lines();
        cache
            .resize(
                &snapshot,
                Size::new(Px::new(8.0).unwrap(), Px::new(10.0).unwrap()),
                &metrics,
            )
            .unwrap();
        assert_eq!(cache.reshaped_lines(), 0);

        cache
            .resize(
                &snapshot,
                Size::new(Px::new(4.0).unwrap(), Px::new(10.0).unwrap()),
                &metrics,
            )
            .unwrap();
        assert_eq!(cache.reshaped_lines(), 1);
        let frame = cache
            .frame(
                &snapshot,
                doc.raw_char_idx(0).unwrap(),
                Size::new(Px::new(4.0).unwrap(), Px::new(10.0).unwrap()),
            )
            .unwrap();
        assert_eq!(frame.lines.len(), 1);
        assert_eq!(frame.lines[0].text, "ab");
    }
}
