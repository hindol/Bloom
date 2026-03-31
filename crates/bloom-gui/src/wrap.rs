use bloom_core::render::{LineSource, PaneFrame, RenderedLine, Style};
use iced::Rectangle;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::{CHAR_WIDTH, FONT_SIZE, GUTTER_WIDTH};

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct PaneViewportState {
    pub(crate) first_wrapped_row: usize,
    pub(crate) horizontal_offset: usize,
}

#[derive(Debug, Clone)]
struct ProjectedRow {
    line_idx: usize,
    source: LineSource,
    byte_start: usize,
    byte_end: usize,
    col_start: usize,
    col_end: usize,
    is_continuation: bool,
    row_height: f32,
    font_size: f32,
    char_width: f32,
}

#[derive(Debug, Clone)]
pub(crate) struct VisibleRow {
    pub(crate) line_idx: usize,
    pub(crate) source: LineSource,
    pub(crate) byte_start: usize,
    pub(crate) byte_end: usize,
    pub(crate) visible_byte_start: usize,
    pub(crate) visible_byte_end: usize,
    pub(crate) col_start: usize,
    pub(crate) visible_col_start: usize,
    pub(crate) is_continuation: bool,
    pub(crate) row_height: f32,
    pub(crate) font_size: f32,
    pub(crate) char_width: f32,
    pub(crate) y: f32,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CursorLayout {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) row_height: f32,
    pub(crate) char_width: f32,
    pub(crate) cell_width: f32,
    pub(crate) line_idx: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct PaneLayout {
    pub(crate) rows: Vec<VisibleRow>,
    pub(crate) cursor: Option<CursorLayout>,
}

pub(crate) fn heading_font_size(level: u8) -> f32 {
    match level {
        1 => FONT_SIZE * 1.5,
        2 => FONT_SIZE * 1.3,
        3 => FONT_SIZE * 1.1,
        _ => FONT_SIZE,
    }
}

pub(crate) fn line_font_size(line: &RenderedLine) -> f32 {
    line.spans
        .iter()
        .find_map(|s| match s.style {
            Style::Heading { level } => Some(heading_font_size(level)),
            _ => None,
        })
        .unwrap_or(FONT_SIZE)
}

pub(crate) fn line_row_height(line: &RenderedLine) -> f32 {
    line_font_size(line) * 1.4
}

pub(crate) fn line_char_width(line: &RenderedLine) -> f32 {
    CHAR_WIDTH * (line_font_size(line) / FONT_SIZE)
}

pub(crate) fn display_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

pub(crate) fn char_col_to_byte_offset(text: &str, column: usize) -> usize {
    text.char_indices()
        .nth(column)
        .map(|(idx, _)| idx)
        .unwrap_or(text.len())
}

pub(crate) fn line_text(line: &RenderedLine) -> &str {
    line.text.trim_end_matches(['\n', '\r'])
}

pub(crate) fn reconcile_viewport_state(
    viewport: &mut PaneViewportState,
    pane: &PaneFrame,
    content_area: Rectangle,
    word_wrap: bool,
    scrolloff: usize,
) {
    let rows = project_rows(pane, content_area.width, word_wrap);
    if rows.is_empty() {
        *viewport = PaneViewportState::default();
        return;
    }

    if !word_wrap {
        viewport.first_wrapped_row = 0;
    } else {
        viewport.first_wrapped_row = viewport.first_wrapped_row.min(rows.len().saturating_sub(1));
        let cursor_row = find_cursor_row_index(pane, &rows);
        let visible_rows =
            visible_row_indices(&rows, viewport.first_wrapped_row, content_area.height);
        let visible_count = visible_rows.len().max(1);
        let margin = scrolloff.min(visible_count.saturating_sub(1));
        let top_margin = viewport.first_wrapped_row.saturating_add(margin);
        let bottom_margin = viewport
            .first_wrapped_row
            .saturating_add(visible_count.saturating_sub(1).saturating_sub(margin));

        if cursor_row < top_margin {
            viewport.first_wrapped_row = cursor_row.saturating_sub(margin);
        } else if cursor_row > bottom_margin {
            viewport.first_wrapped_row = (cursor_row + margin + 1).saturating_sub(visible_count);
        }
        viewport.first_wrapped_row = viewport.first_wrapped_row.min(rows.len().saturating_sub(1));
    }

    if word_wrap {
        viewport.horizontal_offset = 0;
        return;
    }

    let cursor_line_idx = pane.cursor.line.saturating_sub(pane.scroll_offset);
    let Some(line) = pane.visible_lines.get(cursor_line_idx) else {
        viewport.horizontal_offset = 0;
        return;
    };
    let line_text = line_text(line);
    let cursor_byte = char_col_to_byte_offset(line_text, pane.cursor.column);
    let cursor_col = display_width(&line_text[..cursor_byte]);
    let visible_cols = visible_cols_for_line(line, content_area.width);
    let margin = scrolloff.min(visible_cols.saturating_sub(1));

    if cursor_col < viewport.horizontal_offset.saturating_add(margin) {
        viewport.horizontal_offset = cursor_col.saturating_sub(margin);
    }

    let right_edge = viewport
        .horizontal_offset
        .saturating_add(visible_cols.saturating_sub(margin));
    if cursor_col >= right_edge {
        viewport.horizontal_offset = cursor_col
            .saturating_add(margin)
            .saturating_add(1)
            .saturating_sub(visible_cols);
    }

    let total_cols = display_width(line_text);
    let max_offset = total_cols.saturating_sub(visible_cols);
    viewport.horizontal_offset = viewport.horizontal_offset.min(max_offset);
}

pub(crate) fn layout_pane(
    pane: &PaneFrame,
    content_area: Rectangle,
    word_wrap: bool,
    viewport: &PaneViewportState,
) -> PaneLayout {
    let rows = project_rows(pane, content_area.width, word_wrap);
    if rows.is_empty() {
        return PaneLayout {
            rows: Vec::new(),
            cursor: None,
        };
    }

    let first_row = if word_wrap {
        viewport.first_wrapped_row.min(rows.len().saturating_sub(1))
    } else {
        0
    };
    let visible_indices = visible_row_indices(&rows, first_row, content_area.height);

    let mut visible_rows = Vec::with_capacity(visible_indices.len());
    let mut y = content_area.y;
    for &row_idx in &visible_indices {
        let row = &rows[row_idx];
        let Some(line) = pane.visible_lines.get(row.line_idx) else {
            continue;
        };
        let (visible_byte_start, visible_byte_end, visible_col_start, _visible_col_end) =
            visible_window(
                line_text(line),
                row,
                word_wrap,
                viewport.horizontal_offset,
                content_area.width,
            );
        visible_rows.push(VisibleRow {
            line_idx: row.line_idx,
            source: row.source,
            byte_start: row.byte_start,
            byte_end: row.byte_end,
            visible_byte_start,
            visible_byte_end,
            col_start: row.col_start,
            visible_col_start,
            is_continuation: row.is_continuation,
            row_height: row.row_height,
            font_size: row.font_size,
            char_width: row.char_width,
            y,
        });
        y += row.row_height;
    }

    let cursor = build_cursor_layout(pane, &rows, &visible_rows, viewport.horizontal_offset);

    PaneLayout {
        rows: visible_rows,
        cursor,
    }
}

fn project_rows(pane: &PaneFrame, content_width: f32, word_wrap: bool) -> Vec<ProjectedRow> {
    let mut rows = Vec::new();
    for (line_idx, line) in pane.visible_lines.iter().enumerate() {
        let text = line_text(line);
        let row_height = line_row_height(line);
        let font_size = line_font_size(line);
        let char_width = line_char_width(line);
        let visible_cols = visible_cols_for_line(line, content_width);

        if !word_wrap || matches!(line.source, LineSource::BeyondEof) || text.is_empty() {
            rows.push(ProjectedRow {
                line_idx,
                source: line.source,
                byte_start: 0,
                byte_end: text.len(),
                col_start: 0,
                col_end: display_width(text),
                is_continuation: false,
                row_height,
                font_size,
                char_width,
            });
            continue;
        }

        let mut wrapped = wrap_line(text, visible_cols);
        if wrapped.is_empty() {
            wrapped.push((0, text.len(), 0, 0));
        }
        for (idx, (byte_start, byte_end, col_start, col_end)) in wrapped.into_iter().enumerate() {
            rows.push(ProjectedRow {
                line_idx,
                source: line.source,
                byte_start,
                byte_end,
                col_start,
                col_end,
                is_continuation: idx > 0,
                row_height,
                font_size,
                char_width,
            });
        }
    }
    rows
}

fn visible_row_indices(rows: &[ProjectedRow], first_row: usize, content_height: f32) -> Vec<usize> {
    let mut visible = Vec::new();
    let mut used = 0.0;
    for idx in first_row..rows.len() {
        let row_height = rows[idx].row_height;
        if !visible.is_empty() && used + row_height > content_height + 0.5 {
            break;
        }
        visible.push(idx);
        used += row_height;
    }
    if visible.is_empty() && !rows.is_empty() {
        visible.push(first_row.min(rows.len() - 1));
    }
    visible
}

fn build_cursor_layout(
    pane: &PaneFrame,
    rows: &[ProjectedRow],
    visible_rows: &[VisibleRow],
    horizontal_offset: usize,
) -> Option<CursorLayout> {
    let cursor_line_idx = pane.cursor.line.saturating_sub(pane.scroll_offset);
    let line = pane.visible_lines.get(cursor_line_idx)?;
    let text = line_text(line);
    let cursor_byte = char_col_to_byte_offset(text, pane.cursor.column);
    let cursor_display_col = display_width(&text[..cursor_byte]);
    let cursor_row_idx = find_cursor_row_index(pane, rows);
    let visible_row = visible_rows.iter().find(|row| {
        row.line_idx == cursor_line_idx
            && ((cursor_byte >= row.byte_start && cursor_byte < row.byte_end)
                || (cursor_byte == text.len() && row.byte_end == text.len()))
    })?;

    let x_cols = if rows.get(cursor_row_idx)?.is_continuation {
        cursor_display_col.saturating_sub(visible_row.col_start)
    } else {
        cursor_display_col.saturating_sub(visible_row.visible_col_start.max(horizontal_offset))
    };
    let cell_cols = grapheme_width_at(text, cursor_byte).max(1) as f32;

    Some(CursorLayout {
        x: GUTTER_WIDTH + x_cols as f32 * visible_row.char_width,
        y: visible_row.y,
        row_height: visible_row.row_height,
        char_width: visible_row.char_width,
        cell_width: visible_row.char_width * cell_cols,
        line_idx: cursor_line_idx,
    })
}

fn find_cursor_row_index(pane: &PaneFrame, rows: &[ProjectedRow]) -> usize {
    let cursor_line_idx = pane.cursor.line.saturating_sub(pane.scroll_offset);
    let Some(line) = pane.visible_lines.get(cursor_line_idx) else {
        return 0;
    };
    let text = line_text(line);
    let cursor_byte = char_col_to_byte_offset(text, pane.cursor.column);
    rows.iter()
        .enumerate()
        .find(|(_, row)| {
            row.line_idx == cursor_line_idx
                && ((cursor_byte >= row.byte_start && cursor_byte < row.byte_end)
                    || (cursor_byte == text.len() && row.byte_end == text.len()))
        })
        .map(|(idx, _)| idx)
        .unwrap_or_else(|| {
            rows.iter()
                .rposition(|row| row.line_idx == cursor_line_idx)
                .unwrap_or(0)
        })
}

fn visible_window(
    text: &str,
    row: &ProjectedRow,
    word_wrap: bool,
    horizontal_offset: usize,
    content_width: f32,
) -> (usize, usize, usize, usize) {
    if word_wrap {
        return (row.byte_start, row.byte_end, row.col_start, row.col_end);
    }

    let visible_cols = ((text_area_width(content_width) / row.char_width).floor() as usize).max(1);
    let window_start = horizontal_offset.max(row.col_start).min(row.col_end);
    let window_end = window_start.saturating_add(visible_cols).min(row.col_end);
    let visible_byte_start = advance_display_cols(
        text,
        row.byte_start,
        row.byte_end,
        window_start.saturating_sub(row.col_start),
    );
    let visible_byte_end = advance_display_cols(
        text,
        visible_byte_start,
        row.byte_end,
        window_end.saturating_sub(window_start),
    );
    (
        visible_byte_start,
        visible_byte_end,
        window_start,
        window_end,
    )
}

fn visible_cols_for_line(line: &RenderedLine, content_width: f32) -> usize {
    let char_width = line_char_width(line);
    ((text_area_width(content_width) / char_width).floor() as usize).max(1)
}

fn text_area_width(content_width: f32) -> f32 {
    (content_width - GUTTER_WIDTH).max(CHAR_WIDTH)
}

fn wrap_line(text: &str, max_cols: usize) -> Vec<(usize, usize, usize, usize)> {
    if text.is_empty() {
        return vec![(0, 0, 0, 0)];
    }

    let graphemes: Vec<(usize, &str, usize)> = UnicodeSegmentation::grapheme_indices(text, true)
        .map(|(idx, g)| (idx, g, display_width(g)))
        .collect();

    let mut rows = Vec::new();
    let mut start = 0;
    let mut col_start = 0;

    while start < graphemes.len() {
        let mut idx = start;
        let mut width = 0;
        let mut last_break: Option<(usize, usize)> = None;

        while idx < graphemes.len() {
            let (_, grapheme, g_width) = graphemes[idx];
            if idx > start && width + g_width > max_cols {
                break;
            }
            width += g_width;
            idx += 1;
            if grapheme.chars().all(|ch| ch.is_whitespace()) {
                last_break = Some((idx, width));
            }
            if width >= max_cols {
                break;
            }
        }

        let (end_idx, row_width) = if idx < graphemes.len() {
            last_break
                .filter(|(break_idx, _)| *break_idx > start)
                .unwrap_or((idx, width))
        } else {
            (idx, width)
        };

        let byte_start = graphemes[start].0;
        let byte_end = graphemes
            .get(end_idx)
            .map(|(idx, _, _)| *idx)
            .unwrap_or(text.len());
        rows.push((byte_start, byte_end, col_start, col_start + row_width));
        start = end_idx;
        col_start += row_width;
    }

    rows
}

fn advance_display_cols(text: &str, start_byte: usize, end_byte: usize, cols: usize) -> usize {
    if cols == 0 || start_byte >= end_byte {
        return start_byte.min(end_byte);
    }

    let mut consumed = 0;
    let slice = &text[start_byte..end_byte];
    for (rel_idx, grapheme) in UnicodeSegmentation::grapheme_indices(slice, true) {
        let width = display_width(grapheme);
        if consumed + width > cols {
            return start_byte + rel_idx;
        }
        consumed += width;
        if consumed >= cols {
            return start_byte + rel_idx + grapheme.len();
        }
    }
    end_byte
}

fn grapheme_width_at(text: &str, byte_offset: usize) -> usize {
    for (idx, grapheme) in UnicodeSegmentation::grapheme_indices(text, true) {
        let end = idx + grapheme.len();
        if byte_offset < end {
            return display_width(grapheme).max(1);
        }
    }
    1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LINE_HEIGHT;
    use bloom_core::render::{
        CursorShape, CursorState, PaneFrame, PaneKind, PaneRectFrame, RenderedLine,
    };

    fn plain_line(text: &str) -> RenderedLine {
        RenderedLine {
            source: LineSource::Buffer(0),
            text: text.to_string(),
            spans: vec![],
            is_mirror: false,
        }
    }

    fn pane_with_line(text: &str, column: usize) -> PaneFrame {
        PaneFrame {
            id: bloom_core::types::PaneId(0),
            kind: PaneKind::Editor,
            visible_lines: vec![plain_line(text)],
            cursor: CursorState {
                line: 0,
                column,
                shape: CursorShape::Block,
            },
            scroll_offset: 0,
            total_lines: 1,
            is_active: true,
            title: String::new(),
            dirty: false,
            status_bar: bloom_core::render::StatusBarFrame::default(),
            rect: PaneRectFrame {
                x: 0,
                y: 0,
                width: 20,
                content_height: 5,
                total_height: 6,
            },
        }
    }

    #[test]
    fn wrap_line_keeps_grapheme_clusters_intact() {
        let text = "a👨‍👩‍👧‍👦b";
        let rows = wrap_line(text, 2);
        let joined = rows
            .iter()
            .map(|(start, end, _, _)| &text[*start..*end])
            .collect::<String>();
        assert_eq!(joined, text);
        assert!(rows
            .iter()
            .any(|(start, end, _, _)| &text[*start..*end] == "👨‍👩‍👧‍👦"));
    }

    #[test]
    fn reconcile_horizontal_offset_follows_cursor() {
        let pane = pane_with_line("abcdefghijklmnopqrstuvwxyz", 20);
        let mut viewport = PaneViewportState::default();
        let area = Rectangle {
            x: 0.0,
            y: 0.0,
            width: GUTTER_WIDTH + CHAR_WIDTH * 8.0,
            height: LINE_HEIGHT * 4.0,
        };
        reconcile_viewport_state(&mut viewport, &pane, area, false, 2);
        assert!(viewport.horizontal_offset > 0);
    }

    #[test]
    fn layout_uses_horizontal_offset_in_no_wrap_mode() {
        let pane = pane_with_line("abcdefghijk", 8);
        let area = Rectangle {
            x: 0.0,
            y: 0.0,
            width: GUTTER_WIDTH + CHAR_WIDTH * 5.0,
            height: LINE_HEIGHT * 4.0,
        };
        let mut viewport = PaneViewportState::default();
        reconcile_viewport_state(&mut viewport, &pane, area, false, 1);
        let layout = layout_pane(&pane, area, false, &viewport);
        assert_eq!(layout.rows.len(), 1);
        assert!(layout.rows[0].visible_byte_start > 0);
    }
}
