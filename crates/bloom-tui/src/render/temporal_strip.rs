//! Temporal strip renderer — horizontal timeline above the status bar.
//!
//! Shows undo nodes (●), git commits (○), and branch points ([●]) in a
//! horizontal timeline. Preview diff lines rendered in the pane area above.

use bloom_core::render::{DiffLineKind, StripNodeKind, TemporalStripFrame};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style as RStyle};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use ratatui::Frame;

use crate::theme::TuiTheme;

pub(super) fn draw_temporal_strip(
    f: &mut Frame,
    pane_area: Rect,
    strip: &TemporalStripFrame,
    theme: &TuiTheme,
) {
    if strip.items.is_empty() {
        return;
    }

    // Layout: preview takes most of the space, strip takes 3 lines at bottom,
    // status bar is rendered separately by the pane.
    let strip_height = if strip.compact { 1u16 } else { 2u16 };
    let total_overlay = strip_height + 1; // +1 for separator
    if pane_area.height < total_overlay + 4 {
        return; // not enough space
    }

    let preview_area = Rect::new(
        pane_area.x,
        pane_area.y,
        pane_area.width,
        pane_area.height.saturating_sub(total_overlay),
    );
    let strip_area = Rect::new(
        pane_area.x,
        pane_area.y + preview_area.height,
        pane_area.width,
        strip_height,
    );
    let sep_area = Rect::new(
        pane_area.x,
        strip_area.y + strip_height,
        pane_area.width,
        1,
    );

    // --- Draw preview (diff lines) ---
    let bg = RStyle::default().bg(theme.background());
    f.render_widget(Clear, preview_area);
    f.render_widget(ratatui::widgets::Block::default().style(bg), preview_area);

    let mut preview_lines: Vec<Line> = Vec::new();
    // Title line
    let mode_label = match strip.mode {
        bloom_core::render::TemporalMode::PageHistory => "HIST",
        bloom_core::render::TemporalMode::BlockHistory => "HIST",
        bloom_core::render::TemporalMode::DayActivity => "DAY",
    };
    preview_lines.push(Line::from(vec![
        Span::styled(
            format!(" {} ", mode_label),
            RStyle::default()
                .fg(theme.background())
                .bg(theme.accent_yellow())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  {}", strip.title),
            RStyle::default().fg(theme.foreground()).bg(theme.background()),
        ),
    ]));
    preview_lines.push(Line::from(""));

    for dl in &strip.preview_lines {
        let style = match dl.kind {
            DiffLineKind::Context => RStyle::default().fg(theme.faded()),
            DiffLineKind::Added => RStyle::default().fg(theme.accent_green()),
            DiffLineKind::Removed => RStyle::default().fg(theme.accent_red()),
        };
        let prefix = match dl.kind {
            DiffLineKind::Context => "  ",
            DiffLineKind::Added => "+ ",
            DiffLineKind::Removed => "- ",
        };
        preview_lines.push(Line::from(Span::styled(
            format!("{}{}", prefix, dl.text),
            style,
        )));
    }

    let preview_widget = Paragraph::new(preview_lines).style(bg);
    f.render_widget(preview_widget, preview_area);

    // --- Draw strip (horizontal timeline) ---
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
    f.render_widget(Paragraph::new(vec![strip_line]).style(strip_bg), strip_area);

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
    f.render_widget(Paragraph::new(vec![sep_line]).style(strip_bg), sep_area);
}
