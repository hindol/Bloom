//! Temporal strip renderer — horizontal timeline above the status bar.
//!
//! Shows undo nodes (●), git commits (○), and branch points ([●]) in a
//! horizontal timeline. Preview diff lines rendered in the pane area above.

use bloom_core::render::{StripNodeKind, TemporalStripFrame};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style as RStyle};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use ratatui::Frame;

use crate::theme::TuiTheme;

pub(super) fn draw_temporal_strip(
    f: &mut Frame,
    _pane_area: Rect,
    strip_area: Rect,
    strip: &TemporalStripFrame,
    theme: &TuiTheme,
) {
    if strip.items.is_empty() {
        return;
    }

    // --- Draw strip in the drawer area (below status bar) ---
    let strip_bg = RStyle::default().bg(theme.highlight());
    f.render_widget(Clear, strip_area);
    f.render_widget(ratatui::widgets::Block::default().style(strip_bg), strip_area);

    let mut spans: Vec<Span> = Vec::new();
    spans.push(Span::styled("├─", RStyle::default().fg(theme.faded()).bg(theme.highlight())));

    for (i, node) in strip.items.iter().enumerate() {
        let is_selected = i == strip.selected;
        let marker = if node.branch_count > 1 {
            "[●]"
        } else {
            match node.kind {
                StripNodeKind::UndoNode => "●",
                StripNodeKind::GitCommit => "○",
            }
        };

        let node_style = if is_selected {
            RStyle::default()
                .fg(theme.foreground())
                .bg(theme.highlight())
                .add_modifier(Modifier::BOLD)
        } else {
            match node.kind {
                StripNodeKind::UndoNode => {
                    RStyle::default().fg(theme.foreground()).bg(theme.highlight())
                }
                StripNodeKind::GitCommit => {
                    RStyle::default().fg(theme.faded()).bg(theme.highlight())
                }
            }
        };

        let connector = if i > 0 { "── " } else { " " };
        spans.push(Span::styled(connector, RStyle::default().fg(theme.faded()).bg(theme.highlight())));

        if is_selected {
            spans.push(Span::styled("▸", RStyle::default().fg(theme.accent_yellow()).bg(theme.highlight())));
        }
        spans.push(Span::styled(marker, node_style));
        spans.push(Span::styled(
            format!(" {}", node.label),
            node_style,
        ));
    }

    spans.push(Span::styled(
        "─┤",
        RStyle::default().fg(theme.faded()).bg(theme.highlight()),
    ));

    let strip_line = Line::from(spans);
    let strip_line_area = Rect::new(strip_area.x, strip_area.y, strip_area.width, 1);
    f.render_widget(Paragraph::new(vec![strip_line]).style(strip_bg), strip_line_area);

    // Rich mode: second line with descriptions
    if !strip.compact && strip_area.height > 1 {
        let mut detail_spans: Vec<Span> = Vec::new();
        detail_spans.push(Span::styled("│ ", RStyle::default().fg(theme.faded()).bg(theme.highlight())));
        for (i, node) in strip.items.iter().enumerate() {
            let detail = node.detail.as_deref().unwrap_or("");
            let truncated = super::truncate_to_width(detail, 12);
            let pad = if i > 0 { "   " } else { "" };
            let style = if i == strip.selected {
                RStyle::default().fg(theme.foreground()).bg(theme.highlight())
            } else {
                RStyle::default().fg(theme.faded()).bg(theme.highlight())
            };
            detail_spans.push(Span::styled(format!("{}{:<12}", pad, truncated), style));
        }
        let detail_line = Line::from(detail_spans);
        let detail_area = Rect::new(strip_area.x, strip_area.y + 1, strip_area.width, 1);
        f.render_widget(Paragraph::new(vec![detail_line]).style(strip_bg), detail_area);
    }

    // --- Separator line with hints ---
    let hints = match strip.mode {
        bloom_core::render::TemporalMode::PageHistory | bloom_core::render::TemporalMode::BlockHistory => {
            "h/l:scrub  e:detail  r:restore  q:close"
        }
        bloom_core::render::TemporalMode::DayActivity => {
            "h/l:scrub  e:detail  Enter:page  q:close"
        }
    };
    let sep_line = Line::from(vec![
        Span::styled(
            "├",
            RStyle::default().fg(theme.faded()).bg(theme.highlight()),
        ),
        Span::styled(
            format!("─ {} ", hints),
            RStyle::default().fg(theme.faded()).bg(theme.highlight()),
        ),
    ]);
    let hint_y = if !strip.compact && strip_area.height > 2 {
        strip_area.y + 2
    } else if strip_area.height > 1 {
        strip_area.y + 1
    } else {
        return; // no room for hints
    };
    let hint_area = Rect::new(strip_area.x, hint_y, strip_area.width, 1);
    f.render_widget(Paragraph::new(vec![sep_line]).style(strip_bg), hint_area);
}
