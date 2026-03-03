use bloom_core::render::{
    CommandLineFrame, DialogFrame, NotificationLevel, PaneFrame, PaneKind,
    PickerFrame, QuickCaptureFrame, RenderFrame, StatusBar, WhichKeyFrame,
};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style as RStyle};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::theme::ThemePalette;

/// Render the full RenderFrame to the terminal.
pub fn draw(f: &mut Frame, frame: &RenderFrame, theme: &ThemePalette) {
    let area = f.area();

    // Fill background
    f.render_widget(
        Block::default().style(RStyle::default().bg(theme.background)),
        area,
    );

    // Layout: panes take the full area, overlays drawn on top
    draw_panes(f, area, &frame.panes, frame.maximized, frame.hidden_pane_count, theme);

    // Overlays (drawn on top of panes)
    if let Some(picker) = &frame.picker {
        draw_picker(f, area, picker, theme);
    }
    if let Some(wk) = &frame.which_key {
        draw_which_key(f, area, wk, theme);
    }
    if let Some(cmd) = &frame.command_line {
        draw_command_line(f, area, cmd, theme);
    }
    if let Some(qc) = &frame.quick_capture {
        draw_quick_capture(f, area, qc, theme);
    }
    if let Some(dialog) = &frame.dialog {
        draw_dialog(f, area, dialog, theme);
    }
    if let Some(notif) = &frame.notification {
        draw_notification(f, area, &notif.message, &notif.level, theme);
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
    theme: &ThemePalette,
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

fn draw_pane(f: &mut Frame, area: Rect, pane: &PaneFrame, theme: &ThemePalette) {
    if area.height < 2 {
        return;
    }

    // Split into: title bar (1 line) + content + status bar (1 line)
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),                    // title
            Constraint::Min(1),                      // content
            Constraint::Length(1),                    // status bar
        ])
        .split(area);

    draw_pane_title(f, layout[0], pane, theme);

    match &pane.kind {
        PaneKind::Editor => draw_editor_content(f, layout[1], pane, theme),
        PaneKind::Agenda(agenda) => draw_agenda(f, layout[1], agenda, theme),
        PaneKind::Timeline(tl) => draw_timeline(f, layout[1], tl, theme),
        PaneKind::UndoTree(ut) => draw_undo_tree(f, layout[1], ut, theme),
        PaneKind::SetupWizard(sw) => draw_setup_wizard(f, layout[1], sw, theme),
    }

    draw_status_bar(f, layout[2], &pane.status_bar, pane.is_active, theme);

    // Set cursor position for active pane
    if pane.is_active {
        let content_area = layout[1];
        let line_number_width = 4u16;
        let cursor_y = pane.cursor.line.saturating_sub(pane.scroll_offset);
        let cy = content_area.y + cursor_y as u16;
        let cx = content_area.x + line_number_width + pane.cursor.column as u16;
        if cy < content_area.bottom() && cx < content_area.right() {
            f.set_cursor_position((cx, cy));
        }
    }
}

fn draw_pane_title(f: &mut Frame, area: Rect, pane: &PaneFrame, theme: &ThemePalette) {
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

fn draw_editor_content(f: &mut Frame, area: Rect, pane: &PaneFrame, theme: &ThemePalette) {
    let height = area.height as usize;
    let line_number_width = 4u16; // e.g. " 42 "
    let content_width = area.width.saturating_sub(line_number_width);

    for row in 0..height {
        if row >= pane.visible_lines.len() {
            // Tilde lines beyond EOF
            let tilde = Span::styled("~", theme.faded_style());
            f.render_widget(
                Paragraph::new(Line::from(tilde)),
                Rect::new(area.x, area.y + row as u16, area.width, 1),
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
// Status bar
// ---------------------------------------------------------------------------

fn draw_status_bar(
    f: &mut Frame,
    area: Rect,
    status: &StatusBar,
    is_active: bool,
    theme: &ThemePalette,
) {
    let style = if is_active {
        theme.status_bar_style(&status.mode)
    } else {
        theme.status_bar_inactive()
    };

    // Fill bar
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            " ".repeat(area.width as usize),
            style,
        ))),
        area,
    );

    if is_active {
        // Full status: MODE │ filename [+]  pending   line:col │ filetype
        let dirty_mark = if status.dirty { " [+]" } else { "" };
        let recording = status
            .recording_macro
            .map(|c| format!(" @{c}"))
            .unwrap_or_default();
        let pending = if status.pending_keys.is_empty() {
            String::new()
        } else {
            format!(" {}", status.pending_keys)
        };
        let left = format!(
            " {} │ {}{}{}{}",
            status.mode, status.filename, dirty_mark, recording, pending
        );
        let right = format!("{}:{} ", status.line + 1, status.column + 1);

        let left_span = Span::styled(&left, style);
        let right_span = Span::styled(&right, style);

        f.render_widget(
            Paragraph::new(Line::from(left_span)),
            area,
        );

        let rx = area.right().saturating_sub(right.len() as u16);
        if rx > area.x {
            f.render_widget(
                Paragraph::new(Line::from(right_span)),
                Rect::new(rx, area.y, right.len() as u16, 1),
            );
        }
    } else {
        // Compact: just filename
        let text = format!(" {}", status.filename);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(&text, style))),
            area,
        );
    }
}

// ---------------------------------------------------------------------------
// Picker overlay
// ---------------------------------------------------------------------------

fn draw_picker(f: &mut Frame, area: Rect, picker: &PickerFrame, theme: &ThemePalette) {
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

    // Query line
    let query_line = Line::from(vec![
        Span::styled("> ", theme.picker_style()),
        Span::styled(&picker.query, theme.picker_style().add_modifier(Modifier::BOLD)),
    ]);
    f.render_widget(Paragraph::new(query_line), Rect::new(inner.x, inner.y, inner.width, 1));

    // Results
    let results_area = Rect::new(inner.x, inner.y + 1, inner.width, inner.height.saturating_sub(2));
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
        let marginalia = row.marginalia.join("  ");
        let available = results_area.width as usize;
        let label_max = available.saturating_sub(marginalia.len() + marker.len() + 1);
        let label = if row.label.len() > label_max {
            format!("{}…", &row.label[..label_max.saturating_sub(1)])
        } else {
            row.label.clone()
        };
        let pad = available.saturating_sub(marker.len() + label.len() + marginalia.len());
        let text = format!("{marker}{label}{}{marginalia}", " ".repeat(pad));
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(text, style))),
            Rect::new(results_area.x, results_area.y + i as u16, results_area.width, 1),
        );
    }

    // Footer: count
    let footer = format!(
        "  {} of {} results",
        picker.filtered_count, picker.total_count
    );
    let footer_y = inner.y + inner.height.saturating_sub(1);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(footer, theme.faded_style()))),
        Rect::new(inner.x, footer_y, inner.width, 1),
    );
}

// ---------------------------------------------------------------------------
// Which-key popup
// ---------------------------------------------------------------------------

fn draw_which_key(f: &mut Frame, area: Rect, wk: &WhichKeyFrame, theme: &ThemePalette) {
    // Bottom-centered popup
    let max_entries = wk.entries.len() as u16;
    let h = (max_entries + 2).min(area.height / 3).max(3);
    let w = (area.width * 2 / 3).max(20).min(area.width);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.bottom().saturating_sub(h + 1);
    let popup_area = Rect::new(x, y, w, h);

    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {} ", wk.prefix))
        .style(theme.which_key_style())
        .border_style(theme.border_style());
    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    // Entries in columns
    let col_width = 24u16;
    let cols = (inner.width / col_width).max(1);

    for (i, entry) in wk.entries.iter().enumerate() {
        let col = (i as u16) % cols;
        let row = (i as u16) / cols;
        if row >= inner.height {
            break;
        }
        let key_style = if entry.is_group {
            theme.which_key_style().add_modifier(Modifier::BOLD)
        } else {
            theme.which_key_style()
        };
        let label_style = if entry.is_group {
            RStyle::default().fg(theme.salient)
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
                inner.x + col * col_width,
                inner.y + row,
                col_width.min(inner.width - col * col_width),
                1,
            ),
        );
    }
}

// ---------------------------------------------------------------------------
// Command line
// ---------------------------------------------------------------------------

fn draw_command_line(f: &mut Frame, area: Rect, cmd: &CommandLineFrame, theme: &ThemePalette) {
    let y = area.bottom().saturating_sub(1);
    let cmd_area = Rect::new(area.x, y, area.width, 1);

    let style = RStyle::default().fg(theme.foreground).bg(theme.background);
    let text = format!(":{}", cmd.input);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(&text, style))),
        cmd_area,
    );

    // Cursor in command line
    let cx = area.x + 1 + cmd.cursor_pos as u16;
    f.set_cursor_position((cx, y));

    // Error display
    if let Some(err) = &cmd.error {
        let err_y = y.saturating_sub(1);
        let err_style = RStyle::default().fg(theme.critical).bg(theme.background);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(err, err_style))),
            Rect::new(area.x, err_y, area.width, 1),
        );
    }
}

// ---------------------------------------------------------------------------
// Quick capture
// ---------------------------------------------------------------------------

fn draw_quick_capture(
    f: &mut Frame,
    area: Rect,
    qc: &QuickCaptureFrame,
    theme: &ThemePalette,
) {
    let y = area.bottom().saturating_sub(1);
    let qc_area = Rect::new(area.x, y, area.width, 1);

    let style = RStyle::default()
        .fg(theme.foreground)
        .bg(theme.modeline);
    let text = format!("{}{}", qc.prompt, qc.input);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(&text, style))),
        qc_area,
    );

    let cx = area.x + qc.prompt.len() as u16 + qc.cursor_pos as u16;
    f.set_cursor_position((cx, y));
}

// ---------------------------------------------------------------------------
// Dialog
// ---------------------------------------------------------------------------

fn draw_dialog(f: &mut Frame, area: Rect, dialog: &DialogFrame, theme: &ThemePalette) {
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
    theme: &ThemePalette,
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
    theme: &ThemePalette,
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
            .fg(theme.salient)
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
                RStyle::default().fg(theme.foreground)
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
    theme: &ThemePalette,
) {
    // Title
    if area.height == 0 {
        return;
    }
    let title_style = RStyle::default()
        .fg(theme.salient)
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
            RStyle::default().fg(theme.foreground)
        };
        let date_str = entry.date.format("%b %d").to_string();
        let header = format!("  {} · {}", date_str, entry.source_title);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(header, style))),
            Rect::new(area.x, y, area.width, 1),
        );
        y += 1;

        if entry.expanded && y < area.bottom() {
            let ctx_style = RStyle::default().fg(theme.faded);
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
    theme: &ThemePalette,
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
            RStyle::default().fg(theme.salient)
        } else {
            RStyle::default().fg(theme.foreground)
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
            let style = RStyle::default().fg(theme.faded);
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
    theme: &ThemePalette,
) {
    let style = RStyle::default().fg(theme.foreground);
    let message = match sw.step {
        bloom_core::render::SetupStep::ChooseVaultLocation => {
            format!("  Choose vault location: {}", sw.vault_path)
        }
        bloom_core::render::SetupStep::ImportFromLogseq => {
            "  Import from Logseq? (y/n)".to_string()
        }
        bloom_core::render::SetupStep::Complete => "  Setup complete! Press Enter.".to_string(),
    };
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(message, style))),
        Rect::new(area.x, area.y, area.width, 1),
    );
}
