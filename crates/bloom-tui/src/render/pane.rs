use super::*;

pub(super) fn draw_panes(
    f: &mut Frame,
    _area: Rect,
    panes: &[PaneFrame],
    _maximized: bool,
    hidden_count: usize,
    theme: &TuiTheme,
    config: &bloom_core::config::Config,
) {
    if panes.is_empty() {
        return;
    }

    for pane in panes {
        let pane_area = Rect::new(
            pane.rect.x,
            pane.rect.y,
            pane.rect.width,
            pane.rect.total_height,
        );
        draw_pane(f, pane_area, pane, theme, config);
    }

    // Hidden pane count indicator (top-right)
    if hidden_count > 0 {
        let area = f.area();
        let indicator = format!("[{hidden_count} hidden]");
        let x = area.right().saturating_sub(indicator.width() as u16 + 1);
        if x > area.x {
            let span = Span::styled(&indicator, theme.faded_style());
            f.render_widget(
                Paragraph::new(Line::from(span)),
                Rect::new(x, area.y, indicator.width() as u16, 1),
            );
        }
    }
}

pub(super) fn draw_pane(
    f: &mut Frame,
    area: Rect,
    pane: &PaneFrame,
    theme: &TuiTheme,
    config: &bloom_core::config::Config,
) {
    if area.height < 2 {
        return;
    }

    // Content and status bar areas from core-computed rect
    let content_area = Rect::new(
        pane.rect.x,
        pane.rect.y,
        pane.rect.width,
        pane.rect.content_height,
    );
    let status_area = Rect::new(
        pane.rect.x,
        pane.rect.y + pane.rect.content_height,
        pane.rect.width,
        1,
    );

    let wrap_cursor = match &pane.kind {
        PaneKind::Editor => draw_editor_content(f, content_area, pane, theme, config),
        PaneKind::Timeline(tl) => {
            super::timeline::draw_timeline(f, content_area, tl, theme);
            None
        }
        PaneKind::UndoTree(ut) => {
            super::undo_tree::draw_undo_tree(f, content_area, ut, theme);
            None
        }
        PaneKind::SetupWizard(sw) => {
            super::wizard::draw_setup_wizard(f, content_area, sw, theme);
            None
        }
    };

    // Status bar: slot-based for active pane, compact for inactive
    if pane.is_active {
        let cursor =
            super::status_bar::draw_status_bar_slot(f, status_area, &pane.status_bar, theme);
        // Cursor: status bar slots (command line, quick capture) take priority
        if let Some((cx, cy)) = cursor {
            f.set_cursor_position((cx, cy));
        } else if matches!(&pane.kind, PaneKind::Editor) {
            // ScreenMap computes cursor position for both wrap and no-wrap modes.
            if let Some((cx, cy)) = wrap_cursor {
                f.set_cursor_position((cx, cy));
            }
        }
    } else {
        draw_inactive_pane_bar(f, status_area, pane, theme);
    }
}

/// Draw the editor content area. Always returns the cursor `(cx, cy)` position.
/// Uses ScreenMap for both wrap and no-wrap modes — no-wrap is just max-width.
pub(super) fn draw_editor_content(
    f: &mut Frame,
    area: Rect,
    pane: &PaneFrame,
    theme: &TuiTheme,
    config: &bloom_core::config::Config,
) -> Option<(u16, u16)> {
    draw_editor_content_unified(f, area, pane, theme, config)
}

/// Unified rendering path — ScreenMap handles both wrap and no-wrap.
/// Returns cursor `(cx, cy)` for the active pane.
fn draw_editor_content_unified(
    f: &mut Frame,
    area: Rect,
    pane: &PaneFrame,
    theme: &TuiTheme,
    config: &bloom_core::config::Config,
) -> Option<(u16, u16)> {
    use crate::scroll::ScreenScroll;
    use crate::wrap::{MonospaceWidth, ScreenMap};

    let height = area.height as usize;
    let line_number_width = 5u16;
    let total_width = area.width as usize;
    let content_width = if config.word_wrap {
        total_width.saturating_sub(line_number_width as usize)
    } else {
        usize::MAX // no wrapping
    };

    let bg = theme.background();
    let base_normal = RStyle::default().fg(theme.foreground()).bg(bg);
    let faded_bg = theme.faded_style().bg(bg);

    let measure = MonospaceWidth;
    let screen_map = ScreenMap::new(&pane.visible_lines, content_width, &measure);

    // Find the cursor's position in visible_lines by scanning for its buffer line.
    let cursor_entry_idx = ScreenMap::find_buffer_line(&pane.visible_lines, pane.cursor.line)
        .unwrap_or(0);
    let cursor_screen_row = screen_map.cursor_screen_row(
        cursor_entry_idx,
        pane.cursor.column,
        &measure,
        &pane.visible_lines,
    );
    let cursor_col_in_row = screen_map.cursor_col_in_row(
        cursor_entry_idx,
        pane.cursor.column,
        &measure,
        &pane.visible_lines,
    );

    let mut scroll = ScreenScroll::new();
    scroll.ensure_visible(cursor_screen_row, height, config.scrolloff);
    let first_screen_row = scroll.first_screen_row;

    let wrap_indicator = &config.wrap_indicator;

    let mut cursor_pos: Option<(u16, u16)> = None;

    for display_row in 0..height {
        let y = area.y + display_row as u16;
        let screen_row = first_screen_row + display_row;

        let info = screen_map.screen_row_to_line(screen_row);
        if info.is_none() {
            // Beyond EOF
            let tilde_pad = " ".repeat(total_width.saturating_sub(5));
            let line = Line::from(vec![
                Span::styled("  ~  ", faded_bg),
                Span::styled(tilde_pad, RStyle::default().bg(bg)),
            ]);
            f.render_widget(Paragraph::new(line), Rect::new(area.x, y, area.width, 1));
            continue;
        }

        let (line_idx, wrap_offset, byte_start) = info.unwrap();
        let rendered_line = &pane.visible_lines[line_idx];
        let text = rendered_line.text.trim_end_matches(['\n', '\r']);

        let is_cursor_line = pane.is_active
            && rendered_line.source.buffer_line() == Some(pane.cursor.line);
        let base_style = if is_cursor_line {
            theme.current_line_style()
        } else {
            base_normal
        };


        let mut spans: Vec<Span> = Vec::new();

        // Gutter
        if wrap_offset == 0 {
            let lnum = match rendered_line.source.buffer_line() {
                Some(n) => format!("{:>3}  ", n + 1),
                None => "     ".to_string(),
            };
            let lnum_style = if is_cursor_line { base_style } else { faded_bg };
            spans.push(Span::styled(lnum, lnum_style));
        } else {
            let gutter_w = line_number_width as usize;
            let indicator_display = format!(
                "{:>width$} ",
                wrap_indicator,
                width = gutter_w.saturating_sub(1)
            );
            let indicator_display = super::truncate_to_width(&indicator_display, gutter_w);
            let pad = gutter_w.saturating_sub(indicator_display.width());
            let indicator_display = if pad > 0 {
                format!("{}{}", " ".repeat(pad), indicator_display)
            } else {
                indicator_display
            };
            spans.push(Span::styled(indicator_display, faded_bg));
        }

        // Content: slice from byte_start to next break
        let byte_end = screen_map.row_byte_end(line_idx, wrap_offset, &pane.visible_lines);
        let row_text = &text[byte_start..byte_end.min(text.len())];
        let row_text_width = row_text.width();

        // Render with span clipping
        if rendered_line.spans.is_empty() {
            spans.push(Span::styled(row_text, base_style));
        } else {
            let mut last_end = byte_start;
            for span_info in &rendered_line.spans {
                let s = span_info.range.start.max(byte_start).min(text.len());
                let e = span_info.range.end.min(byte_end).min(text.len());
                if s >= e {
                    continue;
                }
                if last_end < s {
                    let gap = &text[last_end..s];
                    spans.push(Span::styled(gap.to_string(), base_style));
                }
                let slice = &text[s..e];
                let style = base_style.patch(theme.style_for(&span_info.style));
                spans.push(Span::styled(slice.to_string(), style));
                last_end = e;
            }
            if last_end < byte_end.min(text.len()) {
                spans.push(Span::styled(
                    text[last_end..byte_end.min(text.len())].to_string(),
                    base_style,
                ));
            }
        }

        // Pad remaining width
        let used = line_number_width as usize + row_text_width;
        if used < total_width {
            spans.push(Span::styled(" ".repeat(total_width - used), base_style));
        }

        f.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect::new(area.x, y, area.width, 1),
        );

        // Track cursor position
        if pane.is_active && screen_row == cursor_screen_row {
            let cy = y;
            let cx = area.x + line_number_width + cursor_col_in_row as u16;
            let frame_area = f.area();
            if cy < area.bottom() && cx < frame_area.width {
                cursor_pos = Some((cx, cy));
            }
        }
    }

    cursor_pos
}

/// Lightweight bar for inactive panes — just the title.
pub(super) fn draw_inactive_pane_bar(
    f: &mut Frame,
    area: Rect,
    pane: &PaneFrame,
    theme: &TuiTheme,
) {
    let style = theme.status_bar_style("NORMAL", false);
    let width = area.width as usize;

    // Fill background
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(" ".repeat(width), style))),
        area,
    );

    let title = truncate_with_ellipsis(&pane.title, width.saturating_sub(2));
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(format!(" {title}"), style))),
        area,
    );
}
