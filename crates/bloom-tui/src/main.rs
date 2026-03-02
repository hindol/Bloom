use std::io::stdout;
use std::path::PathBuf;

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame, Terminal,
};

use bloom_core::editor::{EditorState, Key, KeyResult};
use bloom_core::index::SqliteIndex;
use bloom_core::render::{
    self as bloom_render, AgendaFrame, PickerFrame, RenderFrame, Rgb, Theme, UndoTreeFrame,
    WhichKeyFrame,
};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    // Parse --vault flag manually: [--vault <dir>] [file]
    let mut vault_dir: Option<PathBuf> = None;
    let mut file_arg: Option<PathBuf> = None;
    {
        let mut i = 1;
        while i < args.len() {
            if args[i] == "--vault" {
                i += 1;
                if i < args.len() {
                    vault_dir = Some(PathBuf::from(&args[i]));
                }
            } else {
                file_arg = Some(PathBuf::from(&args[i]));
            }
            i += 1;
        }
    }

    let mut editor = if let Some(vault) = &vault_dir {
        // Vault mode: set up index and vault_root
        let vault = std::fs::canonicalize(vault).unwrap_or_else(|_| vault.clone());
        let db_path = vault.join(".index").join("bloom.db");
        let index = SqliteIndex::open(&db_path).expect("Failed to open vault index");

        let (content, open_path) = if let Some(f) = &file_arg {
            let p = if f.is_absolute() { f.clone() } else { vault.join(f) };
            (std::fs::read_to_string(&p).unwrap_or_default(), Some(p))
        } else {
            // Open first page alphabetically, or empty buffer
            first_page(&vault)
        };

        let mut state = EditorState::new(&content);
        state.vault_root = Some(vault);
        state.index = Some(index);
        if let Some(p) = open_path {
            state.buffer.file_path = Some(p);
        }
        state.buffer.dirty = false;
        state.rebuild_index();
        state.init_fts_worker();
        state
    } else if let Some(path) = &file_arg {
        let content = std::fs::read_to_string(path).unwrap_or_default();
        let mut state = EditorState::new(&content);
        state.buffer.file_path = Some(path.clone());
        state.buffer.dirty = false;
        state
    } else {
        EditorState::new("")
    };

    // Setup terminal
    terminal::enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    loop {
        // Update viewport size from terminal
        let size = terminal.size()?;
        editor.viewport_height = (size.height as usize).saturating_sub(2); // reserve status + cmd

        // Poll background FTS results (non-blocking).
        editor.poll_fts_results();

        // Render
        let frame = editor.render();
        terminal.draw(|f| draw_frame(f, &frame, &editor))?;

        // Handle input
        if event::poll(std::time::Duration::from_millis(50))? {
            if let Event::Key(key_event) = event::read()? {
                if let Some(key) = crossterm_to_key(key_event) {
                    let result = editor.handle_key(key);
                    match result {
                        KeyResult::Quit => break,
                        KeyResult::SaveAndQuit => {
                            save_buffer(&editor);
                            break;
                        }
                        KeyResult::Save => {
                            save_buffer(&editor);
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // Restore terminal
    terminal::disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}

/// Return the first `.md` file in `vault/pages/` (alphabetically), or empty buffer.
fn first_page(vault: &std::path::Path) -> (String, Option<PathBuf>) {
    let pages_dir = vault.join("pages");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&pages_dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().map_or(false, |e| e == "md"))
        .collect();
    files.sort();
    if let Some(p) = files.into_iter().next() {
        let content = std::fs::read_to_string(&p).unwrap_or_default();
        (content, Some(p))
    } else {
        (String::new(), None)
    }
}

fn crossterm_to_key(key_event: event::KeyEvent) -> Option<Key> {
    if key_event.modifiers.contains(KeyModifiers::ALT) {
        return match key_event.code {
            KeyCode::Enter => Some(Key::AltEnter),
            _ => None,
        };
    }
    if key_event.modifiers.contains(KeyModifiers::CONTROL) {
        return match key_event.code {
            KeyCode::Char(c) => Some(Key::Ctrl(c)),
            _ => None,
        };
    }

    match key_event.code {
        KeyCode::Char(c) => Some(Key::Char(c)),
        KeyCode::Esc => Some(Key::Escape),
        KeyCode::Enter => Some(Key::Enter),
        KeyCode::Backspace => Some(Key::Backspace),
        KeyCode::Tab => Some(Key::Tab),
        KeyCode::Left => Some(Key::Left),
        KeyCode::Right => Some(Key::Right),
        KeyCode::Up => Some(Key::Up),
        KeyCode::Down => Some(Key::Down),
        _ => None,
    }
}

fn save_buffer(editor: &EditorState) {
    if let Some(path) = &editor.buffer.file_path {
        let _ = std::fs::write(path, editor.text());
    }
}

/// Helper to convert an Rgb to ratatui Color.
fn rgb_color(rgb: &Rgb) -> Color {
    Color::Rgb(rgb.0, rgb.1, rgb.2)
}

fn draw_frame(f: &mut Frame, render_frame: &RenderFrame, editor: &EditorState) {
    let theme = &editor.theme;
    let area = f.area();

    // Build bottom-bar constraints: optional capture_bar + command line
    let capture_bar_height = if render_frame.capture_bar.is_some() { 1u16 } else { 0 };
    let mut outer_constraints = vec![Constraint::Min(1)]; // editor+status area
    if capture_bar_height > 0 {
        outer_constraints.push(Constraint::Length(1)); // capture bar
    }
    outer_constraints.push(Constraint::Length(1)); // command line

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints(outer_constraints)
        .split(area);

    let panes_area = outer[0];
    let (capture_bar_area, cmd_area) = if render_frame.capture_bar.is_some() {
        (Some(outer[1]), outer[2])
    } else {
        (None, outer[1])
    };

    // Split panes_area among all panes (equal vertical splits)
    let pane_count = render_frame.panes.len().max(1);
    let pane_constraints: Vec<Constraint> = (0..pane_count)
        .map(|_| Constraint::Ratio(1, pane_count as u32))
        .collect();
    let pane_columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(pane_constraints)
        .split(panes_area);

    for (i, pane) in render_frame.panes.iter().enumerate() {
        let col_area = pane_columns[i];

        // Each pane column: editor area + status bar
        let pane_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(1),    // editor
                Constraint::Length(1), // status bar
            ])
            .split(col_area);

        draw_editor(f, pane_chunks[0], pane, editor, &theme);
        draw_status_bar(f, pane_chunks[1], &pane.status_bar, &theme);

        // Only set cursor for the focused pane
        if pane.focused {
            let cursor_row = pane.cursor.row as u16;
            let cursor_col = pane.cursor.col as u16;
            if cursor_row < pane_chunks[0].height && cursor_col < pane_chunks[0].width {
                f.set_cursor_position((
                    pane_chunks[0].x + cursor_col,
                    pane_chunks[0].y + cursor_row,
                ));
            }
        }
    }

    // Draw capture bar if present
    if let (Some(bar_area), Some(text)) = (capture_bar_area, &render_frame.capture_bar) {
        let bar = Paragraph::new(Line::from(vec![
            Span::styled(
                " ▸ ",
                Style::default().bg(Color::DarkGray).fg(rgb_color(&theme.accent)),
            ),
            Span::styled(
                text.clone(),
                Style::default().bg(Color::DarkGray).fg(Color::White),
            ),
        ]));
        f.render_widget(bar, bar_area);
    }

    // Draw command line
    draw_command_line(f, cmd_area, render_frame);

    // Draw picker overlay if present
    if let Some(picker) = &render_frame.picker {
        draw_picker(f, area, picker, &theme);
    }

    // Draw which-key popup if present
    if let Some(which_key) = &render_frame.which_key {
        draw_which_key(f, area, which_key, &theme);
    }

    // Draw undo tree visualizer if present
    if let Some(undo_tree) = &render_frame.undo_tree {
        draw_undo_tree(f, area, undo_tree);
    }

    // Draw agenda overlay if present
    if let Some(agenda) = &render_frame.agenda {
        draw_agenda(f, area, agenda);
    }
}

fn draw_editor(
    f: &mut Frame,
    area: Rect,
    pane: &bloom_core::render::PaneFrame,
    _editor: &EditorState,
    theme: &Theme,
) {

    let lines: Vec<Line> = pane
        .lines
        .iter()
        .take(area.height as usize)
        .map(|rendered_line| {
            if rendered_line.line_number.is_none() {
                // Tilde filler
                let props = theme.tilde;
                Line::from(Span::styled("~", style_from_props(&props)))
            } else if rendered_line.spans.is_empty() {
                Line::from(rendered_line.text.clone())
            } else {
                styled_line(&rendered_line.text, &rendered_line.spans, &theme)
            }
        })
        .collect();

    let paragraph = Paragraph::new(lines)
        .style(Style::default().bg(rgb_color(&theme.bg)));
    f.render_widget(paragraph, area);
}

/// Convert a line with StyledSpans into a ratatui Line with colored Spans.
fn styled_line(text: &str, spans: &[bloom_render::StyledSpan], theme: &Theme) -> Line<'static> {
    if spans.is_empty() || text.is_empty() {
        return Line::from(text.to_string());
    }

    // Sort spans by start position
    let mut sorted: Vec<&bloom_render::StyledSpan> = spans.iter().collect();
    sorted.sort_by_key(|s| (s.start, std::cmp::Reverse(s.end)));

    // Build non-overlapping span segments using a simple sweep
    let mut result: Vec<Span<'static>> = Vec::new();
    let mut pos = 0;

    for styled_span in &sorted {
        let start = styled_span.start.min(text.len());
        let end = styled_span.end.min(text.len());
        if start >= end || start < pos {
            continue; // skip overlapping/past spans
        }
        // Gap before this span: render as normal text
        if pos < start {
            result.push(Span::raw(text[pos..start].to_string()));
        }
        // Render the styled span
        let props = theme.props_for(styled_span.style);
        result.push(Span::styled(
            text[start..end].to_string(),
            style_from_props(&props),
        ));
        pos = end;
    }

    // Trailing text after last span
    if pos < text.len() {
        result.push(Span::raw(text[pos..].to_string()));
    }

    Line::from(result)
}

/// Map bloom-core StyleProps to ratatui Style.
fn style_from_props(props: &bloom_render::StyleProps) -> Style {
    let mut style = Style::default();
    if let Some(Rgb(r, g, b)) = props.fg {
        style = style.fg(Color::Rgb(r, g, b));
    }
    if let Some(Rgb(r, g, b)) = props.bg {
        style = style.bg(Color::Rgb(r, g, b));
    }
    if props.bold {
        style = style.add_modifier(Modifier::BOLD);
    }
    if props.italic {
        style = style.add_modifier(Modifier::ITALIC);
    }
    if props.underline {
        style = style.add_modifier(Modifier::UNDERLINED);
    }
    if props.dim {
        style = style.add_modifier(Modifier::DIM);
    }
    if props.strikethrough {
        style = style.add_modifier(Modifier::CROSSED_OUT);
    }
    style
}

fn draw_status_bar(f: &mut Frame, area: Rect, status: &bloom_core::render::StatusBar, theme: &Theme) {
    let mode_style = match status.mode.as_str() {
        "NORMAL" => Style::default().bg(rgb_color(&theme.status_normal)).fg(Color::White).add_modifier(Modifier::BOLD),
        "INSERT" => Style::default().bg(rgb_color(&theme.status_insert)).fg(Color::Black).add_modifier(Modifier::BOLD),
        "VISUAL" => Style::default().bg(rgb_color(&theme.status_visual)).fg(Color::White).add_modifier(Modifier::BOLD),
        "COMMAND" => Style::default().bg(rgb_color(&theme.status_command)).fg(Color::Black).add_modifier(Modifier::BOLD),
        _ => Style::default().bg(Color::Gray).fg(Color::White),
    };

    let dirty_marker = if status.dirty { " [+]" } else { "" };

    let left = format!(" {} │ {}{} ", status.mode, status.filename, dirty_marker);
    let right = format!(" {} │ {} ", status.position, status.filetype);
    let pending = if status.pending_keys.is_empty() {
        String::new()
    } else {
        format!(" {} ", status.pending_keys)
    };

    let fill_width = (area.width as usize)
        .saturating_sub(left.len())
        .saturating_sub(right.len())
        .saturating_sub(pending.len());

    let bar_text = format!("{}{}{:>fill_width$}{}", left, pending, "", right, fill_width = fill_width);

    let bar = Paragraph::new(Line::from(Span::styled(bar_text, mode_style)));
    f.render_widget(bar, area);
}

fn draw_command_line(f: &mut Frame, area: Rect, render_frame: &RenderFrame) {
    let text = render_frame
        .command_line
        .as_deref()
        .unwrap_or("");
    let line = Paragraph::new(Line::from(text));
    f.render_widget(line, area);
}

/// Returns a centered rect of `width` x `height` within `area`.
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect::new(x, y, w, h)
}

fn draw_picker(f: &mut Frame, area: Rect, picker: &PickerFrame, theme: &Theme) {
    let has_preview = !picker.inline && picker.preview.is_some();

    // Determine overlay size and position
    let popup = if picker.inline {
        // Smaller popup anchored near bottom-center
        let w = (area.width / 2).max(30).min(area.width);
        let h = (picker.items.len() as u16 + 4).min(12).min(area.height);
        let x = area.x + (area.width.saturating_sub(w)) / 2;
        let y = area.y + area.height.saturating_sub(h + 2);
        Rect::new(x, y, w, h)
    } else {
        // Use more width when preview is shown
        let base_w = area.width * 3 / 4;
        let w = if has_preview { base_w.max(60) } else { base_w.max(40) }.min(area.width);
        let h = (area.height * 3 / 4).max(10).min(area.height);
        centered_rect(w, h, area)
    };

    // Clear background and draw border
    f.render_widget(Clear, popup);
    let border_color = rgb_color(&theme.border);
    let surface_color = rgb_color(&theme.surface);
    let accent_color = rgb_color(&theme.accent);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(surface_color))
        .title(Span::styled(
            format!(" {} ", picker.title),
            Style::default().fg(accent_color).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    // Split horizontally: left for results, right for preview
    let (results_area, preview_area) = if has_preview && inner.width > 20 {
        let halves = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(50),
                Constraint::Percentage(50),
            ])
            .split(inner);
        (halves[0], Some(halves[1]))
    } else {
        (inner, None)
    };

    // Layout: query line (1) + results (rest) + count line (1)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // query + filters
            Constraint::Min(1),   // result list
            Constraint::Length(1), // result count
        ])
        .split(results_area);

    // Query line with filter pills
    let mut query_spans = vec![
        Span::styled("❯ ", Style::default().fg(accent_color)),
        Span::raw(&picker.query),
    ];
    for pill in &picker.filters {
        query_spans.push(Span::raw(" "));
        query_spans.push(Span::styled(
            format!("[{}:{}]", pill.kind, pill.label),
            Style::default().fg(Color::Yellow),
        ));
    }
    f.render_widget(Paragraph::new(Line::from(query_spans)), chunks[0]);

    // Result list
    let visible = chunks[1].height as usize;
    let items: Vec<Line> = picker
        .items
        .iter()
        .take(visible)
        .map(|item| {
            let main_style = if item.selected {
                Style::default().bg(rgb_color(&theme.selection_bg)).fg(rgb_color(&theme.selection_fg))
            } else {
                Style::default()
            };

            // Build text with highlighted match indices
            let mut spans: Vec<Span> = Vec::new();
            let prefix = if item.marked && item.selected {
                "✓▸"
            } else if item.marked {
                "✓ "
            } else if item.selected {
                "▸ "
            } else {
                "  "
            };
            spans.push(Span::styled(prefix, main_style));

            for (i, ch) in item.text.chars().enumerate() {
                if item.match_indices.contains(&i) {
                    spans.push(Span::styled(
                        ch.to_string(),
                        main_style.fg(accent_color).add_modifier(Modifier::BOLD),
                    ));
                } else {
                    spans.push(Span::styled(ch.to_string(), main_style));
                }
            }

            // Right-align marginalia
            if !item.marginalia.is_empty() {
                let used: usize = prefix.len() + item.text.len();
                let avail = (chunks[1].width as usize).saturating_sub(used + item.marginalia.len());
                if avail > 0 {
                    spans.push(Span::styled(" ".repeat(avail), main_style));
                }
                spans.push(Span::styled(
                    &item.marginalia,
                    main_style.fg(Color::DarkGray),
                ));
            }

            Line::from(spans)
        })
        .collect();
    f.render_widget(Paragraph::new(items), chunks[1]);

    // Result count
    let count_line = Paragraph::new(Line::from(Span::styled(
        &picker.result_count,
        Style::default().fg(Color::DarkGray),
    )))
    .alignment(Alignment::Right);
    f.render_widget(count_line, chunks[2]);

    // Draw preview pane
    if let (Some(preview_rect), Some(preview_lines)) = (preview_area, &picker.preview) {
        // Draw separator border on the left edge of preview
        let sep_block = Block::default()
            .borders(Borders::LEFT)
            .border_style(Style::default().fg(border_color));
        let preview_inner = sep_block.inner(preview_rect);
        f.render_widget(sep_block, preview_rect);

        let visible_preview = preview_inner.height as usize;
        let lines: Vec<Line> = preview_lines
            .iter()
            .take(visible_preview)
            .map(|rendered_line| {
                if rendered_line.spans.is_empty() {
                    Line::from(rendered_line.text.clone())
                } else {
                    styled_line(&rendered_line.text, &rendered_line.spans, theme)
                }
            })
            .collect();
        f.render_widget(Paragraph::new(lines), preview_inner);
    }

    // Draw action menu popup if open
    if let (Some(menu_items), Some(menu_selected)) = (&picker.action_menu, picker.action_menu_selected) {
        let menu_w = menu_items.iter().map(|s| s.len()).max().unwrap_or(10) as u16 + 6;
        let menu_h = menu_items.len() as u16 + 2;
        let menu_x = results_area.x + results_area.width.saturating_sub(menu_w) / 2;
        let menu_y = results_area.y + 2; // below query line
        let menu_rect = Rect::new(
            menu_x.min(popup.x + popup.width - menu_w),
            menu_y.min(popup.y + popup.height - menu_h),
            menu_w.min(popup.width),
            menu_h.min(popup.height),
        );
        f.render_widget(Clear, menu_rect);
        let menu_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .style(Style::default().bg(surface_color))
            .title(Span::styled(" Actions ", Style::default().fg(accent_color).add_modifier(Modifier::BOLD)));
        let menu_inner = menu_block.inner(menu_rect);
        f.render_widget(menu_block, menu_rect);

        let menu_lines: Vec<Line> = menu_items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let style = if i == menu_selected {
                    Style::default().bg(rgb_color(&theme.selection_bg)).fg(rgb_color(&theme.selection_fg))
                } else {
                    Style::default()
                };
                let prefix = if i == menu_selected { "▸ " } else { "  " };
                Line::from(Span::styled(format!("{prefix}{item}"), style))
            })
            .collect();
        f.render_widget(Paragraph::new(menu_lines), menu_inner);
    }
}

fn draw_which_key(f: &mut Frame, area: Rect, which_key: &WhichKeyFrame, theme: &Theme) {
    let entry_count = which_key.entries.len();
    if entry_count == 0 {
        return;
    }

    // Lay out as columns at the bottom of the screen
    let col_count = 3u16.min(entry_count as u16).max(1);
    let rows_per_col = ((entry_count as f32) / col_count as f32).ceil() as u16;
    let popup_height = (rows_per_col + 2).min(area.height); // +2 for border

    let popup = Rect::new(
        area.x,
        area.y + area.height.saturating_sub(popup_height),
        area.width,
        popup_height,
    );

    f.render_widget(Clear, popup);
    let title = if which_key.prefix.is_empty() {
        " Which Key ".to_string()
    } else {
        format!(" {} … ", which_key.prefix)
    };
    let wk_border_color = rgb_color(&theme.border);
    let wk_surface_color = rgb_color(&theme.surface);
    let wk_accent_color = rgb_color(&theme.accent);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(wk_border_color))
        .style(Style::default().bg(wk_surface_color))
        .title(Span::styled(
            title,
            Style::default().fg(wk_accent_color).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    // Build column layout
    let col_constraints: Vec<Constraint> = (0..col_count)
        .map(|_| Constraint::Ratio(1, col_count as u32))
        .collect();
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(col_constraints)
        .split(inner);

    for (i, entry) in which_key.entries.iter().enumerate() {
        let col_idx = i / rows_per_col as usize;
        let row_idx = i % rows_per_col as usize;

        if col_idx >= columns.len() || row_idx >= columns[col_idx].height as usize {
            break;
        }

        let key_style = if entry.is_group {
            Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        };
        let desc_style = if entry.is_group {
            Style::default().fg(Color::Magenta)
        } else {
            Style::default()
        };
        let suffix = if entry.is_group { " ▸" } else { "" };

        let line = Line::from(vec![
            Span::styled(format!(" {} ", entry.key), key_style),
            Span::styled(format!("{}{}", entry.description, suffix), desc_style),
        ]);

        let cell = Rect::new(
            columns[col_idx].x,
            columns[col_idx].y + row_idx as u16,
            columns[col_idx].width,
            1,
        );
        f.render_widget(Paragraph::new(line), cell);
    }
}

fn draw_undo_tree(f: &mut Frame, area: Rect, undo_tree: &UndoTreeFrame) {
    let entry_count = undo_tree.entries.len();
    let popup_height = (entry_count as u16 + 2).min(12).min(area.height);
    let popup_width = 40u16.min(area.width);
    let popup = Rect::new(
        area.x + (area.width.saturating_sub(popup_width)) / 2,
        area.y + (area.height.saturating_sub(popup_height)) / 2,
        popup_width,
        popup_height,
    );

    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {} ", undo_tree.title));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    for (i, entry) in undo_tree.entries.iter().enumerate() {
        if i as u16 >= inner.height {
            break;
        }
        let marker = if entry.current { "● " } else { "  " };
        let sel = i == undo_tree.selected;
        let style = if sel {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let line = Line::from(Span::styled(
            format!("{marker}branch {}", entry.branch_index),
            style,
        ));
        let row = Rect::new(inner.x, inner.y + i as u16, inner.width, 1);
        f.render_widget(Paragraph::new(line), row);
    }
}

fn draw_agenda(f: &mut Frame, area: Rect, agenda: &AgendaFrame) {
    // Compute total rows: section headers + items
    let row_count: usize = agenda.sections.iter().map(|s| 1 + s.items.len()).sum();
    let popup_height = (row_count as u16 + 2).min(20).min(area.height);
    let popup_width = 60u16.min(area.width);
    let popup = Rect::new(
        area.x + (area.width.saturating_sub(popup_width)) / 2,
        area.y + (area.height.saturating_sub(popup_height)) / 2,
        popup_width,
        popup_height,
    );

    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {} ", agenda.title));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let mut y = 0u16;
    let mut flat_idx = 0usize;
    for section in &agenda.sections {
        if y >= inner.height {
            break;
        }
        // Section header
        let header_style = Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD);
        let header = Line::from(Span::styled(format!("── {} ──", section.label), header_style));
        let row = Rect::new(inner.x, inner.y + y, inner.width, 1);
        f.render_widget(Paragraph::new(header), row);
        y += 1;

        for item in &section.items {
            if y >= inner.height {
                break;
            }
            let check = if item.completed { "[x]" } else { "[ ]" };
            let sel = flat_idx == agenda.selected;
            let style = if sel {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else if item.completed {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default()
            };
            let display = format!("{check} {date} {text}  ({page})",
                date = item.date,
                text = item.text.chars().take(30).collect::<String>(),
                page = item.page_title,
            );
            let line = Line::from(Span::styled(display, style));
            let row = Rect::new(inner.x, inner.y + y, inner.width, 1);
            f.render_widget(Paragraph::new(line), row);
            y += 1;
            flat_idx += 1;
        }
    }
}
