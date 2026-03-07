use super::*;

pub(super) fn draw_agenda(
    f: &mut Frame,
    area: Rect,
    agenda: &AgendaFrame,
    theme: &TuiTheme,
    scrolloff: usize,
) {
    // Full-screen overlay
    f.render_widget(Clear, area);
    f.render_widget(
        Block::default().style(RStyle::default().bg(theme.background())),
        area,
    );

    if area.height < 4 {
        return;
    }

    // Apply left/right margins
    let margin_h = 3u16;
    let padded = Rect::new(
        area.x + margin_h,
        area.y,
        area.width.saturating_sub(margin_h * 2),
        area.height,
    );

    // Split: top ~60% for task list + footer, 1 separator, bottom ~40% for preview
    let has_preview = agenda.preview.is_some() && padded.height >= 10;
    let preview_h = if has_preview {
        (padded.height * 2 / 5).max(3)
    } else {
        0
    };
    let separator_h: u16 = if has_preview { 1 } else { 0 };
    let top_h = padded
        .height
        .saturating_sub(preview_h)
        .saturating_sub(separator_h);

    // Footer takes 1 line at bottom of the top section
    let list_h = top_h.saturating_sub(1);
    let footer_y = padded.y + list_h;

    // Build flat list with section markers for rendering
    struct Section<'a> {
        label: &'a str,
        items: &'a [bloom_core::render::AgendaItem],
        fg: ratatui::style::Color,
    }

    let today_str = chrono::Local::now().format("%Y-%m-%d").to_string();
    let sections = [
        Section {
            label: "Overdue",
            items: &agenda.overdue,
            fg: theme.critical(),
        },
        Section {
            label: &format!("Today · {today_str}"),
            items: &agenda.today,
            fg: theme.foreground(),
        },
        Section {
            label: "Upcoming",
            items: &agenda.upcoming,
            fg: theme.faded(),
        },
    ];

    // Pre-compute max column widths across all items for alignment
    let all_items = agenda
        .overdue
        .iter()
        .chain(agenda.today.iter())
        .chain(agenda.upcoming.iter());
    let max_source_w: usize = all_items
        .clone()
        .map(|i| i.source_page.width())
        .max()
        .unwrap_or(0)
        .min(padded.width as usize / 3);
    let max_date_w: usize = all_items
        .map(|i| {
            i.date
                .map(|d| d.format("%Y-%m-%d").to_string().width())
                .unwrap_or(0)
        })
        .max()
        .unwrap_or(0);
    let source_col_w = max_source_w + 2;
    let date_col_w = if max_date_w > 0 { max_date_w + 2 } else { 0 };

    // Build flat visual rows with spacing
    struct VisualRow<'a> {
        kind: RowKind<'a>,
        section_fg: ratatui::style::Color,
    }
    enum RowKind<'a> {
        Blank,
        Header(&'a str),
        Item {
            item: &'a bloom_core::render::AgendaItem,
            flat_idx: usize,
            is_overdue: bool,
        },
    }

    let mut rows: Vec<VisualRow> = Vec::new();
    let mut flat_idx: usize = 0;
    let mut _is_first_section = true;
    for section in &sections {
        if section.items.is_empty() {
            continue;
        }
        // Blank line before section header (including first — gives top margin)
        rows.push(VisualRow {
            kind: RowKind::Blank,
            section_fg: section.fg,
        });
        rows.push(VisualRow {
            kind: RowKind::Header(section.label),
            section_fg: section.fg,
        });
        // Blank line after header
        rows.push(VisualRow {
            kind: RowKind::Blank,
            section_fg: section.fg,
        });
        for item in section.items {
            rows.push(VisualRow {
                kind: RowKind::Item {
                    item,
                    flat_idx,
                    is_overdue: std::ptr::eq(section.items, &*agenda.overdue),
                },
                section_fg: section.fg,
            });
            flat_idx += 1;
        }
        _is_first_section = false;
    }

    // Find the visual row of the selected item
    let selected_visual_row = rows.iter().position(|r| {
        matches!(&r.kind, RowKind::Item { flat_idx: fi, .. } if *fi == agenda.selected_index)
    }).unwrap_or(0);

    // Scroll offset with Vim-style margin
    let viewport_h = list_h as usize;
    let scroll_offset = {
        let min_offset_bottom = (selected_visual_row + scrolloff + 1).saturating_sub(viewport_h);
        let max_offset_top = selected_visual_row.saturating_sub(scrolloff);
        let max_possible = rows.len().saturating_sub(viewport_h);
        min_offset_bottom
            .min(max_possible)
            .max(min_offset_bottom)
            .min(max_offset_top.max(min_offset_bottom))
    };

    // Render visible rows
    let mut y = padded.y;
    for row in rows.iter().skip(scroll_offset).take(viewport_h) {
        if y >= padded.y + list_h {
            break;
        }
        match &row.kind {
            RowKind::Blank => {
                y += 1;
            }
            RowKind::Header(label) => {
                let header_style = RStyle::default()
                    .fg(theme.salient())
                    .add_modifier(Modifier::BOLD);
                f.render_widget(
                    Paragraph::new(Line::from(Span::styled(label.to_string(), header_style))),
                    Rect::new(padded.x, y, padded.width, 1),
                );
                y += 1;
            }
            RowKind::Item {
                item,
                flat_idx: fi,
                is_overdue,
            } => {
                let is_selected = *fi == agenda.selected_index;
                let base_style = if is_selected {
                    RStyle::default().fg(row.section_fg).bg(theme.mild())
                } else {
                    RStyle::default().fg(row.section_fg)
                };

                let marker = if is_selected { "▸ " } else { "  " };
                let date_str = item
                    .date
                    .map(|d| d.format("%Y-%m-%d").to_string())
                    .unwrap_or_default();
                let available = padded.width as usize;
                let marker_w = 5; // "▸ ☐ " or "  ☐ "

                // Strip @due/@start/@at timestamps from display text (already in date column)
                let clean_text = strip_timestamps(&item.task_text);
                let task_max = available.saturating_sub(marker_w + source_col_w + date_col_w);
                let task_text = truncate_to_width(&clean_text, task_max);
                let task_w = task_text.width();
                let task_pad = task_max.saturating_sub(task_w);

                let source_padded = format!(
                    "{}{}",
                    " ".repeat(source_col_w.saturating_sub(item.source_page.width())),
                    truncate_to_width(&item.source_page, max_source_w),
                );
                let date_padded = if date_col_w > 0 {
                    format!(
                        "{}{}",
                        " ".repeat(date_col_w.saturating_sub(date_str.width())),
                        date_str,
                    )
                } else {
                    String::new()
                };

                let date_fg = if *is_overdue {
                    theme.critical()
                } else {
                    theme.faded()
                };
                let source_style = if is_selected {
                    RStyle::default().fg(theme.faded()).bg(theme.mild())
                } else {
                    theme.faded_style()
                };
                let date_style = if is_selected {
                    RStyle::default().fg(date_fg).bg(theme.mild())
                } else {
                    RStyle::default().fg(date_fg)
                };

                let left_prefix = format!("{marker}☐ ");

                // Full semantic highlighting — same pipeline as the editor
                let parser = bloom_core::parser::BloomMarkdownParser::new();
                let ctx = bloom_core::parser::traits::LineContext::default();
                use bloom_core::parser::traits::DocumentParser as _;
                let styled_spans = parser.highlight_line(&task_text, &ctx);

                let sel_bg = if is_selected {
                    Some(theme.mild())
                } else {
                    None
                };
                let mut task_spans: Vec<Span> = Vec::new();
                task_spans.push(Span::styled(left_prefix, base_style));

                if styled_spans.is_empty() {
                    task_spans.push(Span::styled(task_text.as_str(), base_style));
                } else {
                    let mut last_end = 0usize;
                    for si in &styled_spans {
                        let s = si.range.start.min(task_text.len());
                        let e = si.range.end.min(task_text.len());
                        if s > last_end {
                            task_spans.push(Span::styled(&task_text[last_end..s], base_style));
                        }
                        if s < e {
                            let mut resolved = theme.style_for(&si.style);
                            if let Some(bg) = sel_bg {
                                resolved = resolved.bg(bg);
                            }
                            task_spans.push(Span::styled(&task_text[s..e], resolved));
                        }
                        last_end = e;
                    }
                    if last_end < task_text.len() {
                        task_spans.push(Span::styled(&task_text[last_end..], base_style));
                    }
                }

                task_spans.push(Span::styled(" ".repeat(task_pad), base_style));
                task_spans.push(Span::styled(source_padded, source_style));
                task_spans.push(Span::styled(date_padded, date_style));

                let line = Line::from(task_spans);
                f.render_widget(
                    Paragraph::new(line),
                    Rect::new(padded.x, y, padded.width, 1),
                );
                y += 1;
            }
        }
    }

    // Footer
    if footer_y < padded.bottom() {
        let selection = if agenda.total_open > 0 {
            format!("{}", agenda.selected_index + 1)
        } else {
            "0".to_string()
        };
        let footer = format!(
            "▸ {selection}/{} tasks   {} pages   [x]toggle [Enter]jump [q]close",
            agenda.total_open, agenda.total_pages,
        );
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(footer, theme.faded_style()))),
            Rect::new(padded.x, footer_y, padded.width, 1),
        );
    }

    // Separator + Preview
    if has_preview {
        let sep_y = padded.y + top_h;
        if sep_y < padded.bottom() {
            let sep = "─".repeat(padded.width as usize);
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(sep, theme.faded_style()))),
                Rect::new(padded.x, sep_y, padded.width, 1),
            );
        }

        let preview_y = sep_y + 2; // 1 blank line after separator
        if let Some(text) = &agenda.preview {
            let preview_area = Rect::new(
                padded.x + 2,
                preview_y,
                padded.width.saturating_sub(4),
                padded.bottom().saturating_sub(preview_y),
            );
            super::picker::render_highlighted_preview(f, preview_area, text, "", theme);
        }
    }

    // Place cursor on the selected item row (overrides editor cursor)
    let selected_screen_y = padded.y + (selected_visual_row.saturating_sub(scroll_offset)) as u16;
    if selected_screen_y < padded.y + list_h {
        f.set_cursor_position((padded.x, selected_screen_y));
    }
}
