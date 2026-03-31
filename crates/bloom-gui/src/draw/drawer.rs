use bloom_core::render::{
    ContextStripDay, ContextStripFrame, DiffLineKind, StripNodeKind, TemporalMode,
    TemporalStripFrame, WhichKeyFrame,
};
use bloom_md::theme::ThemePalette;
use iced::Rectangle;

use crate::draw::{
    chars_that_fit, draw_hline, draw_text, fill_rect, rect, text_width, truncate_text,
};
use crate::theme::rgb_to_color;
use crate::{CHAR_WIDTH, LINE_HEIGHT, SPACING_SM};

pub(crate) fn draw_which_key(
    frame: &mut iced::widget::canvas::Frame,
    area: Rectangle,
    which_key: &WhichKeyFrame,
    theme: &ThemePalette,
) {
    let total_chars = chars_that_fit(area.width);
    let col_chars = 20usize;
    let cols = ((total_chars.saturating_sub(4)) / col_chars).max(1);
    let rows = which_key.entries.len().div_ceil(cols).max(1);

    fill_rect(
        frame,
        rect(area.x, area.y, area.width, area.height),
        rgb_to_color(&theme.subtle),
    );
    draw_hline(
        frame,
        area.x,
        area.x + area.width,
        area.y,
        rgb_to_color(&theme.faded),
    );

    // Minimal prefix header in faded.
    if !which_key.prefix.is_empty() {
        draw_text(
            frame,
            area.x + CHAR_WIDTH,
            area.y + 2.0,
            truncate_text(&which_key.prefix, total_chars.saturating_sub(2)),
            rgb_to_color(&theme.faded),
        );
    }

    for (index, entry) in which_key.entries.iter().enumerate() {
        let col = index / rows;
        let row = index % rows;
        if col >= cols {
            break;
        }
        let x = area.x + CHAR_WIDTH + (col * col_chars) as f32 * CHAR_WIDTH;
        let y = area.y + (row + 1) as f32 * LINE_HEIGHT + 2.0;

        // Key character in strong (no pill/box).
        let key = truncate_text(&entry.key, 4);
        draw_text(frame, x, y, &key, rgb_to_color(&theme.strong));

        // Label: "+label" for groups, "label" for leaves.
        let label_x = x + text_width(&key) + SPACING_SM;
        let label_prefix = if entry.is_group { "+" } else { "" };
        let max_label = col_chars.saturating_sub(key.chars().count() + 1 + label_prefix.len());
        let label = format!("{}{}", label_prefix, truncate_text(&entry.label, max_label));
        draw_text(frame, label_x, y, label, rgb_to_color(&theme.foreground));
    }
}

/// Draw the diff preview on its own layer (DiffCanvas).
/// Fills the provided `area` with an opaque background and renders diff lines.
pub(crate) fn draw_temporal_diff_preview(
    frame: &mut iced::widget::canvas::Frame,
    area: Rectangle,
    strip: &TemporalStripFrame,
    theme: &ThemePalette,
) {
    // Opaque background fill — fully covers pane content below.
    fill_rect(frame, area, rgb_to_color(&theme.background));

    let max_chars = chars_that_fit(area.width);
    let max_rows = (area.height / LINE_HEIGHT) as usize;
    // Gutter width: "old | new" → e.g. " 42 │ 43 " = 11 chars
    let gutter_chars = 11;
    let gutter_w = gutter_chars as f32 * CHAR_WIDTH;
    let content_x = area.x + gutter_w;
    let content_chars = max_chars.saturating_sub(gutter_chars);

    for (i, dl) in strip.preview_lines.iter().take(max_rows).enumerate() {
        let y = area.y + i as f32 * LINE_HEIGHT;

        // Line number gutter: "old │ new"
        let old_num = dl
            .old_line
            .map(|n| format!("{:>4}", n + 1))
            .unwrap_or_else(|| "    ".to_string());
        let new_num = dl
            .new_line
            .map(|n| format!("{:<4}", n + 1))
            .unwrap_or_else(|| "    ".to_string());
        let gutter_text = format!("{}│{}", old_num, new_num);
        draw_text(
            frame,
            area.x,
            y,
            gutter_text,
            rgb_to_color(&theme.faded.blend(theme.background, 0.4)),
        );

        // +/- prefix
        let prefix = match dl.kind {
            DiffLineKind::Context => "  ",
            DiffLineKind::Added => "+ ",
            DiffLineKind::Removed => "- ",
            DiffLineKind::Modified => "~ ",
        };
        let line_color = match dl.kind {
            DiffLineKind::Context => &theme.foreground,
            DiffLineKind::Added => &theme.diff_added,
            DiffLineKind::Removed => &theme.diff_removed,
            DiffLineKind::Modified => &theme.accent_yellow,
        };
        draw_text(frame, content_x, y, prefix, rgb_to_color(line_color));

        // Content with word-level diff segments
        let mut x = content_x + CHAR_WIDTH * 2.0;
        for seg in &dl.segments {
            let seg_color = match seg.kind {
                DiffLineKind::Context => &theme.foreground,
                DiffLineKind::Added => &theme.diff_added,
                DiffLineKind::Removed => &theme.diff_removed,
                DiffLineKind::Modified => &theme.accent_yellow,
            };
            let text = truncate_text(&seg.text, content_chars.saturating_sub(2));
            draw_text(frame, x, y, &text, rgb_to_color(seg_color));
            x += text.chars().count() as f32 * CHAR_WIDTH;
        }
    }
}

/// Draw the temporal strip drawer only (nodes, title, hints).
/// Diff preview is rendered on a separate Canvas layer.
pub(crate) fn draw_temporal_strip_drawer(
    frame: &mut iced::widget::canvas::Frame,
    area: Rectangle,
    strip: &TemporalStripFrame,
    theme: &ThemePalette,
) {
    if strip.items.is_empty() {
        return;
    }

    let lines = (area.height / LINE_HEIGHT).floor() as usize;

    fill_rect(frame, area, rgb_to_color(&theme.highlight));
    draw_hline(
        frame,
        area.x,
        area.x + area.width,
        area.y,
        rgb_to_color(&theme.faded),
    );

    let total_chars = chars_that_fit(area.width);
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
        strip.title.to_string()
    } else {
        format!("{} · {} versions", strip.title, strip.items.len())
    };
    draw_text(
        frame,
        area.x + CHAR_WIDTH,
        area.y + 2.0,
        truncate_text(&title, total_chars.saturating_sub(2)),
        rgb_to_color(&theme.faded),
    );

    for (visual_index, node) in strip.items[start..end].iter().enumerate() {
        let selected = start + visual_index == strip.selected;
        let x = area.x + CHAR_WIDTH + (visual_index * node_chars) as f32 * CHAR_WIDTH;
        let y = area.y + LINE_HEIGHT + 2.0;

        // Selected node gets a background fill instead of a triangle prefix.
        if selected {
            let node_w = node_chars as f32 * CHAR_WIDTH;
            fill_rect(
                frame,
                rect(x, y, node_w, LINE_HEIGHT),
                rgb_to_color(&theme.mild),
            );
        }

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
        let cell = format!(
            " {marker} {}",
            truncate_text(&node.label, node_chars.saturating_sub(3))
        );
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

    let selected_visual = strip
        .selected
        .saturating_sub(start)
        .min(end.saturating_sub(start));
    let detail_x = area.x + CHAR_WIDTH + (selected_visual * node_chars) as f32 * CHAR_WIDTH;
    let detail = strip
        .items
        .get(strip.selected)
        .and_then(|node| node.detail.as_deref())
        .unwrap_or("");
    let detail_width = total_chars.saturating_sub(selected_visual * node_chars + 2);
    let summary_line = if strip.compact {
        detail.to_string()
    } else {
        strip.selected_summary.clone()
    };
    draw_text(
        frame,
        detail_x,
        area.y + 2.0 * LINE_HEIGHT + 2.0,
        truncate_text(&summary_line, detail_width),
        rgb_to_color(&theme.accent_yellow),
    );

    if !strip.compact && lines >= 6 {
        draw_text(
            frame,
            area.x + CHAR_WIDTH,
            area.y + 3.0 * LINE_HEIGHT + 2.0,
            truncate_text(&strip.selected_scope, total_chars.saturating_sub(2)),
            rgb_to_color(&theme.faded),
        );
        draw_text(
            frame,
            area.x + CHAR_WIDTH,
            area.y + 4.0 * LINE_HEIGHT + 2.0,
            truncate_text(
                &if strip.selected_context.is_empty() {
                    strip.selected_restore.clone()
                } else {
                    format!("{} · {}", strip.selected_context, strip.selected_restore)
                },
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
        area.x + CHAR_WIDTH,
        area.y + (lines - 1).max(1) as f32 * LINE_HEIGHT + 2.0,
        truncate_text(
            &format!("{hints}   {}", strip.durable_health),
            total_chars.saturating_sub(2),
        ),
        rgb_to_color(&theme.faded),
    );
}

pub(crate) fn draw_context_strip(
    frame: &mut iced::widget::canvas::Frame,
    area: Rectangle,
    strip: &ContextStripFrame,
    theme: &ThemePalette,
) {
    fill_rect(frame, area, rgb_to_color(&theme.background));
    draw_hline(
        frame,
        area.x,
        area.x + area.width,
        area.y,
        rgb_to_color(&theme.faded),
    );

    let col_w = area.width / 3.0;
    draw_context_column(
        frame,
        area.x,
        area.y,
        col_w,
        strip.prev.as_ref(),
        false,
        true,
        theme,
    );
    draw_context_column(
        frame,
        area.x + col_w,
        area.y,
        col_w,
        Some(&strip.current),
        true,
        false,
        theme,
    );
    draw_context_column(
        frame,
        area.x + col_w * 2.0,
        area.y,
        col_w,
        strip.next.as_ref(),
        false,
        false,
        theme,
    );
}

#[allow(clippy::too_many_arguments)]
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
        draw_text(
            frame,
            x + CHAR_WIDTH / 2.0,
            y + 2.0,
            truncate_text(&label, max_chars),
            label_color,
        );
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
