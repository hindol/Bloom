use bloom_core::render::{
    AgendaFrame, DialogFrame, InlineMenuAnchor, InlineMenuFrame, McpIndicator,
    NotificationLevel, PaneFrame, PaneKind, PickerFrame, RenderFrame,
    StatusBarContent, StatusBarFrame, WhichKeyFrame,
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

    // Layout: panes | which-key drawer (optional)
    let wk_h = if let Some(wk) = &frame.which_key {
        let col_width = 24u16;
        let cols = (area.width.saturating_sub(4) / col_width).max(1);
        let rows_needed = ((wk.entries.len() as u16) + cols - 1) / cols;
        // +1 for top padding, +1 for bottom padding
        (rows_needed + 2).min(area.height / 3).max(3)
    } else {
        0
    };

    let pane_h = area.height.saturating_sub(wk_h);
    let pane_area = Rect::new(area.x, area.y, area.width, pane_h);
    let wk_area = if wk_h > 0 {
        Some(Rect::new(area.x, area.y + pane_h, area.width, wk_h))
    } else {
        None
    };

    // Draw panes (each pane includes its own status bar)
    draw_panes(f, pane_area, &frame.panes, frame.maximized, frame.hidden_pane_count, theme);

    // Which-key drawer
    if let (Some(wk), Some(wk_rect)) = (&frame.which_key, wk_area) {
        draw_which_key(f, wk_rect, wk, theme);
    }

    // Overlays — drawn after panes, so their set_cursor_position() wins.
    if let Some(agenda) = &frame.agenda {
        draw_agenda(f, area, agenda, theme, frame.scrolloff);
    }
    if let Some(menu) = &frame.inline_menu {
        draw_inline_menu(f, area, menu, theme);
    }
    if let Some(picker) = &frame.picker {
        draw_picker(f, area, picker, theme);
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

fn draw_pane(f: &mut Frame, area: Rect, pane: &PaneFrame, theme: &TuiTheme) {
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
        PaneKind::Agenda(agenda) => draw_agenda(f, content_area, agenda, theme, 0),
        PaneKind::Timeline(tl) => draw_timeline(f, content_area, tl, theme),
        PaneKind::UndoTree(ut) => draw_undo_tree(f, content_area, ut, theme),
        PaneKind::SetupWizard(sw) => draw_setup_wizard(f, content_area, sw, theme),
    }

    // Status bar: slot-based for active pane, compact for inactive
    if pane.is_active {
        let cursor = draw_status_bar_slot(f, status_area, &pane.status_bar, theme);
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
    // Clear the entire content area to prevent stale cells on buffer switch
    f.render_widget(
        Block::default().style(RStyle::default().bg(theme.background())),
        area,
    );

    let height = area.height as usize;
    let line_number_width = 5u16; // e.g. " 42  " (3-digit number + gutter gap)
    let right_margin = 2u16;
    let content_width = area.width.saturating_sub(line_number_width + right_margin);

    for row in 0..height {
        if row >= pane.visible_lines.len() {
            // Beyond EOF — show ~ in the gutter (where line numbers go)
            let tilde = Span::styled("  ~  ", theme.faded_style());
            f.render_widget(
                Paragraph::new(Line::from(tilde)),
                Rect::new(area.x, area.y + row as u16, line_number_width, 1),
            );
            continue;
        }

        let rendered_line = &pane.visible_lines[row];

        // Line number (right-aligned, faded, with gutter gap)
        let lnum = format!("{:>3}  ", rendered_line.line_number + 1);
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

            let cx = (area.x + 1 + cmd.cursor_pos as u16).min(area.right().saturating_sub(1));
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

            let cx = (area.x + qc.prompt.width() as u16 + qc.cursor_pos as u16).min(area.right().saturating_sub(1));
            Some((cx, area.y))
        }
    }
}

/// Render the normal status bar with per-element typographic weights.
fn draw_normal_status(
    f: &mut Frame,
    area: Rect,
    mode: &str,
    status: &bloom_core::render::NormalStatus,
    theme: &TuiTheme,
) {
    let bar_bg = theme.highlight();
    let base_style = theme.status_bar_style(mode, true);
    let width = area.width as usize;

    // Fill background
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(" ".repeat(width), base_style))),
        area,
    );

    // --- Build right-side spans with individual weights ---
    let mut right_spans: Vec<Span> = Vec::new();
    right_spans.push(Span::raw(" "));

    if let Some(reg) = status.recording_macro {
        // Macro: accent_red — recording state, visually distinct
        let macro_style = RStyle::default().fg(theme.accent_red()).bg(bar_bg);
        right_spans.push(Span::styled(format!("@{reg}"), macro_style));
        right_spans.push(Span::raw("  "));
    }
    if !status.pending_keys.is_empty() {
        // Pending keys: salient + bold — transient but important
        let pending_style = RStyle::default()
            .fg(theme.salient())
            .bg(bar_bg)
            .add_modifier(Modifier::BOLD);
        right_spans.push(Span::styled(status.pending_keys.clone(), pending_style));
        right_spans.push(Span::raw("  "));
    }

    let mcp_animating = matches!(&status.mcp, McpIndicator::Editing { .. });
    let mcp_str = match &status.mcp {
        McpIndicator::Off => String::new(),
        McpIndicator::Idle => "\u{26a1}".to_string(),
        McpIndicator::Editing { tick } => {
            const FRAMES: &[&str] = &["\u{26a1}", "\u{25d0}", "\u{25d1}", "\u{25d2}", "\u{25d3}"];
            FRAMES[(*tick as usize) % FRAMES.len()].to_string()
        }
    };
    if !mcp_str.is_empty() {
        // MCP: faded when idle, salient when animating
        let mcp_fg = if mcp_animating { theme.salient() } else { theme.faded() };
        let mcp_style = RStyle::default().fg(mcp_fg).bg(bar_bg);
        right_spans.push(Span::styled(mcp_str, mcp_style));
        right_spans.push(Span::raw("  "));
    }

    // Line:col — faded, reference info
    let pos_style = RStyle::default().fg(theme.faded()).bg(bar_bg);
    right_spans.push(Span::styled(
        format!("{}:{}", status.line + 1, status.column + 1),
        pos_style,
    ));
    right_spans.push(Span::styled("  ", RStyle::default().bg(bar_bg)));

    let right_width: usize = right_spans.iter().map(|s| s.content.width()).sum();

    // --- Build left-side spans with individual weights ---
    // Mode badge: bold, uses the mode-specific style (already has bg color)
    let mode_style = base_style.add_modifier(Modifier::BOLD);
    let mode_text = format!(" {} ", mode);

    // Separator: faded on bar bg
    let sep_style = RStyle::default().fg(theme.faded()).bg(bar_bg);

    // Title: foreground on bar bg, normal weight
    let title_style = RStyle::default().fg(theme.foreground()).bg(bar_bg);

    // Dirty: salient on bar bg — needs attention
    let dirty_style = RStyle::default().fg(theme.salient()).bg(bar_bg);

    let dirty_mark = if status.dirty { " [+]" } else { "" };
    let title_max = width
        .saturating_sub(mode_text.width())
        .saturating_sub(3)  // " │ "
        .saturating_sub(dirty_mark.width())
        .saturating_sub(right_width);
    let title = truncate_with_ellipsis(&status.title, title_max);

    let mut left_spans: Vec<Span> = Vec::new();
    left_spans.push(Span::styled(&mode_text, mode_style));
    left_spans.push(Span::styled(" \u{2502} ", sep_style));
    left_spans.push(Span::styled(title, title_style));
    if status.dirty {
        left_spans.push(Span::styled(dirty_mark, dirty_style));
    }

    let left_width: usize = left_spans.iter().map(|s| s.content.width()).sum();

    // Render left
    f.render_widget(
        Paragraph::new(Line::from(left_spans)),
        area,
    );

    // Render right
    let rx = area.right().saturating_sub(right_width as u16);
    if rx > area.x + left_width as u16 {
        f.render_widget(
            Paragraph::new(Line::from(right_spans)),
            Rect::new(rx, area.y, right_width as u16, 1),
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
    truncate_to_width(s, max)
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

    // Query line (indented)
    let query_line = Line::from(vec![
        Span::styled(" > ", theme.picker_style()),
        Span::styled(&picker.query, theme.picker_style().add_modifier(Modifier::BOLD)),
    ]);
    f.render_widget(Paragraph::new(query_line), Rect::new(inner.x, inner.y, inner.width, 1));
    // Place cursor at end of query input (overrides editor cursor)
    let query_cx = inner.x + 3 + picker.query.len() as u16;
    f.set_cursor_position((query_cx, inner.y));

    // Results (between query and footer)
    let results_h = top_h.saturating_sub(2); // -1 query, -1 footer
    let results_area = Rect::new(inner.x, inner.y + 1, inner.width, results_h);

    // Show hint when query is below minimum length
    if picker.results.is_empty() && picker.min_query_len > 0 && picker.query.len() < picker.min_query_len {
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

    let available = results_area.width as usize;

    // Scroll offset: keep selected_index visible within the results viewport
    let viewport_h = results_h as usize;
    let scroll_offset = if picker.selected_index >= viewport_h {
        picker.selected_index - viewport_h + 1
    } else {
        0
    };

    // Compute fixed right-column width from the visible results
    let visible_end = (scroll_offset + viewport_h).min(picker.results.len());
    let visible_slice = &picker.results[scroll_offset..visible_end];
    let max_right_w: usize = visible_slice
        .iter()
        .map(|row| row.right.as_deref().unwrap_or("").width())
        .max()
        .unwrap_or(0)
        .min(available * 2 / 5); // cap at 40% of available width
    let max_middle_w: usize = visible_slice
        .iter()
        .map(|row| row.middle.as_deref().unwrap_or("").width())
        .max()
        .unwrap_or(0)
        .min(available / 4); // cap at 25%

    let right_zone = if max_right_w > 0 { max_right_w + 2 } else { 0 }; // +2 padding
    let middle_zone = if max_middle_w > 0 { max_middle_w + 2 } else { 0 };
    let marker_w = 3; // " ▸ " or "   "
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

        // Gap between label and middle/right columns
        let label_gap = available.saturating_sub(marker_w + label_w + middle_zone + right_zone);

        let mut spans: Vec<Span> = Vec::new();
        spans.push(Span::styled(marker, style));

        // Highlight query matches in the label
        if !picker.query.is_empty() && !is_selected {
            let match_spans = bloom_core::render::search_highlight::highlight_matches(
                &label, &picker.query,
            );
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
            let mid_padded = format!("{}{}", middle_text, " ".repeat(middle_zone.saturating_sub(middle_text.width())));
            spans.push(Span::styled(mid_padded, if is_selected { style } else { faded_style }));
        }

        if right_zone > 0 {
            let right_padded = format!("{}{}", " ".repeat(right_zone.saturating_sub(right_text.width())), right_text);
            spans.push(Span::styled(right_padded, if is_selected { style } else { faded_style }));
        }

        f.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect::new(results_area.x, results_area.y + vi as u16, results_area.width, 1),
        );
    }

    // Footer: count with selection position
    let footer = if picker.filtered_count > 0 {
        format!(
            "  {} / {} {}",
            picker.selected_index + 1, picker.filtered_count, picker.status_noun
        )
    } else {
        format!("  0 / {} {}", picker.total_count, picker.status_noun)
    };
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

        // Preview content with padding, search terms highlighted
        let preview_area = Rect::new(
            inner.x + 2,
            sep_y + 1,
            inner.width.saturating_sub(4),
            preview_h.saturating_sub(1),
        );
        let preview_style = theme.faded_style();
        let match_style = theme.style_for(&bloom_core::parser::traits::Style::SearchMatch);
        let search_spans = bloom_core::render::search_highlight::highlight_matches(
            preview_text,
            &picker.query,
        );

        // Build a set of match byte ranges for quick lookup
        let match_ranges: Vec<std::ops::Range<usize>> =
            search_spans.iter().map(|s| s.range.clone()).collect();

        let mut byte_offset = 0usize;
        for (i, line) in preview_text.lines().enumerate() {
            if i as u16 >= preview_area.height {
                break;
            }
            let line_start = byte_offset;
            let line_end = line_start + line.len();

            // Build spans for this line: faded base with search matches overlaid
            let mut spans = Vec::new();
            let mut pos = 0usize;
            for mr in &match_ranges {
                // Convert absolute byte range to line-relative
                if mr.end <= line_start || mr.start >= line_end {
                    continue;
                }
                let rel_start = mr.start.saturating_sub(line_start).min(line.len());
                let rel_end = mr.end.saturating_sub(line_start).min(line.len());
                if rel_start > pos {
                    spans.push(Span::styled(&line[pos..rel_start], preview_style));
                }
                if rel_start < rel_end {
                    spans.push(Span::styled(&line[rel_start..rel_end], match_style));
                }
                pos = rel_end;
            }
            if pos < line.len() {
                spans.push(Span::styled(&line[pos..], preview_style));
            }
            if spans.is_empty() {
                spans.push(Span::styled(line, preview_style));
            }

            f.render_widget(
                Paragraph::new(Line::from(spans)),
                Rect::new(preview_area.x, preview_area.y + i as u16, preview_area.width, 1),
            );
            // +1 for the newline character
            byte_offset = line_end + 1;
        }
    }
}

// ---------------------------------------------------------------------------
// Which-key popup
// ---------------------------------------------------------------------------

fn draw_which_key(f: &mut Frame, area: Rect, wk: &WhichKeyFrame, theme: &TuiTheme) {
    f.render_widget(Clear, area);

    let wk_style = theme.which_key_style();

    // Fill entire area with background (no border — status bar provides separation)
    let block = Block::default().style(wk_style);
    let inner = block.inner(area);
    f.render_widget(block, area);

    // Add horizontal and vertical padding for readability
    let padded = Rect::new(
        inner.x.saturating_add(2),
        inner.y.saturating_add(1),
        inner.width.saturating_sub(4),
        inner.height.saturating_sub(2),  // 1 top + 1 bottom padding
    );

    let col_width = 24u16;
    let cols = (padded.width / col_width).max(1);

    for (i, entry) in wk.entries.iter().enumerate() {
        let col = (i as u16) % cols;
        let row = (i as u16) / cols;
        if row >= padded.height {
            break;
        }
        let key_style = wk_style.add_modifier(Modifier::BOLD);
        let label_style = if entry.is_group {
            RStyle::default().fg(theme.salient()).bg(theme.background())
        } else {
            wk_style
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

    // Place cursor on the selected choice (overrides editor cursor)
    f.set_cursor_position((inner.x + 1, inner.y + 1));
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
// Inline menu (command completion, link picker, tag completion)
// ---------------------------------------------------------------------------

fn draw_inline_menu(
    f: &mut Frame,
    area: Rect,
    menu: &InlineMenuFrame,
    theme: &TuiTheme,
) {
    if menu.items.is_empty() {
        return;
    }

    // Compute column widths from data
    let max_label_w: usize = menu.items.iter()
        .map(|i| i.label.width())
        .max()
        .unwrap_or(0);
    let max_right_w: usize = menu.items.iter()
        .filter_map(|i| i.right.as_ref())
        .map(|r| r.width())
        .max()
        .unwrap_or(0);
    let right_col = if max_right_w > 0 { max_right_w + 2 } else { 0 }; // +2 padding
    let marker_w: usize = 3; // " ▸ " or "   "
    let inner_w = (marker_w + max_label_w + right_col + 2).min(area.width as usize); // +2 border padding
    let menu_w = (inner_w + 2) as u16; // +2 for box borders

    let max_visible: u16 = 8;
    let hint_h: u16 = if menu.hint.is_some() { 1 } else { 0 };
    let item_h = (menu.items.len() as u16).min(max_visible);
    let menu_h = item_h + hint_h + 2; // +2 for box borders

    // Position: CommandLine → above status bar, left-aligned
    let (menu_x, menu_y) = match menu.anchor {
        InlineMenuAnchor::CommandLine => {
            let x = area.x;
            let y = area.bottom().saturating_sub(menu_h + 1); // +1 for status bar
            (x, y)
        }
        InlineMenuAnchor::Cursor { line, col } => {
            let x = (area.x + col as u16).min(area.right().saturating_sub(menu_w));
            let y = area.y + line as u16 + 1; // below cursor line
            (x, y.min(area.bottom().saturating_sub(menu_h)))
        }
    };

    let menu_rect = Rect::new(menu_x, menu_y, menu_w, menu_h);
    f.render_widget(Clear, menu_rect);

    let block = Block::default()
        .borders(Borders::ALL)
        .style(RStyle::default().bg(theme.background()))
        .border_style(theme.faded_style());
    let inner = block.inner(menu_rect);
    f.render_widget(block, menu_rect);

    // Scroll if needed
    let viewport_h = item_h as usize;
    let scroll_offset = if menu.selected >= viewport_h {
        menu.selected - viewport_h + 1
    } else {
        0
    };

    let faded = theme.faded_style();
    for (vi, item) in menu.items.iter().skip(scroll_offset).take(viewport_h).enumerate() {
        let abs_i = scroll_offset + vi;
        let is_selected = abs_i == menu.selected;
        let style = if is_selected {
            theme.picker_selected()
        } else {
            RStyle::default().fg(theme.foreground()).bg(theme.background())
        };
        let marker = if is_selected { " ▸ " } else { "   " };

        let label_w = inner.width as usize - marker_w;
        let mut spans = vec![Span::styled(marker, style)];
        let label_text = truncate_to_width(&item.label, label_w.saturating_sub(right_col));
        spans.push(Span::styled(label_text.clone(), style.add_modifier(Modifier::BOLD)));

        if let Some(right) = &item.right {
            let pad = label_w.saturating_sub(label_text.width() + right_col);
            spans.push(Span::styled(" ".repeat(pad), style));
            let right_style = if is_selected { style } else { faded };
            spans.push(Span::styled(format!("  {right}"), right_style));
        }

        f.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect::new(inner.x, inner.y + vi as u16, inner.width, 1),
        );
    }

    // Hint line
    if let Some(hint) = &menu.hint {
        let hint_y = inner.y + item_h;
        if hint_y < inner.bottom() {
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(format!(" {hint}"), faded))),
                Rect::new(inner.x, hint_y, inner.width, 1),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Special pane types
// ---------------------------------------------------------------------------

fn draw_agenda(
    f: &mut Frame,
    area: Rect,
    agenda: &AgendaFrame,
    theme: &TuiTheme,
    scrolloff: usize,
) {
    // Full-screen overlay
    f.render_widget(Clear, area);
    f.render_widget(
        Block::default().style(RStyle::default().bg(theme.background())),
        area,
    );

    if area.height < 4 {
        return;
    }

    // Apply left/right margins
    let margin_h = 3u16;
    let padded = Rect::new(
        area.x + margin_h,
        area.y,
        area.width.saturating_sub(margin_h * 2),
        area.height,
    );

    // Split: top ~60% for task list + footer, 1 separator, bottom ~40% for preview
    let has_preview = agenda.preview.is_some() && padded.height >= 10;
    let preview_h = if has_preview { (padded.height * 2 / 5).max(3) } else { 0 };
    let separator_h: u16 = if has_preview { 1 } else { 0 };
    let top_h = padded.height.saturating_sub(preview_h).saturating_sub(separator_h);

    // Footer takes 1 line at bottom of the top section
    let list_h = top_h.saturating_sub(1);
    let footer_y = padded.y + list_h;

    // Build flat list with section markers for rendering
    struct Section<'a> {
        label: &'a str,
        items: &'a [bloom_core::render::AgendaItem],
        fg: ratatui::style::Color,
    }

    let today_str = chrono::Local::now().format("%Y-%m-%d").to_string();
    let sections = [
        Section { label: "Overdue", items: &agenda.overdue, fg: theme.critical() },
        Section { label: &format!("Today · {today_str}"), items: &agenda.today, fg: theme.foreground() },
        Section { label: "Upcoming", items: &agenda.upcoming, fg: theme.faded() },
    ];

    // Pre-compute max column widths across all items for alignment
    let all_items = agenda.overdue.iter()
        .chain(agenda.today.iter())
        .chain(agenda.upcoming.iter());
    let max_source_w: usize = all_items.clone()
        .map(|i| i.source_page.width())
        .max()
        .unwrap_or(0)
        .min(padded.width as usize / 3);
    let max_date_w: usize = all_items
        .map(|i| i.date.map(|d| d.format("%Y-%m-%d").to_string().width()).unwrap_or(0))
        .max()
        .unwrap_or(0);
    let source_col_w = max_source_w + 2;
    let date_col_w = if max_date_w > 0 { max_date_w + 2 } else { 0 };

    // Build flat visual rows with spacing
    struct VisualRow<'a> {
        kind: RowKind<'a>,
        section_fg: ratatui::style::Color,
    }
    enum RowKind<'a> {
        Blank,
        Header(&'a str),
        Item { item: &'a bloom_core::render::AgendaItem, flat_idx: usize, is_overdue: bool },
    }

    let mut rows: Vec<VisualRow> = Vec::new();
    let mut flat_idx: usize = 0;
    let mut is_first_section = true;
    for section in &sections {
        if section.items.is_empty() {
            continue;
        }
        // Blank line before section header (including first — gives top margin)
        rows.push(VisualRow { kind: RowKind::Blank, section_fg: section.fg });
        rows.push(VisualRow {
            kind: RowKind::Header(section.label),
            section_fg: section.fg,
        });
        // Blank line after header
        rows.push(VisualRow { kind: RowKind::Blank, section_fg: section.fg });
        for item in section.items {
            rows.push(VisualRow {
                kind: RowKind::Item {
                    item,
                    flat_idx,
                    is_overdue: std::ptr::eq(section.items, &*agenda.overdue),
                },
                section_fg: section.fg,
            });
            flat_idx += 1;
        }
        is_first_section = false;
    }

    // Find the visual row of the selected item
    let selected_visual_row = rows.iter().position(|r| {
        matches!(&r.kind, RowKind::Item { flat_idx: fi, .. } if *fi == agenda.selected_index)
    }).unwrap_or(0);

    // Scroll offset with Vim-style margin
    let viewport_h = list_h as usize;
    let scroll_offset = {
        let min_offset_bottom = (selected_visual_row + scrolloff + 1).saturating_sub(viewport_h);
        let max_offset_top = selected_visual_row.saturating_sub(scrolloff);
        let max_possible = rows.len().saturating_sub(viewport_h);
        min_offset_bottom.min(max_possible).max(min_offset_bottom).min(max_offset_top.max(min_offset_bottom))
    };

    // Render visible rows
    let mut y = padded.y;
    for row in rows.iter().skip(scroll_offset).take(viewport_h) {
        if y >= padded.y + list_h {
            break;
        }
        match &row.kind {
            RowKind::Blank => {
                y += 1;
            }
            RowKind::Header(label) => {
                let header_style = RStyle::default()
                    .fg(theme.salient())
                    .add_modifier(Modifier::BOLD);
                f.render_widget(
                    Paragraph::new(Line::from(Span::styled(
                        label.to_string(),
                        header_style,
                    ))),
                    Rect::new(padded.x, y, padded.width, 1),
                );
                y += 1;
            }
            RowKind::Item { item, flat_idx: fi, is_overdue } => {
                let is_selected = *fi == agenda.selected_index;
                let base_style = if is_selected {
                    RStyle::default().fg(row.section_fg).bg(theme.mild())
                } else {
                    RStyle::default().fg(row.section_fg)
                };

                let marker = if is_selected { "▸ " } else { "  " };
                let date_str = item.date.map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_default();
                let available = padded.width as usize;
                let marker_w = 5; // "▸ ☐ " or "  ☐ "
                let task_max = available.saturating_sub(marker_w + source_col_w + date_col_w);
                let task_text = truncate_to_width(&item.task_text, task_max);
                let task_w = task_text.width();
                let task_pad = task_max.saturating_sub(task_w);

                let source_padded = format!(
                    "{}{}",
                    " ".repeat(source_col_w.saturating_sub(item.source_page.width())),
                    truncate_to_width(&item.source_page, max_source_w),
                );
                let date_padded = if date_col_w > 0 {
                    format!(
                        "{}{}",
                        " ".repeat(date_col_w.saturating_sub(date_str.width())),
                        date_str,
                    )
                } else {
                    String::new()
                };

                let date_fg = if *is_overdue { theme.critical() } else { theme.faded() };
                let source_style = if is_selected {
                    RStyle::default().fg(theme.faded()).bg(theme.mild())
                } else {
                    theme.faded_style()
                };
                let date_style = if is_selected {
                    RStyle::default().fg(date_fg).bg(theme.mild())
                } else {
                    RStyle::default().fg(date_fg)
                };

                let left_part = format!("{marker}☐ {task_text}");

                let line = Line::from(vec![
                    Span::styled(left_part, base_style),
                    Span::styled(" ".repeat(task_pad), base_style),
                    Span::styled(source_padded, source_style),
                    Span::styled(date_padded, date_style),
                ]);
                f.render_widget(Paragraph::new(line), Rect::new(padded.x, y, padded.width, 1));
                y += 1;
            }
        }
    }

    // Footer
    if footer_y < padded.bottom() {
        let selection = if agenda.total_open > 0 {
            format!("{}", agenda.selected_index + 1)
        } else {
            "0".to_string()
        };
        let footer = format!(
            "▸ {selection}/{} tasks   {} pages   [x]toggle [Enter]jump [q]close",
            agenda.total_open,
            agenda.total_pages,
        );
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(footer, theme.faded_style()))),
            Rect::new(padded.x, footer_y, padded.width, 1),
        );
    }

    // Separator + Preview
    if has_preview {
        let sep_y = padded.y + top_h;
        if sep_y < padded.bottom() {
            let sep = "─".repeat(padded.width as usize);
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(sep, theme.faded_style()))),
                Rect::new(padded.x, sep_y, padded.width, 1),
            );
        }

        let preview_y = sep_y + 2; // 1 blank line after separator
        if let Some(text) = &agenda.preview {
            let preview_style = theme.faded_style();
            for (i, line) in text.lines().enumerate() {
                let ly = preview_y + i as u16;
                if ly >= padded.bottom() {
                    break;
                }
                f.render_widget(
                    Paragraph::new(Line::from(Span::styled(
                        format!("  {line}"),
                        preview_style,
                    ))),
                    Rect::new(padded.x, ly, padded.width, 1),
                );
            }
        }
    }

    // Place cursor on the selected item row (overrides editor cursor)
    let selected_screen_y = padded.y + (selected_visual_row.saturating_sub(scroll_offset)) as u16;
    if selected_screen_y < padded.y + list_h {
        f.set_cursor_position((padded.x, selected_screen_y));
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
        Rect::new(x, y, text.width() as u16 + 1, 1),
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
