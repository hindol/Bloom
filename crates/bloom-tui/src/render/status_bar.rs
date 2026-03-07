use super::*;

/// Renders the global status bar. Returns cursor position if the active slot
/// needs the cursor (command line, quick capture).
pub(super) fn draw_status_bar_slot(
    f: &mut Frame,
    area: Rect,
    sb: &StatusBarFrame,
    theme: &TuiTheme,
) -> Option<(u16, u16)> {
    match &sb.content {
        StatusBarContent::Normal(status) => {
            draw_normal_status(f, area, &sb.mode, status, theme);
            None
        }
        StatusBarContent::CommandLine(cmd) => {
            let style = RStyle::default()
                .fg(theme.foreground())
                .bg(theme.background());
            let text = format!(":{}", cmd.input);
            f.render_widget(Paragraph::new(Line::from(Span::styled(&text, style))), area);

            // Error display: overwrite the last pane line above status bar
            if let Some(err) = &cmd.error {
                let err_y = area.y.saturating_sub(1);
                let err_style = RStyle::default()
                    .fg(theme.critical())
                    .bg(theme.background());
                f.render_widget(
                    Paragraph::new(Line::from(Span::styled(err, err_style))),
                    Rect::new(area.x, err_y, area.width, 1),
                );
            }

            let cx = (area.x + 1 + cmd.cursor_pos as u16).min(area.right().saturating_sub(1));
            Some((cx, area.y))
        }
        StatusBarContent::QuickCapture(qc) => {
            let style = RStyle::default()
                .fg(theme.foreground())
                .bg(theme.modeline());
            let text = format!("{}{}", qc.prompt, qc.input);
            f.render_widget(Paragraph::new(Line::from(Span::styled(&text, style))), area);

            let cx = (area.x + qc.prompt.width() as u16 + qc.cursor_pos as u16)
                .min(area.right().saturating_sub(1));
            Some((cx, area.y))
        }
    }
}

/// Render the normal status bar with per-element typographic weights.
fn draw_normal_status(
    f: &mut Frame,
    area: Rect,
    mode: &str,
    status: &bloom_core::render::NormalStatus,
    theme: &TuiTheme,
) {
    let bar_bg = theme.highlight();
    let base_style = theme.status_bar_style(mode, true);
    let width = area.width as usize;

    // Fill background
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(" ".repeat(width), base_style))),
        area,
    );

    // --- Build right-side spans with individual weights ---
    let mut right_spans: Vec<Span> = Vec::new();
    right_spans.push(Span::raw(" "));

    if let Some(reg) = status.recording_macro {
        // Macro: accent_red — recording state, visually distinct
        let macro_style = RStyle::default().fg(theme.accent_red()).bg(bar_bg);
        right_spans.push(Span::styled(format!("@{reg}"), macro_style));
        right_spans.push(Span::raw("  "));
    }
    if !status.pending_keys.is_empty() {
        // Pending keys: salient + bold — transient but important
        let pending_style = RStyle::default()
            .fg(theme.salient())
            .bg(bar_bg)
            .add_modifier(Modifier::BOLD);
        right_spans.push(Span::styled(status.pending_keys.clone(), pending_style));
        right_spans.push(Span::raw("  "));
    }

    // Indexer indicator: visible only while indexing
    if status.indexing {
        let idx_style = RStyle::default().fg(theme.salient()).bg(bar_bg);
        right_spans.push(Span::styled("⟳", idx_style));
        right_spans.push(Span::raw("  "));
    }

    let mcp_animating = matches!(&status.mcp, McpIndicator::Editing { .. });
    let mcp_str = match &status.mcp {
        McpIndicator::Off => String::new(),
        McpIndicator::Idle => "\u{26a1}".to_string(),
        McpIndicator::Editing { tick } => {
            const FRAMES: &[&str] = &["\u{26a1}", "\u{25d0}", "\u{25d1}", "\u{25d2}", "\u{25d3}"];
            FRAMES[(*tick as usize) % FRAMES.len()].to_string()
        }
    };
    if !mcp_str.is_empty() {
        // MCP: faded when idle, salient when animating
        let mcp_fg = if mcp_animating {
            theme.salient()
        } else {
            theme.faded()
        };
        let mcp_style = RStyle::default().fg(mcp_fg).bg(bar_bg);
        right_spans.push(Span::styled(mcp_str, mcp_style));
        right_spans.push(Span::raw("  "));
    }

    // Line:col — faded, reference info
    let pos_style = RStyle::default().fg(theme.faded()).bg(bar_bg);
    right_spans.push(Span::styled(
        format!("{}:{}", status.line + 1, status.column + 1),
        pos_style,
    ));
    right_spans.push(Span::styled("  ", RStyle::default().bg(bar_bg)));

    let right_width: usize = right_spans.iter().map(|s| s.content.width()).sum();

    // --- Build left-side spans with individual weights ---
    // Mode badge: bold, uses the mode-specific style (already has bg color)
    let mode_style = base_style.add_modifier(Modifier::BOLD);
    let mode_text = format!(" {} ", mode);

    // Separator: faded on bar bg
    let sep_style = RStyle::default().fg(theme.faded()).bg(bar_bg);

    // Title: foreground on bar bg, normal weight
    let title_style = RStyle::default().fg(theme.foreground()).bg(bar_bg);

    // Dirty: salient on bar bg — needs attention
    let dirty_style = RStyle::default().fg(theme.salient()).bg(bar_bg);

    let dirty_mark = if status.dirty { " [+]" } else { "" };
    let title_max = width
        .saturating_sub(mode_text.width())
        .saturating_sub(3) // " │ "
        .saturating_sub(dirty_mark.width())
        .saturating_sub(right_width);
    let title = truncate_with_ellipsis(&status.title, title_max);

    let mut left_spans: Vec<Span> = Vec::new();
    left_spans.push(Span::styled(&mode_text, mode_style));
    left_spans.push(Span::styled(" \u{2502} ", sep_style));
    left_spans.push(Span::styled(title, title_style));
    if status.dirty {
        left_spans.push(Span::styled(dirty_mark, dirty_style));
    }

    let left_width: usize = left_spans.iter().map(|s| s.content.width()).sum();

    // Render left
    f.render_widget(Paragraph::new(Line::from(left_spans)), area);

    // Render right
    let rx = area.right().saturating_sub(right_width as u16);
    if rx > area.x + left_width as u16 {
        f.render_widget(
            Paragraph::new(Line::from(right_spans)),
            Rect::new(rx, area.y, right_width as u16, 1),
        );
    }
}
