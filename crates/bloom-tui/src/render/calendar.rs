//! Renders the journal calendar overlay — a month grid with ◆ markers for days
//! with journal entries. The editor buffer behind it shows the selected day's content.

use crate::theme::TuiTheme;
use bloom_core::render::DatePickerFrame;
use chrono::Datelike;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

pub(super) fn draw_calendar(
    f: &mut Frame,
    area: Rect,
    dp: &DatePickerFrame,
    theme: &TuiTheme,
) {
    let grid_height = 10u16; // header + day-names + 6 week rows + footer
    let grid_width = 32u16.min(area.width);

    if area.height < grid_height + 2 || area.width < grid_width {
        return;
    }

    // Center horizontally, anchor to bottom
    let x = area.x + (area.width.saturating_sub(grid_width)) / 2;
    let y = area.y + area.height.saturating_sub(grid_height + 1);
    let cal_rect = Rect::new(x, y, grid_width, grid_height);

    f.render_widget(Clear, cal_rect);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.faded()))
        .style(Style::default().bg(theme.background()));
    f.render_widget(block, cal_rect);

    let inner = Rect::new(cal_rect.x + 1, cal_rect.y + 1, cal_rect.width - 2, grid_height - 2);

    let faded = Style::default().fg(theme.faded()).bg(theme.background());
    let normal = Style::default().fg(theme.foreground()).bg(theme.background());
    let bold = Style::default()
        .fg(theme.foreground())
        .bg(theme.background())
        .add_modifier(Modifier::BOLD);
    let selected_style = Style::default()
        .fg(theme.background())
        .bg(theme.salient());
    let journal_marker = Style::default().fg(theme.salient()).bg(theme.background());

    // Row 0: Month Year
    let month_name = chrono::Month::try_from(dp.month as u8)
        .map(|m| m.name())
        .unwrap_or("???");
    let header = format!("{} {}", month_name, dp.year);
    let header_x = inner.x + (inner.width.saturating_sub(header.len() as u16)) / 2;
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(&header, bold))),
        Rect::new(header_x, inner.y, header.len() as u16, 1),
    );

    // Row 1: Day names
    let day_names = "Mo Tu We Th Fr Sa Su";
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(day_names, faded))),
        Rect::new(inner.x, inner.y + 1, inner.width, 1),
    );

    // Rows 2+: Calendar grid
    let selected_day = dp.selected_date.day();
    let is_selected_month =
        dp.selected_date.year() == dp.year && dp.selected_date.month() == dp.month;
    let is_today_month = dp.today.year() == dp.year && dp.today.month() == dp.month;
    let today_day = dp.today.day();

    for (week_idx, week) in dp.month_view.iter().enumerate() {
        let row_y = inner.y + 2 + week_idx as u16;
        if row_y >= inner.y + inner.height {
            break;
        }

        let mut spans: Vec<Span> = Vec::new();
        for (day_idx, day_opt) in week.iter().enumerate() {
            if day_idx > 0 {
                spans.push(Span::raw(" "));
            }
            match day_opt {
                Some(day) => {
                    let has_journal = dp.journal_days.contains(day);
                    let is_selected = is_selected_month && *day == selected_day;
                    let is_today = is_today_month && *day == today_day;

                    let marker = if has_journal { "◆" } else { " " };
                    let day_str = format!("{day:>2}");

                    if is_selected {
                        spans.push(Span::styled(format!("{marker}{day_str}"), selected_style));
                    } else if is_today {
                        spans.push(Span::styled(marker, journal_marker));
                        spans.push(Span::styled(
                            day_str,
                            normal.add_modifier(Modifier::UNDERLINED),
                        ));
                    } else if has_journal {
                        spans.push(Span::styled(marker, journal_marker));
                        spans.push(Span::styled(day_str, normal));
                    } else {
                        spans.push(Span::styled(format!(" {day_str}"), faded));
                    }
                }
                None => {
                    spans.push(Span::styled("   ", faded));
                }
            }
        }
        f.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect::new(inner.x, row_y, inner.width, 1),
        );
    }

    // Footer: key hints
    let footer_y = inner.y + inner.height.saturating_sub(1);
    let footer = format!("{} entries  [d/]d:skip  ↵:open", dp.journal_days.len());
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(&footer, faded))),
        Rect::new(inner.x, footer_y, inner.width, 1),
    );
}
