use bloom_core::render::{
    CommandLineSlot, CursorShape, LineSource, McpIndicator, NormalStatus, PageHistoryFrame,
    PaneFrame, PaneKind, PaneRectFrame, QuickCaptureSlot, StatusBarContent, TimelineFrame,
    UndoTreeFrame,
};
use bloom_md::theme::ThemePalette;
use iced::{Color, Rectangle};

use crate::draw::{
    chars_that_fit, draw_bar_cursor, draw_text, draw_text_right, fill_rect, rect, text_width,
    truncate_text,
};
use crate::theme::{rgb_to_color, style_to_bg, style_to_color};
use crate::{CHAR_WIDTH, GUTTER_CHARS, GUTTER_WIDTH, LINE_HEIGHT, STATUS_BAR_HEIGHT};

/// Extra pixels the GUI status bar adds beyond what core allocates (1 cell row).
const STATUS_BAR_EXTRA: f32 = STATUS_BAR_HEIGHT - LINE_HEIGHT;

/// Convert a cell-based PaneRectFrame to pixel coordinates.
/// `status_bars_above` is the number of panes whose status bars are stacked
/// above this pane's Y origin (accounts for the taller GUI status bar).
pub(crate) fn pane_pixel_rect(
    r: &PaneRectFrame,
    status_bars_above: usize,
    window_size: iced::Size,
) -> (f32, f32, f32, f32) {
    let pane_x = r.x as f32 * CHAR_WIDTH;
    let pane_y = r.y as f32 * LINE_HEIGHT + status_bars_above as f32 * STATUS_BAR_EXTRA;
    let grid_w = r.width as f32 * CHAR_WIDTH;
    // Extend rightmost pane to window pixel edge.
    let pane_w = if pane_x + grid_w + CHAR_WIDTH >= window_size.width {
        window_size.width - pane_x
    } else {
        grid_w
    };
    let content_h = r.content_height as f32 * LINE_HEIGHT;
    (pane_x, pane_y, pane_w, content_h)
}

/// Draw a pane. `anim` is `Some((cursor_y, highlight_y))` in absolute pixels
/// for the active pane (smooth animated positions), or `None` for inactive panes.
pub(crate) fn draw_pane(
    frame: &mut iced::widget::canvas::Frame,
    pane: &PaneFrame,
    theme: &ThemePalette,
    anim: Option<(f32, f32)>,
    cursor_visible: bool,
    pane_x: f32,
    pane_y: f32,
    pane_w: f32,
    content_h: f32,
) {
    let content_rect = rect(pane_x, pane_y, pane_w, content_h);

    fill_rect(frame, content_rect, rgb_to_color(&theme.background));

    match &pane.kind {
        PaneKind::Editor => {
            draw_editor_content(frame, pane, theme, anim, cursor_visible, pane_x, pane_y, pane_w)
        }
        PaneKind::UndoTree(undo_tree) => draw_undo_tree(frame, content_rect, undo_tree, theme),
        PaneKind::Timeline(timeline) => draw_timeline(frame, content_rect, timeline, theme),
        PaneKind::PageHistory(page_history) => {
            draw_page_history(frame, content_rect, page_history, theme)
        }
        PaneKind::SetupWizard(_) => {}
    }

    if pane.is_active {
        draw_active_status_bar(frame, pane, theme, pane_x, pane_y, pane_w);
    } else {
        draw_inactive_status_bar(frame, pane, theme, pane_x, pane_y, pane_w);
    }
}

fn draw_editor_content(
    frame: &mut iced::widget::canvas::Frame,
    pane: &PaneFrame,
    theme: &ThemePalette,
    anim: Option<(f32, f32)>,
    cursor_visible: bool,
    pane_x: f32,
    pane_y: f32,
    pane_w: f32,
) {
    let content_chars = pane.rect.width as usize;
    let text_chars = content_chars.saturating_sub(GUTTER_CHARS);

    let logical_cursor_row = pane.cursor.line.saturating_sub(pane.scroll_offset);

    // Use animated Y for highlight if available, otherwise logical.
    if pane.is_active {
        let hl_y = anim.map(|(_, hl)| hl).unwrap_or(pane_y + logical_cursor_row as f32 * LINE_HEIGHT);
        fill_rect(
            frame,
            rect(pane_x, hl_y, pane_w, LINE_HEIGHT),
            rgb_to_color(&theme.highlight),
        );
    }

    for (i, line) in pane.visible_lines.iter().enumerate() {
        let y = pane_y + i as f32 * LINE_HEIGHT;

        match line.source {
            LineSource::Buffer(buf_line) => {
                let num_str = format!("{:>width$}", buf_line + 1, width = GUTTER_CHARS - 1);
                let gutter_color = if pane.is_active
                    && line.is_mirror
                    && i == pane.cursor.line.saturating_sub(pane.scroll_offset)
                {
                    rgb_to_color(&theme.salient)
                } else {
                    rgb_to_color(&theme.faded)
                };
                draw_text(frame, pane_x, y, num_str, gutter_color);
            }
            LineSource::BeyondEof => {
                draw_text(
                    frame,
                    pane_x + CHAR_WIDTH,
                    y,
                    "~",
                    rgb_to_color(&theme.faded),
                );
            }
        }

        let text_x = pane_x + GUTTER_WIDTH;
        let line_text = line.text.trim_end_matches(['\n', '\r']);
        let visible_text = truncate_text(line_text, text_chars);

        if line.spans.is_empty() {
            draw_text(
                frame,
                text_x,
                y,
                visible_text,
                rgb_to_color(&theme.foreground),
            );
        } else {
            for span in &line.spans {
                let start = span.range.start.min(visible_text.len());
                let end = span.range.end.min(visible_text.len());
                if start >= end {
                    continue;
                }

                let slice = &visible_text[start..end];
                let span_x = start as f32 * CHAR_WIDTH;
                let span_w = slice.chars().count() as f32 * CHAR_WIDTH;

                // Background wash for styles that need it (Code, LinkText, SearchMatch, etc.)
                if let Some(bg) = style_to_bg(&span.style, theme) {
                    fill_rect(frame, rect(text_x + span_x, y, span_w, LINE_HEIGHT), bg);
                }

                draw_text(
                    frame,
                    text_x + span_x,
                    y,
                    slice.to_string(),
                    style_to_color(&span.style, theme),
                );
            }
        }
    }

    if pane.is_active && cursor_visible {
        let cx = pane_x + GUTTER_WIDTH + pane.cursor.column as f32 * CHAR_WIDTH;
        let cy = anim.map(|(c, _)| c).unwrap_or(
            pane_y + pane.cursor.line.saturating_sub(pane.scroll_offset) as f32 * LINE_HEIGHT,
        );

        match pane.cursor.shape {
            CursorShape::Block => fill_rect(
                frame,
                rect(cx, cy, CHAR_WIDTH, LINE_HEIGHT),
                Color {
                    a: 0.45,
                    ..rgb_to_color(&theme.foreground)
                },
            ),
            CursorShape::Bar => draw_bar_cursor(frame, cx, cy, rgb_to_color(&theme.foreground)),
            CursorShape::Underline => fill_rect(
                frame,
                rect(cx, cy + LINE_HEIGHT - 2.0, CHAR_WIDTH, 2.0),
                rgb_to_color(&theme.foreground),
            ),
        }
    }
}

fn draw_active_status_bar(
    frame: &mut iced::widget::canvas::Frame,
    pane: &PaneFrame,
    theme: &ThemePalette,
    pane_x: f32,
    pane_y: f32,
    pane_w: f32,
) {
    let status_y = pane_y + pane.rect.content_height as f32 * LINE_HEIGHT;
    let text_y = status_y + (STATUS_BAR_HEIGHT - LINE_HEIGHT) / 2.0;

    // Status bar background — always `highlight`, not mode-coloured.
    // Only the mode badge gets the mode colour.
    fill_rect(
        frame,
        rect(pane_x, status_y, pane_w, STATUS_BAR_HEIGHT),
        rgb_to_color(&theme.highlight),
    );
    // Top border for visual anchor.
    crate::draw::draw_hline(
        frame,
        pane_x,
        pane_x + pane_w,
        status_y,
        rgb_to_color(&theme.faded),
    );

    match &pane.status_bar.content {
        StatusBarContent::Normal(normal) => {
            draw_normal_status(frame, pane, normal, theme, pane_x, text_y, pane_w, status_y)
        }
        StatusBarContent::CommandLine(command) => {
            draw_command_line(frame, command, theme, pane_x, pane_y, pane_w, text_y)
        }
        StatusBarContent::QuickCapture(capture) => {
            draw_quick_capture(frame, capture, theme, pane_x, pane_w, text_y)
        }
    }
}

fn draw_inactive_status_bar(
    frame: &mut iced::widget::canvas::Frame,
    pane: &PaneFrame,
    theme: &ThemePalette,
    pane_x: f32,
    pane_y: f32,
    pane_w: f32,
) {
    let status_y = pane_y + pane.rect.content_height as f32 * LINE_HEIGHT;
    fill_rect(
        frame,
        rect(pane_x, status_y, pane_w, STATUS_BAR_HEIGHT),
        rgb_to_color(&theme.subtle),
    );

    let text_y = status_y + (STATUS_BAR_HEIGHT - LINE_HEIGHT) / 2.0;
    let title = truncate_text(&pane.title, chars_that_fit((pane_w - CHAR_WIDTH).max(0.0)));
    draw_text(
        frame,
        pane_x + CHAR_WIDTH / 2.0,
        text_y,
        format!(" {title}"),
        rgb_to_color(&theme.faded),
    );
}

fn draw_normal_status(
    frame: &mut iced::widget::canvas::Frame,
    pane: &PaneFrame,
    normal: &NormalStatus,
    theme: &ThemePalette,
    pane_x: f32,
    status_y: f32,
    pane_w: f32,
    bar_y: f32,
) {
    // Mode badge with coloured background per THEMING.md.
    let mode = &pane.status_bar.mode;
    let (badge_fg, badge_bg) = match mode.as_str() {
        "INSERT" => (rgb_to_color(&theme.background), rgb_to_color(&theme.accent_green)),
        "VISUAL" => (rgb_to_color(&theme.background), rgb_to_color(&theme.popout)),
        "COMMAND" => (rgb_to_color(&theme.background), rgb_to_color(&theme.accent_blue)),
        "HIST" | "DAY" | "JRNL" => (rgb_to_color(&theme.background), rgb_to_color(&theme.accent_yellow)),
        _ => (rgb_to_color(&theme.foreground), rgb_to_color(&theme.mild)),
    };
    let badge_text = format!(" {} ", mode);
    let badge_w = badge_text.chars().count() as f32 * CHAR_WIDTH;
    fill_rect(frame, rect(pane_x, bar_y, badge_w, STATUS_BAR_HEIGHT), badge_bg);
    draw_text(frame, pane_x, status_y, &badge_text, badge_fg);

    // Title + dirty flag after the badge.
    let dirty = if normal.dirty { " [+]" } else { "" };
    let after_badge_x = pane_x + badge_w + CHAR_WIDTH;
    let title_text = format!("{}{}", normal.title, dirty);
    let title_max = chars_that_fit((pane_w - badge_w - 2.0 * CHAR_WIDTH).max(0.0));
    draw_text(
        frame,
        after_badge_x,
        status_y,
        truncate_text(&title_text, title_max),
        rgb_to_color(&theme.foreground),
    );

    // Dirty indicator in salient.
    if normal.dirty {
        let dirty_x = after_badge_x + (normal.title.chars().count()) as f32 * CHAR_WIDTH;
        draw_text(frame, dirty_x, status_y, " [+]", rgb_to_color(&theme.salient));
    }

    // Right-aligned elements — each drawn individually with its spec color.
    if let Some(hints) = &pane.status_bar.right_hints {
        draw_text_right(
            frame,
            pane_x + pane_w - CHAR_WIDTH,
            status_y,
            &truncate_text(hints, chars_that_fit((pane_w * 0.35).max(0.0))),
            rgb_to_color(&theme.faded),
        );
    } else {
        // Build segments with individual colors, draw right-to-left.
        let mut segments: Vec<(&str, String, Color)> = Vec::new();

        // Line:col — faded (rightmost).
        segments.push(("linecol", format!("{}:{}", normal.line + 1, normal.column + 1), rgb_to_color(&theme.faded)));

        // MCP indicator — faded when idle, salient when editing.
        match &normal.mcp {
            McpIndicator::Off => {}
            McpIndicator::Idle => {
                segments.push(("mcp", "⚡".to_string(), rgb_to_color(&theme.faded)));
            }
            McpIndicator::Editing { tick } => {
                const FRAMES: &[&str] = &["⚡", "◐", "◑", "◒", "◓"];
                segments.push(("mcp", FRAMES[(*tick as usize) % FRAMES.len()].to_string(), rgb_to_color(&theme.salient)));
            }
        }

        // Indexer — salient when active.
        if normal.indexing {
            segments.push(("indexer", "⟳".to_string(), rgb_to_color(&theme.salient)));
        }

        // Pending keys — salient.
        if !normal.pending_keys.is_empty() {
            segments.push(("pending", normal.pending_keys.clone(), rgb_to_color(&theme.salient)));
        }

        // Macro recording — accent_red.
        if let Some(recording) = normal.recording_macro {
            segments.push(("macro", format!("@{recording}"), rgb_to_color(&theme.accent_red)));
        }

        // Draw right-to-left: last segment rightmost.
        let gap = 2.0 * CHAR_WIDTH;
        let right_edge = pane_x + pane_w - CHAR_WIDTH;
        let mut cursor_x = right_edge;
        for (_id, text, color) in &segments {
            let w = text_width(text);
            cursor_x -= w;
            draw_text(frame, cursor_x, status_y, text, *color);
            cursor_x -= gap;
        }
    }
}

fn draw_command_line(
    frame: &mut iced::widget::canvas::Frame,
    command: &CommandLineSlot,
    theme: &ThemePalette,
    pane_x: f32,
    pane_y: f32,
    pane_w: f32,
    status_y: f32,
) {
    let input_max = chars_that_fit((pane_w - CHAR_WIDTH).max(0.0));
    let prefix = format!(
        ":{}",
        truncate_text(&command.input, input_max.saturating_sub(1))
    );
    draw_text(
        frame,
        pane_x,
        status_y,
        &prefix,
        rgb_to_color(&theme.foreground),
    );

    if let Some(ghost_text) = &command.ghost_text {
        let ghost_x = pane_x + text_width(&prefix);
        let remaining = chars_that_fit((pane_x + pane_w - ghost_x).max(0.0));
        draw_text(
            frame,
            ghost_x,
            status_y,
            truncate_text(ghost_text, remaining),
            rgb_to_color(&theme.faded),
        );
    }

    if let Some(error) = &command.error {
        let error_y = (status_y - LINE_HEIGHT).max(pane_y);
        draw_text(
            frame,
            pane_x,
            error_y,
            truncate_text(error, input_max),
            rgb_to_color(&theme.critical),
        );
    }

    let cursor_x = (pane_x + (1 + command.cursor_pos) as f32 * CHAR_WIDTH)
        .min(pane_x + pane_w - 2.0)
        .max(pane_x);
    draw_bar_cursor(frame, cursor_x, status_y, rgb_to_color(&theme.foreground));
}

fn draw_quick_capture(
    frame: &mut iced::widget::canvas::Frame,
    capture: &QuickCaptureSlot,
    theme: &ThemePalette,
    pane_x: f32,
    pane_w: f32,
    status_y: f32,
) {
    let content = format!("{}{}", capture.prompt, capture.input);
    draw_text(
        frame,
        pane_x,
        status_y,
        truncate_text(&content, chars_that_fit((pane_w - CHAR_WIDTH).max(0.0))),
        rgb_to_color(&theme.foreground),
    );

    let prompt_chars = capture.prompt.chars().count();
    let cursor_x = (pane_x + (prompt_chars + capture.cursor_pos) as f32 * CHAR_WIDTH)
        .min(pane_x + pane_w - 2.0)
        .max(pane_x);
    draw_bar_cursor(frame, cursor_x, status_y, rgb_to_color(&theme.foreground));
}

fn draw_timeline(
    frame: &mut iced::widget::canvas::Frame,
    area: Rectangle,
    timeline: &TimelineFrame,
    theme: &ThemePalette,
) {
    let max_chars = chars_that_fit(area.width).saturating_sub(2);
    draw_text(
        frame,
        area.x,
        area.y,
        truncate_text(&format!("  Timeline: {}", timeline.target_title), max_chars),
        rgb_to_color(&theme.salient),
    );

    let mut row = 1usize;
    for (index, entry) in timeline.entries.iter().enumerate() {
        let y = area.y + row as f32 * LINE_HEIGHT;
        if y + LINE_HEIGHT > area.y + area.height {
            break;
        }
        let selected = index == timeline.selected_index;
        if selected {
            fill_rect(
                frame,
                rect(area.x, y, area.width, LINE_HEIGHT),
                rgb_to_color(&theme.mild),
            );
        }
        let header = format!("  {} · {}", entry.date.format("%b %d"), entry.source_title);
        draw_text(
            frame,
            area.x,
            y,
            truncate_text(&header, max_chars),
            rgb_to_color(if selected {
                &theme.strong
            } else {
                &theme.foreground
            }),
        );
        row += 1;

        if entry.expanded {
            let y = area.y + row as f32 * LINE_HEIGHT;
            if y + LINE_HEIGHT > area.y + area.height {
                break;
            }
            draw_text(
                frame,
                area.x,
                y,
                truncate_text(&format!("    {}", entry.context), max_chars),
                rgb_to_color(&theme.faded),
            );
            row += 1;
        }
    }
}

fn draw_page_history(
    frame: &mut iced::widget::canvas::Frame,
    area: Rectangle,
    history: &PageHistoryFrame,
    theme: &ThemePalette,
) {
    let max_chars = chars_that_fit(area.width).saturating_sub(1);
    draw_text(
        frame,
        area.x,
        area.y,
        truncate_text(
            &format!(
                " {} — History ({} versions)",
                history.page_title, history.total_versions
            ),
            max_chars,
        ),
        rgb_to_color(&theme.strong),
    );

    let mut row = 2usize;
    for (index, entry) in history.entries.iter().enumerate() {
        let y = area.y + row as f32 * LINE_HEIGHT;
        if y + LINE_HEIGHT > area.y + area.height {
            break;
        }
        let selected = index == history.selected_index;
        if selected {
            fill_rect(
                frame,
                rect(area.x, y, area.width, LINE_HEIGHT),
                rgb_to_color(&theme.mild),
            );
        }

        let left = format!(" {} {}", if selected { "▸" } else { " " }, entry.date);
        let right_width = text_width(&entry.diff_stat);
        draw_text(
            frame,
            area.x,
            y,
            truncate_text(&left, max_chars),
            rgb_to_color(if selected {
                &theme.strong
            } else {
                &theme.foreground
            }),
        );
        draw_text_right(
            frame,
            area.x + area.width - CHAR_WIDTH / 2.0,
            y,
            &entry.diff_stat,
            rgb_to_color(if selected {
                &theme.foreground
            } else {
                &theme.faded
            }),
        );

        let desc_y = area.y + (row + 1) as f32 * LINE_HEIGHT;
        if desc_y + LINE_HEIGHT > area.y + area.height {
            break;
        }
        let desc_max = chars_that_fit((area.width - right_width).max(0.0));
        draw_text(
            frame,
            area.x,
            desc_y,
            truncate_text(&format!("     {}", entry.description), desc_max),
            rgb_to_color(if selected {
                &theme.foreground
            } else {
                &theme.faded
            }),
        );
        row += 3;
    }
}

fn draw_undo_tree(
    frame: &mut iced::widget::canvas::Frame,
    area: Rectangle,
    undo_tree: &UndoTreeFrame,
    theme: &ThemePalette,
) {
    let max_chars = chars_that_fit(area.width).saturating_sub(2);
    let mut row = 0usize;

    for node in &undo_tree.nodes {
        let y = area.y + row as f32 * LINE_HEIGHT;
        if y + LINE_HEIGHT > area.y + area.height {
            break;
        }
        let selected = node.id == undo_tree.selected;
        if selected {
            fill_rect(
                frame,
                rect(area.x, y, area.width, LINE_HEIGHT),
                rgb_to_color(&theme.mild),
            );
        }
        let indent = "  ".repeat(node.depth);
        let marker = if node.is_current { "●" } else { "○" };
        let line = format!("  {indent}{marker} {}", node.description);
        let color = if selected {
            &theme.strong
        } else if node.is_current {
            &theme.salient
        } else {
            &theme.foreground
        };
        draw_text(
            frame,
            area.x,
            y,
            truncate_text(&line, max_chars),
            rgb_to_color(color),
        );
        row += 1;
    }

    if let Some(preview) = &undo_tree.preview {
        row += 1;
        for preview_line in preview.lines() {
            let y = area.y + row as f32 * LINE_HEIGHT;
            if y + LINE_HEIGHT > area.y + area.height {
                break;
            }
            draw_text(
                frame,
                area.x,
                y,
                truncate_text(&format!("  {preview_line}"), max_chars),
                rgb_to_color(&theme.faded),
            );
            row += 1;
        }
    }
}
