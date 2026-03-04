use bloom_core::render::{
    DialogFrame, McpIndicator, NotificationLevel, PaneFrame, PaneKind,
    PickerFrame, RenderFrame, StatusBarContent, StatusBarFrame, WhichKeyFrame,
};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style as RStyle};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

use crate::theme::TuiTheme;

/// Render the full RenderFrame to the terminal.
pub fn draw(f: &mut Frame, frame: &RenderFrame, theme: &TuiTheme) {
    let area = f.area();

    // Fill background
    f.render_widget(
        Block::default().style(RStyle::default().bg(theme.background())),
        area,
    );

    // Layout: panes | status bar (1 line) | which-key drawer (optional)
    let wk_h = if let Some(wk) = &frame.which_key {
        let col_width = 24u16;
        let cols = (area.width.saturating_sub(4) / col_width).max(1);
        let rows_needed = ((wk.entries.len() as u16) + cols - 1) / cols;
        // +1 for top border, +1 for vertical padding
        (rows_needed + 2).min(area.height / 3).max(3)
    } else {
        0
    };

    let status_h = 1u16;
    let pane_h = area.height.saturating_sub(status_h).saturating_sub(wk_h);

    let pane_area = Rect::new(area.x, area.y, area.width, pane_h);
    let status_area = Rect::new(area.x, area.y + pane_h, area.width, status_h);
    let wk_area = if wk_h > 0 {
        Some(Rect::new(area.x, area.y + pane_h + status_h, area.width, wk_h))
    } else {
        None
    };

    // Draw panes
    draw_panes(f, pane_area, &frame.panes, frame.maximized, frame.hidden_pane_count, theme);

    // Draw the global status bar
    let status_bar_cursor = draw_status_bar_slot(f, status_area, &frame.status_bar, theme);

    // Which-key drawer
    if let (Some(wk), Some(wk_rect)) = (&frame.which_key, wk_area) {
        draw_which_key(f, wk_rect, wk, theme);
    }

    // Overlays
    if let Some(picker) = &frame.picker {
        draw_picker(f, area, picker, theme);
    }
    if let Some(dialog) = &frame.dialog {
        draw_dialog(f, area, dialog, theme);
    }
    if let Some(notif) = &frame.notification {
        draw_notification(f, area, &notif.message, &notif.level, theme);
    }

    // Cursor placement: status bar slots take priority over pane cursor
    if let Some((cx, cy)) = status_bar_cursor {
        f.set_cursor_position((cx, cy));
    }
}

// ---------------------------------------------------------------------------
// Panes
// ---------------------------------------------------------------------------

fn draw_panes(
    f: &mut Frame,
    area: Rect,
    panes: &[PaneFrame],
    _maximized: bool,
    hidden_count: usize,
    theme: &TuiTheme,
) {
    if panes.is_empty() {
        return;
    }

    // Simple layout: split equally among panes (binary split tree is handled by core;
    // here we just tile them left-to-right for multiple panes).
    let constraints: Vec<Constraint> = panes
        .iter()
        .map(|_| Constraint::Ratio(1, panes.len() as u32))
        .collect();

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);

    for (i, pane) in panes.iter().enumerate() {
        draw_pane(f, chunks[i], pane, theme);
    }

    // Hidden pane count indicator (top-right)
    if hidden_count > 0 {
        let indicator = format!("[{hidden_count} hidden]");
        let x = area.right().saturating_sub(indicator.len() as u16 + 1);
        if x > area.x {
            let span = Span::styled(&indicator, theme.faded_style());
            f.render_widget(
                Paragraph::new(Line::from(span)),
                Rect::new(x, area.y, indicator.len() as u16, 1),
            );
        }
    }
}

fn draw_pane(f: &mut Frame, area: Rect, pane: &PaneFrame, theme: &TuiTheme) {
    if area.height < 2 {
        return;
    }

    if pane.is_active {
        // Active pane: full content area (status bar is global)
        match &pane.kind {
            PaneKind::Editor => draw_editor_content(f, area, pane, theme),
            PaneKind::Agenda(agenda) => draw_agenda(f, area, agenda, theme),
            PaneKind::Timeline(tl) => draw_timeline(f, area, tl, theme),
            PaneKind::UndoTree(ut) => draw_undo_tree(f, area, ut, theme),
            PaneKind::SetupWizard(sw) => draw_setup_wizard(f, area, sw, theme),
        }

        // Cursor for active editor pane (may be overridden by status bar cursor)
        if matches!(&pane.kind, PaneKind::Editor) {
            let line_number_width = 4u16;
            let cursor_y = pane.cursor.line.saturating_sub(pane.scroll_offset);
            let cy = area.y + cursor_y as u16;
            let cx = area.x + line_number_width + pane.cursor.column as u16;
            if cy < area.bottom() && cx < area.right() {
                f.set_cursor_position((cx, cy));
            }
        }
    } else {
        // Inactive pane: content + 1-line title separator
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(1),                      // content
                Constraint::Length(1),                    // title separator
            ])
            .split(area);

        match &pane.kind {
            PaneKind::Editor => draw_editor_content(f, layout[0], pane, theme),
            PaneKind::Agenda(agenda) => draw_agenda(f, layout[0], agenda, theme),
            PaneKind::Timeline(tl) => draw_timeline(f, layout[0], tl, theme),
            PaneKind::UndoTree(ut) => draw_undo_tree(f, layout[0], ut, theme),
            PaneKind::SetupWizard(sw) => draw_setup_wizard(f, layout[0], sw, theme),
        }

        // Inactive pane chrome: just the title
        draw_inactive_pane_bar(f, layout[1], pane, theme);
    }
}

fn draw_pane_title(f: &mut Frame, area: Rect, pane: &PaneFrame, theme: &TuiTheme) {
    let title_text = if pane.dirty {
        format!(" {} [+]", pane.title)
    } else {
        format!(" {}", pane.title)
    };
    let max_len = area.width as usize;
    let truncated = if title_text.len() > max_len {
        format!("{}…", &title_text[..max_len.saturating_sub(1)])
    } else {
        title_text
    };
    let line = Line::from(Span::styled(truncated, theme.border_style()));
    f.render_widget(Paragraph::new(line), area);
}

fn draw_editor_content(f: &mut Frame, area: Rect, pane: &PaneFrame, theme: &TuiTheme) {
    let height = area.height as usize;
    let line_number_width = 4u16; // e.g. " 42 "
    let content_width = area.width.saturating_sub(line_number_width);

    for row in 0..height {
        if row >= pane.visible_lines.len() {
            // Beyond EOF — show ~ in the gutter (where line numbers go)
            let tilde = Span::styled("  ~ ", theme.faded_style());
            f.render_widget(
                Paragraph::new(Line::from(tilde)),
                Rect::new(area.x, area.y + row as u16, line_number_width, 1),
            );
            continue;
        }

        let rendered_line = &pane.visible_lines[row];

        // Line number (right-aligned, faded)
        let lnum = format!("{:>3} ", rendered_line.line_number + 1);
        let lnum_span = Span::styled(lnum, theme.faded_style());
        f.render_widget(
            Paragraph::new(Line::from(lnum_span)),
            Rect::new(area.x, area.y + row as u16, line_number_width, 1),
        );

        // Is this the cursor line? Apply current-line highlight as base
        let is_cursor_line = pane.is_active
            && rendered_line.line_number == pane.cursor.line;
        let base_style = if is_cursor_line {
            theme.current_line_style()
        } else {
            RStyle::default()
        };

        let text = &rendered_line.text;
        let mut line_spans: Vec<Span> = Vec::new();
        let mut last_end = 0usize;

        if rendered_line.spans.is_empty() {
            // No styled spans — render the whole line in base style
            line_spans.push(Span::styled(text.trim_end_matches('\n'), base_style));
        } else {
            for span_info in &rendered_line.spans {
                let start = span_info.range.start.min(text.len());
                let end = span_info.range.end.min(text.len());
                // Fill gap before this span with base style
                if last_end < start {
                    let gap = &text[last_end..start];
                    line_spans.push(Span::styled(gap.to_string(), base_style));
                }
                let slice = &text[start..end];
                let style = theme.style_for(&span_info.style).patch(base_style);
                line_spans.push(Span::styled(slice.to_string(), style));
                last_end = end;
            }
            // Trailing text after last span
            if last_end < text.len() {
                let tail = text[last_end..].trim_end_matches('\n');
                if !tail.is_empty() {
                    line_spans.push(Span::styled(tail.to_string(), base_style));
                }
            }
        }

        let content_line = Line::from(line_spans);
        f.render_widget(
            Paragraph::new(content_line),
            Rect::new(
                area.x + line_number_width,
                area.y + row as u16,
                content_width,
                1,
            ),
        );
    }
}

// ---------------------------------------------------------------------------
// Status bar (slot-based, global)
// ---------------------------------------------------------------------------

/// Renders the global status bar. Returns cursor position if the active slot
/// needs the cursor (command line, quick capture).
fn draw_status_bar_slot(
    f: &mut Frame,
    area: Rect,
    sb: &StatusBarFrame,
    theme: &TuiTheme,
) -> Option<(u16, u16)> {
    match &sb.content {
        StatusBarContent::Normal(status) => {
            draw_normal_status(f, area, &sb.mode, status, theme);
            None
        }
        StatusBarContent::CommandLine(cmd) => {
            let style = RStyle::default().fg(theme.foreground()).bg(theme.background());
            let text = format!(":{}", cmd.input);
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(&text, style))),
                area,
            );

            // Error display: overwrite the last pane line above status bar
            if let Some(err) = &cmd.error {
                let err_y = area.y.saturating_sub(1);
                let err_style = RStyle::default().fg(theme.critical()).bg(theme.background());
                f.render_widget(
                    Paragraph::new(Line::from(Span::styled(err, err_style))),
                    Rect::new(area.x, err_y, area.width, 1),
                );
            }

            let cx = area.x + 1 + cmd.cursor_pos as u16;
            Some((cx, area.y))
        }
        StatusBarContent::QuickCapture(qc) => {
            let style = RStyle::default()
                .fg(theme.foreground())
                .bg(theme.modeline());
            let text = format!("{}{}", qc.prompt, qc.input);
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(&text, style))),
                area,
            );

            let cx = area.x + qc.prompt.len() as u16 + qc.cursor_pos as u16;
            Some((cx, area.y))
        }
    }
}

/// Render the normal status bar (mode, title, position, etc.)
fn draw_normal_status(
    f: &mut Frame,
    area: Rect,
    mode: &str,
    status: &bloom_core::render::NormalStatus,
    theme: &TuiTheme,
) {
    let style = theme.status_bar_style(mode, true);
    let width = area.width as usize;

    // Fill background
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            " ".repeat(width),
            style,
        ))),
        area,
    );

    // Right side: [macro] [pending] [mcp] line:col
    let mut right_parts: Vec<String> = Vec::new();

    if let Some(reg) = status.recording_macro {
        right_parts.push(format!("@{reg}"));
    }
    if !status.pending_keys.is_empty() {
        right_parts.push(status.pending_keys.clone());
    }

    let mcp_str = match &status.mcp {
        McpIndicator::Off => String::new(),
        McpIndicator::Idle => "\u{26a1}".to_string(),
        McpIndicator::Editing { tick } => {
            const FRAMES: &[&str] = &["\u{26a1}", "\u{25d0}", "\u{25d1}", "\u{25d2}", "\u{25d3}"];
            FRAMES[(tick.clone() as usize) % FRAMES.len()].to_string()
        }
    };
    if !mcp_str.is_empty() {
        right_parts.push(mcp_str);
    }

    right_parts.push(format!("{}:{}", status.line + 1, status.column + 1));

    let right = format!(" {} ", right_parts.join("  "));

    // Left side: MODE │ title [+]
    let dirty_mark = if status.dirty { " [+]" } else { "" };
    let mode_section = format!(" {} \u{2502} ", mode);
    let title_max = width
        .saturating_sub(mode_section.len())
        .saturating_sub(dirty_mark.len())
        .saturating_sub(right.len());
    let title = truncate_with_ellipsis(&status.title, title_max);
    let left = format!("{mode_section}{title}{dirty_mark}");

    // Render left
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(&left, style))),
        area,
    );

    // Render right
    let rx = area.right().saturating_sub(right.len() as u16);
    if rx > area.x + left.len() as u16 {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(&right, style))),
            Rect::new(rx, area.y, right.len() as u16, 1),
        );
    }
}

/// Lightweight bar for inactive panes — just the title.
fn draw_inactive_pane_bar(f: &mut Frame, area: Rect, pane: &PaneFrame, theme: &TuiTheme) {
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

fn truncate_with_ellipsis(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else if max <= 1 {
        String::new()
    } else {
        format!("{}…", &s[..max - 1])
    }
}

// ---------------------------------------------------------------------------
// Picker overlay
// ---------------------------------------------------------------------------

fn draw_picker(f: &mut Frame, area: Rect, picker: &PickerFrame, theme: &TuiTheme) {
    // Center the picker: 60% width, 70% height
    let w = (area.width * 3 / 5).max(30).min(area.width);
    let h = (area.height * 7 / 10).max(10).min(area.height);
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

    // Split inner into results zone and optional preview zone.
    // Layout: query (1) | results | footer (1) | separator (1) | preview
    let has_preview = picker.preview.is_some() && inner.height >= 8;
    let preview_h = if has_preview {
        // Give roughly 1/3 of inner height to preview, minimum 3 lines
        (inner.height / 3).max(3)
    } else {
        0
    };
    // 1 line for the separator between results and preview
    let separator_h = if has_preview { 1 } else { 0 };
    let top_h = inner.height.saturating_sub(preview_h).saturating_sub(separator_h);

    // Query line
    let query_line = Line::from(vec![
        Span::styled("> ", theme.picker_style()),
        Span::styled(&picker.query, theme.picker_style().add_modifier(Modifier::BOLD)),
    ]);
    f.render_widget(Paragraph::new(query_line), Rect::new(inner.x, inner.y, inner.width, 1));

    // Results (between query and footer)
    let results_h = top_h.saturating_sub(2); // -1 query, -1 footer
    let results_area = Rect::new(inner.x, inner.y + 1, inner.width, results_h);
    for (i, row) in picker.results.iter().enumerate() {
        if i as u16 >= results_area.height {
            break;
        }
        let is_selected = i == picker.selected_index;
        let style = if is_selected {
            theme.picker_selected()
        } else {
            theme.picker_style()
        };
        let marker = if is_selected { "▸ " } else { "  " };

        // Build right-aligned text from right column
        let right_text = row.right.as_deref().unwrap_or("");
        // Build middle text
        let middle_text = row.middle.as_deref().unwrap_or("");

        let available = results_area.width as usize;
        let right_pad = 2;
        let marker_w = marker.width();
        let right_w = right_text.width();
        let middle_w = if !middle_text.is_empty() { middle_text.width() + 2 } else { 0 };
        let fixed_w = marker_w + right_w + middle_w + right_pad;
        let label_max = available.saturating_sub(fixed_w + 1);
        let label = truncate_to_width(&row.label, label_max);

        // Compose the line: marker + label + gap + middle + gap + right + right_pad
        let label_w = label.width();
        let used = marker_w + label_w + middle_w + right_w + right_pad;
        let pad = available.saturating_sub(used);

        let text = if !middle_text.is_empty() {
            format!("{marker}{label}  {middle_text}{}{right_text}  ", " ".repeat(pad))
        } else {
            format!("{marker}{label}{}{right_text}  ", " ".repeat(pad))
        };
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(text, style))),
            Rect::new(results_area.x, results_area.y + i as u16, results_area.width, 1),
        );
    }

    // Footer: count
    let footer = format!(
        "  {} of {} {}",
        picker.filtered_count, picker.total_count, picker.status_noun
    );
    let footer_y = inner.y + top_h.saturating_sub(1);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(footer, theme.faded_style()))),
        Rect::new(inner.x, footer_y, inner.width, 1),
    );

    // Preview pane
    if let (true, Some(preview_text)) = (has_preview, &picker.preview) {
        let sep_y = inner.y + top_h;
        // Horizontal separator
        let sep_line = "─".repeat(inner.width as usize);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(sep_line, theme.border_style()))),
            Rect::new(inner.x, sep_y, inner.width, 1),
        );

        // Preview content with padding
        let preview_area = Rect::new(
            inner.x + 2,
            sep_y + 1,
            inner.width.saturating_sub(4),
            preview_h.saturating_sub(1),
        );
        let preview_style = theme.faded_style();
        for (i, line) in preview_text.lines().enumerate() {
            if i as u16 >= preview_area.height {
                break;
            }
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(line, preview_style))),
                Rect::new(preview_area.x, preview_area.y + i as u16, preview_area.width, 1),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Which-key popup
// ---------------------------------------------------------------------------

fn draw_which_key(f: &mut Frame, area: Rect, wk: &WhichKeyFrame, theme: &TuiTheme) {
    f.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::TOP)
        .style(theme.which_key_style())
        .border_style(theme.border_style());
    let inner = block.inner(area);
    f.render_widget(block, area);

    // Add horizontal and vertical padding for readability
    let padded = Rect::new(
        inner.x.saturating_add(2),
        inner.y.saturating_add(1),
        inner.width.saturating_sub(4),
        inner.height.saturating_sub(1),
    );

    let col_width = 24u16;
    let cols = (padded.width / col_width).max(1);

    for (i, entry) in wk.entries.iter().enumerate() {
        let col = (i as u16) % cols;
        let row = (i as u16) / cols;
        if row >= padded.height {
            break;
        }
        let key_style = theme.which_key_style().add_modifier(Modifier::BOLD);
        let label_style = if entry.is_group {
            RStyle::default().fg(theme.salient())
        } else {
            theme.which_key_style()
        };
        let text = Line::from(vec![
            Span::styled(format!("{:<4}", entry.key), key_style),
            Span::styled(&entry.label, label_style),
        ]);
        f.render_widget(
            Paragraph::new(text),
            Rect::new(
                padded.x + col * col_width,
                padded.y + row,
                col_width.min(padded.width.saturating_sub(col * col_width)),
                1,
            ),
        );
    }
}

// ---------------------------------------------------------------------------
// Dialog
// ---------------------------------------------------------------------------

fn draw_dialog(f: &mut Frame, area: Rect, dialog: &DialogFrame, theme: &TuiTheme) {
    let w = (area.width / 2).max(30).min(area.width);
    let h = 5u16.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    let dialog_area = Rect::new(x, y, w, h);

    f.render_widget(Clear, dialog_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .style(theme.picker_style())
        .border_style(theme.border_style());
    let inner = block.inner(dialog_area);
    f.render_widget(block, dialog_area);

    // Message
    if inner.height > 0 {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                &dialog.message,
                theme.picker_style(),
            ))),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );
    }

    // Choices
    if inner.height > 1 {
        let mut choice_spans = Vec::new();
        for (i, choice) in dialog.choices.iter().enumerate() {
            let style = if i == dialog.selected {
                theme.picker_selected().add_modifier(Modifier::BOLD)
            } else {
                theme.picker_style()
            };
            choice_spans.push(Span::styled(format!(" [{}] ", choice), style));
        }
        f.render_widget(
            Paragraph::new(Line::from(choice_spans)),
            Rect::new(inner.x, inner.y + 1, inner.width, 1),
        );
    }
}

// ---------------------------------------------------------------------------
// Notification
// ---------------------------------------------------------------------------

fn draw_notification(
    f: &mut Frame,
    area: Rect,
    message: &str,
    level: &NotificationLevel,
    theme: &TuiTheme,
) {
    let w = (message.len() as u16 + 4).min(area.width);
    let x = area.right().saturating_sub(w + 1);
    let y = area.y;
    let notif_area = Rect::new(x, y, w, 1);

    let style = theme.notification_style(level);
    let text = format!(" {} ", message);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(text, style))),
        notif_area,
    );
}

// ---------------------------------------------------------------------------
// Special pane types
// ---------------------------------------------------------------------------

fn draw_agenda(
    f: &mut Frame,
    area: Rect,
    agenda: &bloom_core::render::AgendaFrame,
    theme: &TuiTheme,
) {
    let mut y = area.y;
    let sections = [
        ("Overdue", &agenda.overdue),
        ("Today", &agenda.today),
        ("Upcoming", &agenda.upcoming),
    ];

    for (label, items) in &sections {
        if y >= area.bottom() {
            break;
        }
        // Section header
        let header_style = RStyle::default()
            .fg(theme.salient())
            .add_modifier(Modifier::BOLD);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("  {label}"),
                header_style,
            ))),
            Rect::new(area.x, y, area.width, 1),
        );
        y += 1;

        for (i, item) in items.iter().enumerate() {
            if y >= area.bottom() {
                break;
            }
            let is_selected = false; // TODO: track global index
            let style = if is_selected {
                theme.picker_selected()
            } else {
                RStyle::default().fg(theme.foreground())
            };
            let _ = i;
            let text = format!("  ☐ {}  ({})", item.task_text, item.source_page);
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(text, style))),
                Rect::new(area.x, y, area.width, 1),
            );
            y += 1;
        }
    }

    // Footer
    if y < area.bottom() {
        let footer = format!(
            "  {} open tasks across {} pages",
            agenda.total_open, agenda.total_pages
        );
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(footer, theme.faded_style()))),
            Rect::new(area.x, y, area.width, 1),
        );
    }
}

fn draw_timeline(
    f: &mut Frame,
    area: Rect,
    tl: &bloom_core::render::TimelineFrame,
    theme: &TuiTheme,
) {
    // Title
    if area.height == 0 {
        return;
    }
    let title_style = RStyle::default()
        .fg(theme.salient())
        .add_modifier(Modifier::BOLD);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("  Timeline: {}", tl.target_title),
            title_style,
        ))),
        Rect::new(area.x, area.y, area.width, 1),
    );

    let mut y = area.y + 1;
    for (i, entry) in tl.entries.iter().enumerate() {
        if y >= area.bottom() {
            break;
        }
        let is_selected = i == tl.selected_index;
        let style = if is_selected {
            theme.picker_selected()
        } else {
            RStyle::default().fg(theme.foreground())
        };
        let date_str = entry.date.format("%b %d").to_string();
        let header = format!("  {} · {}", date_str, entry.source_title);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(header, style))),
            Rect::new(area.x, y, area.width, 1),
        );
        y += 1;

        if entry.expanded && y < area.bottom() {
            let ctx_style = theme.faded_style();
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    format!("    {}", entry.context),
                    ctx_style,
                ))),
                Rect::new(area.x, y, area.width, 1),
            );
            y += 1;
        }
    }
}

fn draw_undo_tree(
    f: &mut Frame,
    area: Rect,
    ut: &bloom_core::render::UndoTreeFrame,
    theme: &TuiTheme,
) {
    let mut y = area.y;
    for node in &ut.nodes {
        if y >= area.bottom() {
            break;
        }
        let is_selected = node.id == ut.selected;
        let style = if is_selected {
            theme.picker_selected().add_modifier(Modifier::BOLD)
        } else if node.is_current {
            RStyle::default().fg(theme.salient())
        } else {
            RStyle::default().fg(theme.foreground())
        };
        let indent = "  ".repeat(node.depth);
        let marker = if node.is_current { "●" } else { "○" };
        let text = format!("  {indent}{marker} {}", node.description);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(text, style))),
            Rect::new(area.x, y, area.width, 1),
        );
        y += 1;
    }

    // Preview at bottom if available
    if let Some(preview) = &ut.preview {
        if y + 1 < area.bottom() {
            y += 1;
            let style = theme.faded_style();
            for line in preview.lines() {
                if y >= area.bottom() {
                    break;
                }
                f.render_widget(
                    Paragraph::new(Line::from(Span::styled(
                        format!("  {line}"),
                        style,
                    ))),
                    Rect::new(area.x, y, area.width, 1),
                );
                y += 1;
            }
        }
    }
}

fn draw_setup_wizard(
    f: &mut Frame,
    area: Rect,
    sw: &bloom_core::render::SetupWizardFrame,
    theme: &TuiTheme,
) {
    use bloom_core::render::{ImportChoice, SetupStep};

    // Fill background
    f.render_widget(
        Block::default().style(RStyle::default().bg(theme.background())),
        area,
    );

    // Border around full screen
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border_style())
        .style(RStyle::default().bg(theme.background()));
    let inner = block.inner(area);
    f.render_widget(block, area);

    // Center content vertically (use top third as offset)
    let y_start = inner.y + inner.height / 5;
    let heading_style = RStyle::default()
        .fg(theme.strong())
        .add_modifier(Modifier::BOLD);
    let text_style = RStyle::default().fg(theme.foreground());
    let faded = theme.faded_style();
    let error_style = RStyle::default().fg(theme.critical());

    let cx = inner.x + 9; // indent for content

    match sw.step {
        SetupStep::Welcome => {
            let title_y = y_start + 2;
            render_line(f, cx, title_y, inner.width, "Bloom 🌱", heading_style);
            render_line(f, cx, title_y + 2, inner.width,
                "A local-first, keyboard-driven note-taking app.", text_style);
            render_line(f, cx, title_y + 4, inner.width,
                "Your notes are stored as Markdown files in a", text_style);
            render_line(f, cx, title_y + 5, inner.width,
                "single folder called a vault. No cloud, no sync \u{2014}", text_style);
            render_line(f, cx, title_y + 6, inner.width,
                "everything stays on your machine.", text_style);

            // Bottom prompt
            let prompt_y = inner.bottom().saturating_sub(2);
            let prompt = "Press Enter to get started";
            let px = inner.right().saturating_sub(prompt.len() as u16 + 2);
            render_line(f, px, prompt_y, inner.width, prompt, faded);
        }

        SetupStep::ChooseVaultLocation => {
            let y = y_start;
            render_line(f, cx, y, inner.width, "Choose vault location", heading_style);
            render_line(f, cx, y + 2, inner.width,
                "This is where your notes, journal, and config", text_style);
            render_line(f, cx, y + 3, inner.width,
                "will live. You can move it later.", text_style);

            // Path input
            let input_y = y + 5;
            let label = "Path: ";
            render_line(f, cx, input_y, inner.width, label, text_style);
            let input_style = RStyle::default().fg(theme.foreground()).bg(theme.modeline());
            let input_w = inner.width.saturating_sub(cx - inner.x + label.len() as u16 + 2);
            let input_x = cx + label.len() as u16;
            let padded: String = format!("{:<width$}", sw.vault_path, width = input_w as usize);
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(padded, input_style))),
                Rect::new(input_x, input_y, input_w, 1),
            );
            // Cursor in path input
            let cursor_x = input_x + sw.vault_path_cursor as u16;
            if cursor_x < inner.right() {
                f.set_cursor_position((cursor_x, input_y));
            }

            // Directory preview
            let prev_y = y + 7;
            render_line(f, cx, prev_y, inner.width, "Bloom will create:", faded);
            render_line(f, cx + 2, prev_y + 1, inner.width, "pages/       \u{2014} topic pages", faded);
            render_line(f, cx + 2, prev_y + 2, inner.width, "journal/     \u{2014} daily journal", faded);
            render_line(f, cx + 2, prev_y + 3, inner.width, "templates/   \u{2014} page templates", faded);
            render_line(f, cx + 2, prev_y + 4, inner.width, "images/      \u{2014} attachments", faded);

            // Error
            if let Some(err) = &sw.error {
                render_line(f, cx, input_y + 2, inner.width, &format!("\u{2717} {err}"), error_style);
            }

            // Nav hints
            let prompt_y = inner.bottom().saturating_sub(2);
            render_line(f, cx, prompt_y, inner.width, "Esc back", faded);
            let confirm = "Enter to confirm";
            let rx = inner.right().saturating_sub(confirm.len() as u16 + 2);
            render_line(f, rx, prompt_y, inner.width, confirm, faded);
        }

        SetupStep::ImportChoice => {
            let y = y_start;
            render_line(f, cx, y, inner.width, "Import from Logseq?", heading_style);
            render_line(f, cx, y + 2, inner.width,
                "If you have an existing Logseq vault, Bloom", text_style);
            render_line(f, cx, y + 3, inner.width,
                "can import your pages, journals, and links.", text_style);
            render_line(f, cx, y + 4, inner.width,
                "Your Logseq files will not be modified.", text_style);

            let opt_y = y + 6;
            let (no_style, yes_style) = if sw.import_choice == ImportChoice::No {
                (RStyle::default().fg(theme.foreground()).bg(theme.mild()), text_style)
            } else {
                (text_style, RStyle::default().fg(theme.foreground()).bg(theme.mild()))
            };
            let no_marker = if sw.import_choice == ImportChoice::No { "\u{25b8} " } else { "  " };
            let yes_marker = if sw.import_choice == ImportChoice::Yes { "\u{25b8} " } else { "  " };
            render_line(f, cx, opt_y, inner.width,
                &format!("{no_marker}No, start fresh"), no_style);
            render_line(f, cx, opt_y + 1, inner.width,
                &format!("{yes_marker}Yes, import from Logseq"), yes_style);

            // Nav hints
            let prompt_y = inner.bottom().saturating_sub(2);
            render_line(f, cx, prompt_y, inner.width, "Esc back", faded);
            let nav = "\u{2191}\u{2193} select         Enter to confirm";
            let rx = inner.right().saturating_sub(nav.len() as u16 + 2);
            render_line(f, rx, prompt_y, inner.width, nav, faded);
        }

        SetupStep::ImportPath => {
            let y = y_start;
            render_line(f, cx, y, inner.width, "Import from Logseq", heading_style);
            render_line(f, cx, y + 2, inner.width,
                "Enter the path to your Logseq vault:", text_style);

            // Path input
            let input_y = y + 4;
            let label = "Path: ";
            render_line(f, cx, input_y, inner.width, label, text_style);
            let input_style = RStyle::default().fg(theme.foreground()).bg(theme.modeline());
            let input_w = inner.width.saturating_sub(cx - inner.x + label.len() as u16 + 2);
            let input_x = cx + label.len() as u16;
            let padded: String = format!("{:<width$}", sw.logseq_path, width = input_w as usize);
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(padded, input_style))),
                Rect::new(input_x, input_y, input_w, 1),
            );
            let cursor_x = input_x + sw.logseq_path_cursor as u16;
            if cursor_x < inner.right() {
                f.set_cursor_position((cursor_x, input_y));
            }

            // Error
            if let Some(err) = &sw.error {
                render_line(f, cx, input_y + 2, inner.width, &format!("\u{2717} {err}"), error_style);
            }

            // Nav hints
            let prompt_y = inner.bottom().saturating_sub(2);
            render_line(f, cx, prompt_y, inner.width, "Esc back", faded);
            let confirm = "Enter to start import";
            let rx = inner.right().saturating_sub(confirm.len() as u16 + 2);
            render_line(f, rx, prompt_y, inner.width, confirm, faded);
        }

        SetupStep::ImportRunning => {
            let y = y_start;
            render_line(f, cx, y, inner.width, "Importing from Logseq...", heading_style);
            if let Some(prog) = &sw.import_progress {
                // Progress bar
                let bar_y = y + 2;
                let bar_w = (inner.width / 2).max(20) as usize;
                let filled = if prog.total > 0 {
                    (prog.done * bar_w) / prog.total
                } else {
                    0
                };
                let bar: String = format!(
                    "{}{} {}/{}",
                    "\u{2588}".repeat(filled),
                    "\u{2591}".repeat(bar_w - filled),
                    prog.done,
                    prog.total,
                );
                render_line(f, cx, bar_y, inner.width, &bar, text_style);

                // Stats
                let mut sy = bar_y + 2;
                let green = RStyle::default().fg(theme.accent_green());
                let yellow = RStyle::default().fg(theme.accent_yellow());
                let red = RStyle::default().fg(theme.critical());
                render_line(f, cx, sy, inner.width,
                    &format!("\u{2713} {} pages imported", prog.pages_imported), green);
                sy += 1;
                render_line(f, cx, sy, inner.width,
                    &format!("\u{2713} {} journals imported", prog.journals_imported), green);
                sy += 1;
                render_line(f, cx, sy, inner.width,
                    &format!("\u{2713} {} links resolved", prog.links_resolved), green);
                sy += 1;
                if !prog.warnings.is_empty() {
                    render_line(f, cx, sy, inner.width,
                        &format!("\u{26a0} {} warnings", prog.warnings.len()), yellow);
                    sy += 1;
                }
                if !prog.errors.is_empty() {
                    render_line(f, cx, sy, inner.width,
                        &format!("\u{2717} {} errors", prog.errors.len()), red);
                }

                if prog.finished {
                    let prompt_y = inner.bottom().saturating_sub(2);
                    let confirm = "Press Enter to continue";
                    let rx = inner.right().saturating_sub(confirm.len() as u16 + 2);
                    render_line(f, rx, prompt_y, inner.width, confirm, faded);
                }
            }
        }

        SetupStep::Complete => {
            let y = y_start + 2;
            render_line(f, cx, y, inner.width, "Your vault is ready \u{1f331}", heading_style);

            let sy = y + 2;
            render_line(f, cx, sy, inner.width,
                &format!("Location:  {}", sw.vault_path), text_style);
            render_line(f, cx, sy + 1, inner.width,
                &format!("Pages:     {}", sw.stats.pages), text_style);
            render_line(f, cx, sy + 2, inner.width,
                &format!("Journal:   {} entries", sw.stats.journals), text_style);

            let ty = sy + 4;
            render_line(f, cx, ty, inner.width, "Tips:", text_style);
            let key_style = RStyle::default().fg(theme.salient());
            let desc_style = text_style;
            let tips = [
                ("SPC j j", "open today's journal"),
                ("SPC f f", "find a page"),
                ("SPC n  ", "create a new page"),
                ("SPC ?  ", "all commands"),
            ];
            for (i, (key, desc)) in tips.iter().enumerate() {
                let tip_y = ty + 1 + i as u16;
                if tip_y < inner.bottom() {
                    let line = Line::from(vec![
                        Span::styled(format!("  {key}     "), key_style),
                        Span::styled(*desc, desc_style),
                    ]);
                    f.render_widget(
                        Paragraph::new(line),
                        Rect::new(cx, tip_y, inner.width.saturating_sub(cx - inner.x), 1),
                    );
                }
            }

            // Prompt
            let prompt_y = inner.bottom().saturating_sub(2);
            let confirm = "Press Enter to open your journal";
            let rx = inner.right().saturating_sub(confirm.len() as u16 + 2);
            render_line(f, rx, prompt_y, inner.width, confirm, faded);
        }
    }
}

fn render_line(f: &mut Frame, x: u16, y: u16, _max_w: u16, text: &str, style: RStyle) {
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(text, style))),
        Rect::new(x, y, text.len() as u16 + 1, 1),
    );
}

/// Truncate a string to fit within `max_width` display columns, appending `…` if truncated.
fn truncate_to_width(s: &str, max_width: usize) -> String {
    use unicode_width::UnicodeWidthChar;
    if s.width() <= max_width {
        return s.to_string();
    }
    let ellipsis_w = 1; // '…' is 1 column wide
    let target = max_width.saturating_sub(ellipsis_w);
    let mut width = 0;
    let mut end = 0;
    for (i, ch) in s.char_indices() {
        let cw = ch.width().unwrap_or(0);
        if width + cw > target {
            break;
        }
        width += cw;
        end = i + ch.len_utf8();
    }
    format!("{}…", &s[..end])
}
