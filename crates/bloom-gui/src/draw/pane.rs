use bloom_core::render::{
    CommandLineSlot, CursorShape, DashboardFrame, LineSource, McpIndicator, NormalStatus,
    PageHistoryFrame, PaneFrame, PaneKind, PaneRectFrame, QuickCaptureSlot, StatusBarContent,
    Style, TimelineFrame, UndoTreeFrame,
};
use bloom_md::theme::ThemePalette;
use iced::{Color, Rectangle};

use crate::draw::{
    byte_offset_to_char_col, chars_that_fit, draw_bar_cursor, draw_text, draw_text_center,
    draw_text_right, draw_text_sized, fill_rect, rect, text_width, truncate_text,
};
use crate::theme::{rgb_to_color, style_to_bg, style_to_color};
use crate::{
    CHAR_WIDTH, FONT_SIZE, GUTTER_CHARS, GUTTER_WIDTH, LINE_HEIGHT, MODELINE_H_PAD, SPACING_MD,
    SPACING_SM, STATUS_BAR_HEIGHT,
};

/// Extra pixels the GUI status bar adds beyond what core allocates (1 cell row).
const STATUS_BAR_EXTRA: f32 = STATUS_BAR_HEIGHT - LINE_HEIGHT;

pub(crate) fn heading_font_size(level: u8) -> f32 {
    match level {
        1 => FONT_SIZE * 1.5,
        2 => FONT_SIZE * 1.3,
        3 => FONT_SIZE * 1.1,
        _ => FONT_SIZE,
    }
}

/// Row height for a line — taller for headings.
pub(crate) fn line_row_height(line: &bloom_core::render::RenderedLine) -> f32 {
    line.spans
        .iter()
        .find_map(|s| match s.style {
            Style::Heading { level } => Some(heading_font_size(level) * 1.4),
            _ => None,
        })
        .unwrap_or(LINE_HEIGHT)
}

/// Compute the Y offset of a given visible line index, accounting for
/// variable row heights (headings are taller).
pub(crate) fn cursor_y_in_pane(
    visible_lines: &[bloom_core::render::RenderedLine],
    target_row: usize,
    pane_y: f32,
) -> f32 {
    let mut y = pane_y;
    for (i, line) in visible_lines.iter().enumerate() {
        if i >= target_row {
            break;
        }
        y += line_row_height(line);
    }
    y
}

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
    content_area: Rectangle,
    modeline_area: Rectangle,
) {
    fill_rect(frame, content_area, rgb_to_color(&theme.background));

    // Fill gap between content bottom and modeline top with background.
    let gap_y = content_area.y + content_area.height;
    let gap_h = modeline_area.y - gap_y;
    if gap_h > 0.5 {
        fill_rect(
            frame,
            rect(content_area.x, gap_y, content_area.width, gap_h),
            rgb_to_color(&theme.background),
        );
    }

    match &pane.kind {
        PaneKind::Editor => {
            draw_editor_content(frame, pane, theme, anim, cursor_visible, content_area)
        }
        PaneKind::UndoTree(undo_tree) => draw_undo_tree(frame, content_area, undo_tree, theme),
        PaneKind::Timeline(timeline) => draw_timeline(frame, content_area, timeline, theme),
        PaneKind::PageHistory(page_history) => {
            draw_page_history(frame, content_area, page_history, theme)
        }
        PaneKind::SetupWizard(_) => {}
        PaneKind::Dashboard(dashboard) => draw_dashboard(frame, content_area, dashboard, theme),
    }

    if pane.is_active {
        draw_active_status_bar(frame, pane, theme, modeline_area);
    } else {
        draw_inactive_status_bar(frame, pane, theme, modeline_area);
    }
}

fn draw_editor_content(
    frame: &mut iced::widget::canvas::Frame,
    pane: &PaneFrame,
    theme: &ThemePalette,
    anim: Option<(f32, f32)>,
    cursor_visible: bool,
    content_area: Rectangle,
) {
    let pane_x = content_area.x;
    let pane_y = content_area.y;
    let pane_w = content_area.width;
    let content_chars = pane.rect.width as usize;
    let text_chars = content_chars.saturating_sub(GUTTER_CHARS);

    // Pre-compute per-line Y offsets and heights (headings get taller rows).
    let mut line_ys: Vec<f32> = Vec::with_capacity(pane.visible_lines.len());
    let mut line_heights: Vec<f32> = Vec::with_capacity(pane.visible_lines.len());
    let mut y_acc = pane_y;
    for line in &pane.visible_lines {
        let h_level = line.spans.iter().find_map(|s| match s.style {
            Style::Heading { level } => Some(level),
            _ => None,
        });
        let row_h = match h_level {
            Some(level) => heading_font_size(level) * 1.4,
            None => LINE_HEIGHT,
        };
        line_ys.push(y_acc);
        line_heights.push(row_h);
        y_acc += row_h;
    }

    let logical_cursor_row = pane.cursor.line.saturating_sub(pane.scroll_offset);

    // Current line highlight.
    if pane.is_active {
        let hl_y = if let Some((_, hl)) = anim {
            hl
        } else {
            line_ys.get(logical_cursor_row).copied().unwrap_or(pane_y)
        };
        let hl_h = line_heights
            .get(logical_cursor_row)
            .copied()
            .unwrap_or(LINE_HEIGHT);
        fill_rect(
            frame,
            rect(pane_x, hl_y, pane_w, hl_h),
            rgb_to_color(&theme.highlight),
        );
    }

    for (i, line) in pane.visible_lines.iter().enumerate() {
        let y = line_ys[i];
        let row_h = line_heights[i];

        match line.source {
            LineSource::Buffer(buf_line) => {
                let num_str = format!("{:>width$}", buf_line + 1, width = GUTTER_CHARS - 1);
                let gutter_color = if pane.is_active
                    && line.is_mirror
                    && i == pane.cursor.line.saturating_sub(pane.scroll_offset)
                {
                    rgb_to_color(&theme.salient)
                } else {
                    // Blend faded toward background — softer than full faded,
                    // stays within the semantic theme system.
                    rgb_to_color(&theme.faded.blend(theme.background, 0.4))
                };
                draw_text_sized(frame, pane_x, y, num_str, gutter_color, FONT_SIZE, row_h);
            }
            LineSource::BeyondEof => {
                draw_text_sized(
                    frame,
                    pane_x + CHAR_WIDTH,
                    y,
                    "~",
                    rgb_to_color(&theme.faded.blend(theme.background, 0.4)),
                    FONT_SIZE,
                    row_h,
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
            let heading_level = line.spans.iter().find_map(|s| match s.style {
                Style::Heading { level } => Some(level),
                _ => None,
            });
            let line_font_size = heading_level.map(heading_font_size).unwrap_or(FONT_SIZE);
            // Uniform char width and font size for the entire line.
            // Heading lines use the scaled size for ALL spans (including block IDs).
            // This keeps width calculations simple and cursor positioning correct.
            let line_cw = CHAR_WIDTH * (line_font_size / FONT_SIZE);

            for span in &line.spans {
                let start = span.byte_range.start.min(line_text.len());
                let end = span.byte_range.end.min(line_text.len());
                if start >= end {
                    continue;
                }

                // Convert byte offsets to char column positions for rendering.
                let col_start = byte_offset_to_char_col(line_text, start);
                let col_end = byte_offset_to_char_col(line_text, end);

                // Skip spans entirely beyond the visible area.
                if col_start >= text_chars {
                    continue;
                }

                let slice = &line_text[start..end];

                // Truncate the slice if it extends past the visible column limit.
                let visible_col_end = col_end.min(text_chars);
                let visible_slice: String = if col_end > text_chars {
                    slice.chars().take(visible_col_end - col_start).collect()
                } else {
                    slice.to_string()
                };
                let span_chars = visible_col_end - col_start;

                let span_x = col_start as f32 * line_cw;
                let span_w = span_chars as f32 * line_cw;

                // Background wash for styles that need it.
                if let Some(bg) = style_to_bg(&span.style, theme) {
                    fill_rect(frame, rect(text_x + span_x, y, span_w, row_h), bg);
                }

                draw_text_sized(
                    frame,
                    text_x + span_x,
                    y,
                    visible_slice.clone(),
                    style_to_color(&span.style, theme),
                    line_font_size,
                    row_h,
                );

                // Strikethrough for checked task text (not the checkbox or block ID).
                if span.style == Style::CheckedTaskText {
                    let leading = visible_slice
                        .chars()
                        .take_while(|c| c.is_whitespace())
                        .count();
                    let trailing = visible_slice
                        .chars()
                        .rev()
                        .take_while(|c| c.is_whitespace())
                        .count();
                    let vis_chars = visible_slice.chars().count();
                    let content_chars = vis_chars.saturating_sub(leading).saturating_sub(trailing);
                    if content_chars > 0 {
                        let strike_start = text_x + span_x + leading as f32 * line_cw;
                        let strike_end = strike_start + content_chars as f32 * line_cw;
                        let strike_y = y + row_h / 2.0;
                        crate::draw::draw_hline(
                            frame,
                            strike_start,
                            strike_end,
                            strike_y,
                            style_to_color(&span.style, theme),
                        );
                    }
                }
            }
        }

        // Image placeholder: render a box with alt text for `![alt](path)` lines.
        if line_text.starts_with("![") {
            if let Some(alt_end) = line_text.find("](") {
                let alt = &line_text[2..alt_end];
                let box_w = pane_w - GUTTER_WIDTH;
                fill_rect(
                    frame,
                    rect(text_x, y, box_w, LINE_HEIGHT),
                    rgb_to_color(&theme.subtle),
                );
                draw_text(
                    frame,
                    text_x + CHAR_WIDTH,
                    y,
                    format!("\u{1F5BC} {alt}"),
                    rgb_to_color(&theme.faded),
                );
            }
        }
    }

    if pane.is_active && cursor_visible {
        let cursor_row_idx = pane.cursor.line.saturating_sub(pane.scroll_offset);
        let cursor_row_h = line_heights
            .get(cursor_row_idx)
            .copied()
            .unwrap_or(LINE_HEIGHT);

        // Simple cursor positioning — uniform font size per line.
        let (cx, cursor_cw) = if let Some(line) = pane.visible_lines.get(cursor_row_idx) {
            let h_level = line.spans.iter().find_map(|s| match s.style {
                Style::Heading { level } => Some(level),
                _ => None,
            });
            let cw = h_level
                .map(|l| CHAR_WIDTH * (heading_font_size(l) / FONT_SIZE))
                .unwrap_or(CHAR_WIDTH);
            (pane_x + GUTTER_WIDTH + pane.cursor.column as f32 * cw, cw)
        } else {
            (
                pane_x + GUTTER_WIDTH + pane.cursor.column as f32 * CHAR_WIDTH,
                CHAR_WIDTH,
            )
        };
        let cy = anim
            .map(|(c, _)| c)
            .unwrap_or(line_ys.get(cursor_row_idx).copied().unwrap_or(pane_y));

        match pane.cursor.shape {
            CursorShape::Block => {
                // Opaque block cursor — then redraw the character in background
                // colour so it's always readable (terminal-style inverse).
                fill_rect(
                    frame,
                    rect(cx, cy, cursor_cw, cursor_row_h),
                    rgb_to_color(&theme.foreground),
                );
                // Extract the character under the cursor and redraw it inverted.
                if let Some(line) = pane.visible_lines.get(cursor_row_idx) {
                    let line_text = line.text.trim_end_matches(['\n', '\r']);
                    if let Some(ch) = line_text.chars().nth(pane.cursor.column) {
                        let font_size = pane
                            .visible_lines
                            .get(cursor_row_idx)
                            .and_then(|l| {
                                l.spans.iter().find_map(|s| match s.style {
                                    Style::Heading { level } => Some(heading_font_size(level)),
                                    _ => None,
                                })
                            })
                            .unwrap_or(FONT_SIZE);
                        draw_text_sized(
                            frame,
                            cx,
                            cy,
                            ch.to_string(),
                            rgb_to_color(&theme.background),
                            font_size,
                            cursor_row_h,
                        );
                    }
                }
            }
            CursorShape::Bar => {
                draw_bar_cursor(frame, cx, cy, cursor_row_h, rgb_to_color(&theme.foreground))
            }
            CursorShape::Underline => fill_rect(
                frame,
                rect(cx, cy + cursor_row_h - 2.0, cursor_cw, 2.0),
                rgb_to_color(&theme.foreground),
            ),
        }
    }
}

fn draw_active_status_bar(
    frame: &mut iced::widget::canvas::Frame,
    pane: &PaneFrame,
    theme: &ThemePalette,
    modeline_area: Rectangle,
) {
    match &pane.status_bar.content {
        StatusBarContent::Normal(normal) => {
            draw_normal_status(frame, pane, normal, theme, modeline_area)
        }
        StatusBarContent::CommandLine(command) => {
            fill_rect(frame, modeline_area, rgb_to_color(&theme.highlight));
            draw_command_line(frame, command, theme, modeline_area)
        }
        StatusBarContent::QuickCapture(capture) => {
            fill_rect(frame, modeline_area, rgb_to_color(&theme.highlight));
            draw_quick_capture(frame, capture, theme, modeline_area)
        }
    }
}

fn draw_inactive_status_bar(
    frame: &mut iced::widget::canvas::Frame,
    pane: &PaneFrame,
    theme: &ThemePalette,
    modeline_area: Rectangle,
) {
    fill_rect(frame, modeline_area, rgb_to_color(&theme.subtle));

    let text_y = modeline_area.y + (modeline_area.height - LINE_HEIGHT) / 2.0;
    let max_chars = chars_that_fit((modeline_area.width - SPACING_MD * 2.0).max(0.0));
    let title = truncate_text(&pane.title, max_chars);
    draw_text(
        frame,
        modeline_area.x + SPACING_MD,
        text_y,
        &title,
        rgb_to_color(&theme.faded),
    );
}

fn draw_normal_status(
    frame: &mut iced::widget::canvas::Frame,
    pane: &PaneFrame,
    normal: &NormalStatus,
    theme: &ThemePalette,
    modeline_area: Rectangle,
) {
    let pane_x = modeline_area.x;
    let pane_w = modeline_area.width;
    let bar_y = modeline_area.y;
    let bar_h = modeline_area.height;
    let text_y = bar_y + (bar_h - LINE_HEIGHT) / 2.0;
    let h_pad = MODELINE_H_PAD;

    // ── 1. Mode segment (left-aligned, fixed width) ──
    let mode = &pane.status_bar.mode;
    let mode_text = format!(" {} ", mode);
    let mode_w = mode_text.chars().count() as f32 * CHAR_WIDTH + SPACING_SM * 2.0 + h_pad;
    let mode_bg = match mode.as_str() {
        "INSERT" => rgb_to_color(&theme.accent_green),
        "VISUAL" => rgb_to_color(&theme.popout),
        "COMMAND" => rgb_to_color(&theme.accent_blue),
        "HIST" | "DAY" | "JRNL" => rgb_to_color(&theme.accent_yellow),
        _ => rgb_to_color(&theme.salient),
    };
    fill_rect(frame, rect(pane_x, bar_y, mode_w, bar_h), mode_bg);
    draw_text(
        frame,
        pane_x + SPACING_SM + h_pad,
        text_y,
        &mode_text,
        rgb_to_color(&theme.background),
    );

    // ── 2. Position segment (right-aligned, fixed width) ──
    let pos_text = format!(" {}:{} ", normal.line + 1, normal.column + 1);
    let pos_w = pos_text.chars().count() as f32 * CHAR_WIDTH + SPACING_SM + h_pad;
    let pos_x = pane_x + pane_w - pos_w;
    fill_rect(
        frame,
        rect(pos_x, bar_y, pos_w, bar_h),
        rgb_to_color(&theme.subtle),
    );
    draw_text(frame, pos_x, text_y, &pos_text, rgb_to_color(&theme.faded));

    // ── 3. File + middle segment (fill between mode and position) ──
    let file_x = pane_x + mode_w;
    let file_w = (pos_x - file_x).max(0.0);
    fill_rect(
        frame,
        rect(file_x, bar_y, file_w, bar_h),
        rgb_to_color(&theme.highlight),
    );

    // File info: " filename [+]"
    let dirty_suffix = if normal.dirty { " [+]" } else { "" };
    let file_label = format!(" {}{}", normal.title, dirty_suffix);
    let file_max = chars_that_fit(file_w * 0.5);
    // Draw full file text in foreground first.
    draw_text(
        frame,
        file_x + SPACING_SM,
        text_y,
        truncate_text(&file_label, file_max),
        rgb_to_color(&theme.foreground),
    );
    // Overdraw the " [+]" portion in salient if dirty.
    if normal.dirty {
        let title_chars = normal.title.chars().count();
        let dirty_prefix_chars = 1 + title_chars; // leading space + title
        if dirty_prefix_chars < file_max {
            let dirty_x = file_x + SPACING_SM + (dirty_prefix_chars as f32) * CHAR_WIDTH;
            draw_text(frame, dirty_x, text_y, " [+]", rgb_to_color(&theme.salient));
        }
    }

    // Middle area: pending keys, indicators, or right hints (right-aligned within file segment).
    let middle_right_edge = pos_x - SPACING_SM;
    if let Some(hints) = &pane.status_bar.right_hints {
        let max = chars_that_fit((file_w * 0.45).max(0.0));
        let hint_text = truncate_text(hints, max);
        let hint_w = text_width(&hint_text);
        let hint_x = (middle_right_edge - hint_w).max(file_x + SPACING_SM);
        draw_text(
            frame,
            hint_x,
            text_y,
            &hint_text,
            rgb_to_color(&theme.faded),
        );
    } else {
        // Build middle segments right-to-left.
        let mut segments: Vec<(String, Color)> = Vec::new();

        // Pending keys — salient.
        if !normal.pending_keys.is_empty() {
            segments.push((normal.pending_keys.clone(), rgb_to_color(&theme.salient)));
        }
        // Macro recording.
        if let Some(recording) = normal.recording_macro {
            segments.push((format!("@{recording}"), rgb_to_color(&theme.accent_red)));
        }
        // MCP indicator.
        match &normal.mcp {
            McpIndicator::Off => {}
            McpIndicator::Idle => {
                segments.push(("⚡".to_string(), rgb_to_color(&theme.faded)));
            }
            McpIndicator::Editing { tick } => {
                const FRAMES: &[&str] = &["⚡", "◐", "◑", "◒", "◓"];
                segments.push((
                    FRAMES[(*tick as usize) % FRAMES.len()].to_string(),
                    rgb_to_color(&theme.salient),
                ));
            }
        }
        // Indexer.
        if normal.indexing {
            segments.push(("⟳".to_string(), rgb_to_color(&theme.salient)));
        }

        let gap = 2.0 * CHAR_WIDTH;
        let mut cursor_x = middle_right_edge;
        for (text, color) in &segments {
            let w = text_width(text);
            cursor_x -= w;
            if cursor_x < file_x + SPACING_SM {
                break;
            }
            draw_text(frame, cursor_x, text_y, text, *color);
            cursor_x -= gap;
        }
    }
}

fn draw_command_line(
    frame: &mut iced::widget::canvas::Frame,
    command: &CommandLineSlot,
    theme: &ThemePalette,
    modeline_area: Rectangle,
) {
    let pane_x = modeline_area.x;
    let pane_w = modeline_area.width;
    let status_y = modeline_area.y + (modeline_area.height - LINE_HEIGHT) / 2.0;
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
        let error_y = (status_y - LINE_HEIGHT).max(modeline_area.y);
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
    draw_bar_cursor(
        frame,
        cursor_x,
        status_y,
        LINE_HEIGHT,
        rgb_to_color(&theme.foreground),
    );
}

fn draw_quick_capture(
    frame: &mut iced::widget::canvas::Frame,
    capture: &QuickCaptureSlot,
    theme: &ThemePalette,
    modeline_area: Rectangle,
) {
    let pane_x = modeline_area.x;
    let pane_w = modeline_area.width;
    let status_y = modeline_area.y + (modeline_area.height - LINE_HEIGHT) / 2.0;
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
    draw_bar_cursor(
        frame,
        cursor_x,
        status_y,
        LINE_HEIGHT,
        rgb_to_color(&theme.foreground),
    );
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

// ---------------------------------------------------------------------------
// Dashboard (empty state)
// ---------------------------------------------------------------------------

fn draw_dashboard(
    frame: &mut iced::widget::canvas::Frame,
    area: Rectangle,
    dashboard: &DashboardFrame,
    theme: &ThemePalette,
) {
    let strong = rgb_to_color(&theme.strong);
    let faded = rgb_to_color(&theme.faded);
    let salient = rgb_to_color(&theme.salient);
    let fg = rgb_to_color(&theme.foreground);

    // --- ASCII Art Logo (top center) ---
    let logo_lines = [
        "██████╗ ██╗      ██████╗  ██████╗ ███╗   ███╗",
        "██╔══██╗██║     ██╔═══██╗██╔═══██╗████╗ ████║",
        "██████╔╝██║     ██║   ██║██║   ██║██╔████╔██║",
        "██╔══██╗██║     ██║   ██║██║   ██║██║╚██╔╝██║",
        "██████╔╝███████╗╚██████╔╝╚██████╔╝██║ ╚═╝ ██║",
        "╚═════╝ ╚══════╝ ╚═════╝  ╚═════╝ ╚═╝     ╚═╝",
    ];
    let logo_y = area.y + 2.0 * LINE_HEIGHT;
    for (i, line) in logo_lines.iter().enumerate() {
        draw_text_center(frame, area, logo_y + i as f32 * LINE_HEIGHT, line, strong);
    }

    let tagline = "Write freely. Let patterns emerge.";
    let tagline_y = logo_y + logo_lines.len() as f32 * LINE_HEIGHT + LINE_HEIGHT;
    draw_text_center(frame, area, tagline_y, tagline, faded);

    // --- Two-column layout: Recent Pages + Quick Actions ---
    let col_top = tagline_y + LINE_HEIGHT * 2.5;
    let left_x = area.x + area.width * 0.1;
    let right_x = area.x + area.width * 0.55;

    // Recent Pages header
    draw_text(frame, left_x, col_top, "Recent Pages", salient);
    let sep_y = col_top + LINE_HEIGHT;
    draw_text(frame, left_x, sep_y, "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}", faded);

    let mut row_y = sep_y + LINE_HEIGHT;
    if dashboard.recent_pages.is_empty() {
        draw_text(frame, left_x, row_y, "No recent pages", faded);
    } else {
        for page in &dashboard.recent_pages {
            draw_text(frame, left_x, row_y, &page.title, fg);
            let time_x = left_x + area.width * 0.28;
            draw_text(frame, time_x, row_y, &page.time_ago, faded);
            row_y += LINE_HEIGHT;
        }
    }

    // Quick Actions header
    draw_text(frame, right_x, col_top, "Quick Actions", salient);
    draw_text(frame, right_x, sep_y, "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}", faded);
    let actions = [
        ("SPC n", "new page"),
        ("SPC j t", "today\u{2019}s journal"),
        ("SPC f f", "find page"),
        ("SPC s s", "search everything"),
        ("SPC a a", "agenda"),
    ];
    let mut action_y = sep_y + LINE_HEIGHT;
    for (key, label) in &actions {
        draw_text(frame, right_x, action_y, *key, strong);
        draw_text(frame, right_x + CHAR_WIDTH * 10.0, action_y, *label, fg);
        action_y += LINE_HEIGHT;
    }

    // --- Second two-column layout: Today + Did You Know? ---
    let sec2_top = col_top + LINE_HEIGHT * 8.0;

    // Today header
    draw_text(frame, left_x, sec2_top, "Today", salient);
    draw_text(
        frame,
        left_x,
        sec2_top + LINE_HEIGHT,
        "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}",
        faded,
    );
    let stats_y = sec2_top + LINE_HEIGHT * 2.0;
    draw_text(
        frame,
        left_x,
        stats_y,
        format!("{} open tasks", dashboard.open_tasks),
        fg,
    );
    draw_text(
        frame,
        left_x,
        stats_y + LINE_HEIGHT,
        format!("{} pages edited", dashboard.pages_edited_today),
        fg,
    );
    draw_text(
        frame,
        left_x,
        stats_y + LINE_HEIGHT * 2.0,
        format!("Journal: {} entries", dashboard.journal_entries_today),
        fg,
    );

    // Did You Know? header
    draw_text(frame, right_x, sec2_top, "Did You Know?", salient);
    draw_text(
        frame,
        right_x,
        sec2_top + LINE_HEIGHT,
        "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}",
        faded,
    );
    // Word-wrap the tip into the available column width.
    let tip_max_chars = ((area.width * 0.35) / CHAR_WIDTH) as usize;
    let tip_lines = wrap_text(&dashboard.tip, tip_max_chars);
    let mut tip_y = sec2_top + LINE_HEIGHT * 2.0;
    for line in &tip_lines {
        draw_text(frame, right_x, tip_y, line.as_str(), fg);
        tip_y += LINE_HEIGHT;
    }

    // --- Footer ---
    let footer_y = area.y + area.height - 2.0 * LINE_HEIGHT;
    draw_text_center(frame, area, footer_y, "SPC j t to start writing", faded);
}

/// Simple word-wrap helper: break `text` into lines of at most `max_chars`.
fn wrap_text(text: &str, max_chars: usize) -> Vec<String> {
    if max_chars == 0 {
        return vec![text.to_string()];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if current.is_empty() {
            current.push_str(word);
        } else if current.len() + 1 + word.len() > max_chars {
            lines.push(std::mem::take(&mut current));
            current.push_str(word);
        } else {
            current.push(' ');
            current.push_str(word);
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FONT_SIZE;

    #[test]
    fn heading_font_size_h1() {
        assert_eq!(heading_font_size(1), FONT_SIZE * 1.5);
    }

    #[test]
    fn heading_font_size_h2() {
        assert_eq!(heading_font_size(2), FONT_SIZE * 1.3);
    }

    #[test]
    fn heading_font_size_h3() {
        assert_eq!(heading_font_size(3), FONT_SIZE * 1.1);
    }

    #[test]
    fn heading_font_size_h4_falls_back() {
        assert_eq!(heading_font_size(4), FONT_SIZE);
    }

    #[test]
    fn heading_row_height_scales_with_font() {
        // H1 row height = heading_font_size(1) * 1.4
        let h1_h = heading_font_size(1) * 1.4;
        assert!(h1_h > LINE_HEIGHT, "H1 row should be taller than normal");
        assert!((h1_h - FONT_SIZE * 1.5 * 1.4).abs() < 0.01);
    }

    #[test]
    fn cursor_y_empty_lines() {
        assert_eq!(cursor_y_in_pane(&[], 0, 10.0), 10.0);
    }

    #[test]
    fn cursor_y_first_line() {
        let lines = vec![make_normal_line("hello")];
        assert_eq!(cursor_y_in_pane(&lines, 0, 5.0), 5.0);
    }

    #[test]
    fn cursor_y_second_line() {
        let lines = vec![make_normal_line("one"), make_normal_line("two")];
        let y = cursor_y_in_pane(&lines, 1, 0.0);
        assert!((y - LINE_HEIGHT).abs() < 0.01);
    }

    fn make_normal_line(text: &str) -> bloom_core::render::RenderedLine {
        bloom_core::render::RenderedLine {
            source: bloom_core::render::LineSource::Buffer(0),
            is_mirror: false,
            text: text.to_string(),
            spans: vec![bloom_md::parser::traits::StyledSpan {
                byte_range: 0..text.len(),
                style: bloom_md::parser::traits::Style::Normal,
            }],
        }
    }
}
