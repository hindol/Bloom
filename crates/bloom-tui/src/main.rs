use bloom_core::buffer::{Buffer, EditOp};
use bloom_core::keymap::dispatch::{Action, EditorContext, KeymapConfig, KeymapDispatcher};
use bloom_core::render::{
    CursorShape, CursorState, Notification, NotificationLevel, PaneFrame,
    PaneKind as RenderPaneKind, PickerFrame, QuickCaptureFrame, RenderFrame, RenderedLine,
    StatusBar, StyledSpan, Style, Viewport, WhichKeyFrame,
};
use bloom_core::types::{self, KeyCode, Modifiers, PaneId};
use bloom_core::vim::{Mode, VimAction, VimState};

use crossterm::{
    event::{self, Event, KeyCode as CtKeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style as RStyle},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Terminal,
};
use std::io;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Editor state (thin composition of bloom-core components)
// ---------------------------------------------------------------------------

struct EditorState {
    buffer: Buffer,
    vim: VimState,
    viewport: Viewport,
    cursor: usize,
    keymap: KeymapDispatcher,
    title: String,
    filename: String,
    should_quit: bool,
}

impl EditorState {
    fn new() -> Self {
        let content = "# Welcome to Bloom \u{1f331}\n\nStart typing to begin.\n";
        let buffer = Buffer::from_text(content);
        let vim = VimState::new();
        let viewport = Viewport::new(24, 80);
        let keymap = KeymapDispatcher::new(&KeymapConfig::default());

        Self {
            buffer,
            vim,
            viewport,
            cursor: 0,
            keymap,
            title: "Welcome".into(),
            filename: "welcome.md".into(),
            should_quit: false,
        }
    }

    fn handle_key(&mut self, key: types::KeyEvent) {
        // 1. Try keymap dispatcher (platform shortcuts, picker, quick-capture)
        let ctx = EditorContext {
            mode: self.vim.mode(),
            buffer: &self.buffer,
            cursor: self.cursor,
            picker_open: false,
            quick_capture_open: false,
            template_mode_active: false,
            active_pane: PaneId(0),
        };
        let actions = self.keymap.dispatch(key.clone(), &ctx);
        if !actions.is_empty() {
            for action in actions {
                self.apply_action(action);
            }
            return;
        }

        // 2. Insert mode: pass characters directly to buffer
        if self.vim.mode() == Mode::Insert {
            match &key.code {
                KeyCode::Char(c) if key.modifiers == Modifiers::none() || key.modifiers == Modifiers::shift() => {
                    self.buffer.insert(self.cursor, &c.to_string());
                    self.cursor += 1;
                    return;
                }
                KeyCode::Enter if key.modifiers == Modifiers::none() => {
                    self.buffer.insert(self.cursor, "\n");
                    self.cursor += 1;
                    return;
                }
                KeyCode::Backspace if key.modifiers == Modifiers::none() => {
                    if self.cursor > 0 {
                        self.buffer.delete(self.cursor - 1..self.cursor);
                        self.cursor -= 1;
                    }
                    return;
                }
                KeyCode::Delete if key.modifiers == Modifiers::none() => {
                    if self.cursor < self.buffer.len_chars() {
                        self.buffer.delete(self.cursor..self.cursor + 1);
                    }
                    return;
                }
                KeyCode::Tab if key.modifiers == Modifiers::none() => {
                    self.buffer.insert(self.cursor, "    ");
                    self.cursor += 4;
                    return;
                }
                _ => {}
            }
        }

        // 3. Vim state machine
        let vim_action = self.vim.process_key(key, &self.buffer, self.cursor);
        self.apply_vim_action(vim_action);
    }

    fn apply_vim_action(&mut self, action: VimAction) {
        match action {
            VimAction::Edit(op) => self.apply_edit(op),
            VimAction::Motion(m) => {
                self.cursor = m.new_position.min(self.buffer.len_chars().saturating_sub(1));
            }
            VimAction::ModeChange(_) => {} // VimState already updated internally
            VimAction::Command(cmd) => self.apply_command(&cmd),
            VimAction::Pending | VimAction::Unhandled => {}
            VimAction::Composite(actions) => {
                for a in actions {
                    self.apply_vim_action(a);
                }
            }
        }
    }

    fn apply_edit(&mut self, op: EditOp) {
        if op.range.is_empty() && !op.replacement.is_empty() {
            self.buffer.insert(op.range.start, &op.replacement);
        } else if op.replacement.is_empty() {
            self.buffer.delete(op.range);
        } else {
            self.buffer.replace(op.range, &op.replacement);
        }
        self.cursor = op.cursor_after.min(self.buffer.len_chars().saturating_sub(1));
    }

    fn apply_action(&mut self, action: Action) {
        match action {
            Action::Quit => self.should_quit = true,
            Action::Save => self.buffer.mark_clean(),
            Action::Undo => {
                self.buffer.undo();
            }
            Action::Redo => {
                self.buffer.redo();
            }
            Action::Edit(op) => self.apply_edit(op),
            Action::Motion(m) => {
                self.cursor = m.new_position.min(self.buffer.len_chars().saturating_sub(1));
            }
            Action::ModeChange(_) => {}
            _ => {} // Other actions not yet wired
        }
    }

    fn apply_command(&mut self, cmd: &str) {
        match cmd {
            "undo" => { self.buffer.undo(); }
            "redo" => { self.buffer.redo(); }
            _ => {}
        }
    }

    fn resize(&mut self, width: u16, height: u16) {
        // Reserve 1 row for status bar
        let editor_height = (height as usize).saturating_sub(1);
        self.viewport = Viewport::new(editor_height, width as usize);
    }

    fn render_frame(&mut self) -> RenderFrame {
        let rope = self.buffer.text();
        let cursor_line = rope.char_to_line(self.cursor.min(self.buffer.len_chars().saturating_sub(1).max(0)));
        let line_start = rope.line_to_char(cursor_line);
        let cursor_col = self.cursor.saturating_sub(line_start);

        self.viewport.ensure_visible(cursor_line);
        let visible = self.viewport.visible_range();

        let mut visible_lines = Vec::new();
        let total_lines = rope.len_lines();
        for line_idx in visible.start..visible.end.min(total_lines) {
            let line_text: String = rope.line(line_idx).to_string();
            let len = line_text.len();
            visible_lines.push(RenderedLine {
                line_number: line_idx,
                spans: if len > 0 {
                    vec![StyledSpan {
                        range: 0..len,
                        style: Style::Normal,
                    }]
                } else {
                    vec![]
                },
            });
        }

        let cursor_shape = match self.vim.mode() {
            Mode::Insert => CursorShape::Bar,
            Mode::Visual { .. } => CursorShape::Underline,
            _ => CursorShape::Block,
        };

        let mode_str = match self.vim.mode() {
            Mode::Normal => "NORMAL",
            Mode::Insert => "INSERT",
            Mode::Visual { .. } => "VISUAL",
            Mode::Command => "COMMAND",
        };

        let pane = PaneFrame {
            id: PaneId(0),
            kind: RenderPaneKind::Editor,
            visible_lines,
            cursor: CursorState {
                line: cursor_line,
                column: cursor_col,
                shape: cursor_shape,
            },
            scroll_offset: self.viewport.first_visible_line,
            is_active: true,
            title: self.title.clone(),
            dirty: self.buffer.is_dirty(),
            status_bar: StatusBar {
                mode: mode_str.into(),
                filename: self.filename.clone(),
                dirty: self.buffer.is_dirty(),
                line: cursor_line,
                column: cursor_col,
                pending_keys: self.vim.pending_keys().to_string(),
                recording_macro: if self.vim.is_recording() { Some('q') } else { None },
                mcp_status: None,
            },
        };

        RenderFrame {
            panes: vec![pane],
            maximized: false,
            hidden_pane_count: 0,
            picker: None,
            which_key: None,
            command_line: None,
            quick_capture: None,
            date_picker: None,
            dialog: None,
            notification: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Key conversion
// ---------------------------------------------------------------------------

fn convert_key(key: event::KeyEvent) -> types::KeyEvent {
    let code = match key.code {
        CtKeyCode::Char(c) => KeyCode::Char(c),
        CtKeyCode::Enter => KeyCode::Enter,
        CtKeyCode::Esc => KeyCode::Esc,
        CtKeyCode::Tab => KeyCode::Tab,
        CtKeyCode::Backspace => KeyCode::Backspace,
        CtKeyCode::Delete => KeyCode::Delete,
        CtKeyCode::Up => KeyCode::Up,
        CtKeyCode::Down => KeyCode::Down,
        CtKeyCode::Left => KeyCode::Left,
        CtKeyCode::Right => KeyCode::Right,
        CtKeyCode::Home => KeyCode::Home,
        CtKeyCode::End => KeyCode::End,
        CtKeyCode::PageUp => KeyCode::PageUp,
        CtKeyCode::PageDown => KeyCode::PageDown,
        CtKeyCode::F(n) => KeyCode::F(n),
        _ => return types::KeyEvent::char(' '),
    };
    types::KeyEvent {
        code,
        modifiers: Modifiers {
            ctrl: key.modifiers.contains(KeyModifiers::CONTROL),
            alt: key.modifiers.contains(KeyModifiers::ALT),
            shift: key.modifiers.contains(KeyModifiers::SHIFT),
            meta: key.modifiers.contains(KeyModifiers::SUPER),
        },
    }
}

// ---------------------------------------------------------------------------
// Rendering helpers
// ---------------------------------------------------------------------------

fn render_frame(f: &mut ratatui::Frame, frame: &RenderFrame, buffer: &Buffer) {
    let area = f.area();

    // Layout: editor pane + status bar
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(area);

    if let Some(pane) = frame.panes.first() {
        render_pane(f, pane, chunks[0], buffer);
        render_status_bar(f, &pane.status_bar, chunks[1]);
    }

    if let Some(picker) = &frame.picker {
        render_picker(f, picker, area);
    }
    if let Some(wk) = &frame.which_key {
        render_which_key(f, wk, area);
    }
    if let Some(qc) = &frame.quick_capture {
        render_quick_capture(f, qc, area);
    }
    if let Some(notif) = &frame.notification {
        render_notification(f, notif, area);
    }
}

fn render_pane(f: &mut ratatui::Frame, pane: &PaneFrame, area: Rect, buffer: &Buffer) {
    let rope = buffer.text();
    let total_lines = rope.len_lines();

    let lines: Vec<Line> = pane
        .visible_lines
        .iter()
        .map(|rl| {
            if rl.line_number < total_lines {
                let line_text: String = rope.line(rl.line_number).to_string();
                // Strip trailing newline for display
                let display = line_text.trim_end_matches('\n').trim_end_matches('\r');
                if rl.spans.is_empty() || display.is_empty() {
                    Line::from(Span::raw(display.to_string()))
                } else {
                    // Render spans with styles
                    let spans: Vec<Span> = rl
                        .spans
                        .iter()
                        .filter_map(|s| {
                            let byte_end = s.range.end.min(display.len());
                            let byte_start = s.range.start.min(byte_end);
                            if byte_start < byte_end {
                                Some(Span::styled(
                                    display[byte_start..byte_end].to_string(),
                                    map_style(&s.style),
                                ))
                            } else {
                                None
                            }
                        })
                        .collect();
                    if spans.is_empty() {
                        Line::from(Span::raw(display.to_string()))
                    } else {
                        Line::from(spans)
                    }
                }
            } else {
                // Beyond EOF — show tilde
                Line::from(Span::styled(
                    "~",
                    RStyle::default().fg(Color::DarkGray),
                ))
            }
        })
        .collect();

    // Fill remaining rows with tildes
    let mut all_lines = lines;
    while all_lines.len() < area.height as usize {
        all_lines.push(Line::from(Span::styled(
            "~",
            RStyle::default().fg(Color::DarkGray),
        )));
    }

    let paragraph = Paragraph::new(all_lines);
    f.render_widget(paragraph, area);

    // Set cursor position
    let cursor_x = area.x + pane.cursor.column as u16;
    let cursor_y = area.y + (pane.cursor.line.saturating_sub(pane.scroll_offset)) as u16;
    if cursor_y < area.bottom() && cursor_x < area.right() {
        f.set_cursor_position((cursor_x, cursor_y));
    }
}

fn render_status_bar(f: &mut ratatui::Frame, sb: &StatusBar, area: Rect) {
    let mode_style = match sb.mode.as_str() {
        "NORMAL" => RStyle::default()
            .bg(Color::Rgb(0xF4, 0xBF, 0x4F))
            .fg(Color::Rgb(0x14, 0x14, 0x14))
            .add_modifier(Modifier::BOLD),
        "INSERT" => RStyle::default()
            .bg(Color::Rgb(0x62, 0xC5, 0x54))
            .fg(Color::Rgb(0x14, 0x14, 0x14))
            .add_modifier(Modifier::BOLD),
        "VISUAL" => RStyle::default()
            .bg(Color::Rgb(0x7A, 0x9E, 0xFF))
            .fg(Color::Rgb(0x14, 0x14, 0x14))
            .add_modifier(Modifier::BOLD),
        "COMMAND" => RStyle::default()
            .bg(Color::Rgb(0x81, 0xA1, 0xC1))
            .fg(Color::Rgb(0x14, 0x14, 0x14))
            .add_modifier(Modifier::BOLD),
        _ => RStyle::default().bg(Color::DarkGray),
    };

    let dirty = if sb.dirty { " [+]" } else { "" };
    let pending = if sb.pending_keys.is_empty() {
        String::new()
    } else {
        format!(" {}", sb.pending_keys)
    };
    let recording = sb
        .recording_macro
        .map(|c| format!(" @{c}"))
        .unwrap_or_default();

    let left = format!(" {} \u{2502} {}{}{}{}", sb.mode, sb.filename, dirty, pending, recording);
    let right = format!("{}:{} ", sb.line + 1, sb.column + 1);

    let width = area.width as usize;
    let padding = width.saturating_sub(left.len() + right.len());
    let status_text = format!("{}{:pad$}{}", left, "", right, pad = padding);

    let status = Paragraph::new(Line::from(Span::styled(status_text, mode_style)));
    f.render_widget(status, area);
}

fn render_picker(f: &mut ratatui::Frame, picker: &PickerFrame, area: Rect) {
    let popup_area = centered_rect(60, 60, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {} ", picker.title))
        .border_style(RStyle::default().fg(Color::Rgb(0xA3, 0xA3, 0xA3)));

    let lines: Vec<Line> = std::iter::once(Line::from(format!("> {}", picker.query)))
        .chain(picker.results.iter().enumerate().map(|(i, row)| {
            let style = if i == picker.selected_index {
                RStyle::default().bg(Color::Rgb(0x47, 0x46, 0x48))
            } else {
                RStyle::default()
            };
            let prefix = if i == picker.selected_index {
                "\u{25b8} "
            } else {
                "  "
            };
            let marginalia = row.marginalia.join(" ");
            Line::from(Span::styled(
                format!("{}{} {}", prefix, row.label, marginalia),
                style,
            ))
        }))
        .collect();

    let paragraph = Paragraph::new(lines).block(block);
    f.render_widget(Clear, popup_area);
    f.render_widget(paragraph, popup_area);
}

fn render_which_key(f: &mut ratatui::Frame, wk: &WhichKeyFrame, area: Rect) {
    let popup_height = (wk.entries.len() as u16 + 2).min(area.height).max(3);
    let popup_area = Rect {
        x: area.x,
        y: area.bottom().saturating_sub(popup_height),
        width: area.width,
        height: popup_height,
    };

    let title = format!(" {} ", wk.prefix);
    let lines: Vec<Line> = wk
        .entries
        .iter()
        .map(|e| {
            let suffix = if e.is_group { " \u{2192}" } else { "" };
            Line::from(format!("  {} \u{2192} {}{}", e.key, e.label, suffix))
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(RStyle::default().fg(Color::Rgb(0xA3, 0xA3, 0xA3)));
    let paragraph = Paragraph::new(lines).block(block);
    f.render_widget(Clear, popup_area);
    f.render_widget(paragraph, popup_area);
}

fn render_quick_capture(f: &mut ratatui::Frame, qc: &QuickCaptureFrame, area: Rect) {
    let popup_area = Rect {
        x: area.x + 2,
        y: area.bottom().saturating_sub(3),
        width: area.width.saturating_sub(4),
        height: 3,
    };
    let text = format!("{}{}", qc.prompt, qc.input);
    let block = Block::default().borders(Borders::ALL);
    let paragraph = Paragraph::new(Line::from(text)).block(block);
    f.render_widget(Clear, popup_area);
    f.render_widget(paragraph, popup_area);
}

fn render_notification(f: &mut ratatui::Frame, notif: &Notification, area: Rect) {
    let style = match notif.level {
        NotificationLevel::Info => RStyle::default().fg(Color::Rgb(0x62, 0xC5, 0x54)),
        NotificationLevel::Warning => RStyle::default().fg(Color::Rgb(0xF2, 0xDA, 0x61)),
        NotificationLevel::Error => RStyle::default().fg(Color::Rgb(0xCF, 0x67, 0x52)),
    };
    let msg_width = (notif.message.len() as u16 + 4).min(area.width);
    let popup_area = Rect {
        x: area.right().saturating_sub(msg_width),
        y: area.y,
        width: msg_width,
        height: 1,
    };
    let paragraph = Paragraph::new(Line::from(Span::styled(&*notif.message, style)));
    f.render_widget(paragraph, popup_area);
}

fn map_style(style: &Style) -> RStyle {
    match style {
        Style::Normal => RStyle::default().fg(Color::Rgb(0xEB, 0xE9, 0xE7)),
        Style::Heading { level } => match level {
            1 => RStyle::default()
                .fg(Color::Rgb(0xF5, 0xF2, 0xF0))
                .add_modifier(Modifier::BOLD),
            2 => RStyle::default()
                .fg(Color::Rgb(0xF4, 0xBF, 0x4F))
                .add_modifier(Modifier::BOLD),
            3 => RStyle::default()
                .fg(Color::Rgb(0xEB, 0xE9, 0xE7))
                .add_modifier(Modifier::BOLD),
            _ => RStyle::default().add_modifier(Modifier::BOLD),
        },
        Style::Bold => RStyle::default().add_modifier(Modifier::BOLD),
        Style::Italic => RStyle::default().add_modifier(Modifier::ITALIC),
        Style::Code => RStyle::default()
            .fg(Color::Rgb(0xEB, 0xE9, 0xE7))
            .bg(Color::Rgb(0x37, 0x37, 0x3E)),
        Style::CodeBlock => RStyle::default()
            .fg(Color::Rgb(0xEB, 0xE9, 0xE7))
            .bg(Color::Rgb(0x37, 0x37, 0x3E)),
        Style::Link => RStyle::default()
            .fg(Color::Rgb(0xF5, 0xF2, 0xF0))
            .bg(Color::Rgb(0x1A, 0x19, 0x19))
            .add_modifier(Modifier::UNDERLINED),
        Style::Tag => RStyle::default().fg(Color::Rgb(0xA3, 0xA3, 0xA3)),
        Style::Timestamp => RStyle::default().fg(Color::Rgb(0xA3, 0xA3, 0xA3)),
        Style::BlockId => RStyle::default()
            .fg(Color::Rgb(0xA3, 0xA3, 0xA3))
            .add_modifier(Modifier::DIM),
        Style::ListMarker => RStyle::default().fg(Color::Rgb(0xEB, 0xE9, 0xE7)),
        Style::CheckboxUnchecked => RStyle::default().fg(Color::Rgb(0xF2, 0xDA, 0x61)),
        Style::CheckboxChecked => RStyle::default()
            .fg(Color::Rgb(0x62, 0xC5, 0x54))
            .add_modifier(Modifier::DIM | Modifier::CROSSED_OUT),
        Style::Frontmatter => RStyle::default()
            .fg(Color::Rgb(0xA3, 0xA3, 0xA3))
            .add_modifier(Modifier::ITALIC),
        Style::BrokenLink => RStyle::default()
            .fg(Color::Rgb(0xCF, 0x67, 0x52))
            .add_modifier(Modifier::CROSSED_OUT),
        Style::SyntaxNoise => RStyle::default()
            .fg(Color::Rgb(0xA3, 0xA3, 0xA3))
            .add_modifier(Modifier::DIM),
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut editor = EditorState::new();

    // Set initial viewport size from terminal
    let size = terminal.size()?;
    editor.resize(size.width, size.height);

    let tick_rate = Duration::from_millis(16);

    loop {
        // Render
        let frame = editor.render_frame();
        let buffer_ref = &editor.buffer;
        terminal.draw(|f| render_frame(f, &frame, buffer_ref))?;

        // Poll events
        if event::poll(tick_rate)? {
            match event::read()? {
                Event::Key(key) => {
                    // Only handle Press events (crossterm 0.28 sends Press + Release)
                    if key.kind == event::KeyEventKind::Press {
                        let bloom_key = convert_key(key);
                        editor.handle_key(bloom_key);
                    }
                }
                Event::Resize(w, h) => {
                    editor.resize(w, h);
                }
                _ => {}
            }
        }

        if editor.should_quit {
            break;
        }
    }

    // Cleanup
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;
    Ok(())
}
