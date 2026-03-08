use super::*;

pub(super) fn draw_setup_wizard(
    f: &mut Frame,
    area: Rect,
    sw: &bloom_core::render::SetupWizardFrame,
    theme: &TuiTheme,
) {
    use bloom_core::render::{ImportChoice, SetupStep};

    // Fill background
    f.render_widget(
        Block::default().style(RStyle::default().bg(theme.background())),
        area,
    );

    // Border around full screen
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border_style())
        .style(RStyle::default().bg(theme.background()));
    let inner = block.inner(area);
    f.render_widget(block, area);

    // Center content vertically (use top third as offset)
    let y_start = inner.y + inner.height / 5;
    let heading_style = RStyle::default()
        .fg(theme.strong())
        .add_modifier(Modifier::BOLD);
    let text_style = RStyle::default().fg(theme.foreground());
    let faded = theme.faded_style();
    let error_style = RStyle::default().fg(theme.critical());

    let cx = inner.x + 9; // indent for content

    match sw.step {
        SetupStep::Welcome => {
            let title_y = y_start + 2;
            render_line(f, cx, title_y, inner.width, "Bloom 🌱", heading_style);
            render_line(
                f,
                cx,
                title_y + 2,
                inner.width,
                "A local-first, keyboard-driven note-taking app.",
                text_style,
            );
            render_line(
                f,
                cx,
                title_y + 4,
                inner.width,
                "Your notes are stored as Markdown files in a",
                text_style,
            );
            render_line(
                f,
                cx,
                title_y + 5,
                inner.width,
                "single folder called a vault. No cloud, no sync \u{2014}",
                text_style,
            );
            render_line(
                f,
                cx,
                title_y + 6,
                inner.width,
                "everything stays on your machine.",
                text_style,
            );

            // Bottom prompt
            let prompt_y = inner.bottom().saturating_sub(2);
            let prompt = "Press Enter to get started";
            let px = inner.right().saturating_sub(prompt.len() as u16 + 2);
            render_line(f, px, prompt_y, inner.width, prompt, faded);
        }

        SetupStep::ChooseVaultLocation => {
            let y = y_start;
            render_line(
                f,
                cx,
                y,
                inner.width,
                "Choose vault location",
                heading_style,
            );
            render_line(
                f,
                cx,
                y + 2,
                inner.width,
                "This is where your notes, journal, and config",
                text_style,
            );
            render_line(
                f,
                cx,
                y + 3,
                inner.width,
                "will live. You can move it later.",
                text_style,
            );

            // Path input
            let input_y = y + 5;
            let label = "Path: ";
            render_line(f, cx, input_y, inner.width, label, text_style);
            let input_style = RStyle::default()
                .fg(theme.foreground())
                .bg(theme.modeline());
            let input_w = inner
                .width
                .saturating_sub(cx - inner.x + label.len() as u16 + 2);
            let input_x = cx + label.len() as u16;
            let padded: String = format!("{:<width$}", sw.vault_path, width = input_w as usize);
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(padded, input_style))),
                Rect::new(input_x, input_y, input_w, 1),
            );
            // Cursor in path input
            let cursor_x = input_x + sw.vault_path_cursor as u16;
            if cursor_x < inner.right() {
                f.set_cursor_position((cursor_x, input_y));
            }

            // Directory preview
            let prev_y = y + 7;
            render_line(f, cx, prev_y, inner.width, "Bloom will create:", faded);
            render_line(
                f,
                cx + 2,
                prev_y + 1,
                inner.width,
                "pages/       \u{2014} topic pages",
                faded,
            );
            render_line(
                f,
                cx + 2,
                prev_y + 2,
                inner.width,
                "journal/     \u{2014} daily journal",
                faded,
            );
            render_line(
                f,
                cx + 2,
                prev_y + 3,
                inner.width,
                "templates/   \u{2014} page templates",
                faded,
            );
            render_line(
                f,
                cx + 2,
                prev_y + 4,
                inner.width,
                "images/      \u{2014} attachments",
                faded,
            );

            // Error
            if let Some(err) = &sw.error {
                render_line(
                    f,
                    cx,
                    input_y + 2,
                    inner.width,
                    &format!("\u{2717} {err}"),
                    error_style,
                );
            }

            // Nav hints
            let prompt_y = inner.bottom().saturating_sub(2);
            render_line(f, cx, prompt_y, inner.width, "Esc back", faded);
            let confirm = "Enter to confirm";
            let rx = inner.right().saturating_sub(confirm.len() as u16 + 2);
            render_line(f, rx, prompt_y, inner.width, confirm, faded);
        }

        SetupStep::ImportChoice => {
            let y = y_start;
            render_line(f, cx, y, inner.width, "Import from Logseq?", heading_style);
            render_line(
                f,
                cx,
                y + 2,
                inner.width,
                "If you have an existing Logseq vault, Bloom",
                text_style,
            );
            render_line(
                f,
                cx,
                y + 3,
                inner.width,
                "can import your pages, journals, and links.",
                text_style,
            );
            render_line(
                f,
                cx,
                y + 4,
                inner.width,
                "Your Logseq files will not be modified.",
                text_style,
            );

            let opt_y = y + 6;
            let (no_style, yes_style) = if sw.import_choice == ImportChoice::No {
                (
                    RStyle::default().fg(theme.foreground()).bg(theme.mild()),
                    text_style,
                )
            } else {
                (
                    text_style,
                    RStyle::default().fg(theme.foreground()).bg(theme.mild()),
                )
            };
            let no_marker = if sw.import_choice == ImportChoice::No {
                "\u{25b8} "
            } else {
                "  "
            };
            let yes_marker = if sw.import_choice == ImportChoice::Yes {
                "\u{25b8} "
            } else {
                "  "
            };
            render_line(
                f,
                cx,
                opt_y,
                inner.width,
                &format!("{no_marker}No, start fresh"),
                no_style,
            );
            render_line(
                f,
                cx,
                opt_y + 1,
                inner.width,
                &format!("{yes_marker}Yes, import from Logseq"),
                yes_style,
            );

            // Nav hints
            let prompt_y = inner.bottom().saturating_sub(2);
            render_line(f, cx, prompt_y, inner.width, "Esc back", faded);
            let nav = "\u{2191}\u{2193} select         Enter to confirm";
            let rx = inner.right().saturating_sub(nav.len() as u16 + 2);
            render_line(f, rx, prompt_y, inner.width, nav, faded);
        }

        SetupStep::ImportPath => {
            let y = y_start;
            render_line(f, cx, y, inner.width, "Import from Logseq", heading_style);
            render_line(
                f,
                cx,
                y + 2,
                inner.width,
                "Enter the path to your Logseq vault:",
                text_style,
            );

            // Path input
            let input_y = y + 4;
            let label = "Path: ";
            render_line(f, cx, input_y, inner.width, label, text_style);
            let input_style = RStyle::default()
                .fg(theme.foreground())
                .bg(theme.modeline());
            let input_w = inner
                .width
                .saturating_sub(cx - inner.x + label.len() as u16 + 2);
            let input_x = cx + label.len() as u16;
            let padded: String = format!("{:<width$}", sw.logseq_path, width = input_w as usize);
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(padded, input_style))),
                Rect::new(input_x, input_y, input_w, 1),
            );
            let cursor_x = input_x + sw.logseq_path_cursor as u16;
            if cursor_x < inner.right() {
                f.set_cursor_position((cursor_x, input_y));
            }

            // Error
            if let Some(err) = &sw.error {
                render_line(
                    f,
                    cx,
                    input_y + 2,
                    inner.width,
                    &format!("\u{2717} {err}"),
                    error_style,
                );
            }

            // Nav hints
            let prompt_y = inner.bottom().saturating_sub(2);
            render_line(f, cx, prompt_y, inner.width, "Esc back", faded);
            let confirm = "Enter to start import";
            let rx = inner.right().saturating_sub(confirm.len() as u16 + 2);
            render_line(f, rx, prompt_y, inner.width, confirm, faded);
        }

        SetupStep::ImportRunning => {
            let y = y_start;
            render_line(
                f,
                cx,
                y,
                inner.width,
                "Importing from Logseq...",
                heading_style,
            );
            if let Some(prog) = &sw.import_progress {
                // Progress bar
                let bar_y = y + 2;
                let bar_w = (inner.width / 2).max(20) as usize;
                let filled = if prog.total > 0 {
                    (prog.done * bar_w) / prog.total
                } else {
                    0
                };
                let bar: String = format!(
                    "{}{} {}/{}",
                    "\u{2588}".repeat(filled),
                    "\u{2591}".repeat(bar_w - filled),
                    prog.done,
                    prog.total,
                );
                render_line(f, cx, bar_y, inner.width, &bar, text_style);

                // Stats
                let mut sy = bar_y + 2;
                let green = RStyle::default().fg(theme.accent_green());
                let yellow = RStyle::default().fg(theme.accent_yellow());
                let red = RStyle::default().fg(theme.critical());
                render_line(
                    f,
                    cx,
                    sy,
                    inner.width,
                    &format!("\u{2713} {} pages imported", prog.pages_imported),
                    green,
                );
                sy += 1;
                render_line(
                    f,
                    cx,
                    sy,
                    inner.width,
                    &format!("\u{2713} {} journals imported", prog.journals_imported),
                    green,
                );
                sy += 1;
                render_line(
                    f,
                    cx,
                    sy,
                    inner.width,
                    &format!("\u{2713} {} links resolved", prog.links_resolved),
                    green,
                );
                sy += 1;
                if !prog.warnings.is_empty() {
                    render_line(
                        f,
                        cx,
                        sy,
                        inner.width,
                        &format!("\u{26a0} {} warnings", prog.warnings.len()),
                        yellow,
                    );
                    sy += 1;
                }
                if !prog.errors.is_empty() {
                    render_line(
                        f,
                        cx,
                        sy,
                        inner.width,
                        &format!("\u{2717} {} errors", prog.errors.len()),
                        red,
                    );
                }

                if prog.finished {
                    let prompt_y = inner.bottom().saturating_sub(2);
                    let confirm = "Press Enter to continue";
                    let rx = inner.right().saturating_sub(confirm.len() as u16 + 2);
                    render_line(f, rx, prompt_y, inner.width, confirm, faded);
                }
            }
        }

        SetupStep::Complete => {
            let y = y_start + 2;
            render_line(
                f,
                cx,
                y,
                inner.width,
                "Your vault is ready \u{1f331}",
                heading_style,
            );

            let sy = y + 2;
            render_line(
                f,
                cx,
                sy,
                inner.width,
                &format!("Location:  {}", sw.vault_path),
                text_style,
            );
            render_line(
                f,
                cx,
                sy + 1,
                inner.width,
                &format!("Pages:     {}", sw.stats.pages),
                text_style,
            );
            render_line(
                f,
                cx,
                sy + 2,
                inner.width,
                &format!("Journal:   {} entries", sw.stats.journals),
                text_style,
            );

            let ty = sy + 4;
            render_line(f, cx, ty, inner.width, "Tips:", text_style);
            let key_style = RStyle::default().fg(theme.salient());
            let desc_style = text_style;
            let tips = [
                ("SPC j j", "open today's journal"),
                ("SPC f f", "find a page"),
                ("SPC n  ", "create a new page"),
                ("SPC ?  ", "all commands"),
            ];
            for (i, (key, desc)) in tips.iter().enumerate() {
                let tip_y = ty + 1 + i as u16;
                if tip_y < inner.bottom() {
                    let line = Line::from(vec![
                        Span::styled(format!("  {key}     "), key_style),
                        Span::styled(*desc, desc_style),
                    ]);
                    f.render_widget(
                        Paragraph::new(line),
                        Rect::new(cx, tip_y, inner.width.saturating_sub(cx - inner.x), 1),
                    );
                }
            }

            // Prompt
            let prompt_y = inner.bottom().saturating_sub(2);
            let confirm = "Press Enter to open your journal";
            let rx = inner.right().saturating_sub(confirm.len() as u16 + 2);
            render_line(f, rx, prompt_y, inner.width, confirm, faded);
        }
    }
}

pub(super) fn render_line(f: &mut Frame, x: u16, y: u16, max_w: u16, text: &str, style: RStyle) {
    let w = (text.width() as u16).min(max_w.saturating_sub(x));
    if w == 0 {
        return;
    }
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(text, style))),
        Rect::new(x, y, w, 1),
    );
}
