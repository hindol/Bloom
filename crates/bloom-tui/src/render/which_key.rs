use super::*;

pub(super) fn draw_which_key(f: &mut Frame, area: Rect, wk: &WhichKeyFrame, theme: &TuiTheme) {
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
        inner.height.saturating_sub(2), // 1 top + 1 bottom padding
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
