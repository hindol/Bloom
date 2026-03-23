use bloom_core::render::{
    DatePickerFrame, DialogFrame, ImportChoice, PickerFrame, SetupStep, SetupWizardFrame,
    ViewFrame, ViewRow,
};
use bloom_md::theme::ThemePalette;
use iced::Rectangle;

use crate::draw::{
    chars_that_fit, draw_bar_cursor, draw_hline, draw_text, draw_text_center, draw_text_right,
    fill_panel, fill_rect, inset, rect, stroke_rect, text_width, truncate_text,
};
use crate::theme::rgb_to_color;
use crate::{CHAR_WIDTH, LINE_HEIGHT, SPACING_MD};

pub(crate) fn draw_picker(
    frame: &mut iced::widget::canvas::Frame,
    area: Rectangle,
    picker: &PickerFrame,
    theme: &ThemePalette,
) {
    let content_chars = chars_that_fit(area.width).saturating_sub(4);
    let max_visible: usize = 10;
    let num_results = picker.results.len().min(max_visible);

    let panel_top = area.y;
    let panel_bottom = area.y + area.height;

    let available_h = panel_bottom - panel_top;
    let available_lines = (available_h / LINE_HEIGHT) as usize;
    let num_visible = num_results.min(available_lines.saturating_sub(3));

    // Layout from bottom up within the allocated rect.
    let query_y = panel_bottom - LINE_HEIGHT;
    let status_line_y = query_y - LINE_HEIGHT;
    let results_bottom_y = status_line_y;
    let results_top_y = results_bottom_y - num_visible as f32 * LINE_HEIGHT;

    // Opaque background covering the picker area.
    fill_rect(frame, area, rgb_to_color(&theme.background));

    // Top separator.
    draw_hline(
        frame,
        area.x,
        area.x + area.width,
        panel_top,
        rgb_to_color(&theme.faded),
    );

    // ── Status line ("5 of 120 pages") ──
    let footer = if picker.filtered_count > 0 {
        format!(
            "{} of {} {}",
            picker.selected_index + 1,
            picker.filtered_count,
            picker.status_noun
        )
    } else {
        format!("0 of {} {}", picker.total_count, picker.status_noun)
    };
    draw_text(
        frame,
        area.x + SPACING_MD,
        status_line_y,
        truncate_text(&footer, content_chars),
        rgb_to_color(&theme.faded),
    );

    // ── Results (newest/best at bottom, closest to query) ──
    let right_chars = picker
        .results
        .iter()
        .filter_map(|row| row.right.as_ref())
        .map(|text| text.chars().count())
        .max()
        .unwrap_or(0)
        .min(18);
    let middle_chars = picker
        .results
        .iter()
        .filter_map(|row| row.middle.as_ref())
        .map(|text| text.chars().count())
        .max()
        .unwrap_or(0)
        .min((content_chars / 4).max(8));
    let label_chars = content_chars.saturating_sub(right_chars + middle_chars + 6);

    let viewport = num_visible.max(1);
    let scroll = if picker.selected_index >= viewport {
        picker.selected_index - viewport + 1
    } else {
        0
    };
    let visible_rows: Vec<_> = picker.results.iter().skip(scroll).take(viewport).collect();

    for (vi, row) in visible_rows.iter().enumerate() {
        let y = results_top_y + vi as f32 * LINE_HEIGHT;
        let selected = scroll + vi == picker.selected_index;
        if selected {
            fill_rect(
                frame,
                rect(area.x, y, area.width, LINE_HEIGHT),
                rgb_to_color(&theme.mild),
            );
        }
        draw_text(
            frame,
            area.x + SPACING_MD,
            y,
            format!(" {}", truncate_text(&row.label, label_chars)),
            rgb_to_color(&theme.foreground),
        );
        if let Some(middle) = &row.middle {
            draw_text(
                frame,
                area.x + SPACING_MD + (label_chars + 3) as f32 * CHAR_WIDTH,
                y,
                truncate_text(middle, middle_chars),
                rgb_to_color(&theme.faded),
            );
        }
        if let Some(right) = &row.right {
            draw_text_right(
                frame,
                area.x + area.width - SPACING_MD,
                y,
                &truncate_text(right, right_chars),
                rgb_to_color(&theme.faded),
            );
        }
    }

    if picker.results.is_empty()
        && picker.min_query_len > 0
        && picker.query.len() < picker.min_query_len
    {
        let empty_area = rect(
            area.x,
            results_top_y,
            area.width,
            num_visible.max(1) as f32 * LINE_HEIGHT,
        );
        draw_text_center(
            frame,
            empty_area,
            results_top_y,
            "Type to search…",
            rgb_to_color(&theme.faded),
        );
    }

    // Separator between results and query.
    draw_hline(
        frame,
        area.x,
        area.x + area.width,
        query_y,
        rgb_to_color(&theme.faded),
    );

    // ── Query line: "{title} > {query}█" ──
    let prompt = format!("{} > ", picker.title);
    draw_text(
        frame,
        area.x + SPACING_MD,
        query_y,
        truncate_text(&prompt, content_chars),
        rgb_to_color(&theme.faded),
    );
    let query_x = area.x + SPACING_MD + text_width(&prompt);
    if picker.query_selected && !picker.query.is_empty() {
        fill_rect(
            frame,
            rect(
                query_x - 2.0,
                query_y,
                text_width(&picker.query) + CHAR_WIDTH,
                LINE_HEIGHT,
            ),
            rgb_to_color(&theme.mild),
        );
    }
    draw_text(
        frame,
        query_x,
        query_y,
        truncate_text(
            &picker.query,
            content_chars.saturating_sub(prompt.chars().count()),
        ),
        rgb_to_color(&theme.foreground),
    );
    draw_bar_cursor(
        frame,
        (query_x + picker.query.chars().count() as f32 * CHAR_WIDTH)
            .min(area.x + area.width - SPACING_MD - 2.0),
        query_y,
        LINE_HEIGHT,
        rgb_to_color(&theme.foreground),
    );
}

pub(crate) fn draw_dialog(
    frame: &mut iced::widget::canvas::Frame,
    area: Rectangle,
    dialog: &DialogFrame,
    theme: &ThemePalette,
) {
    fill_panel(
        frame,
        area,
        rgb_to_color(&theme.background),
        rgb_to_color(&theme.faded),
    );

    let inner = inset(area, CHAR_WIDTH);
    let max_chars = chars_that_fit(inner.width).saturating_sub(1);
    draw_text_center(
        frame,
        inner,
        inner.y,
        &truncate_text(&dialog.message, max_chars),
        rgb_to_color(&theme.foreground),
    );

    let mut x = inner.x;
    let y = inner.y + LINE_HEIGHT * 1.75;
    for (index, choice) in dialog.choices.iter().enumerate() {
        let label = format!("[{}]", choice);
        let w = text_width(&label) + CHAR_WIDTH;
        if index == dialog.selected {
            fill_rect(
                frame,
                rect(x - 2.0, y, w, LINE_HEIGHT),
                rgb_to_color(&theme.mild),
            );
        }
        draw_text(
            frame,
            x,
            y,
            label,
            rgb_to_color(if index == dialog.selected {
                &theme.strong
            } else {
                &theme.foreground
            }),
        );
        x += w + CHAR_WIDTH;
    }
}

pub(crate) fn draw_date_picker(
    frame: &mut iced::widget::canvas::Frame,
    area: Rectangle,
    picker: &DatePickerFrame,
    theme: &ThemePalette,
) {
    let content_chars = chars_that_fit(area.width).saturating_sub(4);

    let panel_top = area.y;

    // Opaque background.
    fill_rect(frame, area, rgb_to_color(&theme.background));

    // Top separator.
    draw_hline(
        frame,
        area.x,
        area.x + area.width,
        panel_top,
        rgb_to_color(&theme.faded),
    );

    let mut y = panel_top + LINE_HEIGHT * 0.5;
    let x_left = area.x + SPACING_MD;

    // Prompt line.
    draw_text(
        frame,
        x_left,
        y,
        truncate_text(&picker.prompt, content_chars),
        rgb_to_color(&theme.faded),
    );
    y += LINE_HEIGHT;

    // Month header.
    let month_name = month_name(picker.month);
    draw_text(
        frame,
        x_left + 4.0 * CHAR_WIDTH,
        y,
        format!("{} {}", month_name, picker.year),
        rgb_to_color(&theme.strong),
    );
    y += LINE_HEIGHT;

    // Day-of-week header.
    draw_text(
        frame,
        x_left,
        y,
        " Mo  Tu  We  Th  Fr  Sa  Su",
        rgb_to_color(&theme.faded),
    );
    y += LINE_HEIGHT;

    // Calendar grid.
    let (selected_year, selected_month_num, selected_day) = date_parts(&picker.selected_date);
    let selected_month = selected_year == picker.year && selected_month_num == picker.month;
    let (today_year, today_month_num, today_day) = date_parts(&picker.today);
    let today_month = today_year == picker.year && today_month_num == picker.month;

    for week in &picker.month_view {
        for (day_index, day) in week.iter().enumerate() {
            let x = x_left + day_index as f32 * 4.0 * CHAR_WIDTH;
            let cell = rect(x, y, 3.0 * CHAR_WIDTH, LINE_HEIGHT);
            if let Some(day) = day {
                let selected = selected_month && *day == selected_day;
                let today = today_month && *day == today_day;
                let has_journal = picker.journal_days.contains(day);
                if selected {
                    fill_rect(frame, cell, rgb_to_color(&theme.salient));
                }
                if has_journal && !selected {
                    draw_text(frame, x, y, "◆", rgb_to_color(&theme.accent_yellow));
                }
                let day_color = if selected {
                    &theme.background
                } else if today {
                    &theme.strong
                } else {
                    &theme.foreground
                };
                draw_text(
                    frame,
                    x + CHAR_WIDTH,
                    y,
                    format!("{:>2}", day),
                    rgb_to_color(day_color),
                );
            }
        }
        y += LINE_HEIGHT;
    }

    // Hint / footer line.
    let footer = format!("{} entries  ↵:open  ⎋:close", picker.journal_days.len());
    draw_text(
        frame,
        x_left,
        y,
        truncate_text(&footer, content_chars),
        rgb_to_color(&theme.faded),
    );
}

pub(crate) fn draw_setup_wizard(
    frame: &mut iced::widget::canvas::Frame,
    area: Rectangle,
    wizard: &SetupWizardFrame,
    theme: &ThemePalette,
) {
    fill_rect(frame, area, rgb_to_color(&theme.background));
    stroke_rect(
        frame,
        rect(
            area.x + 1.0,
            area.y + 1.0,
            (area.width - 2.0).max(0.0),
            (area.height - 2.0).max(0.0),
        ),
        rgb_to_color(&theme.faded),
    );

    let left = area.x + 9.0 * CHAR_WIDTH;
    let top = area.y + (area.height * 0.2).max(2.0 * LINE_HEIGHT);
    let right = area.x + area.width - 3.0 * CHAR_WIDTH;
    let content_chars = chars_that_fit((right - left).max(0.0));
    let bottom = area.y + area.height;

    let fg = rgb_to_color(&theme.foreground);
    let faded = rgb_to_color(&theme.faded);
    let heading = rgb_to_color(&theme.strong);
    let salient = rgb_to_color(&theme.salient);
    let success = rgb_to_color(&theme.accent_green);
    let warn = rgb_to_color(&theme.accent_yellow);
    let error = rgb_to_color(&theme.critical);

    match wizard.step {
        SetupStep::Welcome => {
            wizard_line(frame, left, top + 2.0 * LINE_HEIGHT, "Bloom 🌱", heading);
            wizard_line(
                frame,
                left,
                top + 4.0 * LINE_HEIGHT,
                "A local-first, keyboard-driven note-taking app.",
                fg,
            );
            wizard_line(
                frame,
                left,
                top + 6.0 * LINE_HEIGHT,
                "Your notes live in a single Markdown vault on your machine.",
                fg,
            );
            wizard_line(
                frame,
                right - text_width("Press Enter to get started"),
                bottom - 2.0 * LINE_HEIGHT,
                "Press Enter to get started",
                faded,
            );
        }
        SetupStep::ChooseVaultLocation => {
            wizard_line(frame, left, top, "Choose vault location", heading);
            wizard_line(
                frame,
                left,
                top + 2.0 * LINE_HEIGHT,
                "This is where your notes, journal, and config will live.",
                fg,
            );
            draw_input_row(
                frame,
                left,
                top + 5.0 * LINE_HEIGHT,
                right,
                "Path: ",
                &wizard.vault_path,
                wizard.vault_path_cursor,
                theme,
            );
            wizard_line(
                frame,
                left,
                top + 7.0 * LINE_HEIGHT,
                "Bloom will create:",
                faded,
            );
            wizard_line(
                frame,
                left + 2.0 * CHAR_WIDTH,
                top + 8.0 * LINE_HEIGHT,
                "pages/     — topic pages",
                faded,
            );
            wizard_line(
                frame,
                left + 2.0 * CHAR_WIDTH,
                top + 9.0 * LINE_HEIGHT,
                "journal/   — daily journal",
                faded,
            );
            wizard_line(
                frame,
                left + 2.0 * CHAR_WIDTH,
                top + 10.0 * LINE_HEIGHT,
                "templates/ — page templates",
                faded,
            );
            wizard_line(
                frame,
                left + 2.0 * CHAR_WIDTH,
                top + 11.0 * LINE_HEIGHT,
                "images/    — attachments",
                faded,
            );
            if let Some(message) = &wizard.error {
                wizard_line(
                    frame,
                    left,
                    top + 13.0 * LINE_HEIGHT,
                    &format!("✗ {}", truncate_text(message, content_chars)),
                    error,
                );
            }
            wizard_line(frame, left, bottom - 2.0 * LINE_HEIGHT, "Esc back", faded);
            wizard_line(
                frame,
                right - text_width("Enter to confirm"),
                bottom - 2.0 * LINE_HEIGHT,
                "Enter to confirm",
                faded,
            );
        }
        SetupStep::ImportChoice => {
            wizard_line(frame, left, top, "Import from Logseq?", heading);
            wizard_line(
                frame,
                left,
                top + 2.0 * LINE_HEIGHT,
                "Bloom can import pages, journals, and links from an existing vault.",
                fg,
            );
            let no_selected = wizard.import_choice == ImportChoice::No;
            let yes_selected = wizard.import_choice == ImportChoice::Yes;
            draw_choice(
                frame,
                left,
                top + 6.0 * LINE_HEIGHT,
                "No, start fresh",
                no_selected,
                theme,
            );
            draw_choice(
                frame,
                left,
                top + 7.0 * LINE_HEIGHT,
                "Yes, import from Logseq",
                yes_selected,
                theme,
            );
            wizard_line(frame, left, bottom - 2.0 * LINE_HEIGHT, "Esc back", faded);
            wizard_line(
                frame,
                right - text_width("↑↓ select   Enter to confirm"),
                bottom - 2.0 * LINE_HEIGHT,
                "↑↓ select   Enter to confirm",
                faded,
            );
        }
        SetupStep::ImportPath => {
            wizard_line(frame, left, top, "Import from Logseq", heading);
            wizard_line(
                frame,
                left,
                top + 2.0 * LINE_HEIGHT,
                "Enter the path to your Logseq vault:",
                fg,
            );
            draw_input_row(
                frame,
                left,
                top + 4.0 * LINE_HEIGHT,
                right,
                "Path: ",
                &wizard.logseq_path,
                wizard.logseq_path_cursor,
                theme,
            );
            if let Some(message) = &wizard.error {
                wizard_line(
                    frame,
                    left,
                    top + 6.0 * LINE_HEIGHT,
                    &format!("✗ {}", truncate_text(message, content_chars)),
                    error,
                );
            }
            wizard_line(frame, left, bottom - 2.0 * LINE_HEIGHT, "Esc back", faded);
            wizard_line(
                frame,
                right - text_width("Enter to start import"),
                bottom - 2.0 * LINE_HEIGHT,
                "Enter to start import",
                faded,
            );
        }
        SetupStep::ImportRunning => {
            wizard_line(frame, left, top, "Importing from Logseq...", heading);
            if let Some(progress) = &wizard.import_progress {
                let bar_width = (area.width * 0.35).max(20.0 * CHAR_WIDTH);
                let bar_area = rect(left, top + 2.0 * LINE_HEIGHT, bar_width, LINE_HEIGHT);
                fill_rect(frame, bar_area, rgb_to_color(&theme.subtle));
                let ratio = if progress.total > 0 {
                    progress.done as f32 / progress.total as f32
                } else {
                    0.0
                };
                fill_rect(
                    frame,
                    rect(
                        bar_area.x,
                        bar_area.y,
                        bar_area.width * ratio.clamp(0.0, 1.0),
                        bar_area.height,
                    ),
                    rgb_to_color(&theme.salient),
                );
                stroke_rect(frame, bar_area, rgb_to_color(&theme.faded));
                wizard_line(
                    frame,
                    left + bar_width + CHAR_WIDTH,
                    top + 2.0 * LINE_HEIGHT,
                    &format!("{}/{}", progress.done, progress.total),
                    fg,
                );
                wizard_line(
                    frame,
                    left,
                    top + 4.0 * LINE_HEIGHT,
                    &format!("✓ {} pages imported", progress.pages_imported),
                    success,
                );
                wizard_line(
                    frame,
                    left,
                    top + 5.0 * LINE_HEIGHT,
                    &format!("✓ {} journals imported", progress.journals_imported),
                    success,
                );
                wizard_line(
                    frame,
                    left,
                    top + 6.0 * LINE_HEIGHT,
                    &format!("✓ {} links resolved", progress.links_resolved),
                    success,
                );
                if !progress.warnings.is_empty() {
                    wizard_line(
                        frame,
                        left,
                        top + 7.0 * LINE_HEIGHT,
                        &format!("⚠ {} warnings", progress.warnings.len()),
                        warn,
                    );
                }
                if !progress.errors.is_empty() {
                    wizard_line(
                        frame,
                        left,
                        top + 8.0 * LINE_HEIGHT,
                        &format!("✗ {} errors", progress.errors.len()),
                        error,
                    );
                }
                if progress.finished {
                    wizard_line(
                        frame,
                        right - text_width("Press Enter to continue"),
                        bottom - 2.0 * LINE_HEIGHT,
                        "Press Enter to continue",
                        faded,
                    );
                }
            }
        }
        SetupStep::Complete => {
            wizard_line(
                frame,
                left,
                top + 2.0 * LINE_HEIGHT,
                "Your vault is ready 🌱",
                heading,
            );
            wizard_line(
                frame,
                left,
                top + 4.0 * LINE_HEIGHT,
                &format!("Location: {}", wizard.vault_path),
                fg,
            );
            wizard_line(
                frame,
                left,
                top + 5.0 * LINE_HEIGHT,
                &format!("Pages: {}", wizard.stats.pages),
                fg,
            );
            wizard_line(
                frame,
                left,
                top + 6.0 * LINE_HEIGHT,
                &format!("Journal: {} entries", wizard.stats.journals),
                fg,
            );
            wizard_line(frame, left, top + 8.0 * LINE_HEIGHT, "Tips:", salient);
            let tips = [
                ("SPC j j", "open today's journal"),
                ("SPC f f", "find a page"),
                ("SPC n", "create a new page"),
                ("SPC ?", "all commands"),
            ];
            for (index, (keys, description)) in tips.iter().enumerate() {
                let y = top + (9 + index) as f32 * LINE_HEIGHT;
                wizard_line(frame, left + 2.0 * CHAR_WIDTH, y, keys, salient);
                wizard_line(frame, left + 14.0 * CHAR_WIDTH, y, description, fg);
            }
            wizard_line(
                frame,
                right - text_width("Press Enter to open your journal"),
                bottom - 2.0 * LINE_HEIGHT,
                "Press Enter to open your journal",
                faded,
            );
        }
    }
}

fn wizard_line(
    frame: &mut iced::widget::canvas::Frame,
    x: f32,
    y: f32,
    text: &str,
    color: iced::Color,
) {
    draw_text(frame, x, y, text, color);
}

fn month_name(month: u32) -> &'static str {
    match month {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => "Unknown",
    }
}

fn date_parts(date: &impl ToString) -> (i32, u32, u32) {
    let text = date.to_string();
    let mut parts = text.split('-');
    let year = parts
        .next()
        .and_then(|part| part.parse::<i32>().ok())
        .unwrap_or_default();
    let month = parts
        .next()
        .and_then(|part| part.parse::<u32>().ok())
        .unwrap_or_default();
    let day = parts
        .next()
        .and_then(|part| part.parse::<u32>().ok())
        .unwrap_or_default();
    (year, month, day)
}

#[allow(clippy::too_many_arguments)]
fn draw_input_row(
    frame: &mut iced::widget::canvas::Frame,
    left: f32,
    y: f32,
    right: f32,
    label: &str,
    value: &str,
    cursor: usize,
    theme: &ThemePalette,
) {
    let label_w = text_width(label);
    draw_text(frame, left, y, label, rgb_to_color(&theme.foreground));
    let input_x = left + label_w;
    let input_w = (right - input_x).max(10.0 * CHAR_WIDTH);
    fill_rect(
        frame,
        rect(input_x, y, input_w, LINE_HEIGHT),
        rgb_to_color(&theme.modeline),
    );
    draw_text(
        frame,
        input_x + 2.0,
        y,
        truncate_text(value, chars_that_fit(input_w).saturating_sub(1)),
        rgb_to_color(&theme.foreground),
    );
    draw_bar_cursor(
        frame,
        (input_x + cursor as f32 * CHAR_WIDTH + 2.0).min(input_x + input_w - 2.0),
        y,
        LINE_HEIGHT,
        rgb_to_color(&theme.foreground),
    );
}

fn draw_choice(
    frame: &mut iced::widget::canvas::Frame,
    x: f32,
    y: f32,
    label: &str,
    selected: bool,
    theme: &ThemePalette,
) {
    if selected {
        fill_rect(
            frame,
            rect(
                x - 2.0,
                y,
                text_width(label) + 4.0 * CHAR_WIDTH,
                LINE_HEIGHT,
            ),
            rgb_to_color(&theme.mild),
        );
    }
    let prefix = if selected { "▸ " } else { "  " };
    draw_text(
        frame,
        x,
        y,
        format!("{prefix}{label}"),
        rgb_to_color(if selected {
            &theme.strong
        } else {
            &theme.foreground
        }),
    );
}

// ---------------------------------------------------------------------------
// View overlay (BQL / Agenda)
// ---------------------------------------------------------------------------

pub(crate) fn draw_view(
    frame: &mut iced::widget::canvas::Frame,
    area: Rectangle,
    view_frame: &ViewFrame,
    theme: &ThemePalette,
) {
    let content_chars = chars_that_fit(area.width).saturating_sub(4);
    let max_visible: usize = 12;
    let num_results = view_frame.rows.len().min(max_visible);
    let panel_top = area.y;
    let panel_bottom = area.y + area.height;
    let available_h = panel_bottom - panel_top;
    let available_lines = (available_h / LINE_HEIGHT) as usize;

    // Lines needed: title(1) + rows + status(1) + optional query(1) + optional error(1).
    let extra = 2
        + if view_frame.is_prompt { 1 } else { 0 }
        + if view_frame.error.is_some() { 1 } else { 0 };
    let num_visible = num_results.min(available_lines.saturating_sub(extra + 1));

    // Layout from bottom up within the allocated rect.
    let mut bottom_y = panel_bottom;

    // Query line (if prompt mode) — bottom-most element.
    let query_y = if view_frame.is_prompt {
        bottom_y -= LINE_HEIGHT;
        Some(bottom_y)
    } else {
        None
    };

    // Status / footer line.
    bottom_y -= LINE_HEIGHT;
    let status_y = bottom_y;

    // Rows area.
    let rows_bottom_y = bottom_y;
    let rows_top_y = rows_bottom_y - num_visible as f32 * LINE_HEIGHT;

    // Error line (if present).
    let error_y = if view_frame.error.is_some() {
        let ey = rows_top_y - LINE_HEIGHT;
        Some(ey)
    } else {
        None
    };

    // Title line.
    let title_y = error_y.unwrap_or(rows_top_y) - LINE_HEIGHT;

    let actual_top = title_y - LINE_HEIGHT * 0.5;

    // Opaque background.
    fill_rect(
        frame,
        rect(area.x, actual_top, area.width, panel_bottom - actual_top),
        rgb_to_color(&theme.background),
    );

    // Top separator.
    draw_hline(
        frame,
        area.x,
        area.x + area.width,
        actual_top,
        rgb_to_color(&theme.faded),
    );

    // ── Title ──
    draw_text(
        frame,
        area.x + SPACING_MD,
        title_y,
        truncate_text(&view_frame.title, content_chars),
        rgb_to_color(&theme.strong),
    );

    // ── Error ──
    if let Some(ey) = error_y {
        if let Some(err) = &view_frame.error {
            draw_text(
                frame,
                area.x + SPACING_MD,
                ey,
                truncate_text(err, content_chars),
                rgb_to_color(&theme.critical),
            );
        }
    }

    // ── Result rows ──
    let right_margin = area.x + area.width - SPACING_MD;
    for (vi, row) in view_frame.rows.iter().take(num_visible).enumerate() {
        let y = rows_top_y + vi as f32 * LINE_HEIGHT;
        let is_selected = vi == view_frame.selected;
        if is_selected {
            fill_rect(
                frame,
                rect(area.x, y, area.width, LINE_HEIGHT),
                rgb_to_color(&theme.mild),
            );
        }
        match row {
            ViewRow::SectionHeader(title) => {
                draw_text(
                    frame,
                    area.x + SPACING_MD + CHAR_WIDTH,
                    y,
                    truncate_text(title, content_chars.saturating_sub(2)),
                    rgb_to_color(&theme.salient),
                );
            }
            ViewRow::Data {
                cells,
                is_task,
                task_done,
            } => {
                let prefix = if *is_task {
                    if *task_done {
                        "  [x] "
                    } else {
                        "  [ ] "
                    }
                } else {
                    "  "
                };
                let checkbox_color = if *task_done {
                    rgb_to_color(&theme.accent_green)
                } else if *is_task {
                    rgb_to_color(&theme.accent_yellow)
                } else {
                    rgb_to_color(&theme.foreground)
                };
                draw_text(
                    frame,
                    area.x + SPACING_MD,
                    y,
                    prefix.to_string(),
                    checkbox_color,
                );

                let text_x = area.x + SPACING_MD + prefix.len() as f32 * CHAR_WIDTH;
                let text_color = if *task_done {
                    &theme.faded
                } else {
                    &theme.foreground
                };

                // First cell is the main text; remaining cells go to the right margin.
                if let Some((first, rest)) = cells.split_first() {
                    let marginalia = rest.join("  ");
                    let margin_chars = marginalia.chars().count();
                    let main_chars = content_chars
                        .saturating_sub(prefix.len())
                        .saturating_sub(margin_chars + 2);
                    draw_text(
                        frame,
                        text_x,
                        y,
                        truncate_text(first, main_chars),
                        rgb_to_color(text_color),
                    );
                    if !marginalia.is_empty() {
                        draw_text_right(
                            frame,
                            right_margin,
                            y,
                            &truncate_text(&marginalia, margin_chars),
                            rgb_to_color(&theme.faded),
                        );
                    }
                }
            }
        }
    }

    // ── Status line ──
    let footer = format!(
        "{} of {} results",
        if view_frame.total > 0 {
            view_frame.selected + 1
        } else {
            0
        },
        view_frame.total,
    );
    draw_text(
        frame,
        area.x + SPACING_MD,
        status_y,
        truncate_text(&footer, content_chars),
        rgb_to_color(&theme.faded),
    );

    // ── Query line (prompt mode) ──
    if let Some(qy) = query_y {
        draw_hline(
            frame,
            area.x,
            area.x + area.width,
            qy,
            rgb_to_color(&theme.faded),
        );
        let prompt = format!("{} > ", view_frame.title);
        draw_text(
            frame,
            area.x + SPACING_MD,
            qy,
            truncate_text(&prompt, content_chars),
            rgb_to_color(&theme.faded),
        );
        let query_x = area.x + SPACING_MD + text_width(&prompt);
        draw_text(
            frame,
            query_x,
            qy,
            truncate_text(
                &view_frame.query,
                content_chars.saturating_sub(prompt.chars().count()),
            ),
            rgb_to_color(&theme.foreground),
        );
        draw_bar_cursor(
            frame,
            (query_x + view_frame.query_cursor as f32 * CHAR_WIDTH)
                .min(area.x + area.width - SPACING_MD - 2.0),
            qy,
            LINE_HEIGHT,
            rgb_to_color(&theme.foreground),
        );
    }
}
