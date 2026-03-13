//! Renders the journal scrubber as a 3-line panel with separator lines.
//!
//! ```text
//! ┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄
//!   ◄ Mar 8 Sat         ▸ Mar 10 Mon                  Mar 12 Wed ►
//!   3 items · #rust       5 items · #rust #editors      2 items
//!   [ ] Review ropey      [ ] Fix parser bug           [x] Read DDIA
//! ┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄
//! ```

use crate::theme::TuiTheme;
use bloom_core::render::ContextStripFrame;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

pub(super) fn draw_context_strip(
    f: &mut Frame,
    pane_area: Rect,
    strip: &ContextStripFrame,
    theme: &TuiTheme,
) {
    // 5 rows: separator + 3 content + separator
    let strip_h = 5u16;
    if pane_area.height < strip_h + 2 {
        return;
    }

    let y = pane_area.y + pane_area.height.saturating_sub(strip_h + 1);
    let w = pane_area.width;
    let col_w = (w / 3) as usize;

    let bg = Style::default().bg(theme.background());
    let faded = Style::default().fg(theme.faded()).bg(theme.background());
    let normal = Style::default().fg(theme.foreground()).bg(theme.background());
    let strong = Style::default()
        .fg(theme.foreground())
        .bg(theme.background())
        .add_modifier(Modifier::BOLD);
    let salient = Style::default().fg(theme.salient()).bg(theme.background());

    // Fill background (buffer color)
    for row in 0..strip_h {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(" ".repeat(w as usize), bg))),
            Rect::new(pane_area.x, y + row, w, 1),
        );
    }

    // Separator line (top)
    let sep = "┄".repeat(w as usize);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(&sep, faded))),
        Rect::new(pane_area.x, y, w, 1),
    );
    // Separator line (bottom)
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(&sep, faded))),
        Rect::new(pane_area.x, y + 4, w, 1),
    );

    // --- Row 1: Date labels ---
    let mut row0: Vec<Span> = Vec::new();
    row0.push(Span::styled(" ", bg));
    if let Some(prev) = &strip.prev {
        row0.push(Span::styled("◄ ", faded));
        row0.push(Span::styled(pad_to(&prev.label, col_w.saturating_sub(4)), faded));
    } else {
        row0.push(Span::styled(pad_to("", col_w.saturating_sub(1)), bg));
    }
    row0.push(Span::styled("▸ ", salient));
    row0.push(Span::styled(pad_to(&strip.current.label, col_w.saturating_sub(3)), strong));
    if let Some(next) = &strip.next {
        row0.push(Span::styled(pad_to(&next.label, col_w.saturating_sub(3)), faded));
        row0.push(Span::styled("►", faded));
    }
    f.render_widget(
        Paragraph::new(Line::from(row0)),
        Rect::new(pane_area.x, y + 1, w, 1),
    );

    // --- Row 2: Stats ---
    let mut row1: Vec<Span> = Vec::new();
    row1.push(Span::styled("  ", bg));
    if let Some(prev) = &strip.prev {
        row1.push(Span::styled(pad_to(&prev.stats, col_w.saturating_sub(1)), faded));
    } else {
        row1.push(Span::styled(pad_to("", col_w.saturating_sub(1)), bg));
    }
    row1.push(Span::styled(pad_to(&strip.current.stats, col_w.saturating_sub(1)), normal));
    if let Some(next) = &strip.next {
        row1.push(Span::styled(pad_to(&next.stats, col_w.saturating_sub(1)), faded));
    }
    f.render_widget(
        Paragraph::new(Line::from(row1)),
        Rect::new(pane_area.x, y + 2, w, 1),
    );

    // --- Row 3: First task/line ---
    let mut row2: Vec<Span> = Vec::new();
    row2.push(Span::styled("  ", bg));
    if let Some(prev) = &strip.prev {
        row2.push(Span::styled(
            pad_to(&truncate(&prev.first_line, col_w.saturating_sub(2)), col_w.saturating_sub(1)),
            faded,
        ));
    } else {
        row2.push(Span::styled(pad_to("", col_w.saturating_sub(1)), bg));
    }
    row2.push(Span::styled(
        pad_to(&truncate(&strip.current.first_line, col_w.saturating_sub(2)), col_w.saturating_sub(1)),
        normal,
    ));
    if let Some(next) = &strip.next {
        row2.push(Span::styled(
            pad_to(&truncate(&next.first_line, col_w.saturating_sub(2)), col_w.saturating_sub(1)),
            faded,
        ));
    }
    f.render_widget(
        Paragraph::new(Line::from(row2)),
        Rect::new(pane_area.x, y + 3, w, 1),
    );
}

fn pad_to(s: &str, width: usize) -> String {
    use unicode_width::UnicodeWidthStr;
    let w = s.width();
    if w >= width {
        s.to_string()
    } else {
        format!("{}{}", s, " ".repeat(width - w))
    }
}

fn truncate(s: &str, max: usize) -> String {
    use unicode_width::UnicodeWidthStr;
    if s.width() <= max {
        s.to_string()
    } else {
        let mut w = 0;
        let mut end = 0;
        for (i, ch) in s.char_indices() {
            let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
            if w + cw > max.saturating_sub(1) {
                break;
            }
            w += cw;
            end = i + ch.len_utf8();
        }
        format!("{}…", &s[..end])
    }
}
