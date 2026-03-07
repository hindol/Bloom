use super::*;

pub(super) fn draw_dialog(f: &mut Frame, area: Rect, dialog: &DialogFrame, theme: &TuiTheme) {
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
