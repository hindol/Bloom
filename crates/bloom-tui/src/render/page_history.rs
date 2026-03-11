use super::*;

pub(super) fn draw_page_history(
    f: &mut Frame,
    area: Rect,
    ph: &bloom_core::render::PageHistoryFrame,
    theme: &TuiTheme,
) {
    if area.height < 2 {
        return;
    }

    // Title line
    let title = format!(
        " {} — History ({} versions)",
        ph.page_title, ph.total_versions,
    );
    let title_style = RStyle::default()
        .fg(theme.strong())
        .add_modifier(Modifier::BOLD);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(title, title_style))),
        Rect::new(area.x, area.y, area.width, 1),
    );

    let mut y = area.y + 1;

    // Draw a blank line after title if space allows
    if y < area.bottom() {
        y += 1;
    }

    for (i, entry) in ph.entries.iter().enumerate() {
        if y >= area.bottom() {
            break;
        }

        let is_selected = i == ph.selected_index;
        let marker = if is_selected { "▸" } else { " " };

        // Date and diff stat on the same line
        let date_style = if is_selected {
            theme.picker_selected().add_modifier(Modifier::BOLD)
        } else {
            RStyle::default().fg(theme.foreground())
        };

        let stat_style = if is_selected {
            theme.picker_selected()
        } else {
            RStyle::default().fg(theme.faded())
        };

        // Build the line: "▸ Mar 8, 14:32                          +12 / -0"
        let date_part = format!(" {marker} {}", entry.date);
        let stat_part = &entry.diff_stat;
        let padding = (area.width as usize)
            .saturating_sub(date_part.len() + stat_part.len() + 2);
        let pad_str = " ".repeat(padding);

        let line = Line::from(vec![
            Span::styled(date_part, date_style),
            Span::raw(pad_str),
            Span::styled(stat_part.clone(), stat_style),
            Span::raw(" "),
        ]);
        f.render_widget(Paragraph::new(line), Rect::new(area.x, y, area.width, 1));
        y += 1;

        // Description line (indented)
        if !entry.description.is_empty() && y < area.bottom() {
            let desc_style = if is_selected {
                theme.picker_selected()
            } else {
                RStyle::default().fg(theme.faded())
            };
            let desc = format!("     {}", entry.description);
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(desc, desc_style))),
                Rect::new(area.x, y, area.width, 1),
            );
            y += 1;
        }

        // Blank line between entries
        if y < area.bottom() {
            y += 1;
        }
    }
}
