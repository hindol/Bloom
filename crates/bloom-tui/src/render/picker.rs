use super::*;

pub(super) fn draw_picker(f: &mut Frame, area: Rect, picker: &PickerFrame, theme: &TuiTheme) {
    // Adaptive sizing per ADAPTIVE_LAYOUT.md
    // Wide pickers (search, backlinks) get more horizontal space
    let w_pct = if picker.wide {
        if area.width >= 180 {
            90
        } else if area.width >= 140 {
            85
        } else {
            80
        }
    } else if area.width >= 180 {
        75
    } else if area.width >= 140 {
        65
    } else if area.width < 80 {
        90
    } else {
        60
    };
    let h_pct = if area.height >= 50 {
        75
    } else if area.height >= 30 {
        70
    } else {
        60
    };
    let w = (area.width * w_pct / 100).max(30).min(area.width);
    let h = (area.height * h_pct / 100).max(10).min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    let picker_area = Rect::new(x, y, w, h);

    f.render_widget(Clear, picker_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {} ", picker.title))
        .style(theme.picker_style())
        .border_style(theme.border_style());
    let inner = block.inner(picker_area);
    f.render_widget(block, picker_area);

    if inner.height < 3 {
        return;
    }

    // Determine layout: side-by-side preview (wide) or bottom preview (compact)
    let side_preview = picker_area.width >= 80 && area.width >= 160;
    let (content_area, side_preview_area) = if side_preview {
        let left_w = inner.width * 60 / 100;
        let right_w = inner.width.saturating_sub(left_w + 1); // 1 for separator
        let left = Rect::new(inner.x, inner.y, left_w, inner.height);
        let right = Rect::new(inner.x + left_w + 1, inner.y, right_w, inner.height);
        (left, Some(right))
    } else {
        (inner, None)
    };

    // Bottom preview only when not using side-by-side layout
    let has_bottom_preview = !side_preview && picker.preview.is_some() && inner.height >= 8;
    let preview_h = if has_bottom_preview {
        // Give roughly 1/3 of inner height to preview, minimum 3 lines
        (inner.height / 3).max(3)
    } else {
        0
    };
    // 1 line for the separator between results and preview
    let separator_h = if has_bottom_preview { 1 } else { 0 };
    let top_h = content_area
        .height
        .saturating_sub(preview_h)
        .saturating_sub(separator_h);

    // Query line (indented)
    let query_style = if picker.query_selected && !picker.query.is_empty() {
        // Mild background indicates select-all — typing replaces the query
        RStyle::default().fg(theme.foreground()).bg(theme.mild())
    } else {
        theme.picker_style().add_modifier(Modifier::BOLD)
    };
    let query_line = Line::from(vec![
        Span::styled(" > ", theme.picker_style()),
        Span::styled(&picker.query, query_style),
    ]);
    f.render_widget(
        Paragraph::new(query_line),
        Rect::new(content_area.x, content_area.y, content_area.width, 1),
    );
    // Place cursor at end of query input (overrides editor cursor)
    let query_cx = content_area.x + 3 + picker.query.width() as u16;
    f.set_cursor_position((query_cx, content_area.y));

    // Results (between query and footer)
    let results_h = top_h.saturating_sub(2); // -1 query, -1 footer
    let results_area = Rect::new(
        content_area.x,
        content_area.y + 1,
        content_area.width,
        results_h,
    );

    // Show hint when query is below minimum length
    if picker.results.is_empty()
        && picker.min_query_len > 0
        && picker.query.len() < picker.min_query_len
    {
        let hint = "Type to search…";
        let hx = results_area.x + (results_area.width.saturating_sub(hint.width() as u16)) / 2;
        let hy = results_area.y + results_area.height / 2;
        if hy < results_area.bottom() {
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(hint, theme.faded_style()))),
                Rect::new(hx, hy, hint.width() as u16, 1),
            );
        }
    }

    let _available = results_area.width as usize;

    // Multi-column grid (ADAPTIVE_LAYOUT.md §4): for short items on wide screens
    let max_item_w: usize = picker
        .results
        .iter()
        .map(|r| r.label.width() + r.right.as_deref().map_or(0, |s| s.width() + 2))
        .max()
        .unwrap_or(0);
    let multi_col = area.width >= 140 && max_item_w <= 30 && !picker.results.is_empty();

    if multi_col {
        draw_picker_multi_column(f, results_area, picker, theme);
    } else {
        draw_picker_single_column(f, results_area, picker, area, theme);
    }

    // Footer: count with selection position
    let footer = if picker.filtered_count > 0 {
        format!(
            "  {} / {} {}",
            picker.selected_index + 1,
            picker.filtered_count,
            picker.status_noun
        )
    } else {
        format!("  0 / {} {}", picker.total_count, picker.status_noun)
    };
    let footer_y = content_area.y + top_h.saturating_sub(1);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(footer, theme.faded_style()))),
        Rect::new(content_area.x, footer_y, content_area.width, 1),
    );

    // Side-by-side preview (wide layout)
    if let (Some(right_area), Some(preview_text)) = (side_preview_area, &picker.preview) {
        // Vertical separator
        let sep_x = right_area.x.saturating_sub(1);
        for row in 0..inner.height {
            f.render_widget(
                Paragraph::new(Line::from(Span::styled("│", theme.border_style()))),
                Rect::new(sep_x, inner.y + row, 1, 1),
            );
        }
        // Preview with 1-cell padding
        let preview_area = Rect::new(
            right_area.x + 1,
            right_area.y,
            right_area.width.saturating_sub(2),
            right_area.height,
        );
        render_highlighted_preview(f, preview_area, preview_text, &picker.query, theme);
    }

    // Bottom preview (compact fallback)
    if let (true, Some(preview_text)) = (has_bottom_preview, &picker.preview) {
        let sep_y = content_area.y + top_h;
        // Horizontal separator
        let sep_line = "─".repeat(inner.width as usize);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(sep_line, theme.border_style()))),
            Rect::new(inner.x, sep_y, inner.width, 1),
        );

        let preview_area = Rect::new(
            inner.x + 2,
            sep_y + 1,
            inner.width.saturating_sub(4),
            preview_h.saturating_sub(1),
        );
        render_highlighted_preview(f, preview_area, preview_text, &picker.query, theme);
    }
}

/// Single-column result list (standard layout).
pub(super) fn draw_picker_single_column(
    f: &mut Frame,
    results_area: Rect,
    picker: &PickerFrame,
    terminal_area: Rect,
    theme: &TuiTheme,
) {
    let available = results_area.width as usize;
    let viewport_h = results_area.height as usize;
    let scroll_offset = if picker.selected_index >= viewport_h {
        picker.selected_index - viewport_h + 1
    } else {
        0
    };

    let visible_end = (scroll_offset + viewport_h).min(picker.results.len());
    let visible_slice = &picker.results[scroll_offset..visible_end];

    // Column layout: label (flex) | middle/tags (capped) | right/date (fixed)
    // Right column is fixed-width (dates are always ~10 chars)
    let max_right_w: usize = visible_slice
        .iter()
        .map(|row| row.right.as_deref().unwrap_or("").width())
        .max()
        .unwrap_or(0)
        .min(16); // dates/counts never need more than 16 chars
    let right_zone = if max_right_w > 0 { max_right_w + 2 } else { 0 };

    // Middle column (tags) is capped and hidden on narrow terminals
    let middle_zone = if terminal_area.width < 100 {
        0
    } else {
        let max_middle_w: usize = visible_slice
            .iter()
            .map(|row| row.middle.as_deref().unwrap_or("").width())
            .max()
            .unwrap_or(0);
        if max_middle_w == 0 {
            0
        } else {
            // Cap at 20% of available or 30 chars, whichever is smaller
            let cap = (available / 5).min(30);
            max_middle_w.min(cap) + 2
        }
    };

    let marker_w = 3;
    let label_max = available.saturating_sub(marker_w + middle_zone + right_zone);

    let faded_style = theme.faded_style();

    for (vi, row) in visible_slice.iter().enumerate() {
        let abs_i = scroll_offset + vi;
        let is_selected = abs_i == picker.selected_index;
        let style = if is_selected {
            theme.picker_selected()
        } else {
            theme.picker_style()
        };
        let marker = if is_selected { " ▸ " } else { "   " };

        let label = truncate_to_width(&row.label, label_max);
        let label_w = label.width();
        let middle_text = row.middle.as_deref().unwrap_or("");
        let right_text = row.right.as_deref().unwrap_or("");
        let label_gap = available.saturating_sub(marker_w + label_w + middle_zone + right_zone);

        let mut spans: Vec<Span> = Vec::new();
        spans.push(Span::styled(marker, style));

        if !picker.query.is_empty() && !is_selected {
            let match_spans =
                bloom_core::render::search_highlight::highlight_matches(&label, &picker.query);
            let match_style = theme.style_for(&bloom_core::parser::traits::Style::SearchMatch);
            if match_spans.is_empty() {
                spans.push(Span::styled(label.clone(), style));
            } else {
                let mut pos = 0;
                for ms in &match_spans {
                    let s = ms.range.start.min(label.len());
                    let e = ms.range.end.min(label.len());
                    if s > pos {
                        spans.push(Span::styled(&label[pos..s], style));
                    }
                    if s < e {
                        spans.push(Span::styled(&label[s..e], match_style));
                    }
                    pos = e;
                }
                if pos < label.len() {
                    spans.push(Span::styled(&label[pos..], style));
                }
            }
        } else {
            spans.push(Span::styled(label.clone(), style));
        }
        spans.push(Span::styled(" ".repeat(label_gap), style));

        if middle_zone > 0 {
            let mid_truncated = truncate_to_width(middle_text, middle_zone.saturating_sub(2));
            let mid_w = mid_truncated.width();
            let mid_padded = format!(
                "{}{}",
                mid_truncated,
                " ".repeat(middle_zone.saturating_sub(mid_w))
            );
            spans.push(Span::styled(
                mid_padded,
                if is_selected { style } else { faded_style },
            ));
        }
        if right_zone > 0 {
            let right_padded = format!(
                "{}{}",
                " ".repeat(right_zone.saturating_sub(right_text.width())),
                right_text
            );
            spans.push(Span::styled(
                right_padded,
                if is_selected { style } else { faded_style },
            ));
        }

        f.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect::new(
                results_area.x,
                results_area.y + vi as u16,
                results_area.width,
                1,
            ),
        );
    }
}

/// Multi-column grid layout for short-item pickers (Tags, Commands, Templates).
/// Per ADAPTIVE_LAYOUT.md §4: newspaper flow, ↑↓ within column, max 4 columns.
pub(super) fn draw_picker_multi_column(
    f: &mut Frame,
    results_area: Rect,
    picker: &PickerFrame,
    theme: &TuiTheme,
) {
    if picker.results.is_empty() {
        return;
    }

    // Column sizing per spec: col_width = max(item_width) + 4, col_count = min(4, width/col_width)
    let max_item_w: usize = picker
        .results
        .iter()
        .map(|r| r.label.width() + r.right.as_deref().map_or(0, |s| s.width() + 2))
        .max()
        .unwrap_or(10);
    let col_width = (max_item_w + 6).max(12); // +4 padding + 2 for marker
    let col_count = (results_area.width as usize / col_width).clamp(1, 4);
    let rows_per_col = results_area.height as usize;
    let items_per_page = col_count * rows_per_col;

    // Scroll: which page of items is visible
    let page_start = (picker.selected_index / items_per_page) * items_per_page;

    let faded_style = theme.faded_style();
    let actual_col_w = results_area.width as usize / col_count;

    for (i, row) in picker
        .results
        .iter()
        .enumerate()
        .skip(page_start)
        .take(items_per_page)
    {
        let local_i = i - page_start;
        let col = local_i / rows_per_col;
        let row_in_col = local_i % rows_per_col;

        let is_selected = i == picker.selected_index;
        let style = if is_selected {
            theme.picker_selected()
        } else {
            theme.picker_style()
        };
        let marker = if is_selected { " ▸ " } else { "   " };

        let right_text = row.right.as_deref().unwrap_or("");
        let right_w = right_text.width();
        let label_max = actual_col_w.saturating_sub(3 + right_w + 2); // marker + right + gap
        let label = truncate_to_width(&row.label, label_max);
        let label_w = label.width();
        let gap = actual_col_w.saturating_sub(3 + label_w + right_w);

        let mut spans: Vec<Span> = Vec::new();
        spans.push(Span::styled(marker, style));
        spans.push(Span::styled(label, style));
        spans.push(Span::styled(" ".repeat(gap), style));
        if right_w > 0 {
            spans.push(Span::styled(
                right_text,
                if is_selected { style } else { faded_style },
            ));
        }

        let cx = results_area.x + (col * actual_col_w) as u16;
        let cy = results_area.y + row_in_col as u16;
        if cy < results_area.bottom() {
            f.render_widget(
                Paragraph::new(Line::from(spans)),
                Rect::new(cx, cy, actual_col_w as u16, 1),
            );
        }
    }
}

/// Render preview text with semantic highlighting and optional search term overlay.
/// Used by both the picker preview pane and the agenda preview pane.
pub(super) fn render_highlighted_preview(
    f: &mut Frame,
    area: Rect,
    text: &str,
    search_query: &str,
    theme: &TuiTheme,
) {
    let faded = theme.faded_style();
    let match_style = theme.style_for(&bloom_core::parser::traits::Style::SearchMatch);
    let search_spans = bloom_core::render::search_highlight::highlight_matches(text, search_query);
    let match_ranges: Vec<std::ops::Range<usize>> =
        search_spans.iter().map(|s| s.range.clone()).collect();

    let parser = bloom_core::parser::BloomMarkdownParser::new();
    use bloom_core::parser::traits::DocumentParser as _;
    let mut line_ctx = bloom_core::parser::traits::LineContext::default();

    let mut byte_offset = 0usize;
    for (i, line) in text.lines().enumerate() {
        if i as u16 >= area.height {
            break;
        }
        let line_start = byte_offset;
        let line_end = line_start + line.len();

        if line.trim().starts_with("```") {
            line_ctx.in_code_block = !line_ctx.in_code_block;
        }

        let syntax_spans = parser.highlight_line(line, &line_ctx);

        // Build base spans from syntax highlighting
        let mut base_spans: Vec<(std::ops::Range<usize>, RStyle)> = Vec::new();
        if syntax_spans.is_empty() {
            base_spans.push((0..line.len(), faded));
        } else {
            let mut last = 0;
            for si in &syntax_spans {
                let s = si.range.start.min(line.len());
                let e = si.range.end.min(line.len());
                if s > last {
                    base_spans.push((last..s, faded));
                }
                if s < e {
                    base_spans.push((s..e, theme.style_for(&si.style)));
                }
                last = e;
            }
            if last < line.len() {
                base_spans.push((last..line.len(), faded));
            }
        }

        // Overlay search match highlighting
        let mut spans: Vec<Span> = Vec::new();
        for (range, style) in &base_spans {
            let mut pos = range.start;
            for mr in &match_ranges {
                if mr.end <= line_start || mr.start >= line_end {
                    continue;
                }
                let rel_start = mr
                    .start
                    .saturating_sub(line_start)
                    .max(range.start)
                    .min(range.end);
                let rel_end = mr
                    .end
                    .saturating_sub(line_start)
                    .max(range.start)
                    .min(range.end);
                if rel_start > pos && pos < range.end {
                    spans.push(Span::styled(&line[pos..rel_start.min(range.end)], *style));
                }
                if rel_start < rel_end {
                    spans.push(Span::styled(&line[rel_start..rel_end], match_style));
                }
                pos = rel_end;
            }
            if pos < range.end {
                spans.push(Span::styled(&line[pos..range.end], *style));
            }
        }
        if spans.is_empty() {
            spans.push(Span::styled(line, faded));
        }

        f.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect::new(area.x, area.y + i as u16, area.width, 1),
        );
        byte_offset = line_end + 1;
    }
}
