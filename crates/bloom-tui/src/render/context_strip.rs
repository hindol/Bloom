//! Renders the 3-line context strip for temporal navigation (journal day-hopping,
//! page history, day activity). Appears above the status bar of the active pane.

use crate::theme::TuiTheme;
use bloom_core::render::ContextStripFrame;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

/// Draw the context strip as a 3-line panel at the bottom of the pane area.
/// The strip overlays the last 4 rows (3 content + 1 border) above the status bar.
pub(super) fn draw_context_strip(
    f: &mut Frame,
    pane_area: Rect,
    strip: &ContextStripFrame,
    theme: &TuiTheme,
) {
    let strip_height = 5u16; // 3 content lines + top/bottom border
    if pane_area.height < strip_height + 2 {
        return; // Not enough room
    }

    // Position the strip at the bottom of the pane area, just above the status bar.
    // The status bar is the last row of each pane's total_height.
    // We overlay just above it.
    let y = pane_area.y + pane_area.height.saturating_sub(strip_height + 1);
    let strip_rect = Rect::new(pane_area.x, y, pane_area.width, strip_height);

    let faded = Style::default().fg(theme.faded());
    let strong = Style::default()
        .fg(theme.foreground())
        .add_modifier(Modifier::BOLD);
    let salient = Style::default().fg(theme.salient());
    let bg = Style::default().bg(theme.background());

    // Border
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(faded)
        .style(bg);
    f.render_widget(block, strip_rect);

    let inner = Rect::new(
        strip_rect.x + 1,
        strip_rect.y + 1,
        strip_rect.width.saturating_sub(2),
        3,
    );

    // Previous day (faded)
    if let Some(prev) = &strip.prev_label {
        let line = Line::from(Span::styled(format!("  {prev}"), faded));
        f.render_widget(
            Paragraph::new(line),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );
    }

    // Current day (highlighted with ▸ marker)
    let current_line = Line::from(vec![
        Span::styled("▸ ", salient),
        Span::styled(&strip.current_label, strong),
    ]);
    f.render_widget(
        Paragraph::new(current_line).style(Style::default().bg(theme.highlight())),
        Rect::new(inner.x, inner.y + 1, inner.width, 1),
    );

    // Next day (faded)
    if let Some(next) = &strip.next_label {
        let line = Line::from(Span::styled(format!("  {next}"), faded));
        f.render_widget(
            Paragraph::new(line),
            Rect::new(inner.x, inner.y + 2, inner.width, 1),
        );
    }
}
