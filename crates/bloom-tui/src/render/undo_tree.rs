use super::*;

pub(super) fn draw_undo_tree(
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
                    Paragraph::new(Line::from(Span::styled(format!("  {line}"), style))),
                    Rect::new(area.x, y, area.width, 1),
                );
                y += 1;
            }
        }
    }
}
