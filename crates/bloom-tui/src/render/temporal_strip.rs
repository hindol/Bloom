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
    pane_area: Rect,
    strip_area: Rect,
    strip: &TemporalStripFrame,
    theme: &TuiTheme,
) {
    if strip.items.is_empty() {
        return;
    }

    // --- Draw preview ---
    // Page history: full-page diff overlay
    // Block history: inline diff on one line only (rest of page stays normal)
    if strip.mode == bloom_core::render::TemporalMode::BlockHistory {
        // Inline block diff: render word-diff segments on the block's line
        if let Some(block_line) = strip.block_line {
            if !strip.block_diff_segments.is_empty() {
                // Find the screen Y for this buffer line
                // block_line is 0-based buffer line; pane starts at pane_area.y
                // Account for viewport scroll (approximation: assume line is visible)
                let gutter_w = 5u16; // line number width
                let y = pane_area.y + block_line as u16; // simplified; real impl should check viewport
                if y < pane_area.y + pane_area.height.saturating_sub(1) {
                    let line_area = Rect::new(
                        pane_area.x + gutter_w,
                        y,
                        pane_area.width.saturating_sub(gutter_w),
                        1,
                    );
                    let bg = RStyle::default().bg(theme.background());
                    f.render_widget(Clear, line_area);
                    let mut spans: Vec<Span> = Vec::new();
                    for seg in &strip.block_diff_segments {
                        let style = match seg.kind {
                            bloom_core::render::DiffLineKind::Context => {
                                RStyle::default().fg(theme.foreground()).bg(theme.background())
                            }
                            bloom_core::render::DiffLineKind::Added => {
                                RStyle::default().fg(theme.accent_green()).bg(theme.background())
                            }
                            bloom_core::render::DiffLineKind::Removed => {
                                RStyle::default()
                                    .fg(theme.accent_red())
                                    .bg(theme.background())
                                    .add_modifier(Modifier::CROSSED_OUT)
                            }
                        };
                        spans.push(Span::styled(&seg.text, style));
                    }
                    f.render_widget(Paragraph::new(vec![Line::from(spans)]).style(bg), line_area);
                }
            }
        }
    } else if !strip.preview_lines.is_empty() && pane_area.height > 2 {
        // Content area = pane minus status bar (last line)
        let content_area = Rect::new(
            pane_area.x,
            pane_area.y,
            pane_area.width,
            pane_area.height.saturating_sub(1),
        );
        let bg = RStyle::default().bg(theme.background());
        f.render_widget(Clear, content_area);
        f.render_widget(ratatui::widgets::Block::default().style(bg), content_area);

        let mut preview_lines: Vec<Line> = Vec::new();
        for dl in &strip.preview_lines {
            let prefix = match dl.kind {
                bloom_core::render::DiffLineKind::Context => "  ",
                bloom_core::render::DiffLineKind::Added => "+ ",
                bloom_core::render::DiffLineKind::Removed => "- ",
            };
            let mut spans: Vec<Span> = Vec::new();
            let prefix_style = match dl.kind {
                bloom_core::render::DiffLineKind::Context => {
                    RStyle::default().fg(theme.faded()).bg(theme.background())
                }
                bloom_core::render::DiffLineKind::Added => {
                    RStyle::default().fg(theme.accent_green()).bg(theme.background())
                }
                bloom_core::render::DiffLineKind::Removed => {
                    RStyle::default().fg(theme.accent_red()).bg(theme.background())
                }
            };
            spans.push(Span::styled(prefix, prefix_style));

            for seg in &dl.segments {
                let style = match seg.kind {
                    bloom_core::render::DiffLineKind::Context => {
                        RStyle::default().fg(theme.faded()).bg(theme.background())
                    }
                    bloom_core::render::DiffLineKind::Added => {
                        RStyle::default().fg(theme.accent_green()).bg(theme.background())
                    }
                    bloom_core::render::DiffLineKind::Removed => {
                        RStyle::default()
                            .fg(theme.accent_red())
                            .bg(theme.background())
                            .add_modifier(Modifier::CROSSED_OUT)
                    }
                };
                spans.push(Span::styled(&seg.text, style));
            }
            preview_lines.push(Line::from(spans));
        }
        f.render_widget(Paragraph::new(preview_lines).style(bg), content_area);
    }

    // --- Draw strip in the drawer area (below status bar) ---
    let strip_bg = RStyle::default().bg(theme.highlight());
    f.render_widget(Clear, strip_area);
    f.render_widget(ratatui::widgets::Block::default().style(strip_bg), strip_area);

    let faded = RStyle::default().fg(theme.faded()).bg(theme.highlight());
    let bright = RStyle::default().fg(theme.foreground()).bg(theme.highlight());
    let accent = RStyle::default().fg(theme.accent_yellow()).bg(theme.highlight());
    let width = strip_area.width as usize;

    // Horizontal scrolling: fixed node width, viewport centered on selected
    let node_w: usize = if strip.compact { 12 } else { 16 };
    let visible_count = (width.saturating_sub(4)) / node_w.max(1);
    let total = strip.items.len();
    let half = visible_count / 2;
    let viewport_start = if strip.selected <= half {
        0
    } else if strip.selected + half >= total {
        total.saturating_sub(visible_count)
    } else {
        strip.selected - half
    };
    let viewport_end = (viewport_start + visible_count).min(total);

    let version_count = total;
    let first_label = strip.items.first().map(|n| n.label.as_str()).unwrap_or("");
    let last_label = strip.items.last().map(|n| n.label.as_str()).unwrap_or("");

    // --- Line 1: Title bar ---
    let title_text = if strip.compact {
        format!("├─ {} ", strip.title)
    } else {
        format!("├─ {} ── {} versions ── {}–{} ", strip.title, version_count, first_label, last_label)
    };
    f.render_widget(
        Paragraph::new(vec![Line::from(Span::styled(
            super::truncate_to_width(&title_text, width), faded,
        ))]).style(strip_bg),
        Rect::new(strip_area.x, strip_area.y, strip_area.width, 1),
    );

    // --- Line 2: Timeline nodes (scrollable) ---
    let mut node_spans: Vec<Span> = Vec::new();
    node_spans.push(Span::styled("│ ", faded));
    for i in viewport_start..viewport_end {
        let node = &strip.items[i];
        let is_selected = i == strip.selected;
        let marker = if node.branch_count > 1 { "[●]" } else {
            match node.kind {
                StripNodeKind::UndoNode => "●",
                StripNodeKind::GitCommit => "○",
            }
        };
        let node_style = if is_selected {
            RStyle::default().fg(theme.foreground()).bg(theme.highlight()).add_modifier(Modifier::BOLD)
        } else {
            match node.kind {
                StripNodeKind::UndoNode => bright,
                StripNodeKind::GitCommit => faded,
            }
        };
        let label = super::truncate_to_width(&node.label, node_w.saturating_sub(4));
        let cell = if is_selected {
            format!("▸{} {}", marker, label)
        } else {
            format!(" {} {}", marker, label)
        };
        node_spans.push(Span::styled(format!("{:<w$}", cell, w = node_w), node_style));
    }
    f.render_widget(
        Paragraph::new(vec![Line::from(node_spans)]).style(strip_bg),
        Rect::new(strip_area.x, strip_area.y + 1, strip_area.width, 1),
    );

    // --- Line 3: Cursor + selected description ---
    let selected_desc = strip.items.get(strip.selected)
        .and_then(|n| n.detail.as_deref()).unwrap_or("");
    let sel_visual = strip.selected.saturating_sub(viewport_start);
    let arrow_pad = 2 + sel_visual * node_w;
    let mut desc_str = String::new();
    desc_str.push_str("│ ");
    for _ in 0..arrow_pad.saturating_sub(2) { desc_str.push(' '); }
    desc_str.push_str("▲ ");
    desc_str.push_str(&super::truncate_to_width(selected_desc, width.saturating_sub(arrow_pad + 4)));
    f.render_widget(
        Paragraph::new(vec![Line::from(Span::styled(
            super::truncate_to_width(&desc_str, width), accent,
        ))]).style(strip_bg),
        Rect::new(strip_area.x, strip_area.y + 2, strip_area.width, 1),
    );

    // --- Rich mode: line 4 (all visible descriptions) and line 5 (stat) ---
    if !strip.compact && strip_area.height >= 6 {
        let mut desc_spans: Vec<Span> = Vec::new();
        desc_spans.push(Span::styled("│ ", faded));
        for i in viewport_start..viewport_end {
            let node = &strip.items[i];
            let detail = node.detail.as_deref().unwrap_or("");
            let truncated = super::truncate_to_width(detail, node_w.saturating_sub(2));
            let style = if i == strip.selected { bright } else { faded };
            desc_spans.push(Span::styled(format!("{:<w$}", truncated, w = node_w), style));
        }
        f.render_widget(
            Paragraph::new(vec![Line::from(desc_spans)]).style(strip_bg),
            Rect::new(strip_area.x, strip_area.y + 3, strip_area.width, 1),
        );
        // Line 5: empty/stat placeholder
        f.render_widget(
            Paragraph::new(vec![Line::from(Span::styled("│", faded))]).style(strip_bg),
            Rect::new(strip_area.x, strip_area.y + 4, strip_area.width, 1),
        );
    }

    // --- Last line: key hints ---
    let hints = match strip.mode {
        bloom_core::render::TemporalMode::PageHistory
        | bloom_core::render::TemporalMode::BlockHistory => "h/l:scrub  e:detail  r:restore  d:diff  q:close",
        bloom_core::render::TemporalMode::DayActivity => "h/l:scrub  e:detail  Enter:page  q:close",
    };
    f.render_widget(
        Paragraph::new(vec![Line::from(Span::styled(format!("├─ {} ", hints), faded))]).style(strip_bg),
        Rect::new(strip_area.x, strip_area.y + strip_area.height.saturating_sub(1), strip_area.width, 1),
    );
}
