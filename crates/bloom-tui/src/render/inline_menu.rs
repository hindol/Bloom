use super::*;

pub(super) fn draw_inline_menu(
    f: &mut Frame,
    area: Rect,
    menu: &InlineMenuFrame,
    theme: &TuiTheme,
) {
    if menu.items.is_empty() {
        return;
    }

    // Compute column widths from data
    let max_label_w: usize = menu
        .items
        .iter()
        .map(|i| i.label.width())
        .max()
        .unwrap_or(0);
    let max_right_w: usize = menu
        .items
        .iter()
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
    for (vi, item) in menu
        .items
        .iter()
        .skip(scroll_offset)
        .take(viewport_h)
        .enumerate()
    {
        let abs_i = scroll_offset + vi;
        let is_selected = abs_i == menu.selected;
        let style = if is_selected {
            theme.picker_selected()
        } else {
            RStyle::default()
                .fg(theme.foreground())
                .bg(theme.background())
        };
        let marker = if is_selected { " ▸ " } else { "   " };

        let label_w = inner.width as usize - marker_w;
        let mut spans = vec![Span::styled(marker, style)];
        let label_text = truncate_to_width(&item.label, label_w.saturating_sub(right_col));
        spans.push(Span::styled(
            label_text.clone(),
            style.add_modifier(Modifier::BOLD),
        ));

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
