use bloom_core::buffer::{Buffer, EditOp};
use bloom_core::keymap::dispatch::{
    Action, EditorContext, KeymapConfig, KeymapDispatcher, PickerInputAction, PickerKind,
    QuickCaptureKind, ResizeOp,
};
use bloom_core::parser::highlight::highlight_line;
use bloom_core::parser::traits::LineContext;
use bloom_core::render::{
    AgendaFrame, AgendaItem, CommandLineFrame, CursorShape, CursorState, DatePickerFrame,
    DialogFrame, Notification, NotificationLevel, PaneFrame, PaneKind as RenderPaneKind,
    PickerFrame, PickerRow, QuickCaptureFrame, RenderFrame, RenderedLine, SetupStep,
    SetupWizardFrame, StatusBar, Style, TimelineFrame, UndoTreeFrame, Viewport, WhichKeyContext,
    WhichKeyEntry, WhichKeyFrame,
};
use bloom_core::types::{self, KeyCode, Modifiers, PaneId};
use bloom_core::vim::{Mode, VimAction, VimState};
use bloom_core::which_key::{self, WhichKeyLookup};
use bloom_core::window::{Direction as WindowDirection, SplitDirection};

use chrono::{Datelike, Duration as ChronoDuration, Local, NaiveDate};
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
use std::collections::BTreeSet;
use std::io;
use std::time::{Duration, Instant};

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
    which_key_tree: which_key::WhichKeyTree,
    leader_active: bool,
    leader_keys: Vec<types::KeyEvent>,
    picker: Option<PickerState>,
    agenda: Option<AgendaState>,
    quick_capture: Option<CaptureState>,
    notification: Option<Notification>,
}

struct PickerState {
    kind: PickerKind,
    query: String,
    all_results: Vec<PickerItem>,
    filtered_results: Vec<PickerItem>,
    filters: Vec<String>,
    focused_filter: Option<usize>,
    marked_indices: BTreeSet<usize>,
    pending_gg: bool,
    selected_index: usize,
}

#[derive(Clone)]
struct PickerItem {
    label: String,
    marginalia: Vec<String>,
    preview: String,
}

struct CaptureState {
    kind: QuickCaptureKind,
    input: String,
    cursor_pos: usize,
}

#[derive(Clone)]
struct AgendaTask {
    line: usize,
    task_text: String,
    source_page: String,
    date: Option<NaiveDate>,
    tags: Vec<String>,
    section: AgendaSection,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AgendaSection {
    Overdue,
    Today,
    Upcoming,
}

#[derive(Clone, Copy)]
enum AgendaViewMode {
    Day,
    Week,
}

struct AgendaState {
    all_tasks: Vec<AgendaTask>,
    filtered_tasks: Vec<AgendaTask>,
    selected_index: usize,
    tag_filter: Option<String>,
    date_range: Option<(NaiveDate, NaiveDate)>,
    view_mode: AgendaViewMode,
    pending_v: bool,
    prompt: Option<AgendaPrompt>,
}

struct AgendaPrompt {
    kind: AgendaPromptKind,
    input: String,
}

enum AgendaPromptKind {
    Reschedule,
    TagFilter,
    DateFilter,
}

impl PickerState {
    fn title(&self) -> String {
        match self.kind {
            PickerKind::FindPage => "Find Page".into(),
            PickerKind::SwitchBuffer => "Switch Buffer".into(),
            PickerKind::Search => "Search".into(),
            PickerKind::Journal => "Search Journal".into(),
            PickerKind::Tags => "Search Tags".into(),
            PickerKind::Backlinks(_) => "Backlinks".into(),
            PickerKind::UnlinkedMentions(_) => "Unlinked Mentions".into(),
            PickerKind::AllCommands => "All Commands".into(),
            PickerKind::InlineLink => "Insert Link".into(),
            PickerKind::Templates => "Templates".into(),
        }
    }

    fn filter(&mut self) {
        let q = self.query.to_lowercase();
        self.filtered_results = self
            .all_results
            .iter()
            .filter(|item| {
                if q.is_empty() {
                    true
                } else {
                    item.label.to_lowercase().contains(&q)
                        || item
                            .marginalia
                            .iter()
                            .any(|m| m.to_lowercase().contains(&q))
                }
            })
            .filter(|item| {
                self.filters.iter().all(|f| {
                    let needle = f.split(':').nth(1).unwrap_or(f).to_lowercase();
                    item.label.to_lowercase().contains(&needle)
                        || item.preview.to_lowercase().contains(&needle)
                        || item
                            .marginalia
                            .iter()
                            .any(|m| m.to_lowercase().contains(&needle))
                })
            })
            .cloned()
            .collect();
        if self.filtered_results.is_empty() {
            self.selected_index = 0;
            self.marked_indices.clear();
        } else {
            self.selected_index = self.selected_index.min(self.filtered_results.len() - 1);
            self.marked_indices = self
                .marked_indices
                .iter()
                .copied()
                .filter(|idx| *idx < self.filtered_results.len())
                .collect();
        }
        if self.filters.is_empty() {
            self.focused_filter = None;
        } else {
            let last = self.filters.len().saturating_sub(1);
            self.focused_filter = Some(self.focused_filter.unwrap_or(last).min(last));
        }
    }
}

impl AgendaState {
    fn new(tasks: Vec<AgendaTask>) -> Self {
        let mut state = Self {
            all_tasks: tasks,
            filtered_tasks: Vec::new(),
            selected_index: 0,
            tag_filter: None,
            date_range: None,
            view_mode: AgendaViewMode::Week,
            pending_v: false,
            prompt: None,
        };
        state.refilter();
        state
    }

    fn refilter(&mut self) {
        let today = Local::now().date_naive();
        let mode_range = match self.view_mode {
            AgendaViewMode::Day => Some((today, today)),
            AgendaViewMode::Week => Some((today, today + ChronoDuration::days(6))),
        };
        let effective_range = self.date_range.or(mode_range);

        self.filtered_tasks = self
            .all_tasks
            .iter()
            .filter(|task| {
                if let Some(tag) = &self.tag_filter {
                    let needle = tag.to_lowercase();
                    if !task.tags.iter().any(|t| t.to_lowercase() == needle) {
                        return false;
                    }
                }
                if let Some((start, end)) = effective_range {
                    match task.date {
                        Some(d) => {
                            if d < start || d > end {
                                return false;
                            }
                        }
                        None => {
                            if !matches!(self.view_mode, AgendaViewMode::Day) {
                                return false;
                            }
                        }
                    }
                }
                true
            })
            .cloned()
            .collect();

        if self.filtered_tasks.is_empty() {
            self.selected_index = 0;
        } else {
            self.selected_index = self.selected_index.min(self.filtered_tasks.len() - 1);
        }
    }

    fn selected_task(&self) -> Option<&AgendaTask> {
        self.filtered_tasks.get(self.selected_index)
    }

    fn to_frame(&self) -> AgendaFrame {
        let mut overdue = Vec::new();
        let mut today = Vec::new();
        let mut upcoming = Vec::new();

        for task in &self.filtered_tasks {
            let item = AgendaItem {
                task_text: task.task_text.clone(),
                source_page: task.source_page.clone(),
                date: task.date,
                tags: task.tags.clone(),
            };
            match task.section {
                AgendaSection::Overdue => overdue.push(item),
                AgendaSection::Today => today.push(item),
                AgendaSection::Upcoming => upcoming.push(item),
            }
        }

        let total_pages = self
            .filtered_tasks
            .iter()
            .map(|t| t.source_page.clone())
            .collect::<BTreeSet<_>>()
            .len();

        AgendaFrame {
            overdue,
            today,
            upcoming,
            selected_index: self.selected_index,
            total_open: self.filtered_tasks.len(),
            total_pages,
        }
    }
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
            which_key_tree: which_key::default_tree(),
            leader_active: false,
            leader_keys: Vec::new(),
            picker: None,
            agenda: None,
            quick_capture: None,
            notification: None,
        }
    }

    fn handle_key(&mut self, key: types::KeyEvent) {
        self.cleanup_ephemeral();

        if self.agenda.is_some() {
            self.handle_agenda_key(&key);
            return;
        }

        if self.picker.is_none() && self.quick_capture.is_none() && self.handle_leader_key(&key) {
            return;
        }

        if self.picker.is_some() {
            self.handle_picker_key(&key);
            return;
        }

        // 1. Try keymap dispatcher (platform shortcuts, picker, quick-capture)
        let ctx = EditorContext {
            mode: self.vim.mode(),
            buffer: &self.buffer,
            cursor: self.cursor,
            picker_open: false,
            quick_capture_open: self.quick_capture.is_some(),
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

        // 2. Overlay input handling
        if self.quick_capture.is_some() {
            if self.apply_quick_capture_key(&key) {
                return;
            }
            return;
        }

        // 3. Insert mode: pass characters directly to buffer
        if self.vim.mode() == Mode::Insert {
            match &key.code {
                KeyCode::Char(c)
                    if key.modifiers == Modifiers::none()
                        || key.modifiers == Modifiers::shift() =>
                {
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

        // 4. Vim state machine
        let vim_action = self.vim.process_key(key, &self.buffer, self.cursor);
        self.apply_vim_action(vim_action);
    }

    fn apply_vim_action(&mut self, action: VimAction) {
        match action {
            VimAction::Edit(op) => self.apply_edit(op),
            VimAction::Motion(m) => {
                self.cursor = m
                    .new_position
                    .min(self.buffer.len_chars().saturating_sub(1));
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
        self.cursor = op
            .cursor_after
            .min(self.buffer.len_chars().saturating_sub(1));
    }

    fn apply_action(&mut self, action: Action) {
        match action {
            Action::Quit => self.should_quit = true,
            Action::Save => {
                self.buffer.mark_clean();
                self.notify("Saved", NotificationLevel::Info);
            }
            Action::Undo => {
                self.buffer.undo();
            }
            Action::Redo => {
                self.buffer.redo();
            }
            Action::Edit(op) => self.apply_edit(op),
            Action::Motion(m) => {
                self.cursor = m
                    .new_position
                    .min(self.buffer.len_chars().saturating_sub(1));
            }
            Action::ModeChange(_) => {}
            Action::OpenPicker(kind) => self.open_picker(kind),
            Action::ClosePicker => self.picker = None,
            Action::PickerInput(input) => self.apply_picker_input(input),
            Action::QuickCapture(kind) => {
                self.quick_capture = Some(CaptureState {
                    kind,
                    input: String::new(),
                    cursor_pos: 0,
                });
            }
            Action::SubmitQuickCapture(input) => self.submit_quick_capture(input),
            Action::CancelQuickCapture => self.quick_capture = None,
            Action::OpenPage(id) => {
                self.title = format!("Page {}", id.to_hex());
                self.filename = format!("{}.md", id.to_hex());
                self.buffer = Buffer::from_text(&format!("# {}\n\n", self.title));
                self.cursor = 0;
            }
            Action::OpenJournal(date) => {
                self.title = format!("Journal {}", date);
                self.filename = format!("journal-{}.md", date);
                self.buffer = Buffer::from_text(&format!("# Journal {}\n\n", date));
                self.cursor = self.buffer.len_chars().saturating_sub(1);
            }
            Action::SplitWindow(direction) => {
                let msg = match direction {
                    SplitDirection::Vertical => "Split vertical",
                    SplitDirection::Horizontal => "Split horizontal",
                };
                self.notify(msg, NotificationLevel::Info);
            }
            Action::NavigateWindow(direction) => {
                let msg = match direction {
                    WindowDirection::Left => "Navigate left",
                    WindowDirection::Right => "Navigate right",
                    WindowDirection::Up => "Navigate up",
                    WindowDirection::Down => "Navigate down",
                };
                self.notify(msg, NotificationLevel::Info);
            }
            Action::CloseWindow => self.notify("Close window", NotificationLevel::Info),
            Action::ResizeWindow(op) => {
                let msg = match op {
                    ResizeOp::IncreaseWidth => "Increase width",
                    ResizeOp::DecreaseWidth => "Decrease width",
                    ResizeOp::IncreaseHeight => "Increase height",
                    ResizeOp::DecreaseHeight => "Decrease height",
                };
                self.notify(msg, NotificationLevel::Info);
            }
            Action::ToggleTask => {
                if self.agenda.is_some() {
                    self.toggle_selected_agenda_task();
                } else {
                    self.notify("Toggle task", NotificationLevel::Info)
                }
            }
            Action::FollowLink => self.notify("Follow link", NotificationLevel::Info),
            Action::CopyToClipboard(text) => {
                self.notify(format!("Copied: {text}"), NotificationLevel::Info);
            }
            Action::OpenTimeline(_) => self.notify("Open timeline", NotificationLevel::Info),
            Action::OpenAgenda => self.toggle_agenda(),
            Action::OpenUndoTree => self.notify("Open undo tree", NotificationLevel::Info),
            Action::OpenDatePicker(_) => {
                if let Some(agenda) = self.agenda.as_mut() {
                    agenda.prompt = Some(AgendaPrompt {
                        kind: AgendaPromptKind::Reschedule,
                        input: String::new(),
                    });
                } else {
                    self.notify("Open date picker", NotificationLevel::Info)
                }
            }
            Action::DialogResponse(_) => self.notify("Dialog response", NotificationLevel::Info),
            Action::Refactor(_) => self.notify("Refactor action", NotificationLevel::Info),
            Action::TemplateAdvance => self.notify("Template advance", NotificationLevel::Info),
            Action::RebuildIndex => self.notify("Rebuild index", NotificationLevel::Info),
            Action::ToggleMcp => self.notify("Toggle MCP", NotificationLevel::Info),
            Action::Noop => {}
        }
    }

    fn apply_command(&mut self, cmd: &str) {
        let cmd = cmd.trim().trim_start_matches(':');
        match cmd {
            "undo" => {
                self.buffer.undo();
            }
            "redo" => {
                self.buffer.redo();
            }
            "w" | "write" => {
                self.buffer.mark_clean();
            }
            "q" | "quit" | "qa" | "qall" => {
                self.should_quit = true;
            }
            "wq" | "x" | "write-quit" => {
                self.buffer.mark_clean();
                self.should_quit = true;
            }
            "rebuild-index" => self.apply_action(Action::RebuildIndex),
            "agenda" => self.apply_action(Action::OpenAgenda),
            _ if cmd.starts_with("theme ") => {
                self.notify("Theme switch requested", NotificationLevel::Info)
            }
            _ if cmd.starts_with("rename ") => {
                self.notify("Rename requested", NotificationLevel::Info)
            }
            _ if cmd.starts_with("import-logseq ") => {
                self.notify("Import requested", NotificationLevel::Info)
            }
            "" => {}
            _ => self.notify(
                format!("Unknown command: {cmd}"),
                NotificationLevel::Warning,
            ),
        }
    }

    fn resize(&mut self, width: u16, height: u16) {
        // Reserve 1 row for status bar
        let editor_height = (height as usize).saturating_sub(1);
        self.viewport = Viewport::new(editor_height, width as usize);
    }

    fn render_frame(&mut self) -> RenderFrame {
        self.cleanup_ephemeral();
        let rope = self.buffer.text();
        let cursor_line = rope.char_to_line(
            self.cursor
                .min(self.buffer.len_chars().saturating_sub(1).max(0)),
        );
        let line_start = rope.line_to_char(cursor_line);
        let cursor_col = self.cursor.saturating_sub(line_start);

        self.viewport.ensure_visible(cursor_line);
        let visible = self.viewport.visible_range();

        let mut visible_lines = Vec::new();
        let total_lines = rope.len_lines();

        // Track context for syntax highlighting (frontmatter / code blocks)
        let mut ctx = LineContext::default();
        // Scan lines before the viewport to establish context
        for pre_idx in 0..visible.start.min(total_lines) {
            let pre_text: String = rope.line(pre_idx).to_string();
            let trimmed = pre_text.trim();
            if pre_idx == 0 && trimmed == "---" {
                ctx.in_frontmatter = true;
            } else if ctx.in_frontmatter && trimmed == "---" {
                ctx.in_frontmatter = false;
            } else if !ctx.in_code_block && trimmed.starts_with("```") {
                ctx.in_code_block = true;
                ctx.code_fence_lang = trimmed.strip_prefix("```").map(|s| s.trim().to_string());
            } else if ctx.in_code_block && trimmed.starts_with("```") {
                ctx.in_code_block = false;
                ctx.code_fence_lang = None;
            }
        }

        for line_idx in visible.start..visible.end.min(total_lines) {
            let line_text: String = rope.line(line_idx).to_string();
            let trimmed = line_text.trim();

            // Update context for fence/frontmatter boundaries
            if line_idx == 0 && trimmed == "---" {
                ctx.in_frontmatter = true;
            } else if ctx.in_frontmatter && trimmed == "---" {
                let spans = highlight_line(&line_text, &ctx);
                visible_lines.push(RenderedLine {
                    line_number: line_idx,
                    spans,
                });
                ctx.in_frontmatter = false;
                continue;
            } else if !ctx.in_code_block && trimmed.starts_with("```") {
                ctx.in_code_block = true;
                ctx.code_fence_lang =
                    trimmed.strip_prefix("```").map(|s| s.trim().to_string());
            } else if ctx.in_code_block && trimmed.starts_with("```") {
                let spans = highlight_line(&line_text, &ctx);
                visible_lines.push(RenderedLine {
                    line_number: line_idx,
                    spans,
                });
                ctx.in_code_block = false;
                ctx.code_fence_lang = None;
                continue;
            }

            let spans = highlight_line(&line_text, &ctx);
            visible_lines.push(RenderedLine {
                line_number: line_idx,
                spans,
            });
        }

        let agenda_frame = self.agenda.as_ref().map(|a| a.to_frame());
        let cursor_shape = if agenda_frame.is_some() {
            CursorShape::Block
        } else {
            match self.vim.mode() {
                Mode::Insert => CursorShape::Bar,
                Mode::Visual { .. } => CursorShape::Underline,
                _ => CursorShape::Block,
            }
        };

        let mode_str = if agenda_frame.is_some() {
            "AGENDA"
        } else {
            match self.vim.mode() {
                Mode::Normal => "NORMAL",
                Mode::Insert => "INSERT",
                Mode::Visual { .. } => "VISUAL",
                Mode::Command => "COMMAND",
            }
        };

        let pane = PaneFrame {
            id: PaneId(0),
            kind: if let Some(agenda) = agenda_frame {
                RenderPaneKind::Agenda(agenda)
            } else {
                RenderPaneKind::Editor
            },
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
                pending_keys: self.pending_keys_text(),
                recording_macro: if self.vim.is_recording() {
                    Some('q')
                } else {
                    None
                },
                mcp_status: None,
            },
        };

        RenderFrame {
            panes: vec![pane],
            maximized: false,
            hidden_pane_count: 0,
            picker: self.current_picker_frame(),
            which_key: self.current_which_key_frame(),
            command_line: self.current_command_line_frame(),
            quick_capture: self.current_quick_capture_frame(),
            date_picker: None,
            dialog: None,
            notification: self.notification.clone(),
        }
    }

    fn handle_leader_key(&mut self, key: &types::KeyEvent) -> bool {
        if self.vim.mode() != Mode::Normal {
            self.clear_leader();
            return false;
        }

        if !self.leader_active {
            if key.code == KeyCode::Char(' ') && key.modifiers == Modifiers::none() {
                self.leader_active = true;
                self.leader_keys.clear();
                return true;
            }
            return false;
        }

        match key.code {
            KeyCode::Esc => {
                self.clear_leader();
                true
            }
            KeyCode::Char(c) => {
                let mut key_event = types::KeyEvent::char(c);
                if key.modifiers.ctrl {
                    key_event = types::KeyEvent::ctrl(c);
                }
                self.leader_keys.push(key_event);
                match self.which_key_tree.lookup(&self.leader_keys) {
                    WhichKeyLookup::Prefix(_) => true,
                    WhichKeyLookup::Action(action) => {
                        self.execute_leader_action(&action);
                        self.clear_leader();
                        true
                    }
                    WhichKeyLookup::NoMatch => {
                        self.notify("No leader binding", NotificationLevel::Warning);
                        self.clear_leader();
                        true
                    }
                }
            }
            _ => {
                self.clear_leader();
                true
            }
        }
    }

    fn clear_leader(&mut self) {
        self.leader_active = false;
        self.leader_keys.clear();
    }

    fn pending_keys_text(&self) -> String {
        if self.leader_active {
            let mut keys = String::from("SPC");
            for key in &self.leader_keys {
                keys.push(' ');
                keys.push_str(&key.to_string());
            }
            return keys;
        }
        self.vim.pending_keys().to_string()
    }

    fn current_which_key_frame(&self) -> Option<WhichKeyFrame> {
        if !self.leader_active {
            return None;
        }
        match self.which_key_tree.lookup(&self.leader_keys) {
            WhichKeyLookup::Prefix(entries) => Some(WhichKeyFrame {
                entries: entries
                    .into_iter()
                    .map(|e| WhichKeyEntry {
                        key: e.key,
                        label: e.label,
                        is_group: e.is_group,
                    })
                    .collect(),
                prefix: self.pending_keys_text(),
                context: WhichKeyContext::Leader,
            }),
            _ => None,
        }
    }

    fn current_picker_frame(&self) -> Option<PickerFrame> {
        self.picker.as_ref().map(|p| PickerFrame {
            title: p.title(),
            query: p.query.clone(),
            results: p
                .filtered_results
                .iter()
                .enumerate()
                .map(|(idx, item)| PickerRow {
                    label: if p.marked_indices.contains(&idx) {
                        format!("* {}", item.label)
                    } else {
                        item.label.clone()
                    },
                    marginalia: item.marginalia.clone(),
                })
                .collect(),
            selected_index: p.selected_index,
            filters: p
                .filters
                .iter()
                .enumerate()
                .map(|(i, f)| {
                    if p.focused_filter == Some(i) {
                        format!("[*{f}]")
                    } else {
                        format!("[{f}]")
                    }
                })
                .collect(),
            preview: if matches!(p.kind, PickerKind::InlineLink) {
                None
            } else {
                p.filtered_results
                    .get(p.selected_index)
                    .map(|item| item.preview.clone())
            },
            total_count: p.all_results.len(),
            filtered_count: p.filtered_results.len(),
        })
    }

    fn current_quick_capture_frame(&self) -> Option<QuickCaptureFrame> {
        self.quick_capture.as_ref().map(|qc| QuickCaptureFrame {
            prompt: match qc.kind {
                QuickCaptureKind::Note => "📓 Append to journal > ".to_string(),
                QuickCaptureKind::Task => "☐ Append task > ".to_string(),
            },
            input: qc.input.clone(),
            cursor_pos: qc.cursor_pos,
        })
    }

    fn current_command_line_frame(&self) -> Option<CommandLineFrame> {
        if let Some(agenda) = &self.agenda {
            if let Some(prompt) = &agenda.prompt {
                let (prefix, description) = match prompt.kind {
                    AgendaPromptKind::Reschedule => ("reschedule> ", "YYYY-MM-DD"),
                    AgendaPromptKind::TagFilter => ("tag> ", "filter by tag"),
                    AgendaPromptKind::DateFilter => ("date> ", "YYYY-MM-DD..YYYY-MM-DD"),
                };
                let input = format!("{prefix}{}", prompt.input);
                return Some(CommandLineFrame {
                    cursor_pos: input.len(),
                    input,
                    completions: vec![bloom_core::render::Completion {
                        text: description.to_string(),
                        description: "agenda prompt".to_string(),
                    }],
                    selected_completion: None,
                    error: None,
                });
            }
        }

        if self.vim.mode() != Mode::Command {
            return None;
        }
        let input = format!(":{}", self.vim.pending_keys());
        Some(CommandLineFrame {
            cursor_pos: input.len(),
            input,
            completions: vec![],
            selected_completion: None,
            error: None,
        })
    }

    fn notify(&mut self, message: impl Into<String>, level: NotificationLevel) {
        self.notification = Some(Notification {
            message: message.into(),
            level,
            expires_at: Instant::now() + Duration::from_secs(3),
        });
    }

    fn cleanup_ephemeral(&mut self) {
        if self
            .notification
            .as_ref()
            .is_some_and(|n| n.expires_at <= Instant::now())
        {
            self.notification = None;
        }
    }

    fn toggle_agenda(&mut self) {
        if self.agenda.is_some() {
            self.agenda = None;
            self.notify("Agenda closed", NotificationLevel::Info);
            return;
        }
        let tasks = self.parse_agenda_tasks();
        self.agenda = Some(AgendaState::new(tasks));
        self.notify("Agenda opened", NotificationLevel::Info);
    }

    fn refresh_agenda(&mut self) {
        let Some(old) = self.agenda.take() else {
            return;
        };
        let mut rebuilt = AgendaState::new(self.parse_agenda_tasks());
        rebuilt.tag_filter = old.tag_filter;
        rebuilt.date_range = old.date_range;
        rebuilt.view_mode = old.view_mode;
        rebuilt.prompt = old.prompt;
        rebuilt.pending_v = old.pending_v;
        rebuilt.selected_index = old.selected_index;
        rebuilt.refilter();
        self.agenda = Some(rebuilt);
    }

    fn parse_agenda_tasks(&self) -> Vec<AgendaTask> {
        let today = Local::now().date_naive();
        let rope = self.buffer.text();
        let mut out = Vec::new();
        for line_idx in 0..rope.len_lines() {
            let raw: String = rope.line(line_idx).to_string();
            let line = raw.trim_end_matches('\n').trim_end_matches('\r');
            let trimmed = line.trim_start();
            if !trimmed.starts_with("- [ ]") {
                continue;
            }
            let task_text = trimmed
                .strip_prefix("- [ ]")
                .unwrap_or(trimmed)
                .trim()
                .to_string();
            let due = extract_marker_date(line, "due");
            let start = extract_marker_date(line, "start");
            let date = due.or(start);
            let section = if let Some(d) = due {
                if d < today {
                    AgendaSection::Overdue
                } else if d == today {
                    AgendaSection::Today
                } else {
                    AgendaSection::Upcoming
                }
            } else if let Some(s) = start {
                if s <= today {
                    AgendaSection::Today
                } else {
                    AgendaSection::Upcoming
                }
            } else {
                AgendaSection::Today
            };
            let tags = extract_tags(line);
            out.push(AgendaTask {
                line: line_idx,
                task_text,
                source_page: self.filename.clone(),
                date,
                tags,
                section,
            });
        }
        out
    }

    fn handle_agenda_key(&mut self, key: &types::KeyEvent) {
        if self.handle_agenda_prompt_key(key) {
            return;
        }

        if self.agenda.is_none() {
            return;
        }

        if self.agenda.as_ref().is_some_and(|a| a.pending_v) {
            if let KeyCode::Char(c) = key.code {
                let agenda = self.agenda.as_mut().expect("agenda exists");
                match c {
                    'd' => {
                        agenda.view_mode = AgendaViewMode::Day;
                        agenda.refilter();
                    }
                    'w' => {
                        agenda.view_mode = AgendaViewMode::Week;
                        agenda.refilter();
                    }
                    _ => {}
                }
            }
            if let Some(agenda) = self.agenda.as_mut() {
                agenda.pending_v = false;
            }
            return;
        }

        match key.code {
            KeyCode::Esc => self.agenda = None,
            KeyCode::Down | KeyCode::Char('j') if key.modifiers == Modifiers::none() => {
                let agenda = self.agenda.as_mut().expect("agenda exists");
                if !agenda.filtered_tasks.is_empty() {
                    agenda.selected_index =
                        (agenda.selected_index + 1).min(agenda.filtered_tasks.len() - 1);
                }
            }
            KeyCode::Up | KeyCode::Char('k') if key.modifiers == Modifiers::none() => {
                let agenda = self.agenda.as_mut().expect("agenda exists");
                agenda.selected_index = agenda.selected_index.saturating_sub(1);
            }
            KeyCode::Char('q') if key.modifiers == Modifiers::none() => self.agenda = None,
            KeyCode::Enter => self.jump_to_selected_agenda_task(false),
            KeyCode::Char('o') if key.modifiers == Modifiers::none() => {
                self.jump_to_selected_agenda_task(true)
            }
            KeyCode::Char('x') if key.modifiers == Modifiers::none() => {
                self.toggle_selected_agenda_task()
            }
            KeyCode::Char('s') if key.modifiers == Modifiers::none() => {
                let agenda = self.agenda.as_mut().expect("agenda exists");
                agenda.prompt = Some(AgendaPrompt {
                    kind: AgendaPromptKind::Reschedule,
                    input: String::new(),
                });
            }
            KeyCode::Char('t') if key.modifiers == Modifiers::none() => {
                let agenda = self.agenda.as_mut().expect("agenda exists");
                agenda.prompt = Some(AgendaPrompt {
                    kind: AgendaPromptKind::TagFilter,
                    input: agenda.tag_filter.clone().unwrap_or_default(),
                });
            }
            KeyCode::Char('d') if key.modifiers == Modifiers::none() => {
                let agenda = self.agenda.as_mut().expect("agenda exists");
                agenda.prompt = Some(AgendaPrompt {
                    kind: AgendaPromptKind::DateFilter,
                    input: String::new(),
                });
            }
            KeyCode::Char('v') if key.modifiers == Modifiers::none() => {
                let agenda = self.agenda.as_mut().expect("agenda exists");
                agenda.pending_v = true;
            }
            _ => {}
        }
    }

    fn handle_agenda_prompt_key(&mut self, key: &types::KeyEvent) -> bool {
        if self.agenda.is_none() {
            return false;
        }
        if self
            .agenda
            .as_ref()
            .and_then(|a| a.prompt.as_ref())
            .is_none()
        {
            return false;
        }
        match key.code {
            KeyCode::Esc => {
                let agenda = self.agenda.as_mut().expect("agenda exists");
                agenda.prompt = None;
                true
            }
            KeyCode::Enter => {
                self.apply_agenda_prompt();
                true
            }
            KeyCode::Backspace if key.modifiers == Modifiers::none() => {
                let agenda = self.agenda.as_mut().expect("agenda exists");
                let prompt = agenda.prompt.as_mut().expect("prompt exists");
                prompt.input.pop();
                true
            }
            KeyCode::Char(c)
                if key.modifiers == Modifiers::none() || key.modifiers == Modifiers::shift() =>
            {
                let agenda = self.agenda.as_mut().expect("agenda exists");
                let prompt = agenda.prompt.as_mut().expect("prompt exists");
                prompt.input.push(c);
                true
            }
            _ => true,
        }
    }

    fn apply_agenda_prompt(&mut self) {
        let Some(agenda) = self.agenda.as_mut() else {
            return;
        };
        let Some(prompt) = agenda.prompt.take() else {
            return;
        };
        match prompt.kind {
            AgendaPromptKind::TagFilter => {
                let input = prompt.input.trim().trim_start_matches('#').to_string();
                agenda.tag_filter = if input.is_empty() { None } else { Some(input) };
                agenda.refilter();
            }
            AgendaPromptKind::DateFilter => {
                let value = prompt.input.trim();
                if value.is_empty() {
                    agenda.date_range = None;
                    agenda.refilter();
                    return;
                }
                if let Some((start, end)) = parse_date_range(value) {
                    agenda.date_range = Some((start, end));
                    agenda.refilter();
                } else {
                    self.notify("Invalid date range", NotificationLevel::Warning);
                }
            }
            AgendaPromptKind::Reschedule => {
                let value = prompt.input.trim();
                match NaiveDate::parse_from_str(value, "%Y-%m-%d") {
                    Ok(date) => self.reschedule_selected_agenda_task(date),
                    Err(_) => self.notify("Invalid date format", NotificationLevel::Warning),
                }
            }
        }
    }

    fn jump_to_selected_agenda_task(&mut self, split: bool) {
        let Some((line, source)) = self
            .agenda
            .as_ref()
            .and_then(|a| a.selected_task().map(|t| (t.line, t.source_page.clone())))
        else {
            return;
        };
        let rope = self.buffer.text();
        self.cursor = rope
            .line_to_char(line.min(rope.len_lines().saturating_sub(1)))
            .min(self.buffer.len_chars().saturating_sub(1));
        self.agenda = None;
        if split {
            self.notify(
                format!("Opened {source} in split (fallback to single pane)"),
                NotificationLevel::Info,
            );
        }
    }

    fn toggle_selected_agenda_task(&mut self) {
        let Some(line) = self
            .agenda
            .as_ref()
            .and_then(|a| a.selected_task().map(|t| t.line))
        else {
            return;
        };
        let Some(current) = self.buffer_line(line) else {
            return;
        };
        let updated = if current.contains("- [ ]") {
            current.replacen("- [ ]", "- [x]", 1)
        } else if current.contains("- [x]") {
            current.replacen("- [x]", "- [ ]", 1)
        } else {
            current
        };
        self.replace_buffer_line(line, &updated);
        self.refresh_agenda();
    }

    fn reschedule_selected_agenda_task(&mut self, new_date: NaiveDate) {
        let Some(line) = self
            .agenda
            .as_ref()
            .and_then(|a| a.selected_task().map(|t| t.line))
        else {
            return;
        };
        let Some(current) = self.buffer_line(line) else {
            return;
        };
        let formatted = new_date.to_string();
        let updated = if let Some((start, end)) = due_date_span(&current) {
            let mut s = current.clone();
            s.replace_range(start..end, &formatted);
            s
        } else {
            let trimmed = current.trim_end_matches('\n').trim_end_matches('\r');
            let suffix = if current.ends_with('\n') { "\n" } else { "" };
            format!("{trimmed} @due({formatted}){suffix}")
        };
        self.replace_buffer_line(line, &updated);
        self.refresh_agenda();
    }

    fn buffer_line(&self, line: usize) -> Option<String> {
        let rope = self.buffer.text();
        if line >= rope.len_lines() {
            return None;
        }
        Some(rope.line(line).to_string())
    }

    fn replace_buffer_line(&mut self, line: usize, new_line: &str) {
        let rope = self.buffer.text();
        if line >= rope.len_lines() {
            return;
        }
        let start = rope.line_to_char(line);
        let end = if line + 1 < rope.len_lines() {
            rope.line_to_char(line + 1)
        } else {
            self.buffer.len_chars()
        };
        self.buffer.replace(start..end, new_line);
    }

    fn seed_picker_rows(&self, kind: &PickerKind) -> Vec<PickerItem> {
        let base = vec![PickerItem {
            label: self.title.clone(),
            marginalia: vec![self.filename.clone()],
            preview: self.buffer.text().to_string(),
        }];
        match kind {
            PickerKind::FindPage | PickerKind::SwitchBuffer => base,
            PickerKind::Search => vec![
                PickerItem {
                    label: "Welcome".into(),
                    marginalia: vec!["Current buffer".into()],
                    preview: "Welcome note preview".into(),
                },
                PickerItem {
                    label: "Start typing".into(),
                    marginalia: vec!["Hint".into()],
                    preview: "Type to narrow search results".into(),
                },
            ],
            PickerKind::Journal => vec![PickerItem {
                label: "Today".into(),
                marginalia: vec!["Journal".into()],
                preview: "# Journal\n\n- Entry preview".into(),
            }],
            PickerKind::Tags => vec![PickerItem {
                label: "#todo".into(),
                marginalia: vec!["Tag".into()],
                preview: "Pages using #todo".into(),
            }],
            PickerKind::Backlinks(_) | PickerKind::UnlinkedMentions(_) => {
                vec![PickerItem {
                    label: "No related pages yet".into(),
                    marginalia: vec![],
                    preview: "No preview available".into(),
                }]
            }
            PickerKind::AllCommands => vec![
                PickerItem {
                    label: "write".into(),
                    marginalia: vec!["Save".into()],
                    preview: "Save current buffer".into(),
                },
                PickerItem {
                    label: "quit".into(),
                    marginalia: vec!["Quit".into()],
                    preview: "Quit application".into(),
                },
            ],
            PickerKind::InlineLink => vec![PickerItem {
                label: self.title.clone(),
                marginalia: vec!["Current page".into()],
                preview: String::new(),
            }],
            PickerKind::Templates => vec![PickerItem {
                label: "Blank note".into(),
                marginalia: vec!["Template".into()],
                preview: "Create a new blank note".into(),
            }],
        }
    }

    fn open_picker(&mut self, kind: PickerKind) {
        let mut picker = PickerState {
            all_results: self.seed_picker_rows(&kind),
            filtered_results: Vec::new(),
            kind,
            query: String::new(),
            filters: Vec::new(),
            focused_filter: None,
            marked_indices: BTreeSet::new(),
            pending_gg: false,
            selected_index: 0,
        };
        picker.filter();
        self.picker = Some(picker);
    }

    fn handle_picker_key(&mut self, key: &types::KeyEvent) {
        if key.modifiers.ctrl {
            match key.code {
                KeyCode::Char('n') | KeyCode::Char('j') => {
                    self.apply_picker_input(PickerInputAction::MoveSelection(1))
                }
                KeyCode::Char('p') | KeyCode::Char('k') => {
                    self.apply_picker_input(PickerInputAction::MoveSelection(-1))
                }
                KeyCode::Char('u') => {
                    if let Some(picker) = self.picker.as_mut() {
                        picker.query.clear();
                        picker.pending_gg = false;
                        picker.filter();
                    }
                }
                KeyCode::Char('g') => self.apply_picker_input(PickerInputAction::Cancel),
                KeyCode::Char('t') => self.add_picker_filter("tag:todo"),
                KeyCode::Char('d') => self.add_picker_filter("date:this-week"),
                KeyCode::Char('l') => self.add_picker_filter("links:current"),
                KeyCode::Char('s') => self.add_picker_filter("task:open"),
                KeyCode::Left => {
                    if let Some(picker) = self.picker.as_mut() {
                        if !picker.filters.is_empty() {
                            let cur = picker.focused_filter.unwrap_or(0);
                            picker.focused_filter = Some(cur.saturating_sub(1));
                        }
                    }
                }
                KeyCode::Right => {
                    if let Some(picker) = self.picker.as_mut() {
                        if !picker.filters.is_empty() {
                            let cur = picker.focused_filter.unwrap_or(0);
                            picker.focused_filter = Some((cur + 1).min(picker.filters.len() - 1));
                        }
                    }
                }
                KeyCode::Backspace => {
                    if let Some(picker) = self.picker.as_mut() {
                        picker.filters.clear();
                        picker.focused_filter = None;
                        picker.filter();
                    }
                }
                _ => {}
            }
            return;
        }

        if key.modifiers.alt && key.code == KeyCode::Enter {
            self.create_page_from_picker_query();
            return;
        }

        match key.code {
            KeyCode::Esc => self.apply_picker_input(PickerInputAction::Cancel),
            KeyCode::Enter => self.apply_picker_input(PickerInputAction::Select),
            KeyCode::Up => self.apply_picker_input(PickerInputAction::MoveSelection(-1)),
            KeyCode::Down => self.apply_picker_input(PickerInputAction::MoveSelection(1)),
            KeyCode::Tab => self.apply_picker_input(PickerInputAction::ToggleMark),
            KeyCode::Backspace => {
                if let Some(picker) = self.picker.as_mut() {
                    if picker.query.is_empty() {
                        if let Some(idx) = picker.focused_filter {
                            if idx < picker.filters.len() {
                                picker.filters.remove(idx);
                            }
                            if picker.filters.is_empty() {
                                picker.focused_filter = None;
                            } else {
                                picker.focused_filter = Some(idx.min(picker.filters.len() - 1));
                            }
                            picker.filter();
                        }
                    } else {
                        picker.query.pop();
                        picker.pending_gg = false;
                        picker.filter();
                    }
                }
            }
            KeyCode::Char('g') if key.modifiers == Modifiers::none() => {
                if let Some(picker) = self.picker.as_mut() {
                    if picker.query.is_empty() {
                        if picker.pending_gg {
                            picker.selected_index = 0;
                            picker.pending_gg = false;
                        } else {
                            picker.pending_gg = true;
                        }
                    } else {
                        picker.query.push('g');
                        picker.pending_gg = false;
                        picker.filter();
                    }
                }
            }
            KeyCode::Char('G') if key.modifiers == Modifiers::shift() => {
                if let Some(picker) = self.picker.as_mut() {
                    if picker.query.is_empty() && !picker.filtered_results.is_empty() {
                        picker.selected_index = picker.filtered_results.len() - 1;
                    } else {
                        picker.query.push('G');
                        picker.filter();
                    }
                    picker.pending_gg = false;
                }
            }
            KeyCode::Char(c)
                if key.modifiers == Modifiers::none() || key.modifiers == Modifiers::shift() =>
            {
                if let Some(picker) = self.picker.as_mut() {
                    picker.query.push(c);
                    picker.pending_gg = false;
                    picker.filter();
                }
            }
            _ => {
                if let Some(picker) = self.picker.as_mut() {
                    picker.pending_gg = false;
                }
            }
        }
    }

    fn add_picker_filter(&mut self, filter: &str) {
        if let Some(picker) = self.picker.as_mut() {
            if !picker.filters.iter().any(|f| f == filter) {
                picker.filters.push(filter.to_string());
            }
            picker.focused_filter = Some(picker.filters.len().saturating_sub(1));
            picker.filter();
        }
    }

    fn create_page_from_picker_query(&mut self) {
        let Some(picker) = &self.picker else {
            return;
        };
        let title = picker.query.trim();
        if title.is_empty() {
            self.notify("Enter a query to create", NotificationLevel::Warning);
            return;
        }
        if matches!(picker.kind, PickerKind::InlineLink) {
            let link = format!("[[{title}]]");
            self.buffer.insert(self.cursor, &link);
            self.cursor += link.chars().count();
        } else {
            self.title = title.to_string();
            self.filename = format!("{}.md", title.to_lowercase().replace(' ', "-"));
            self.buffer = Buffer::from_text(&format!("# {}\n\n", title));
            self.cursor = self.buffer.len_chars().saturating_sub(1);
        }
        self.picker = None;
        self.notify("Created from query", NotificationLevel::Info);
    }

    fn apply_picker_input(&mut self, input: PickerInputAction) {
        if self.picker.is_none() {
            return;
        }
        match input {
            PickerInputAction::UpdateQuery(s) => {
                let picker = self.picker.as_mut().expect("picker exists");
                if s.is_empty() {
                    picker.query.pop();
                } else {
                    picker.query.push_str(&s);
                }
                picker.pending_gg = false;
                picker.filter();
            }
            PickerInputAction::MoveSelection(delta) => {
                let picker = self.picker.as_mut().expect("picker exists");
                if picker.filtered_results.is_empty() {
                    picker.selected_index = 0;
                    return;
                }
                let len = picker.filtered_results.len() as i32;
                let idx = (picker.selected_index as i32 + delta).clamp(0, len - 1);
                picker.selected_index = idx as usize;
                picker.pending_gg = false;
            }
            PickerInputAction::Select => self.apply_picker_selection(),
            PickerInputAction::ToggleMark => {
                if let Some(picker) = self.picker.as_mut() {
                    if picker.marked_indices.contains(&picker.selected_index) {
                        picker.marked_indices.remove(&picker.selected_index);
                    } else if !picker.filtered_results.is_empty() {
                        picker.marked_indices.insert(picker.selected_index);
                    }
                }
            }
            PickerInputAction::Cancel => self.picker = None,
        }
    }

    fn apply_picker_selection(&mut self) {
        let Some(picker) = &self.picker else {
            return;
        };
        let kind = picker.kind.clone();
        let picked_labels: Vec<String> = if picker.marked_indices.is_empty() {
            picker
                .filtered_results
                .get(picker.selected_index)
                .map(|item| vec![item.label.clone()])
                .unwrap_or_default()
        } else {
            picker
                .marked_indices
                .iter()
                .filter_map(|idx| picker.filtered_results.get(*idx))
                .map(|item| item.label.clone())
                .collect()
        };
        let selected = picked_labels.first().cloned().unwrap_or_default();

        match kind {
            PickerKind::InlineLink => {
                let text = picked_labels
                    .iter()
                    .map(|label| format!("[[{label}]]"))
                    .collect::<Vec<_>>()
                    .join(" ");
                self.buffer.insert(self.cursor, &text);
                self.cursor += text.chars().count();
            }
            PickerKind::FindPage | PickerKind::SwitchBuffer => {
                self.title = selected.clone();
                self.filename = format!("{}.md", selected.to_lowercase().replace(' ', "-"));
            }
            _ => self.notify(
                format!("Selected {} item(s)", picked_labels.len().max(1)),
                NotificationLevel::Info,
            ),
        }
        self.picker = None;
    }

    fn apply_quick_capture_key(&mut self, key: &types::KeyEvent) -> bool {
        let Some(qc) = &mut self.quick_capture else {
            return false;
        };
        match key.code {
            KeyCode::Char(c)
                if key.modifiers == Modifiers::none() || key.modifiers == Modifiers::shift() =>
            {
                qc.input.push(c);
                qc.cursor_pos += 1;
                true
            }
            KeyCode::Backspace if key.modifiers == Modifiers::none() => {
                if qc.cursor_pos > 0 {
                    qc.input.pop();
                    qc.cursor_pos -= 1;
                }
                true
            }
            KeyCode::Enter if key.modifiers == Modifiers::none() => {
                self.submit_quick_capture(String::new());
                true
            }
            _ => false,
        }
    }

    fn submit_quick_capture(&mut self, input: String) {
        let Some(qc) = self.quick_capture.take() else {
            return;
        };
        let text = if input.is_empty() { qc.input } else { input };
        if text.trim().is_empty() {
            self.notify("Quick capture canceled", NotificationLevel::Warning);
            return;
        }
        let insertion = match qc.kind {
            QuickCaptureKind::Note => format!("\n{text}\n"),
            QuickCaptureKind::Task => format!("\n- [ ] {text}\n"),
        };
        let at = self.buffer.len_chars();
        self.buffer.insert(at, &insertion);
        self.cursor = at + insertion.chars().count().saturating_sub(1);
        self.notify("Quick capture saved", NotificationLevel::Info);
    }

    fn execute_leader_action(&mut self, action_id: &str) {
        match action_id {
            "find_page" => self.apply_action(Action::OpenPicker(PickerKind::FindPage)),
            "rename_page" => self.notify("Rename page", NotificationLevel::Info),
            "delete_page" => self.notify("Delete page", NotificationLevel::Warning),
            "switch_buffer" => self.apply_action(Action::OpenPicker(PickerKind::SwitchBuffer)),
            "close_buffer" => self.notify("Close buffer", NotificationLevel::Info),
            "journal_today" => {
                self.title = "Journal Today".into();
                self.filename = "journal-today.md".into();
                self.buffer = Buffer::from_text("# Journal Today\n\n");
                self.cursor = self.buffer.len_chars().saturating_sub(1);
            }
            "journal_prev" => self.notify("Open previous journal", NotificationLevel::Info),
            "journal_next" => self.notify("Open next journal", NotificationLevel::Info),
            "journal_append" => self.apply_action(Action::QuickCapture(QuickCaptureKind::Note)),
            "journal_task" => self.apply_action(Action::QuickCapture(QuickCaptureKind::Task)),
            "search" => self.apply_action(Action::OpenPicker(PickerKind::Search)),
            "search_journal" => self.apply_action(Action::OpenPicker(PickerKind::Journal)),
            "search_tags" => self.apply_action(Action::OpenPicker(PickerKind::Tags)),
            "search_backlinks" => {
                self.apply_action(Action::OpenPicker(PickerKind::Backlinks(types::PageId([
                    0, 0, 0, 0,
                ]))))
            }
            "search_unlinked" => self.apply_action(Action::OpenPicker(
                PickerKind::UnlinkedMentions(types::PageId([0, 0, 0, 0])),
            )),
            "insert_link" => self.apply_action(Action::OpenPicker(PickerKind::InlineLink)),
            "yank_link" => {
                self.apply_action(Action::CopyToClipboard(format!("[[{}]]", self.title)))
            }
            "timeline" => self.apply_action(Action::OpenTimeline(types::PageId([0, 0, 0, 0]))),
            "backlinks" => {
                self.apply_action(Action::OpenPicker(PickerKind::Backlinks(types::PageId([
                    0, 0, 0, 0,
                ]))))
            }
            "agenda" => self.apply_action(Action::OpenAgenda),
            "split_vertical" => self.apply_action(Action::SplitWindow(SplitDirection::Vertical)),
            "split_horizontal" => {
                self.apply_action(Action::SplitWindow(SplitDirection::Horizontal))
            }
            "navigate_left" => self.apply_action(Action::NavigateWindow(WindowDirection::Left)),
            "navigate_down" => self.apply_action(Action::NavigateWindow(WindowDirection::Down)),
            "navigate_up" => self.apply_action(Action::NavigateWindow(WindowDirection::Up)),
            "navigate_right" => self.apply_action(Action::NavigateWindow(WindowDirection::Right)),
            "close_window" => self.apply_action(Action::CloseWindow),
            "balance" => self.notify("Balance windows", NotificationLevel::Info),
            "maximize" => self.notify("Toggle maximize", NotificationLevel::Info),
            "split_page" => self.notify("Refactor: split page", NotificationLevel::Info),
            "merge_pages" => self.notify("Refactor: merge pages", NotificationLevel::Info),
            "move_block" => self.notify("Refactor: move block", NotificationLevel::Info),
            "undo_tree" => self.apply_action(Action::OpenUndoTree),
            "new_from_template" => self.apply_action(Action::OpenPicker(PickerKind::Templates)),
            other => self.notify(
                format!("Unhandled leader action: {other}"),
                NotificationLevel::Warning,
            ),
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
    if let Some(cmd) = &frame.command_line {
        render_command_line(f, cmd, area);
    }
    if let Some(qc) = &frame.quick_capture {
        render_quick_capture(f, qc, area);
    }
    if let Some(dp) = &frame.date_picker {
        render_date_picker(f, dp, area);
    }
    if let Some(dialog) = &frame.dialog {
        render_dialog(f, dialog, area);
    }
    if let Some(notif) = &frame.notification {
        render_notification(f, notif, area);
    }
}

fn render_pane(f: &mut ratatui::Frame, pane: &PaneFrame, area: Rect, buffer: &Buffer) {
    match &pane.kind {
        RenderPaneKind::Agenda(agenda) => {
            render_agenda_pane(f, agenda, area);
            return;
        }
        RenderPaneKind::UndoTree(undo) => {
            render_undo_tree_pane(f, undo, area);
            return;
        }
        RenderPaneKind::Timeline(tl) => {
            render_timeline_pane(f, tl, area);
            return;
        }
        RenderPaneKind::SetupWizard(wiz) => {
            render_setup_wizard_pane(f, wiz, area);
            return;
        }
        RenderPaneKind::Editor => {}
    }

    let rope = buffer.text();
    let total_lines = rope.len_lines();

    // Line number gutter width (at least 3 chars + 1 space separator)
    let max_line = pane
        .visible_lines
        .last()
        .map(|l| l.line_number + 1)
        .unwrap_or(total_lines);
    let gutter_width = format!("{}", max_line).len().max(3) + 1;
    let gutter_w = (gutter_width as u16).min(area.width.saturating_sub(4));
    let editor_area = Rect {
        x: area.x + gutter_w,
        y: area.y,
        width: area.width.saturating_sub(gutter_w),
        height: area.height,
    };
    let gutter_area = Rect {
        x: area.x,
        y: area.y,
        width: gutter_w,
        height: area.height,
    };

    // Build gutter lines
    let gutter_style = RStyle::default().fg(Color::Rgb(0xA3, 0xA3, 0xA3));
    let cursor_line_style = RStyle::default().fg(Color::Rgb(0xF4, 0xBF, 0x4F));
    let mut gutter_lines: Vec<Line> = pane
        .visible_lines
        .iter()
        .map(|rl| {
            let num = format!("{:>width$} ", rl.line_number + 1, width = gutter_width - 1);
            let style = if rl.line_number == pane.cursor.line {
                cursor_line_style
            } else {
                gutter_style
            };
            Line::from(Span::styled(num, style))
        })
        .collect();
    while gutter_lines.len() < area.height as usize {
        gutter_lines.push(Line::from(Span::styled(
            " ".repeat(gutter_width),
            gutter_style,
        )));
    }
    let gutter = Paragraph::new(gutter_lines);
    f.render_widget(gutter, gutter_area);

    // Build editor lines
    let lines: Vec<Line> = pane
        .visible_lines
        .iter()
        .map(|rl| {
            if rl.line_number < total_lines {
                let line_text: String = rope.line(rl.line_number).to_string();
                let display = line_text.trim_end_matches('\n').trim_end_matches('\r');
                if rl.spans.is_empty() || display.is_empty() {
                    Line::from(Span::raw(display.to_string()))
                } else {
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
                Line::from(Span::styled("~", RStyle::default().fg(Color::DarkGray)))
            }
        })
        .collect();

    let mut all_lines = lines;
    while all_lines.len() < area.height as usize {
        all_lines.push(Line::from(Span::styled(
            "~",
            RStyle::default().fg(Color::DarkGray),
        )));
    }

    let paragraph = Paragraph::new(all_lines);
    f.render_widget(paragraph, editor_area);

    // Set cursor position (offset by gutter width)
    let cursor_x = area.x + gutter_w + pane.cursor.column as u16;
    let cursor_y = area.y + (pane.cursor.line.saturating_sub(pane.scroll_offset)) as u16;
    if cursor_y < area.bottom() && cursor_x < area.right() {
        f.set_cursor_position((cursor_x, cursor_y));
    }
}

fn render_agenda_pane(f: &mut ratatui::Frame, agenda: &AgendaFrame, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();
    let mut global_idx = 0usize;

    lines.push(Line::from(" Overdue"));
    for item in &agenda.overdue {
        lines.push(render_agenda_item_line(
            item,
            global_idx == agenda.selected_index,
        ));
        global_idx += 1;
    }
    lines.push(Line::from(String::new()));
    lines.push(Line::from(format!(
        " Today · {}",
        Local::now().date_naive().format("%b %-d")
    )));
    for item in &agenda.today {
        lines.push(render_agenda_item_line(
            item,
            global_idx == agenda.selected_index,
        ));
        global_idx += 1;
    }
    lines.push(Line::from(String::new()));
    lines.push(Line::from(" Upcoming"));
    for item in &agenda.upcoming {
        lines.push(render_agenda_item_line(
            item,
            global_idx == agenda.selected_index,
        ));
        global_idx += 1;
    }
    lines.push(Line::from(String::new()));
    lines.push(Line::from(format!(
        " {} open tasks across {} pages",
        agenda.total_open, agenda.total_pages
    )));

    while lines.len() < area.height as usize {
        lines.push(Line::from(String::new()));
    }

    let paragraph = Paragraph::new(lines);
    f.render_widget(paragraph, area);
}

fn render_agenda_item_line(item: &AgendaItem, selected: bool) -> Line<'_> {
    let prefix = if selected { " ▸ ☐ " } else { "   ☐ " };
    let date = item
        .date
        .map(|d| d.format("%b %-d").to_string())
        .unwrap_or_default();
    let tags = if item.tags.is_empty() {
        String::new()
    } else {
        format!(" {}", item.tags.join(" "))
    };
    let text = format!(
        "{prefix}{}  {}{} · {}",
        item.task_text, date, tags, item.source_page
    );
    if selected {
        Line::from(Span::styled(
            text,
            RStyle::default().bg(Color::Rgb(0x37, 0x37, 0x3E)),
        ))
    } else {
        Line::from(text)
    }
}

fn render_undo_tree_pane(f: &mut ratatui::Frame, undo: &UndoTreeFrame, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        " Undo Tree",
        RStyle::default()
            .fg(Color::Rgb(0xF4, 0xBF, 0x4F))
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(String::new()));

    for node in &undo.nodes {
        let indent = "  ".repeat(node.depth);
        let branch_marker = if node.branch > 0 {
            format!("{}├─ ", indent)
        } else {
            format!("{}│  ", indent)
        };
        let marker = if node.is_current { "● " } else { "○ " };
        let sel = node.id == undo.selected;
        let style = if sel {
            RStyle::default().bg(Color::Rgb(0x37, 0x37, 0x3E))
        } else if node.is_current {
            RStyle::default().fg(Color::Rgb(0x62, 0xC5, 0x54))
        } else {
            RStyle::default().fg(Color::Rgb(0xA3, 0xA3, 0xA3))
        };
        let text = format!("{branch_marker}{marker}{}", node.description);
        lines.push(Line::from(Span::styled(text, style)));
    }

    if let Some(preview) = &undo.preview {
        lines.push(Line::from(String::new()));
        lines.push(Line::from(Span::styled(
            "─── Preview ───",
            RStyle::default().fg(Color::Rgb(0xA3, 0xA3, 0xA3)),
        )));
        for l in preview.lines().take(area.height.saturating_sub(lines.len() as u16) as usize) {
            lines.push(Line::from(Span::styled(
                l.to_string(),
                RStyle::default()
                    .fg(Color::Rgb(0xA3, 0xA3, 0xA3))
                    .add_modifier(Modifier::DIM),
            )));
        }
    }

    while lines.len() < area.height as usize {
        lines.push(Line::from(String::new()));
    }

    let paragraph = Paragraph::new(lines);
    f.render_widget(paragraph, area);
}

fn render_timeline_pane(f: &mut ratatui::Frame, tl: &TimelineFrame, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        format!(" Timeline: {}", tl.target_title),
        RStyle::default()
            .fg(Color::Rgb(0xF4, 0xBF, 0x4F))
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(String::new()));

    for (i, entry) in tl.entries.iter().enumerate() {
        let sel = i == tl.selected_index;
        let prefix = if sel { " ▸ " } else { "   " };
        let date_str = entry.date.format("%Y-%m-%d").to_string();
        let header_style = if sel {
            RStyle::default().bg(Color::Rgb(0x37, 0x37, 0x3E))
        } else {
            RStyle::default()
        };
        lines.push(Line::from(vec![
            Span::styled(prefix.to_string(), header_style),
            Span::styled(
                date_str,
                header_style.fg(Color::Rgb(0xF2, 0xDA, 0x61)),
            ),
            Span::styled("  ".to_string(), header_style),
            Span::styled(entry.source_title.clone(), header_style),
        ]));
        if entry.expanded {
            for ctx_line in entry.context.lines() {
                lines.push(Line::from(Span::styled(
                    format!("     {ctx_line}"),
                    RStyle::default()
                        .fg(Color::Rgb(0xA3, 0xA3, 0xA3))
                        .add_modifier(Modifier::DIM),
                )));
            }
        }
    }

    while lines.len() < area.height as usize {
        lines.push(Line::from(String::new()));
    }

    let paragraph = Paragraph::new(lines);
    f.render_widget(paragraph, area);
}

fn render_setup_wizard_pane(f: &mut ratatui::Frame, wiz: &SetupWizardFrame, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(String::new()));
    lines.push(Line::from(Span::styled(
        "  🌱 Welcome to Bloom",
        RStyle::default()
            .fg(Color::Rgb(0xF4, 0xBF, 0x4F))
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(String::new()));

    match wiz.step {
        SetupStep::ChooseVaultLocation => {
            lines.push(Line::from("  Choose a location for your vault:"));
            lines.push(Line::from(String::new()));
            lines.push(Line::from(Span::styled(
                format!("  > {}", wiz.vault_path),
                RStyle::default().fg(Color::Rgb(0x62, 0xC5, 0x54)),
            )));
            lines.push(Line::from(String::new()));
            lines.push(Line::from(Span::styled(
                "  Press Enter to confirm, or type a new path.",
                RStyle::default().fg(Color::Rgb(0xA3, 0xA3, 0xA3)),
            )));
        }
        SetupStep::ImportFromLogseq => {
            lines.push(Line::from("  Import from Logseq?"));
            lines.push(Line::from(String::new()));
            lines.push(Line::from(Span::styled(
                format!("  Vault: {}", wiz.vault_path),
                RStyle::default().fg(Color::Rgb(0xA3, 0xA3, 0xA3)),
            )));
            lines.push(Line::from(String::new()));
            lines.push(Line::from("  [Y] Yes, import   [N] No, start fresh"));
        }
        SetupStep::Complete => {
            lines.push(Line::from(Span::styled(
                "  ✓ Setup complete!",
                RStyle::default().fg(Color::Rgb(0x62, 0xC5, 0x54)),
            )));
            lines.push(Line::from(String::new()));
            lines.push(Line::from(Span::styled(
                format!("  Vault: {}", wiz.vault_path),
                RStyle::default().fg(Color::Rgb(0xA3, 0xA3, 0xA3)),
            )));
            lines.push(Line::from(String::new()));
            lines.push(Line::from("  Press any key to start editing."));
        }
    }

    while lines.len() < area.height as usize {
        lines.push(Line::from(String::new()));
    }

    let paragraph = Paragraph::new(lines);
    f.render_widget(paragraph, area);
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
        "AGENDA" => RStyle::default()
            .bg(Color::Rgb(0xF2, 0xDA, 0x61))
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

    let left = format!(
        " {} \u{2502} {}{}{}{}",
        sb.mode, sb.filename, dirty, pending, recording
    );
    let right = format!("{}:{} ", sb.line + 1, sb.column + 1);

    let width = area.width as usize;
    let padding = width.saturating_sub(left.len() + right.len());
    let status_text = format!("{}{:pad$}{}", left, "", right, pad = padding);

    let status = Paragraph::new(Line::from(Span::styled(status_text, mode_style)));
    f.render_widget(status, area);
}

fn render_picker(f: &mut ratatui::Frame, picker: &PickerFrame, area: Rect) {
    let is_inline = picker.title == "Insert Link";
    let popup_area = if is_inline {
        Rect {
            x: area.x + 4,
            y: area.bottom().saturating_sub(8),
            width: area.width.saturating_sub(8),
            height: 7,
        }
    } else {
        centered_rect(78, 76, area)
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {} ", picker.title))
        .border_style(RStyle::default().fg(Color::Rgb(0xA3, 0xA3, 0xA3)));

    let inner = Rect {
        x: popup_area.x + 1,
        y: popup_area.y + 1,
        width: popup_area.width.saturating_sub(2),
        height: popup_area.height.saturating_sub(2),
    };
    let text_width = inner.width.saturating_sub(2) as usize;

    let query = format!("> {}", picker.query);
    let filters = picker.filters.join(" ");
    let query_line = if filters.is_empty() {
        query
    } else {
        let gap = text_width.saturating_sub(query.len() + filters.len());
        format!("{query}{}{filters}", " ".repeat(gap))
    };

    let result_rows = if is_inline {
        inner.height.saturating_sub(2) as usize
    } else {
        ((inner.height as usize).saturating_sub(6) / 2).max(3)
    };
    let mut lines: Vec<Line> = vec![Line::from(query_line), Line::from(String::new())];

    for i in 0..result_rows {
        if let Some(row) = picker.results.get(i) {
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
            let left = format!("{prefix}{}", row.label);
            let right = row.marginalia.join(" ");
            let row_text = if right.is_empty() {
                left
            } else {
                let gap = text_width.saturating_sub(left.len() + right.len());
                format!("{left}{}{right}", " ".repeat(gap))
            };
            lines.push(Line::from(Span::styled(row_text, style)));
        } else {
            lines.push(Line::from(String::new()));
        }
    }

    if !is_inline {
        lines.push(Line::from(String::new()));
        lines.push(Line::from(format!(
            "{} results (filtered from {} total)",
            picker.filtered_count, picker.total_count
        )));
        lines.push(Line::from("─".repeat(text_width)));
        let preview = picker.preview.as_deref().unwrap_or("No preview available");
        let preview_lines = inner.height.saturating_sub(lines.len() as u16) as usize;
        for l in preview.lines().take(preview_lines.max(1)) {
            lines.push(Line::from(l.to_string()));
        }
    }

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

fn render_command_line(f: &mut ratatui::Frame, cmd: &CommandLineFrame, area: Rect) {
    let popup_area = Rect {
        x: area.x,
        y: area.bottom().saturating_sub(2),
        width: area.width,
        height: 1,
    };
    let paragraph = Paragraph::new(Line::from(Span::raw(cmd.input.clone())));
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

fn render_date_picker(f: &mut ratatui::Frame, dp: &DatePickerFrame, area: Rect) {
    let popup_area = centered_rect(40, 50, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {} ", dp.prompt))
        .border_style(RStyle::default().fg(Color::Rgb(0xA3, 0xA3, 0xA3)));

    let mut lines: Vec<Line> = Vec::new();
    let header = dp.selected_date.format("%B %Y").to_string();
    lines.push(Line::from(Span::styled(
        format!("  {header}"),
        RStyle::default()
            .fg(Color::Rgb(0xF4, 0xBF, 0x4F))
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(Span::styled(
        "  Mo Tu We Th Fr Sa Su",
        RStyle::default().fg(Color::Rgb(0xA3, 0xA3, 0xA3)),
    )));

    let selected_day = dp.selected_date.day();
    for week in &dp.month_view {
        let mut spans: Vec<Span> = vec![Span::raw("  ")];
        for day_opt in week {
            match day_opt {
                Some(day) => {
                    let text = format!("{:>2} ", day);
                    let style = if *day == selected_day {
                        RStyle::default()
                            .bg(Color::Rgb(0xF4, 0xBF, 0x4F))
                            .fg(Color::Rgb(0x14, 0x14, 0x14))
                    } else {
                        RStyle::default()
                    };
                    spans.push(Span::styled(text, style));
                }
                None => spans.push(Span::raw("   ")),
            }
        }
        lines.push(Line::from(spans));
    }

    lines.push(Line::from(String::new()));
    lines.push(Line::from(Span::styled(
        format!("  Selected: {}", dp.selected_date),
        RStyle::default().fg(Color::Rgb(0x62, 0xC5, 0x54)),
    )));

    let paragraph = Paragraph::new(lines).block(block);
    f.render_widget(Clear, popup_area);
    f.render_widget(paragraph, popup_area);
}

fn render_dialog(f: &mut ratatui::Frame, dialog: &DialogFrame, area: Rect) {
    let height = (dialog.choices.len() as u16 + 4).min(area.height).max(5);
    let width = dialog
        .message
        .len()
        .max(dialog.choices.iter().map(|c| c.len()).max().unwrap_or(0) + 6)
        .max(20) as u16
        + 4;
    let width = width.min(area.width.saturating_sub(4));
    let popup_area = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(RStyle::default().fg(Color::Rgb(0xA3, 0xA3, 0xA3)));

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        format!("  {}", dialog.message),
        RStyle::default().add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(String::new()));

    for (i, choice) in dialog.choices.iter().enumerate() {
        let sel = i == dialog.selected;
        let prefix = if sel { "  ▸ " } else { "    " };
        let style = if sel {
            RStyle::default().bg(Color::Rgb(0x37, 0x37, 0x3E))
        } else {
            RStyle::default()
        };
        lines.push(Line::from(Span::styled(
            format!("{prefix}{choice}"),
            style,
        )));
    }

    let paragraph = Paragraph::new(lines).block(block);
    f.render_widget(Clear, popup_area);
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

fn extract_marker_date(line: &str, marker: &str) -> Option<NaiveDate> {
    let token = format!("@{marker}(");
    let start = line.find(&token)? + token.len();
    let rest = &line[start..];
    let end_rel = rest.find(')')?;
    let raw = &rest[..end_rel];
    let value = raw.get(0..10).unwrap_or(raw);
    NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()
}

fn due_date_span(line: &str) -> Option<(usize, usize)> {
    let marker = "@due(";
    let start = line.find(marker)? + marker.len();
    let rest = &line[start..];
    let end_rel = rest.find(')')?;
    Some((start, start + end_rel))
}

fn extract_tags(line: &str) -> Vec<String> {
    line.split_whitespace()
        .filter_map(|tok| {
            let trimmed = tok
                .trim_matches(|c: char| !c.is_alphanumeric() && c != '#' && c != '-' && c != '_');
            if trimmed.starts_with('#') && trimmed.len() > 1 {
                Some(trimmed.to_string())
            } else {
                None
            }
        })
        .collect()
}

fn parse_date_range(input: &str) -> Option<(NaiveDate, NaiveDate)> {
    if let Some((a, b)) = input.split_once("..") {
        let start = NaiveDate::parse_from_str(a.trim(), "%Y-%m-%d").ok()?;
        let end = NaiveDate::parse_from_str(b.trim(), "%Y-%m-%d").ok()?;
        Some((start.min(end), start.max(end)))
    } else {
        let d = NaiveDate::parse_from_str(input.trim(), "%Y-%m-%d").ok()?;
        Some((d, d))
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
