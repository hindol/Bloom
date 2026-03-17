use bloom_core::render::{
    DatePickerFrame, DialogFrame, ImportChoice, PickerFrame, SetupStep, SetupWizardFrame,
    ViewFrame, ViewRow,
};
use bloom_md::theme::ThemePalette;
use iced::Size;

use crate::draw::{
    chars_that_fit, draw_bar_cursor, draw_hline, draw_overlay_scrim, draw_text,
    draw_text_center, draw_text_right, fill_panel, fill_rect, inset, rect, stroke_rect,
    text_width, truncate_text,
};
use crate::theme::rgb_to_color;
use crate::{CHAR_WIDTH, LINE_HEIGHT};

pub(crate) fn draw_picker(
    frame: &mut iced::widget::canvas::Frame,
    size: Size,
    picker: &PickerFrame,
    theme: &ThemePalette,
) {
    draw_overlay_scrim(frame, size, rgb_to_color(&theme.background), 0.45);

    let width = (size.width * if picker.wide { 0.72 } else { 0.60 }).max(40.0 * CHAR_WIDTH);
    let height = (size.height * 0.70).max(12.0 * LINE_HEIGHT);
    let area = rect(
        (size.width - width).max(0.0) / 2.0,
        (size.height - height).max(0.0) / 2.0,
        width.min(size.width - 8.0),
        height.min(size.height - 8.0),
    );
    fill_panel(
        frame,
        area,
        rgb_to_color(&theme.background),
        rgb_to_color(&theme.faded),
    );

    let inner = inset(area, CHAR_WIDTH);
    let total_lines = ((inner.height / LINE_HEIGHT).floor() as usize).max(8);
    let preview_lines = if picker.preview.is_some() {
        (total_lines / 4).clamp(4, 8)
    } else {
        0
    };
    let separator_lines = usize::from(preview_lines > 0);
    let result_lines = total_lines.saturating_sub(3 + preview_lines + separator_lines);
    let content_chars = chars_that_fit(inner.width).saturating_sub(2);

    draw_text(
        frame,
        inner.x,
        inner.y,
        truncate_text(&picker.title, content_chars),
        rgb_to_color(&theme.strong),
    );
    draw_hline(
        frame,
        area.x + 1.0,
        area.x + area.width - 1.0,
        inner.y + LINE_HEIGHT - 3.0,
        rgb_to_color(&theme.faded),
    );

    let query_y = inner.y + LINE_HEIGHT;
    draw_text(frame, inner.x, query_y, ">", rgb_to_color(&theme.faded));
    let query_x = inner.x + 2.0 * CHAR_WIDTH;
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
        truncate_text(&picker.query, content_chars.saturating_sub(2)),
        rgb_to_color(&theme.foreground),
    );
    draw_bar_cursor(
        frame,
        (query_x + picker.query.chars().count() as f32 * CHAR_WIDTH)
            .min(inner.x + inner.width - 2.0),
        query_y,
        rgb_to_color(&theme.foreground),
    );

    let result_y = inner.y + 2.0 * LINE_HEIGHT;
    let viewport = result_lines.max(1);
    let scroll = if picker.selected_index >= viewport {
        picker.selected_index - viewport + 1
    } else {
        0
    };
    let visible = picker.results.iter().skip(scroll).take(viewport);
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

    for (visible_index, row) in visible.enumerate() {
        let y = result_y + visible_index as f32 * LINE_HEIGHT;
        let selected = scroll + visible_index == picker.selected_index;
        if selected {
            fill_rect(
                frame,
                rect(inner.x - 2.0, y, inner.width + 4.0, LINE_HEIGHT),
                rgb_to_color(&theme.mild),
            );
        }
        draw_text(
            frame,
            inner.x,
            y,
            format!(" {}", truncate_text(&row.label, label_chars)),
            rgb_to_color(if selected { &theme.strong } else { &theme.foreground }),
        );
        if let Some(middle) = &row.middle {
            draw_text(
                frame,
                inner.x + (label_chars + 3) as f32 * CHAR_WIDTH,
                y,
                truncate_text(middle, middle_chars),
                rgb_to_color(if selected { &theme.foreground } else { &theme.faded }),
            );
        }
        if let Some(right) = &row.right {
            draw_text_right(
                frame,
                inner.x + inner.width,
                y,
                &truncate_text(right, right_chars),
                rgb_to_color(if selected { &theme.foreground } else { &theme.faded }),
            );
        }
    }

    if picker.results.is_empty() && picker.min_query_len > 0 && picker.query.len() < picker.min_query_len {
        draw_text_center(
            frame,
            inner,
            result_y + LINE_HEIGHT,
            "Type to search…",
            rgb_to_color(&theme.faded),
        );
    }

    let footer_y = result_y + result_lines as f32 * LINE_HEIGHT;
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
        inner.x,
        footer_y,
        truncate_text(&footer, content_chars),
        rgb_to_color(&theme.faded),
    );

    if let Some(preview) = &picker.preview {
        let sep_y = footer_y + LINE_HEIGHT - 3.0;
        draw_hline(
            frame,
            area.x + 1.0,
            area.x + area.width - 1.0,
            sep_y,
            rgb_to_color(&theme.faded),
        );
        let start_y = footer_y + LINE_HEIGHT;
        let preview_chars = chars_that_fit(inner.width).saturating_sub(1);
        for (index, line) in preview.lines().take(preview_lines).enumerate() {
            draw_text(
                frame,
                inner.x,
                start_y + index as f32 * LINE_HEIGHT,
                truncate_text(line, preview_chars),
                rgb_to_color(&theme.faded),
            );
        }
    }
}

pub(crate) fn draw_dialog(
    frame: &mut iced::widget::canvas::Frame,
    size: Size,
    dialog: &DialogFrame,
    theme: &ThemePalette,
) {
    draw_overlay_scrim(frame, size, rgb_to_color(&theme.background), 0.45);

    let width = (size.width * 0.5).max(30.0 * CHAR_WIDTH).min(size.width - 8.0);
    let height = (4.5 * LINE_HEIGHT).min(size.height - 8.0);
    let area = rect(
        (size.width - width).max(0.0) / 2.0,
        (size.height - height).max(0.0) / 2.0,
        width,
        height,
    );
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
            fill_rect(frame, rect(x - 2.0, y, w, LINE_HEIGHT), rgb_to_color(&theme.mild));
        }
        draw_text(
            frame,
            x,
            y,
            label,
            rgb_to_color(if index == dialog.selected { &theme.strong } else { &theme.foreground }),
        );
        x += w + CHAR_WIDTH;
    }
}

pub(crate) fn draw_date_picker(
    frame: &mut iced::widget::canvas::Frame,
    size: Size,
    picker: &DatePickerFrame,
    theme: &ThemePalette,
) {
    draw_overlay_scrim(frame, size, rgb_to_color(&theme.background), 0.35);

    let width = 34.0 * CHAR_WIDTH;
    let height = 11.0 * LINE_HEIGHT;
    let area = rect(
        (size.width - width).max(0.0) / 2.0,
        (size.height - height).max(0.0) / 2.0,
        width.min(size.width - 8.0),
        height.min(size.height - 8.0),
    );
    fill_panel(
        frame,
        area,
        rgb_to_color(&theme.background),
        rgb_to_color(&theme.faded),
    );

    let inner = inset(area, CHAR_WIDTH);
    let month_name = month_name(picker.month);
    draw_text_center(
        frame,
        inner,
        inner.y,
        &truncate_text(&picker.prompt, chars_that_fit(inner.width).saturating_sub(1)),
        rgb_to_color(&theme.faded),
    );
    draw_text_center(
        frame,
        inner,
        inner.y + LINE_HEIGHT,
        &format!("{} {}", month_name, picker.year),
        rgb_to_color(&theme.strong),
    );
    draw_text(
        frame,
        inner.x,
        inner.y + 2.0 * LINE_HEIGHT,
        "Mo  Tu  We  Th  Fr  Sa  Su",
        rgb_to_color(&theme.faded),
    );

    let (selected_year, selected_month_num, selected_day) = date_parts(&picker.selected_date);
    let selected_month = selected_year == picker.year && selected_month_num == picker.month;
    let (today_year, today_month_num, today_day) = date_parts(&picker.today);
    let today_month = today_year == picker.year && today_month_num == picker.month;

    for (week_index, week) in picker.month_view.iter().enumerate() {
        let y = inner.y + (3 + week_index) as f32 * LINE_HEIGHT;
        for (day_index, day) in week.iter().enumerate() {
            let x = inner.x + day_index as f32 * 4.0 * CHAR_WIDTH;
            let cell = rect(x, y, 3.0 * CHAR_WIDTH, LINE_HEIGHT);
            match day {
                Some(day) => {
                    let selected = selected_month && *day == selected_day;
                    let today = today_month && *day == today_day;
                    let has_journal = picker.journal_days.contains(day);
                    if selected {
                        fill_rect(frame, cell, rgb_to_color(&theme.salient));
                    }
                    if today {
                        stroke_rect(frame, cell, rgb_to_color(&theme.accent_yellow));
                    }
                    if has_journal {
                        draw_text(frame, x, y, "◆", rgb_to_color(&theme.salient));
                    }
                    draw_text(
                        frame,
                        x + CHAR_WIDTH,
                        y,
                        format!("{:>2}", day),
                        rgb_to_color(if selected { &theme.background } else { &theme.foreground }),
                    );
                }
                None => draw_text(frame, x, y, "   ", rgb_to_color(&theme.faded)),
            }
        }
    }

    let footer = format!("{} journal days", picker.journal_days.len());
    draw_text_center(
        frame,
        inner,
        inner.y + 10.0 * LINE_HEIGHT,
        &footer,
        rgb_to_color(&theme.faded),
    );
}

pub(crate) fn draw_setup_wizard(
    frame: &mut iced::widget::canvas::Frame,
    size: Size,
    wizard: &SetupWizardFrame,
    theme: &ThemePalette,
) {
    fill_rect(
        frame,
        rect(0.0, 0.0, size.width, size.height),
        rgb_to_color(&theme.background),
    );
    stroke_rect(
        frame,
        rect(1.0, 1.0, (size.width - 2.0).max(0.0), (size.height - 2.0).max(0.0)),
        rgb_to_color(&theme.faded),
    );

    let left = 9.0 * CHAR_WIDTH;
    let top = (size.height * 0.2).max(2.0 * LINE_HEIGHT);
    let right = size.width - 3.0 * CHAR_WIDTH;
    let content_chars = chars_that_fit((right - left).max(0.0));

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
                size.height - 2.0 * LINE_HEIGHT,
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
            wizard_line(frame, left, top + 7.0 * LINE_HEIGHT, "Bloom will create:", faded);
            wizard_line(frame, left + 2.0 * CHAR_WIDTH, top + 8.0 * LINE_HEIGHT, "pages/     — topic pages", faded);
            wizard_line(frame, left + 2.0 * CHAR_WIDTH, top + 9.0 * LINE_HEIGHT, "journal/   — daily journal", faded);
            wizard_line(frame, left + 2.0 * CHAR_WIDTH, top + 10.0 * LINE_HEIGHT, "templates/ — page templates", faded);
            wizard_line(frame, left + 2.0 * CHAR_WIDTH, top + 11.0 * LINE_HEIGHT, "images/    — attachments", faded);
            if let Some(message) = &wizard.error {
                wizard_line(
                    frame,
                    left,
                    top + 13.0 * LINE_HEIGHT,
                    &format!("✗ {}", truncate_text(message, content_chars)),
                    error,
                );
            }
            wizard_line(frame, left, size.height - 2.0 * LINE_HEIGHT, "Esc back", faded);
            wizard_line(
                frame,
                right - text_width("Enter to confirm"),
                size.height - 2.0 * LINE_HEIGHT,
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
            draw_choice(frame, left, top + 6.0 * LINE_HEIGHT, "No, start fresh", no_selected, theme);
            draw_choice(
                frame,
                left,
                top + 7.0 * LINE_HEIGHT,
                "Yes, import from Logseq",
                yes_selected,
                theme,
            );
            wizard_line(frame, left, size.height - 2.0 * LINE_HEIGHT, "Esc back", faded);
            wizard_line(
                frame,
                right - text_width("↑↓ select   Enter to confirm"),
                size.height - 2.0 * LINE_HEIGHT,
                "↑↓ select   Enter to confirm",
                faded,
            );
        }
        SetupStep::ImportPath => {
            wizard_line(frame, left, top, "Import from Logseq", heading);
            wizard_line(frame, left, top + 2.0 * LINE_HEIGHT, "Enter the path to your Logseq vault:", fg);
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
            wizard_line(frame, left, size.height - 2.0 * LINE_HEIGHT, "Esc back", faded);
            wizard_line(
                frame,
                right - text_width("Enter to start import"),
                size.height - 2.0 * LINE_HEIGHT,
                "Enter to start import",
                faded,
            );
        }
        SetupStep::ImportRunning => {
            wizard_line(frame, left, top, "Importing from Logseq...", heading);
            if let Some(progress) = &wizard.import_progress {
                let bar_width = (size.width * 0.35).max(20.0 * CHAR_WIDTH);
                let bar_area = rect(left, top + 2.0 * LINE_HEIGHT, bar_width, LINE_HEIGHT);
                fill_rect(frame, bar_area, rgb_to_color(&theme.subtle));
                let ratio = if progress.total > 0 {
                    progress.done as f32 / progress.total as f32
                } else {
                    0.0
                };
                fill_rect(
                    frame,
                    rect(bar_area.x, bar_area.y, bar_area.width * ratio.clamp(0.0, 1.0), bar_area.height),
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
                        size.height - 2.0 * LINE_HEIGHT,
                        "Press Enter to continue",
                        faded,
                    );
                }
            }
        }
        SetupStep::Complete => {
            wizard_line(frame, left, top + 2.0 * LINE_HEIGHT, "Your vault is ready 🌱", heading);
            wizard_line(frame, left, top + 4.0 * LINE_HEIGHT, &format!("Location: {}", wizard.vault_path), fg);
            wizard_line(frame, left, top + 5.0 * LINE_HEIGHT, &format!("Pages: {}", wizard.stats.pages), fg);
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
                size.height - 2.0 * LINE_HEIGHT,
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
            rect(x - 2.0, y, text_width(label) + 4.0 * CHAR_WIDTH, LINE_HEIGHT),
            rgb_to_color(&theme.mild),
        );
    }
    let prefix = if selected { "▸ " } else { "  " };
    draw_text(
        frame,
        x,
        y,
        format!("{prefix}{label}"),
        rgb_to_color(if selected { &theme.strong } else { &theme.foreground }),
    );
}

// ---------------------------------------------------------------------------
// View overlay (BQL / Agenda)
// ---------------------------------------------------------------------------

pub(crate) fn draw_view(
    frame: &mut iced::widget::canvas::Frame,
    size: Size,
    view_frame: &ViewFrame,
    theme: &ThemePalette,
) {
    draw_overlay_scrim(frame, size, rgb_to_color(&theme.background), 0.45);
    let margin = 2.0 * CHAR_WIDTH;
    let panel = rect(margin, margin, size.width - 2.0 * margin, size.height - 2.0 * margin);
    fill_panel(frame, panel, rgb_to_color(&theme.subtle), rgb_to_color(&theme.faded));

    let inner = inset(panel, CHAR_WIDTH);
    let max_chars = chars_that_fit(inner.width);
    let mut y = inner.y;

    // Title.
    draw_text(frame, inner.x, y, view_frame.title.clone(), rgb_to_color(&theme.strong));
    y += LINE_HEIGHT;

    // Query (if prompt mode).
    if view_frame.is_prompt {
        let query_display = format!("  > {}", view_frame.query);
        draw_text(frame, inner.x, y, truncate_text(&query_display, max_chars), rgb_to_color(&theme.foreground));
        let cursor_x = inner.x + (4 + view_frame.query_cursor) as f32 * CHAR_WIDTH;
        draw_bar_cursor(frame, cursor_x, y, rgb_to_color(&theme.foreground));
        y += LINE_HEIGHT;
    }

    // Error.
    if let Some(err) = &view_frame.error {
        draw_text(frame, inner.x, y, truncate_text(err, max_chars), rgb_to_color(&theme.critical));
        y += LINE_HEIGHT;
    }

    y += LINE_HEIGHT * 0.5;

    // Rows.
    let max_rows = ((inner.y + inner.height - y) / LINE_HEIGHT) as usize;
    for (i, row) in view_frame.rows.iter().enumerate().take(max_rows) {
        let is_selected = i == view_frame.selected;
        if is_selected {
            fill_rect(frame, rect(inner.x, y, inner.width, LINE_HEIGHT), rgb_to_color(&theme.mild));
        }
        match row {
            ViewRow::SectionHeader(title) => {
                draw_text(frame, inner.x + CHAR_WIDTH, y, truncate_text(title, max_chars - 2), rgb_to_color(&theme.salient));
            }
            ViewRow::Data { cells, is_task, task_done } => {
                let prefix = if *is_task {
                    if *task_done { "  [x] " } else { "  [ ] " }
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
                draw_text(frame, inner.x, y, prefix.to_string(), checkbox_color);

                let text_x = inner.x + prefix.len() as f32 * CHAR_WIDTH;
                let remaining = max_chars.saturating_sub(prefix.len());
                let cell_text = cells.join("  ");
                let text_color = if *task_done { &theme.faded } else { &theme.foreground };
                draw_text(frame, text_x, y, truncate_text(&cell_text, remaining), rgb_to_color(text_color));
            }
        }
        y += LINE_HEIGHT;
    }

    // Footer.
    let footer_y = inner.y + inner.height - LINE_HEIGHT;
    let footer = format!("  {} of {} results", view_frame.selected + 1, view_frame.total);
    draw_text(frame, inner.x, footer_y, footer, rgb_to_color(&theme.faded));
}
