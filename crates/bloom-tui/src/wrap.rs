use bloom_core::render::RenderedLine;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Measure the display width of a text slice.
/// Returns a unit appropriate for the frontend — columns for TUI, pixels for GUI.
pub trait MeasureWidth {
    fn width(&self, text: &str) -> usize;
}

pub struct MonospaceWidth;

impl MeasureWidth for MonospaceWidth {
    fn width(&self, text: &str) -> usize {
        UnicodeWidthStr::width(text)
    }
}

pub struct ScreenMap {
    entries: Vec<ScreenEntry>,
    total_screen_rows: usize,
}

struct ScreenEntry {
    screen_row_start: usize,
    row_count: usize,
    break_offsets: Vec<usize>,
}

impl ScreenMap {
    pub fn new(lines: &[RenderedLine], max_width: usize, measure: &dyn MeasureWidth) -> Self {
        let effective_width = if max_width == 0 { 1 } else { max_width };
        let mut entries = Vec::with_capacity(lines.len());
        let mut screen_row = 0;

        for line in lines {
            let text = line.text.trim_end_matches(['\n', '\r']);
            let breaks = compute_breaks(text, effective_width, measure);
            let row_count = breaks.len();
            entries.push(ScreenEntry {
                screen_row_start: screen_row,
                row_count,
                break_offsets: breaks,
            });
            screen_row += row_count;
        }

        ScreenMap {
            entries,
            total_screen_rows: screen_row,
        }
    }

    #[allow(dead_code)]
    pub fn total_screen_rows(&self) -> usize {
        self.total_screen_rows
    }

    /// Find the index in visible_lines for a given buffer line number.
    /// Returns None if the buffer line is not present in visible_lines.
    pub fn find_buffer_line(lines: &[RenderedLine], buffer_line: usize) -> Option<usize> {
        lines
            .iter()
            .position(|l| l.source.buffer_line() == Some(buffer_line))
    }

    /// Map a buffer (line_idx, column) to an absolute screen row.
    pub fn cursor_screen_row(
        &self,
        line_idx: usize,
        column: usize,
        _measure: &dyn MeasureWidth,
        lines: &[RenderedLine],
    ) -> usize {
        let entry = match self.entries.get(line_idx) {
            Some(e) => e,
            None => return self.total_screen_rows.saturating_sub(1),
        };
        let text = lines[line_idx].text.trim_end_matches(['\n', '\r']);
        let byte_col = char_col_to_byte(text, column);
        let wrap_idx = entry.wrap_index_for_byte(byte_col);
        entry.screen_row_start + wrap_idx
    }

    /// Column within the wrapped screen row for a given buffer (line_idx, column).
    pub fn cursor_col_in_row(
        &self,
        line_idx: usize,
        column: usize,
        measure: &dyn MeasureWidth,
        lines: &[RenderedLine],
    ) -> usize {
        let entry = match self.entries.get(line_idx) {
            Some(e) => e,
            None => return 0,
        };
        let text = lines[line_idx].text.trim_end_matches(['\n', '\r']);
        let byte_col = char_col_to_byte(text, column);
        let wrap_idx = entry.wrap_index_for_byte(byte_col);
        let row_start = entry.break_offsets[wrap_idx];
        let slice = &text[row_start..byte_col.min(text.len())];
        measure.width(slice)
    }

    /// Map an absolute screen row to (line_idx, wrap_offset, byte_start).
    /// Returns None if screen_row is out of range.
    pub fn screen_row_to_line(&self, screen_row: usize) -> Option<(usize, usize, usize)> {
        if self.entries.is_empty() {
            return None;
        }
        // Binary search for the entry containing this screen row
        let idx = match self
            .entries
            .binary_search_by_key(&screen_row, |e| e.screen_row_start)
        {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        };
        let entry = &self.entries[idx];
        if screen_row < entry.screen_row_start
            || screen_row >= entry.screen_row_start + entry.row_count
        {
            return None;
        }
        let wrap_offset = screen_row - entry.screen_row_start;
        let byte_start = entry.break_offsets[wrap_offset];
        Some((idx, wrap_offset, byte_start))
    }

    /// Get the byte end for a given line_idx and wrap_offset.
    pub fn row_byte_end(
        &self,
        line_idx: usize,
        wrap_offset: usize,
        lines: &[RenderedLine],
    ) -> usize {
        let entry = &self.entries[line_idx];
        let text = lines[line_idx].text.trim_end_matches(['\n', '\r']);
        if wrap_offset + 1 < entry.break_offsets.len() {
            entry.break_offsets[wrap_offset + 1]
        } else {
            text.len()
        }
    }
}

impl ScreenEntry {
    /// Which wrapped row (0-based within this entry) contains the given byte offset.
    fn wrap_index_for_byte(&self, byte: usize) -> usize {
        // Find the last break_offset <= byte
        match self.break_offsets.binary_search(&byte) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        }
    }
}

/// Compute the byte offsets where each wrapped row starts.
/// Always starts with [0]. A line that fits in max_width returns [0].
fn compute_breaks(text: &str, max_width: usize, measure: &dyn MeasureWidth) -> Vec<usize> {
    if text.is_empty() {
        return vec![0];
    }

    let mut breaks = vec![0usize];
    let mut row_start = 0;
    let mut current_width = 0usize;
    let mut last_ws_byte: Option<usize> = None;

    for (byte_idx, ch) in text.char_indices() {
        if byte_idx < row_start {
            continue;
        }
        let cw = UnicodeWidthChar::width(ch).unwrap_or(0);

        if current_width + cw > max_width {
            // Need to break
            let break_at = if let Some(ws_byte) = last_ws_byte {
                // Break after whitespace — next row starts at ws_byte
                ws_byte
            } else {
                // No whitespace found — hard break at current position
                byte_idx
            };

            if break_at <= row_start {
                // Edge case: single character wider than max_width, advance past it
                let next = byte_idx + ch.len_utf8();
                breaks.push(next);
                row_start = next;
                current_width = 0;
                last_ws_byte = None;
                continue;
            }

            breaks.push(break_at);
            row_start = break_at;
            // Recalculate width from new row_start up to current char
            let slice = &text[row_start..byte_idx];
            current_width = measure.width(slice) + cw;
            last_ws_byte = None;
        } else {
            current_width += cw;
        }

        if ch.is_whitespace() {
            // Record the byte position after this whitespace as a potential break point
            last_ws_byte = Some(byte_idx + ch.len_utf8());
        }
    }

    breaks
}

/// Convert a character-offset column to a byte offset within `text`.
fn char_col_to_byte(text: &str, col: usize) -> usize {
    text.char_indices()
        .nth(col)
        .map(|(i, _)| i)
        .unwrap_or(text.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bloom_core::render::RenderedLine;

    fn make_line(text: &str) -> RenderedLine {
        RenderedLine {
            source: bloom_core::render::LineSource::Buffer(0),
            text: text.to_string(),
            spans: vec![],
            is_mirror: false,
        }
    }

    #[test]
    fn no_wrap_short_line() {
        let lines = [make_line("hello")];
        let wm = ScreenMap::new(&lines, 80, &MonospaceWidth);
        assert_eq!(wm.total_screen_rows(), 1);
        assert_eq!(wm.screen_row_to_line(0), Some((0, 0, 0)));
    }

    #[test]
    fn wrap_at_word_boundary() {
        // "hello world" is 11 chars; max_width=7 should break at the space
        let lines = [make_line("hello world")];
        let wm = ScreenMap::new(&lines, 7, &MonospaceWidth);
        assert_eq!(wm.total_screen_rows(), 2);
        assert_eq!(wm.screen_row_to_line(0), Some((0, 0, 0)));
        assert_eq!(wm.screen_row_to_line(1), Some((0, 1, 6))); // "world" starts at byte 6
    }

    #[test]
    fn wrap_hard_break_no_space() {
        let lines = [make_line("abcdefghij")];
        let wm = ScreenMap::new(&lines, 5, &MonospaceWidth);
        assert_eq!(wm.total_screen_rows(), 2);
        assert_eq!(wm.screen_row_to_line(0), Some((0, 0, 0)));
        assert_eq!(wm.screen_row_to_line(1), Some((0, 1, 5)));
    }

    #[test]
    fn empty_line() {
        let lines = [make_line("")];
        let wm = ScreenMap::new(&lines, 80, &MonospaceWidth);
        assert_eq!(wm.total_screen_rows(), 1);
        assert_eq!(wm.screen_row_to_line(0), Some((0, 0, 0)));
    }

    #[test]
    fn line_exactly_at_max_width() {
        let lines = [make_line("12345")];
        let wm = ScreenMap::new(&lines, 5, &MonospaceWidth);
        assert_eq!(wm.total_screen_rows(), 1);
    }

    #[test]
    fn strips_trailing_newline() {
        let lines = [make_line("hello\n")];
        let wm = ScreenMap::new(&lines, 80, &MonospaceWidth);
        assert_eq!(wm.total_screen_rows(), 1);
    }

    #[test]
    fn cursor_screen_row_wrapped() {
        // "hello world" wrapped at 7: row0="hello " row1="world"
        let lines = [make_line("hello world")];
        let wm = ScreenMap::new(&lines, 7, &MonospaceWidth);
        // cursor at column 0 (h) -> screen row 0
        assert_eq!(wm.cursor_screen_row(0, 0, &MonospaceWidth, &lines), 0);
        // cursor at column 6 (w of "world") -> screen row 1
        assert_eq!(wm.cursor_screen_row(0, 6, &MonospaceWidth, &lines), 1);
    }

    #[test]
    fn cursor_col_in_row_wrapped() {
        let lines = [make_line("hello world")];
        let wm = ScreenMap::new(&lines, 7, &MonospaceWidth);
        // column 6 = 'w' of "world", should be col 0 in its wrapped row
        assert_eq!(wm.cursor_col_in_row(0, 6, &MonospaceWidth, &lines), 0);
        // column 7 = 'o' of "world", should be col 1
        assert_eq!(wm.cursor_col_in_row(0, 7, &MonospaceWidth, &lines), 1);
    }

    #[test]
    fn multiple_lines() {
        let lines = [make_line("short"), make_line("hello world wraps here")];
        let wm = ScreenMap::new(&lines, 10, &MonospaceWidth);
        // line 0: "short" -> 1 screen row
        // line 1: "hello world wraps here" -> should wrap
        assert!(wm.total_screen_rows() >= 3);
        assert_eq!(wm.screen_row_to_line(0), Some((0, 0, 0)));
        assert_eq!(wm.screen_row_to_line(1).unwrap().0, 1);
    }

    #[test]
    fn cjk_characters() {
        // Each CJK char is width 2
        let lines = [make_line("你好世界测试")]; // 6 chars, 12 columns
        let wm = ScreenMap::new(&lines, 6, &MonospaceWidth);
        // Should wrap: "你好世" (6 cols) | "界测试" (6 cols)
        assert_eq!(wm.total_screen_rows(), 2);
    }

    #[test]
    fn whitespace_only_line() {
        let lines = [make_line("     ")];
        let wm = ScreenMap::new(&lines, 3, &MonospaceWidth);
        assert_eq!(wm.total_screen_rows(), 2);
    }

    #[test]
    fn out_of_range_screen_row() {
        let lines = [make_line("hi")];
        let wm = ScreenMap::new(&lines, 80, &MonospaceWidth);
        assert_eq!(wm.screen_row_to_line(5), None);
    }
}
