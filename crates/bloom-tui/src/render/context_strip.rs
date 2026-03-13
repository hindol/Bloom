//! Renders the context strip as a single horizontal line for journal day-hopping.
//! Shows:  ◄ prev day  │  ▸ current day  │  next day ►
//! Overlays the last content line above the status bar. Auto-hides after timeout.

use crate::theme::TuiTheme;
use bloom_core::render::ContextStripFrame;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

/// Draw the context strip as a single horizontal line above the status bar.
pub(super) fn draw_context_strip(
    f: &mut Frame,
    pane_area: Rect,
    strip: &ContextStripFrame,
    theme: &TuiTheme,
) {
    if pane_area.height < 3 {
        return;
    }

    // Position: one row above the status bar (last row of pane content)
    let y = pane_area.y + pane_area.height.saturating_sub(2);
    let line_rect = Rect::new(pane_area.x, y, pane_area.width, 1);

    let faded = Style::default().fg(theme.faded()).bg(theme.highlight());
    let strong = Style::default()
        .fg(theme.foreground())
        .bg(theme.highlight())
        .add_modifier(Modifier::BOLD);
    let salient = Style::default().fg(theme.salient()).bg(theme.highlight());
    let sep = Style::default().fg(theme.faded()).bg(theme.highlight());

    let mut spans: Vec<Span> = Vec::new();
    spans.push(Span::styled(" ", faded));

    // Previous day
    if let Some(prev) = &strip.prev_label {
        spans.push(Span::styled("◄ ", faded));
        spans.push(Span::styled(prev.as_str(), faded));
    }

    spans.push(Span::styled("  │  ", sep));

    // Current day (highlighted)
    spans.push(Span::styled("▸ ", salient));
    spans.push(Span::styled(strip.current_label.as_str(), strong));

    spans.push(Span::styled("  │  ", sep));

    // Next day
    if let Some(next) = &strip.next_label {
        spans.push(Span::styled(next.as_str(), faded));
        spans.push(Span::styled(" ►", faded));
    }

    f.render_widget(Paragraph::new(Line::from(spans)), line_rect);
}
