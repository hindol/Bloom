use super::*;

pub(super) fn draw_timeline(
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
