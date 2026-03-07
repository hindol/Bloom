use super::*;

pub(super) fn draw_panes(
    f: &mut Frame,
    _area: Rect,
    panes: &[PaneFrame],
    _maximized: bool,
    hidden_count: usize,
    theme: &TuiTheme,
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
        draw_pane(f, pane_area, pane, theme);
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

pub(super) fn draw_pane(f: &mut Frame, area: Rect, pane: &PaneFrame, theme: &TuiTheme) {
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

    match &pane.kind {
        PaneKind::Editor => draw_editor_content(f, content_area, pane, theme),
        PaneKind::Agenda(agenda) => super::agenda::draw_agenda(f, content_area, agenda, theme, 0),
        PaneKind::Timeline(tl) => super::timeline::draw_timeline(f, content_area, tl, theme),
        PaneKind::UndoTree(ut) => super::undo_tree::draw_undo_tree(f, content_area, ut, theme),
        PaneKind::SetupWizard(sw) => super::wizard::draw_setup_wizard(f, content_area, sw, theme),
    }

    // Status bar: slot-based for active pane, compact for inactive
    if pane.is_active {
        let cursor =
            super::status_bar::draw_status_bar_slot(f, status_area, &pane.status_bar, theme);
        // Cursor: status bar slots (command line, quick capture) take priority
        if let Some((cx, cy)) = cursor {
            f.set_cursor_position((cx, cy));
        } else if matches!(&pane.kind, PaneKind::Editor) {
            let line_number_width = 5u16;
            let cursor_y = pane.cursor.line.saturating_sub(pane.scroll_offset);
            let cy = content_area.y + cursor_y as u16;
            let cx = content_area.x + line_number_width + pane.cursor.column as u16;
            let frame_area = f.area();
            if cy < content_area.bottom() && cx < frame_area.width {
                f.set_cursor_position((cx, cy));
            }
        }
    } else {
        draw_inactive_pane_bar(f, status_area, pane, theme);
    }
}

pub(super) fn draw_editor_content(f: &mut Frame, area: Rect, pane: &PaneFrame, theme: &TuiTheme) {
    let height = area.height as usize;
    let line_number_width = 5u16;
    let total_width = area.width as usize;
    let _content_width = total_width.saturating_sub(line_number_width as usize);

    let bg = theme.background();
    let base_normal = RStyle::default().fg(theme.foreground()).bg(bg);
    let faded_bg = theme.faded_style().bg(bg);

    for row in 0..height {
        let y = area.y + row as u16;

        if row >= pane.visible_lines.len() {
            // Beyond EOF: full-width row with tilde in gutter, spaces in content
            let tilde_pad = " ".repeat(total_width.saturating_sub(5));
            let line = Line::from(vec![
                Span::styled("  ~  ", faded_bg),
                Span::styled(tilde_pad, RStyle::default().bg(bg)),
            ]);
            f.render_widget(Paragraph::new(line), Rect::new(area.x, y, area.width, 1));
            continue;
        }

        let rendered_line = &pane.visible_lines[row];

        // Determine base style (cursor line highlight or normal)
        let is_cursor_line = pane.is_active && rendered_line.line_number == pane.cursor.line;
        let base_style = if is_cursor_line {
            theme.current_line_style()
        } else {
            base_normal
        };

        // Build the full row: line_number + content + padding
        let mut spans: Vec<Span> = Vec::new();

        // Line number gutter
        let lnum = format!("{:>3}  ", rendered_line.line_number + 1);
        let lnum_style = if is_cursor_line { base_style } else { faded_bg };
        spans.push(Span::styled(lnum, lnum_style));

        // Content text with syntax highlighting
        let text = rendered_line.text.trim_end_matches(['\n', '\r']);
        let text_width = text.width();

        if rendered_line.spans.is_empty() {
            spans.push(Span::styled(text, base_style));
        } else {
            let mut last_end = 0usize;
            for span_info in &rendered_line.spans {
                let start = span_info.range.start.min(text.len());
                let end = span_info.range.end.min(text.len());
                if last_end < start {
                    let gap = &text[last_end..start];
                    spans.push(Span::styled(gap.to_string(), base_style));
                }
                let slice = &text[start..end];
                let style = base_style.patch(theme.style_for(&span_info.style));
                spans.push(Span::styled(slice.to_string(), style));
                last_end = end;
            }
            if last_end < text.len() {
                spans.push(Span::styled(&text[last_end..], base_style));
            }
        }

        // Pad remaining width with spaces in base_style
        let used = line_number_width as usize + text_width;
        if used < total_width {
            spans.push(Span::styled(" ".repeat(total_width - used), base_style));
        }

        f.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect::new(area.x, y, area.width, 1),
        );
    }
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
