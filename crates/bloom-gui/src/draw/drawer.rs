use bloom_core::render::{
    ContextStripDay, ContextStripFrame, DiffLineKind, PaneFrame, StripNodeKind, TemporalMode,
    TemporalStripFrame, WhichKeyFrame,
};
use bloom_md::theme::ThemePalette;
use iced::Size;

use crate::draw::{
    chars_that_fit, draw_hline, draw_text, fill_rect, rect, truncate_text,
};
use crate::theme::rgb_to_color;
use crate::{CHAR_WIDTH, LINE_HEIGHT};

pub(crate) fn draw_which_key(
    frame: &mut iced::widget::canvas::Frame,
    size: Size,
    which_key: &WhichKeyFrame,
    theme: &ThemePalette,
) {
    let total_chars = chars_that_fit(size.width);
    let col_chars = 24usize;
    let cols = ((total_chars.saturating_sub(4)) / col_chars).max(1);
    let rows = which_key.entries.len().div_ceil(cols).max(1);
    let panel_lines = rows + 2;
    let panel_h = panel_lines as f32 * LINE_HEIGHT;
    let panel_y = (size.height - panel_h).max(0.0);

    fill_rect(
        frame,
        rect(0.0, panel_y, size.width, panel_h),
        rgb_to_color(&theme.subtle),
    );
    draw_hline(frame, 0.0, size.width, panel_y, rgb_to_color(&theme.faded));

    draw_text(
        frame,
        CHAR_WIDTH,
        panel_y + 2.0,
        truncate_text(
            &which_key.prefix,
            chars_that_fit(size.width).saturating_sub(2),
        ),
        rgb_to_color(&theme.faded),
    );

    for (index, entry) in which_key.entries.iter().enumerate() {
        let col = index / rows;
        let row = index % rows;
        if col >= cols {
            break;
        }
        let x = CHAR_WIDTH + (col * col_chars) as f32 * CHAR_WIDTH;
        let y = panel_y + (row + 1) as f32 * LINE_HEIGHT + 2.0;
        let key = format!("{:<4}", truncate_text(&entry.key, 4));
        let label = truncate_text(&entry.label, col_chars.saturating_sub(5));
        draw_text(frame, x, y, key, rgb_to_color(&theme.strong));
        draw_text(
            frame,
            x + 5.0 * CHAR_WIDTH,
            y,
            label,
            rgb_to_color(if entry.is_group { &theme.salient } else { &theme.foreground }),
        );
    }
}

/// Draw the diff preview on its own layer (DiffCanvas).
/// Fills the active pane content area with an opaque background and renders diff lines.
pub(crate) fn draw_temporal_diff_preview(
    frame: &mut iced::widget::canvas::Frame,
    size: Size,
    strip: &TemporalStripFrame,
    theme: &ThemePalette,
    active_pane: Option<&PaneFrame>,
) {
    let lines = if strip.compact { 4 } else { 6 };
    let panel_h = lines as f32 * LINE_HEIGHT;
    let panel_y = (size.height - panel_h).max(0.0);

    let (pane_x, pane_y, pane_w) = if let Some(pane) = active_pane {
        (
            pane.rect.x as f32 * CHAR_WIDTH,
            pane.rect.y as f32 * LINE_HEIGHT,
            pane.rect.width as f32 * CHAR_WIDTH,
        )
    } else {
        (0.0, 0.0, size.width)
    };
    let preview_h = (panel_y - pane_y).max(0.0);

    // Opaque background fill — fully covers pane content below.
    fill_rect(frame, rect(pane_x, pane_y, pane_w, preview_h), rgb_to_color(&theme.background));

    let max_chars = chars_that_fit(pane_w);
    let max_rows = (preview_h / LINE_HEIGHT) as usize;
    for (i, dl) in strip.preview_lines.iter().take(max_rows).enumerate() {
        let y = pane_y + i as f32 * LINE_HEIGHT;
        let prefix = match dl.kind {
            DiffLineKind::Context => "  ",
            DiffLineKind::Added => "+ ",
            DiffLineKind::Removed => "- ",
            DiffLineKind::Modified => "~ ",
        };
        let line_color = match dl.kind {
            DiffLineKind::Context => &theme.foreground,
            DiffLineKind::Added => &theme.accent_green,
            DiffLineKind::Removed => &theme.accent_red,
            DiffLineKind::Modified => &theme.accent_yellow,
        };
        draw_text(frame, pane_x + CHAR_WIDTH, y, prefix, rgb_to_color(line_color));
        let mut x = pane_x + CHAR_WIDTH * 3.0;
        for seg in &dl.segments {
            let seg_color = match seg.kind {
                DiffLineKind::Context => &theme.foreground,
                DiffLineKind::Added => &theme.accent_green,
                DiffLineKind::Removed => &theme.accent_red,
                DiffLineKind::Modified => &theme.accent_yellow,
            };
            let text = truncate_text(&seg.text, max_chars.saturating_sub(4));
            draw_text(frame, x, y, &text, rgb_to_color(seg_color));
            x += text.chars().count() as f32 * CHAR_WIDTH;
        }
    }
}

/// Draw the temporal strip drawer only (nodes, title, hints).
/// Diff preview is rendered on a separate Canvas layer.
pub(crate) fn draw_temporal_strip_drawer(
    frame: &mut iced::widget::canvas::Frame,
    size: Size,
    strip: &TemporalStripFrame,
    theme: &ThemePalette,
) {
    if strip.items.is_empty() {
        return;
    }

    let lines = if strip.compact { 4 } else { 6 };
    let panel_h = lines as f32 * LINE_HEIGHT;
    let panel_y = (size.height - panel_h).max(0.0);

    let panel = rect(0.0, panel_y, size.width, panel_h);
    fill_rect(frame, panel, rgb_to_color(&theme.highlight));
    draw_hline(frame, 0.0, size.width, panel_y, rgb_to_color(&theme.faded));

    let total_chars = chars_that_fit(size.width);
    let node_chars = if strip.compact { 12usize } else { 16usize };
    let visible = ((total_chars.saturating_sub(4)) / node_chars).max(1);
    let total = strip.items.len();
    let half = visible / 2;
    let start = if strip.selected <= half {
        0
    } else if strip.selected + half >= total {
        total.saturating_sub(visible)
    } else {
        strip.selected - half
    };
    let end = (start + visible).min(total);

    let title = if strip.compact {
        format!("{}", strip.title)
    } else {
        format!("{} · {} versions", strip.title, strip.items.len())
    };
    draw_text(
        frame,
        CHAR_WIDTH,
        panel_y + 2.0,
        truncate_text(&title, total_chars.saturating_sub(2)),
        rgb_to_color(&theme.faded),
    );

    for (visual_index, node) in strip.items[start..end].iter().enumerate() {
        let selected = start + visual_index == strip.selected;
        let x = CHAR_WIDTH + (visual_index * node_chars) as f32 * CHAR_WIDTH;
        let y = panel_y + LINE_HEIGHT + 2.0;
        let marker = if node.branch_count > 1 {
            "◆"
        } else if node.skip {
            "·"
        } else {
            match node.kind {
                StripNodeKind::UndoNode => "●",
                StripNodeKind::GitCommit => "○",
            }
        };
        let prefix = if selected { "▸" } else { " " };
        let cell = format!("{prefix}{marker} {}", truncate_text(&node.label, node_chars.saturating_sub(3)));
        let color = if selected {
            rgb_to_color(&theme.strong)
        } else if node.skip {
            rgb_to_color(&theme.faded)
        } else if matches!(node.kind, StripNodeKind::UndoNode) {
            rgb_to_color(&theme.foreground)
        } else {
            rgb_to_color(&theme.faded)
        };
        draw_text(frame, x, y, truncate_text(&cell, node_chars), color);
    }

    let selected_visual = strip.selected.saturating_sub(start).min(end.saturating_sub(start));
    let indicator_x = CHAR_WIDTH + (selected_visual * node_chars) as f32 * CHAR_WIDTH + 2.0 * CHAR_WIDTH;
    let detail = strip
        .items
        .get(strip.selected)
        .and_then(|node| node.detail.as_deref())
        .unwrap_or("");
    draw_text(
        frame,
        indicator_x,
        panel_y + 2.0 * LINE_HEIGHT + 2.0,
        "▲",
        rgb_to_color(&theme.accent_yellow),
    );
    draw_text(
        frame,
        indicator_x + 2.0 * CHAR_WIDTH,
        panel_y + 2.0 * LINE_HEIGHT + 2.0,
        truncate_text(detail, total_chars.saturating_sub(selected_visual * node_chars + 6)),
        rgb_to_color(&theme.accent_yellow),
    );

    if !strip.compact && lines >= 6 {
        let mode = match strip.mode {
            TemporalMode::PageHistory => "Page history",
            TemporalMode::BlockHistory => "Block history",
            TemporalMode::DayActivity => "Day activity",
        };
        let kinds = strip
            .items
            .get(strip.selected)
            .map(|node| match node.kind {
                StripNodeKind::UndoNode => "Undo node",
                StripNodeKind::GitCommit => "Git commit",
            })
            .unwrap_or("");
        draw_text(
            frame,
            CHAR_WIDTH,
            panel_y + 3.0 * LINE_HEIGHT + 2.0,
            truncate_text(&format!("{} · {}", mode, kinds), total_chars.saturating_sub(2)),
            rgb_to_color(&theme.faded),
        );
        draw_text(
            frame,
            CHAR_WIDTH,
            panel_y + 4.0 * LINE_HEIGHT + 2.0,
            truncate_text(
                &strip
                    .items
                    .iter()
                    .skip(start)
                    .take(end.saturating_sub(start))
                    .map(|node| truncate_text(node.detail.as_deref().unwrap_or(""), node_chars.saturating_sub(2)))
                    .collect::<Vec<_>>()
                    .join("  "),
                total_chars.saturating_sub(2),
            ),
            rgb_to_color(&theme.faded),
        );
    }

    let hints = match strip.mode {
        TemporalMode::PageHistory | TemporalMode::BlockHistory => {
            "h/l scrub   e detail   r restore   d diff   q close"
        }
        TemporalMode::DayActivity => "h/l scrub   e detail   Enter open   q close",
    };
    draw_text(
        frame,
        CHAR_WIDTH,
        panel_y + (lines - 1) as f32 * LINE_HEIGHT + 2.0,
        truncate_text(hints, total_chars.saturating_sub(2)),
        rgb_to_color(&theme.faded),
    );
}

pub(crate) fn draw_context_strip(
    frame: &mut iced::widget::canvas::Frame,
    size: Size,
    strip: &ContextStripFrame,
    theme: &ThemePalette,
) {
    let panel_h = 3.0 * LINE_HEIGHT;
    let panel_y = (size.height - panel_h).max(0.0);
    fill_rect(
        frame,
        rect(0.0, panel_y, size.width, panel_h),
        rgb_to_color(&theme.background),
    );
    draw_hline(frame, 0.0, size.width, panel_y, rgb_to_color(&theme.faded));

    let col_w = size.width / 3.0;
    draw_context_column(frame, 0.0, panel_y, col_w, strip.prev.as_ref(), false, true, theme);
    draw_context_column(
        frame,
        col_w,
        panel_y,
        col_w,
        Some(&strip.current),
        true,
        false,
        theme,
    );
    draw_context_column(frame, col_w * 2.0, panel_y, col_w, strip.next.as_ref(), false, false, theme);
}

fn draw_context_column(
    frame: &mut iced::widget::canvas::Frame,
    x: f32,
    y: f32,
    width: f32,
    day: Option<&ContextStripDay>,
    selected: bool,
    is_prev: bool,
    theme: &ThemePalette,
) {
    let max_chars = chars_that_fit(width).saturating_sub(2);
    let label_color = if selected {
        rgb_to_color(&theme.strong)
    } else {
        rgb_to_color(&theme.faded)
    };
    let body_color = if selected {
        rgb_to_color(&theme.foreground)
    } else {
        rgb_to_color(&theme.faded)
    };

    if let Some(day) = day {
        let mut label = day.label.clone();
        if selected {
            label = format!("▸ {label}");
        } else if is_prev {
            label = format!("◄ {label}");
        } else {
            label = format!("{label} ►");
        }
        draw_text(frame, x + CHAR_WIDTH / 2.0, y + 2.0, truncate_text(&label, max_chars), label_color);
        draw_text(
            frame,
            x + CHAR_WIDTH / 2.0,
            y + LINE_HEIGHT + 2.0,
            truncate_text(&day.stats, max_chars),
            body_color,
        );
        draw_text(
            frame,
            x + CHAR_WIDTH / 2.0,
            y + 2.0 * LINE_HEIGHT + 2.0,
            truncate_text(&day.first_line, max_chars),
            body_color,
        );
    }
}
