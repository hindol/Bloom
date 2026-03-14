//! Live Views overlay rendering — full-screen modal with column-aligned results.

use bloom_core::render::{ViewFrame, ViewRow};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use unicode_width::UnicodeWidthStr;

use crate::theme::TuiTheme;

pub fn draw_view(f: &mut Frame, view: &ViewFrame, theme: &TuiTheme) {
    let area = f.area();
    f.render_widget(Clear, area);

    let border = Block::default()
        .title(format!(" {} ", if view.is_prompt { "Query Prompt" } else { &view.title }))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.faded()))
        .style(Style::default().bg(theme.background()));
    let inner = border.inner(area);
    f.render_widget(border, area);

    if inner.height < 4 {
        return;
    }

    let mut y = inner.y;
    let w = inner.width;

    // --- Query input (prompt mode) ---
    if view.is_prompt {
        let query_style = Style::default().fg(theme.foreground()).bg(theme.background());
        let cursor_style = Style::default().bg(theme.salient()).fg(theme.background());
        let prompt = "> ";
        let q = &view.query;
        let cp = view.query_cursor.min(q.len());
        let spans = vec![
            Span::styled(prompt, Style::default().fg(theme.faded())),
            Span::styled(&q[..cp], query_style),
            Span::styled(
                if cp < q.len() { &q[cp..cp + 1] } else { " " },
                cursor_style,
            ),
            Span::styled(if cp < q.len() { &q[cp + 1..] } else { "" }, query_style),
        ];
        f.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect::new(inner.x, y, w, 1),
        );
        y += 1;
        // Separator
        let sep = Style::default().fg(theme.faded()).bg(theme.background());
        f.render_widget(
            Paragraph::new(Line::from(Span::styled("─".repeat(w as usize), sep))),
            Rect::new(inner.x, y, w, 1),
        );
        y += 1;
    }

    // --- Error display ---
    if let Some(err) = &view.error {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("✗ {err}"),
                Style::default().fg(theme.critical()),
            )))
            .wrap(Wrap { trim: false }),
            Rect::new(inner.x, y, w, 2),
        );
        return;
    }

    // Reserve footer
    let footer_y = inner.y + inner.height - 1;
    let _content_h = footer_y.saturating_sub(y);

    if view.rows.is_empty() {
        f.render_widget(
            Paragraph::new("No results").style(Style::default().fg(theme.faded())),
            Rect::new(inner.x, y, w, 1),
        );
    } else {
        // --- Compute column widths ---
        let col_count = view.columns.len().max(
            view.rows
                .iter()
                .filter_map(|r| match r {
                    ViewRow::Data { cells, .. } => Some(cells.len()),
                    _ => None,
                })
                .max()
                .unwrap_or(0),
        );

        let mut col_widths: Vec<usize> = vec![0; col_count];
        // Seed from headers
        for (i, h) in view.columns.iter().enumerate() {
            if i < col_widths.len() {
                col_widths[i] = col_widths[i].max(h.width());
            }
        }
        // Seed from data
        for row in &view.rows {
            if let ViewRow::Data { cells, .. } = row {
                for (i, c) in cells.iter().enumerate() {
                    if i < col_widths.len() {
                        col_widths[i] = col_widths[i].max(c.width());
                    }
                }
            }
        }
        // Cap each column to a reasonable max
        let available = w as usize;
        let total_w: usize = col_widths.iter().sum::<usize>() + col_count.saturating_sub(1) * 2;
        if total_w > available {
            // Shrink proportionally
            let scale = available as f64 / total_w as f64;
            for cw in &mut col_widths {
                *cw = ((*cw as f64 * scale) as usize).max(4);
            }
        }

        // --- Column header row ---
        if !view.columns.is_empty() {
            let header_style = Style::default()
                .fg(theme.faded())
                .bg(theme.background())
                .add_modifier(Modifier::BOLD);
            let spans: Vec<Span> = view
                .columns
                .iter()
                .enumerate()
                .map(|(i, h)| {
                    let cw = col_widths.get(i).copied().unwrap_or(10);
                    Span::styled(pad(h, cw), header_style)
                })
                .collect();
            f.render_widget(
                Paragraph::new(Line::from(
                    spans
                        .into_iter()
                        .flat_map(|s| vec![s, Span::styled("  ", header_style)])
                        .collect::<Vec<_>>(),
                )),
                Rect::new(inner.x, y, w, 1),
            );
            y += 1;
        }

        // --- Scrolling ---
        let vis_h = footer_y.saturating_sub(y) as usize;
        let scroll_start = if view.selected >= vis_h {
            view.selected - vis_h + 1
        } else {
            0
        };

        // --- Rows ---
        let normal = Style::default().fg(theme.foreground()).bg(theme.background());
        let faded = Style::default().fg(theme.faded()).bg(theme.background());
        let section = Style::default()
            .fg(theme.salient())
            .bg(theme.background())
            .add_modifier(Modifier::BOLD);
        let selected_bg = Style::default().bg(theme.mild());

        let mut data_idx: usize = 0; // index among selectable (data) rows
        for row in view.rows.iter().skip(scroll_start) {
            if y >= footer_y {
                break;
            }
            match row {
                ViewRow::SectionHeader(label) => {
                    f.render_widget(
                        Paragraph::new(Line::from(Span::styled(label.as_str(), section))),
                        Rect::new(inner.x, y, w, 1),
                    );
                    y += 1;
                }
                ViewRow::Data {
                    cells,
                    is_task,
                    task_done,
                } => {
                    let is_sel = data_idx == view.selected;
                    let base = if is_sel {
                        selected_bg
                    } else if *task_done {
                        faded
                    } else {
                        normal
                    };

                    let mut spans: Vec<Span> = Vec::new();
                    if *is_task {
                        let cb = if *task_done { "[x] " } else { "[ ] " };
                        let cb_style = if *task_done {
                            Style::default().fg(theme.accent_green()).bg(base.bg.unwrap_or(Color::Reset))
                        } else {
                            Style::default().fg(theme.accent_yellow()).bg(base.bg.unwrap_or(Color::Reset))
                        };
                        spans.push(Span::styled(cb, cb_style));
                    }

                    for (i, cell) in cells.iter().enumerate() {
                        let cw = col_widths.get(i).copied().unwrap_or(10);
                        spans.push(Span::styled(pad(cell, cw), base));
                        if i + 1 < cells.len() {
                            spans.push(Span::styled("  ", base));
                        }
                    }

                    // Fill rest of line with bg
                    f.render_widget(
                        Paragraph::new(Line::from(spans)),
                        Rect::new(inner.x, y, w, 1),
                    );
                    y += 1;
                    data_idx += 1;
                }
            }
        }
    }

    // --- Footer ---
    let hint = if view.is_prompt {
        "↑↓:nav  Enter:execute  Esc:close"
    } else {
        "j/k:nav  Enter:jump  x:toggle  q:close"
    };
    let footer = format!(
        "{} result{}  {}",
        view.total,
        if view.total == 1 { "" } else { "s" },
        hint,
    );
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            footer,
            Style::default().fg(theme.faded()).bg(theme.background()),
        ))),
        Rect::new(inner.x, footer_y, w, 1),
    );
}

fn pad(s: &str, width: usize) -> String {
    let w = s.width();
    if w >= width {
        s.chars().take(width).collect()
    } else {
        format!("{}{}", s, " ".repeat(width - w))
    }
}
