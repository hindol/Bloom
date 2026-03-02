// Vim modal editing engine.
//
// Architecture: EditorState holds buffer + mode + cursor + pending keys.
// `handle_key()` is the single entry point — it consumes a KeyEvent and
// mutates state. Tests use the `assert_keys` harness (see tests module).

use std::path::PathBuf;

use crate::buffer::Buffer;
use crate::document::Frontmatter;
use crate::fts_worker::FtsWorker;
use crate::parser;
use crate::journal::{self, JournalService};
use crate::picker::{
    ActivePicker, BacklinksSource, BufferPickerItem, BufferPickerSource, CommandListSource,
    CommandPickerItem, DrillDownSource, FindPagesSource, FullTextSearchSource, Picker, PickerKind,
    PickerSource, SearchJournalSource, SearchTagsSource, TemplatePickerSource,
    UnlinkedMentionsSource,
};
use crate::template;
use crate::index::SqliteIndex;
use crate::timeline::{TimelineEntry, TimelineService};
use crate::render::{
    CursorShape, CursorState, Diagnostic, DiagnosticKind, PaneFrame, PickerFrame,
    PickerItem as RenderPickerItem, RenderFrame, RenderedLine, StatusBar, Style, StyledSpan,
    Theme, UndoTreeEntry, UndoTreeFrame,
};
use crate::store::{self, LocalFileStore, NoteStore};
use crate::whichkey::{WhichKeyRegistry, WhichKeyState, WhichKeyStep};
use crate::window::{Direction, PaneId, WindowLayout};

use std::collections::HashMap;
use std::collections::HashSet;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Vim editing modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
    Visual,
    Command,
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mode::Normal => write!(f, "NORMAL"),
            Mode::Insert => write!(f, "INSERT"),
            Mode::Visual => write!(f, "VISUAL"),
            Mode::Command => write!(f, "COMMAND"),
        }
    }
}

/// A key event fed into the editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Escape,
    Enter,
    Backspace,
    Tab,
    Left,
    Right,
    Up,
    Down,
    /// Ctrl+<char>
    Ctrl(char),
    /// Alt+Enter (Meta-Return)
    AltEnter,
}

/// Result from handle_key informing the frontend what happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyResult {
    Handled,
    Pending,
    Quit,
    SaveAndQuit,
    Save,
}

/// Actions dispatched from SPC-leader key sequences.
///
/// Most variants are stubs — wiring todos will fill them in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeaderAction {
    // File
    FindFile,
    SaveFile,
    RecentFiles,
    // Buffer
    ListBuffers,
    CloseBuffer,
    NextBuffer,
    PrevBuffer,
    // Search
    SearchProject,
    SearchReplace,
    SearchJournal,
    SearchBacklinks,
    SearchUnlinked,
    // Journal
    JournalToday,
    JournalTomorrow,
    JournalPrev,
    JournalNext,
    JournalAppend,
    JournalTask,
    // Window
    WindowSplitVertical,
    WindowSplitHorizontal,
    WindowClose,
    WindowMoveH,
    WindowMoveJ,
    WindowMoveK,
    WindowMoveL,
    WindowMaximize,
    WindowBalance,
    // Links
    FollowLink,
    BackLinks,
    TimelineView,
    // Tags
    SearchTags,
    AddTag,
    // Agenda
    AgendaView,
    // New
    NewPage,
    NewJournal,
    // Refactor
    Rename,
    // Undo
    UndoTree,
    UndoVisualizer,
    // Insert
    InsertTemplate,
    // Toggles
    TogglePreview,
    ToggleLineNumbers,
    // Help
    HelpKeybindings,
    HelpAbout,
    // Search commands
    SearchCommands,
    // All commands
    AllCommands,
    /// Placeholder for unimplemented leaves.
    Noop,
}

/// Capture bar input mode (journal quick-append).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureBarMode {
    /// Plain text append (`SPC j a`).
    Append,
    /// Task append (`SPC j t`) — prefixes with `- [ ] `.
    Task,
}

/// Per-pane state (buffer, cursor, scroll offset).
#[derive(Debug)]
pub struct PaneState {
    pub buffer: Buffer,
    pub cursor: usize,
    pub scroll_offset: usize,
}

impl Default for PaneState {
    fn default() -> Self {
        Self {
            buffer: Buffer::from_str(""),
            cursor: 0,
            scroll_offset: 0,
        }
    }
}

/// The full editor state — inspectable by tests without rendering.
#[derive(Debug)]
pub struct EditorState {
    pub buffer: Buffer,
    pub mode: Mode,
    /// Byte offset cursor position in the buffer.
    pub cursor: usize,
    /// Pending operator (waiting for motion).
    pub pending_op: Option<char>,
    /// Awaiting text-object identifier after `i`/`a` motion (e.g. `diw`).
    /// Stores (operator, 'i' or 'a', count).
    pub awaiting_text_obj: Option<(char, char, usize)>,
    /// Accumulated count prefix (e.g., "3" in "3dw").
    pub count: Option<usize>,
    /// Command-line input (active when mode == Command).
    pub command_input: String,
    /// Number of visible rows in the viewport.
    pub viewport_height: usize,
    /// First visible line index (0-based, for scrolling).
    pub scroll_offset: usize,
    /// Visual mode anchor (byte offset where visual selection started).
    pub visual_anchor: usize,
    /// Which-key leader registry (SPC-prefix keybindings).
    pub leader_registry: WhichKeyRegistry,
    /// Which-key leader state machine.
    pub leader_state: WhichKeyState,
    /// True when we are inside a SPC-leader sequence.
    pub leader_active: bool,
    /// Last dispatched leader action (useful for tests / wiring).
    pub last_leader_action: Option<LeaderAction>,
    /// Vault root path — enables journal / store operations when set.
    pub vault_root: Option<PathBuf>,
    /// Active capture bar mode (journal quick-append / task).
    pub capture_bar_mode: Option<CaptureBarMode>,
    /// Text being composed in the capture bar.
    pub capture_bar_input: String,
    /// Active picker overlay (find page, search tags, etc.).
    pub active_picker: Option<ActivePicker>,
    /// Window layout for split panes.
    pub window_layout: WindowLayout,
    /// Per-pane state (buffer, cursor, scroll).
    pub pane_states: HashMap<PaneId, PaneState>,
    /// Optional vault index for link resolution.
    pub index: Option<SqliteIndex>,
    /// Active timeline overlay entries.
    pub timeline_entries: Vec<TimelineEntry>,
    /// Selected index in the timeline overlay.
    pub timeline_selected: usize,
    /// True when the timeline overlay is open.
    pub timeline_active: bool,
    /// True when the undo tree visualizer overlay is open.
    pub undo_tree_active: bool,
    /// Selected index in the undo tree visualizer.
    pub undo_tree_selected: usize,
    /// Last buffer-changing key sequence (for `.` repeat).
    pub last_change: Option<Vec<Key>>,
    /// Keys being recorded for the current change.
    recording_keys: Vec<Key>,
    /// Whether we are actively recording a change.
    is_recording: bool,
    /// True while replaying a `.` command (prevents re-recording).
    is_replaying: bool,
    /// Active theme.
    pub theme: Theme,
    /// Background buffers — previously visited files kept in memory.
    pub open_buffers: Vec<Buffer>,
    /// Transient one-line message shown in the command line area.
    pub transient_message: Option<String>,
    /// Last copied string from picker actions.
    pub last_copied_text: Option<String>,
    /// Background FTS search worker (off-UI-thread).
    pub fts_worker: Option<FtsWorker>,
    /// Generation counter for the last FTS results applied.
    fts_applied_generation: u64,
    /// Named registers (`"a`–`"z`, `""` unnamed, `"+` clipboard).
    pub registers: HashMap<char, String>,
    /// Pending register prefix for the next yank/delete/paste.
    pub pending_register: Option<char>,
    /// Macro registers (`qa`…`q`, `@a`).
    pub macro_registers: HashMap<char, Vec<Key>>,
    /// Which macro register is currently being recorded into.
    pub macro_recording: Option<char>,
    /// Last replayed macro register (for `@@`).
    pub last_macro: Option<char>,
    /// Named marks (`ma`, `'a`, `` `a ``).
    pub marks: HashMap<char, usize>,
    /// Active template tab stops for Tab navigation after template expansion.
    pub tab_stops: Vec<template::TabStop>,
    /// Current index into `tab_stops`.
    pub tab_stop_index: usize,
    /// Active agenda view data.
    pub agenda_view: Option<crate::agenda::AgendaView>,
    /// Selected index in the agenda overlay (flat list).
    pub agenda_selected: usize,
    /// True when the agenda overlay is open.
    pub agenda_active: bool,
}

impl EditorState {
    pub fn new(text: &str) -> Self {
        let registry = Self::build_leader_registry();
        Self {
            buffer: Buffer::from_str(text),
            mode: Mode::Normal,
            cursor: 0,
            pending_op: None,
            awaiting_text_obj: None,
            count: None,
            command_input: String::new(),
            viewport_height: 24,
            scroll_offset: 0,
            visual_anchor: 0,
            leader_registry: registry,
            leader_state: WhichKeyState::new(),
            leader_active: false,
            last_leader_action: None,
            vault_root: None,
            capture_bar_mode: None,
            capture_bar_input: String::new(),
            active_picker: None,
            window_layout: WindowLayout::new(),
            pane_states: HashMap::new(),
            index: None,
            timeline_entries: Vec::new(),
            timeline_selected: 0,
            timeline_active: false,
            undo_tree_active: false,
            undo_tree_selected: 0,
            last_change: None,
            recording_keys: Vec::new(),
            is_recording: false,
            is_replaying: false,
            theme: Theme::bloom_default(),
            open_buffers: Vec::new(),
            transient_message: None,
            last_copied_text: None,
            fts_worker: None,
            fts_applied_generation: 0,
            registers: HashMap::new(),
            pending_register: None,
            macro_registers: HashMap::new(),
            macro_recording: None,
            last_macro: None,
            marks: HashMap::new(),
            tab_stops: Vec::new(),
            tab_stop_index: 0,
            agenda_view: None,
            agenda_selected: 0,
            agenda_active: false,
        }
    }

    // ── Leader registry ─────────────────────────────────────────────────

    fn build_leader_registry() -> WhichKeyRegistry {
        let mut r = WhichKeyRegistry::default();
        // Top-level groups
        let groups: &[(&str, &str)] = &[
            ("f", "file"),
            ("b", "buffer"),
            ("s", "search"),
            ("j", "journal"),
            ("w", "window"),
            ("l", "links"),
            ("t", "tags"),
            ("a", "agenda"),
            ("n", "new"),
            ("r", "refactor"),
            ("u", "undo"),
            ("i", "insert"),
            ("T", "toggles"),
            ("h", "help"),
            ("?", "search commands"),
        ];
        for &(key, desc) in groups {
            let _ = r.register_group([key], desc);
        }

        // Leaf commands per group
        let _ = r.register_command(["f", "f"], "file.find", "Find file");
        let _ = r.register_command(["f", "s"], "file.save", "Save file");
        let _ = r.register_command(["f", "r"], "file.recent", "Recent files");

        let _ = r.register_command(["b", "b"], "buffer.list", "List buffers");
        let _ = r.register_command(["b", "d"], "buffer.close", "Close buffer");
        let _ = r.register_command(["b", "n"], "buffer.next", "Next buffer");
        let _ = r.register_command(["b", "p"], "buffer.prev", "Previous buffer");

        let _ = r.register_command(["s", "s"], "search.fulltext", "Search all notes");
        let _ = r.register_command(["s", "p"], "search.project", "Search project");
        let _ = r.register_command(["s", "r"], "search.replace", "Search & replace");
        let _ = r.register_command(["s", "j"], "search.journal", "Search journal");
        let _ = r.register_command(["s", "t"], "tag.search", "Search tags");
        let _ = r.register_command(["s", "l"], "search.backlinks", "Backlinks to page");
        let _ = r.register_command(["s", "u"], "search.unlinked", "Unlinked mentions");

        let _ = r.register_command(["j", "j"], "journal.today", "Today's journal");
        let _ = r.register_command(["j", "m"], "journal.tomorrow", "Tomorrow's journal");
        let _ = r.register_command(["j", "p"], "journal.prev", "Previous journal day");
        let _ = r.register_command(["j", "n"], "journal.next", "Next journal day");
        let _ = r.register_command(["j", "a"], "journal.append", "Quick append to journal");
        let _ = r.register_command(["j", "t"], "journal.task", "Quick add task to journal");
        let _ = r.register_command(["j", "x"], "journal.task", "Quick add task to journal");

        let _ = r.register_command(["w", "v"], "window.split.vertical", "Vertical split");
        let _ = r.register_command(["w", "s"], "window.split.horizontal", "Horizontal split");
        let _ = r.register_command(["w", "d"], "window.close", "Close window");
        let _ = r.register_command(["w", "h"], "window.move.h", "Move left");
        let _ = r.register_command(["w", "j"], "window.move.j", "Move down");
        let _ = r.register_command(["w", "k"], "window.move.k", "Move up");
        let _ = r.register_command(["w", "l"], "window.move.l", "Move right");
        let _ = r.register_command(["w", "m"], "window.maximize", "Maximize toggle");
        let _ = r.register_command(["w", "="], "window.balance", "Balance panes");

        let _ = r.register_command(["l", "f"], "link.follow", "Follow link");
        let _ = r.register_command(["l", "b"], "link.backlinks", "Back-links");
        let _ = r.register_command(["l", "t"], "link.timeline", "Timeline view");

        let _ = r.register_command(["t", "s"], "tag.search", "Search tags");
        let _ = r.register_command(["t", "a"], "tag.add", "Add tag");

        let _ = r.register_command(["a", "v"], "agenda.view", "Agenda view");
        let _ = r.register_command(["a", "a"], "agenda.view", "Agenda view");

        let _ = r.register_command(["n", "p"], "new.page", "New page");
        let _ = r.register_command(["n", "j"], "new.journal", "New journal");

        let _ = r.register_command(["r", "r"], "refactor.rename", "Rename");

        let _ = r.register_command(["u", "t"], "undo.tree", "Undo tree");
        let _ = r.register_command(["u", "u"], "undo.visualizer", "Undo visualizer");

        let _ = r.register_command(["i", "t"], "insert.template", "Insert template");

        let _ = r.register_command(["T", "p"], "toggle.preview", "Toggle preview");
        let _ = r.register_command(["T", "l"], "toggle.line_numbers", "Toggle line numbers");

        let _ = r.register_command(["h", "k"], "help.keybindings", "Keybindings");
        let _ = r.register_command(["h", "a"], "help.about", "About");

        let _ = r.register_command(["?"], "search.commands", "Search commands");

        let _ = r.register_command(["SPC"], "all.commands", "All commands");

        r
    }

    // ── Key dispatch ────────────────────────────────────────────────────

    /// The primary entry point. Consumes a key and mutates state.
    pub fn handle_key(&mut self, key: Key) -> KeyResult {
        self.transient_message = None;
        // Record macro keys (except the `q` that stops recording).
        if self.macro_recording.is_some() {
            if self.mode == Mode::Normal && key == Key::Char('q') && self.pending_op.is_none() && self.awaiting_text_obj.is_none() {
                // Stop recording — handled in handle_normal, don't push `q` into macro.
            } else {
                if let Some(reg) = self.macro_recording {
                    self.macro_registers.entry(reg).or_default().push(key.clone());
                }
            }
        }
        // Undo tree visualizer intercept: if active, route keys to it.
        if self.undo_tree_active {
            return self.handle_undo_tree(key);
        }
        // Timeline intercept: if the timeline overlay is open, route keys to it.
        if self.timeline_active {
            return self.handle_timeline(key);
        }
        // Agenda intercept: if the agenda overlay is open, route keys to it.
        if self.agenda_active {
            return self.handle_agenda(key);
        }
        // Picker intercept: if a picker overlay is open, route keys to it.
        if self.active_picker.is_some() {
            return self.handle_picker(key);
        }
        // Capture-bar intercept: when active, route keys to capture handler.
        if self.capture_bar_mode.is_some() {
            return self.handle_capture_bar(key);
        }
        // Leader-mode intercept: if active, route all keys through leader handler.
        if self.leader_active {
            return self.handle_leader(key);
        }
        match self.mode {
            Mode::Normal => self.handle_normal(key),
            Mode::Insert => self.handle_insert(key),
            Mode::Visual => self.handle_visual(key),
            Mode::Command => self.handle_command(key),
        }
    }

    /// Feed a sequence of keys (convenience for tests).
    pub fn feed_keys(&mut self, keys: &[Key]) {
        for key in keys {
            let result = self.handle_key(key.clone());
            if result == KeyResult::Quit || result == KeyResult::SaveAndQuit {
                break;
            }
        }
    }

    // ── Dot-repeat recording helpers ────────────────────────────────────

    /// Start recording a new change sequence.
    fn dot_record_start(&mut self, key: &Key) {
        if self.is_replaying { return; }
        self.recording_keys.clear();
        self.recording_keys.push(key.clone());
        self.is_recording = true;
    }

    /// Append a key to the current recording.
    fn dot_record_push(&mut self, key: &Key) {
        if self.is_replaying { return; }
        if self.is_recording {
            self.recording_keys.push(key.clone());
        }
    }

    /// Finish recording and save as `last_change`.
    fn dot_record_finish(&mut self) {
        if self.is_replaying { return; }
        if self.is_recording {
            self.last_change = Some(self.recording_keys.clone());
            self.recording_keys.clear();
            self.is_recording = false;
        }
    }

    // ── Register helpers ────────────────────────────────────────────────

    /// Store `text` into the pending register (or unnamed `""`).
    /// Also always writes to the unnamed register.
    fn store_register(&mut self, text: &str) {
        let reg = self.pending_register.take().unwrap_or('"');
        self.registers.insert(reg, text.to_string());
        // Always update unnamed register as well.
        if reg != '"' {
            self.registers.insert('"', text.to_string());
        }
        // Clipboard simulation: if `+` register, update last_copied_text.
        if reg == '+' {
            self.last_copied_text = Some(text.to_string());
        }
    }

    /// Read from the pending register (or unnamed `""`).
    fn read_register(&mut self) -> String {
        let reg = self.pending_register.take().unwrap_or('"');
        if reg == '+' {
            self.last_copied_text.clone().unwrap_or_default()
        } else {
            self.registers.get(&reg).cloned().unwrap_or_default()
        }
    }

    // ── Leader mode ─────────────────────────────────────────────────────

    fn handle_leader(&mut self, key: Key) -> KeyResult {
        // Escape cancels leader sequence.
        if key == Key::Escape {
            self.leader_state.reset();
            self.leader_active = false;
            return KeyResult::Handled;
        }

        let key_str = match &key {
            Key::Char(' ') => "SPC".to_string(),
            Key::Char(c) => c.to_string(),
            _ => {
                // Non-char keys are not valid in leader sequences — reset.
                self.leader_state.reset();
                self.leader_active = false;
                return KeyResult::Handled;
            }
        };

        let step = self.leader_state.advance(&key_str, &self.leader_registry);
        match step {
            WhichKeyStep::Pending(_) => KeyResult::Pending,
            WhichKeyStep::Execute(cmd) => {
                self.leader_active = false;
                self.dispatch_leader(&cmd.id)
            }
            WhichKeyStep::UnknownPrefix { .. } => {
                self.leader_active = false;
                KeyResult::Handled
            }
        }
    }

    fn dispatch_leader(&mut self, command_id: &str) -> KeyResult {
        let action = match command_id {
            "file.find" => LeaderAction::FindFile,
            "file.save" => LeaderAction::SaveFile,
            "file.recent" => LeaderAction::RecentFiles,
            "buffer.list" => LeaderAction::ListBuffers,
            "buffer.close" => LeaderAction::CloseBuffer,
            "buffer.next" => LeaderAction::NextBuffer,
            "buffer.prev" => LeaderAction::PrevBuffer,
            "search.fulltext" => LeaderAction::SearchProject,
            "search.project" => LeaderAction::SearchProject,
            "search.replace" => LeaderAction::SearchReplace,
            "search.journal" => LeaderAction::SearchJournal,
            "search.backlinks" => LeaderAction::SearchBacklinks,
            "search.unlinked" => LeaderAction::SearchUnlinked,
            "journal.today" => LeaderAction::JournalToday,
            "journal.tomorrow" => LeaderAction::JournalTomorrow,
            "journal.prev" => LeaderAction::JournalPrev,
            "journal.next" => LeaderAction::JournalNext,
            "journal.append" => LeaderAction::JournalAppend,
            "journal.task" => LeaderAction::JournalTask,
            "window.split.vertical" => LeaderAction::WindowSplitVertical,
            "window.split.horizontal" => LeaderAction::WindowSplitHorizontal,
            "window.close" => LeaderAction::WindowClose,
            "window.move.h" => LeaderAction::WindowMoveH,
            "window.move.j" => LeaderAction::WindowMoveJ,
            "window.move.k" => LeaderAction::WindowMoveK,
            "window.move.l" => LeaderAction::WindowMoveL,
            "window.maximize" => LeaderAction::WindowMaximize,
            "window.balance" => LeaderAction::WindowBalance,
            "link.follow" => LeaderAction::FollowLink,
            "link.backlinks" => LeaderAction::BackLinks,
            "link.timeline" => LeaderAction::TimelineView,
            "tag.search" => LeaderAction::SearchTags,
            "tag.add" => LeaderAction::AddTag,
            "agenda.view" => LeaderAction::AgendaView,
            "new.page" => LeaderAction::NewPage,
            "new.journal" => LeaderAction::NewJournal,
            "refactor.rename" => LeaderAction::Rename,
            "undo.tree" => LeaderAction::UndoTree,
            "undo.visualizer" => LeaderAction::UndoVisualizer,
            "insert.template" => LeaderAction::InsertTemplate,
            "toggle.preview" => LeaderAction::TogglePreview,
            "toggle.line_numbers" => LeaderAction::ToggleLineNumbers,
            "help.keybindings" => LeaderAction::HelpKeybindings,
            "help.about" => LeaderAction::HelpAbout,
            "search.commands" => LeaderAction::SearchCommands,
            "all.commands" => LeaderAction::AllCommands,
            _ => LeaderAction::Noop,
        };
        self.last_leader_action = Some(action.clone());
        self.execute_leader_action(&action);
        KeyResult::Handled
    }

    fn execute_leader_action(&mut self, action: &LeaderAction) {
        match action {
            LeaderAction::JournalToday => self.open_journal_today(),
            LeaderAction::JournalPrev => self.open_journal_prev(),
            LeaderAction::JournalNext => self.open_journal_next(),
            LeaderAction::JournalAppend => {
                self.capture_bar_mode = Some(CaptureBarMode::Append);
                self.capture_bar_input.clear();
            }
            LeaderAction::JournalTask => {
                self.capture_bar_mode = Some(CaptureBarMode::Task);
                self.capture_bar_input.clear();
            }
            LeaderAction::FindFile => self.open_picker_find_page(),
            LeaderAction::ListBuffers => self.open_picker_buffers(),
            LeaderAction::AllCommands => self.open_picker_all_commands(),
            LeaderAction::WindowSplitVertical => {
                let new_pane = self.window_layout.split_vertical();
                self.save_current_pane_state();
                self.pane_states.insert(new_pane, PaneState::default());
                self.load_pane_state(new_pane);
            }
            LeaderAction::WindowSplitHorizontal => {
                let new_pane = self.window_layout.split_horizontal();
                self.save_current_pane_state();
                self.pane_states.insert(new_pane, PaneState::default());
                self.load_pane_state(new_pane);
            }
            LeaderAction::WindowClose => {
                let old_focused = self.window_layout.focused();
                if self.window_layout.close_focused() {
                    self.pane_states.remove(&old_focused);
                    self.load_pane_state(self.window_layout.focused());
                }
            }
            LeaderAction::WindowMoveH => {
                self.save_current_pane_state();
                self.window_layout.move_focus(Direction::Left);
                self.load_pane_state(self.window_layout.focused());
            }
            LeaderAction::WindowMoveJ => {
                self.save_current_pane_state();
                self.window_layout.move_focus(Direction::Down);
                self.load_pane_state(self.window_layout.focused());
            }
            LeaderAction::WindowMoveK => {
                self.save_current_pane_state();
                self.window_layout.move_focus(Direction::Up);
                self.load_pane_state(self.window_layout.focused());
            }
            LeaderAction::WindowMoveL => {
                self.save_current_pane_state();
                self.window_layout.move_focus(Direction::Right);
                self.load_pane_state(self.window_layout.focused());
            }
            LeaderAction::WindowMaximize => {
                self.window_layout.toggle_maximize_focused();
            }
            LeaderAction::WindowBalance => {
                self.window_layout.balance_panes();
            }
            LeaderAction::UndoVisualizer => {
                self.undo_tree_active = true;
                self.undo_tree_selected = 0;
            }
            LeaderAction::TimelineView => self.open_timeline(),
            LeaderAction::AgendaView => self.open_agenda(),
            LeaderAction::SearchTags => self.open_picker_search_tags(),
            LeaderAction::SearchProject => self.open_picker_fulltext_search(),
            LeaderAction::SearchJournal => self.open_picker_search_journal(),
            LeaderAction::SearchBacklinks => self.open_picker_backlinks(),
            LeaderAction::SearchUnlinked => self.open_picker_unlinked_mentions(),
            LeaderAction::BackLinks => self.open_picker_backlinks(),
            LeaderAction::NewPage => self.open_picker_templates(),
            LeaderAction::SaveFile => {
                // TODO: trigger save via KeyResult when execute_leader_action
                // is refactored to return KeyResult
            }
            _ => {}
        }
    }

    // ── Undo tree visualizer ────────────────────────────────────────────

    fn handle_undo_tree(&mut self, key: Key) -> KeyResult {
        let branches = self.buffer.undo_branches();
        let count = branches.len().max(1);
        match key {
            Key::Escape | Key::Char('q') => {
                self.undo_tree_active = false;
            }
            Key::Char('j') => {
                if self.undo_tree_selected + 1 < count {
                    self.undo_tree_selected += 1;
                }
            }
            Key::Char('k') => {
                if self.undo_tree_selected > 0 {
                    self.undo_tree_selected -= 1;
                }
            }
            Key::Char('h') | Key::Char('l') | Key::Enter => {
                if !branches.is_empty() {
                    self.buffer.switch_branch(self.undo_tree_selected);
                    self.undo_tree_active = false;
                }
            }
            _ => {}
        }
        KeyResult::Handled
    }

    // ── Pane state helpers ────────────────────────────────────────────────

    /// Save current editor buffer/cursor/scroll into the focused pane's state.
    fn save_current_pane_state(&mut self) {
        let focused = self.window_layout.focused();
        let state = self.pane_states.entry(focused).or_default();
        std::mem::swap(&mut state.buffer, &mut self.buffer);
        state.cursor = self.cursor;
        state.scroll_offset = self.scroll_offset;
    }

    /// Load a pane's state into the editor fields.
    fn load_pane_state(&mut self, pane_id: PaneId) {
        if let Some(state) = self.pane_states.get_mut(&pane_id) {
            std::mem::swap(&mut self.buffer, &mut state.buffer);
            self.cursor = state.cursor;
            self.scroll_offset = state.scroll_offset;
        }
    }

    // ── Buffer list management ──────────────────────────────────────────

    /// Save the current buffer to open_buffers (if it has a file path and
    /// isn't already there), then replace it with `new_buffer`.
    fn switch_to_buffer(&mut self, new_buffer: Buffer) {
        self.stash_current_buffer();
        self.buffer = new_buffer;
        self.cursor = 0;
        self.scroll_offset = 0;
    }

    /// Push the current buffer into open_buffers if it has a path and
    /// isn't already stored there.
    fn stash_current_buffer(&mut self) {
        if let Some(ref path) = self.buffer.file_path {
            // Update existing entry or insert new one.
            if let Some(existing) = self.open_buffers.iter_mut().find(|b| {
                b.file_path.as_ref() == Some(path)
            }) {
                // Replace with current state (preserves edits).
                std::mem::swap(existing, &mut self.buffer);
            } else {
                // Move the buffer into the list.
                let mut saved = Buffer::new();
                std::mem::swap(&mut saved, &mut self.buffer);
                self.open_buffers.push(saved);
            }
        }
    }

    /// Take a buffer out of open_buffers by path, or read from disk.
    fn take_or_read_buffer(&mut self, path: &std::path::Path) -> Option<Buffer> {
        // Check open_buffers first (preserves in-memory edits).
        if let Some(idx) = self.open_buffers.iter().position(|b| {
            b.file_path.as_deref() == Some(path)
        }) {
            return Some(self.open_buffers.remove(idx));
        }
        // Fall back to reading from disk.
        let content = std::fs::read_to_string(path).ok()?;
        let mut buf = Buffer::from_str(&content);
        buf.file_path = Some(path.to_path_buf());
        buf.dirty = false;
        Some(buf)
    }

    // ── Picker helpers ──────────────────────────────────────────────────

    /// Open the "Find Page" picker — populated from index if available.
    fn open_picker_find_page(&mut self) {
        let source = if let Some(ref index) = self.index {
            FindPagesSource::from_index(index).unwrap_or_else(|_| FindPagesSource::empty())
        } else {
            FindPagesSource::empty()
        };
        self.active_picker = Some(ActivePicker {
            kind: PickerKind::Default,
            title: "Find Page".into(),
            inner: Box::new(Picker::new(source)),
            inline: false,
            inline_trigger_len: 0,
            is_embed: false,
            filter_pills: Vec::new(),
            typed_query: String::new(),
            preview: None,
            marked: HashSet::new(),
            supports_batch_select: false,
            action_menu_open: false,
            action_menu_selected: 0,
            action_menu_items: crate::picker::FIND_PAGE_ACTION_MENU_ITEMS.iter().map(|s| s.to_string()).collect(),
            drill_down_page_id: None,
            drill_down_page_title: None,
        });
        self.update_picker_preview();
    }

    /// Open the "All Commands" picker — populated from leader registry.
    fn open_picker_all_commands(&mut self) {
        let items: Vec<_> = self
            .leader_registry
            .all_commands_detailed()
            .into_iter()
            .map(|(keys, id, desc)| {
                let category = id
                    .split('.')
                    .next()
                    .unwrap_or_default()
                    .to_string();
                CommandPickerItem {
                    command_id: id.clone(),
                    name: desc.clone(),
                    keybinding: Some(keys),
                    category: category.clone(),
                    description: format!("{desc}\n\nCommand: {id}\nCategory: {category}"),
                }
            })
            .collect();
        let source = CommandListSource::new(items);
        self.active_picker = Some(ActivePicker {
            kind: PickerKind::Default,
            title: "All Commands".into(),
            inner: Box::new(Picker::new(source)),
            inline: false,
            inline_trigger_len: 0,
            is_embed: false,
            filter_pills: Vec::new(),
            typed_query: String::new(),
            preview: None,
            marked: HashSet::new(),
            supports_batch_select: false,
            action_menu_open: false,
            action_menu_selected: 0,
            action_menu_items: crate::picker::DEFAULT_ACTION_MENU_ITEMS.iter().map(|s| s.to_string()).collect(),
            drill_down_page_id: None,
            drill_down_page_title: None,
        });
        self.update_picker_preview();
    }

    /// Open the "Switch Buffer" picker — lists the current buffer + all pane buffers.
    fn open_picker_buffers(&mut self) {
        let mut items = Vec::new();
        let mut seen = HashSet::new();
        let max_preview = 20;

        fn is_journal_path(p: &std::path::Path) -> bool {
            p.components().any(|c: std::path::Component<'_>| c.as_os_str() == "journal")
        }

        fn preview_lines(buf: &Buffer, scroll: usize, max: usize) -> Vec<String> {
            let text = buf.text();
            text.lines().skip(scroll).take(max).map(String::from).collect()
        }

        // Current (focused) buffer — always first (most recently focused).
        if let Some(ref path) = self.buffer.file_path {
            let title = path.file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "[untitled]".into());
            seen.insert(path.clone());
            items.push(BufferPickerItem {
                title,
                path: path.clone(),
                dirty: self.buffer.dirty,
                active: true,
                is_journal: is_journal_path(path),
                preview_lines: preview_lines(&self.buffer, self.scroll_offset, max_preview),
            });
        }

        // Pane state buffers.
        for ps in self.pane_states.values() {
            if let Some(ref path) = ps.buffer.file_path {
                if seen.insert(path.clone()) {
                    let title = path.file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "[untitled]".into());
                    items.push(BufferPickerItem {
                        title,
                        path: path.clone(),
                        dirty: ps.buffer.dirty,
                        active: false,
                        is_journal: is_journal_path(path),
                        preview_lines: preview_lines(&ps.buffer, ps.scroll_offset, max_preview),
                    });
                }
            }
        }

        // Background buffers (previously visited, kept in memory).
        for buf in &self.open_buffers {
            if let Some(ref path) = buf.file_path {
                if seen.insert(path.clone()) {
                    let title = path.file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "[untitled]".into());
                    items.push(BufferPickerItem {
                        title,
                        path: path.clone(),
                        dirty: buf.dirty,
                        active: false,
                        is_journal: is_journal_path(path),
                        preview_lines: preview_lines(buf, 0, max_preview),
                    });
                }
            }
        }

        let source = BufferPickerSource::new(items);
        self.active_picker = Some(ActivePicker {
            kind: PickerKind::Default,
            title: "Switch Buffer".into(),
            inner: Box::new(Picker::new(source)),
            inline: false,
            inline_trigger_len: 0,
            is_embed: false,
            filter_pills: Vec::new(),
            typed_query: String::new(),
            preview: None,
            marked: HashSet::new(),
            supports_batch_select: false,
            action_menu_open: false,
            action_menu_selected: 0,
            action_menu_items: crate::picker::BUFFER_ACTION_MENU_ITEMS.iter().map(|s| s.to_string()).collect(),
            drill_down_page_id: None,
            drill_down_page_title: None,
        });
        self.update_buffer_picker_preview();
    }

    /// Open the "Search Tags" picker — populated from index if available.
    fn open_picker_search_tags(&mut self) {
        if let Some(ref index) = self.index {
            if let Ok(source) = SearchTagsSource::from_index(index) {
                self.active_picker = Some(ActivePicker {
                    kind: PickerKind::SearchTags,
                    title: "Search Tags".into(),
                    inner: Box::new(Picker::new(source)),
                    inline: false,
                    inline_trigger_len: 0,
                    is_embed: false,
                    filter_pills: Vec::new(),
                    typed_query: String::new(),
                    preview: None,
                    marked: HashSet::new(),
                    supports_batch_select: false,
                    action_menu_open: false,
                    action_menu_selected: 0,
                    action_menu_items: crate::picker::TAGS_ACTION_MENU_ITEMS.iter().map(|s| s.to_string()).collect(),
                    drill_down_page_id: None,
                    drill_down_page_title: None,
                });
                self.update_picker_preview();
            }
        }
    }

    /// Open the "Search Journal" picker — populated from index if available.
    fn open_picker_search_journal(&mut self) {
        if let Some(ref index) = self.index {
            if let Ok(source) = SearchJournalSource::from_index(index) {
                self.active_picker = Some(ActivePicker {
                    kind: PickerKind::Default,
                    title: "Search Journal".into(),
                    inner: Box::new(Picker::new(source)),
                    inline: false,
                    inline_trigger_len: 0,
                    is_embed: false,
                    filter_pills: Vec::new(),
                    typed_query: String::new(),
                    preview: None,
                    marked: HashSet::new(),
                    supports_batch_select: false,
                    action_menu_open: false,
                    action_menu_selected: 0,
                    action_menu_items: crate::picker::JOURNAL_ACTION_MENU_ITEMS.iter().map(|s| s.to_string()).collect(),
                    drill_down_page_id: None,
                    drill_down_page_title: None,
                });
                self.update_picker_preview();
            }
        }
    }

    /// Open the "Backlinks" picker for the current page.
    fn open_picker_backlinks(&mut self) {
        if let Some(ref index) = self.index {
            if let Some(page_id) = self.current_page_id() {
                let resolver = crate::resolver::Resolver::new(index);
                if let Ok(source) = BacklinksSource::for_page_id(&resolver, &page_id) {
                    self.active_picker = Some(ActivePicker {
                        kind: PickerKind::Default,
                        title: "Backlinks".into(),
                        inner: Box::new(Picker::new(source)),
                        inline: false,
                        inline_trigger_len: 0,
                        is_embed: false,
                        filter_pills: Vec::new(),
                        typed_query: String::new(),
                        preview: None,
                        marked: HashSet::new(),
                        supports_batch_select: false,
                        action_menu_open: false,
                        action_menu_selected: 0,
                        action_menu_items: crate::picker::BACKLINKS_ACTION_MENU_ITEMS.iter().map(|s| s.to_string()).collect(),
                        drill_down_page_id: None,
                        drill_down_page_title: None,
                    });
                    self.update_picker_preview();
                }
            }
        }
    }

    /// Open the "Full-Text Search" picker — uses index FTS.
    /// Starts with results from index if available, otherwise empty.
    fn open_picker_fulltext_search(&mut self) {
        let source = if let Some(ref index) = self.index {
            FullTextSearchSource::from_index(index, "")
                .unwrap_or_else(|_| FullTextSearchSource::empty())
        } else {
            FullTextSearchSource::empty()
        };
        self.active_picker = Some(ActivePicker {
            kind: PickerKind::Default,
            title: "Search".into(),
            inner: Box::new(Picker::new(source)),
            inline: false,
            inline_trigger_len: 0,
            is_embed: false,
            filter_pills: Vec::new(),
            typed_query: String::new(),
            preview: None,
            marked: HashSet::new(),
            supports_batch_select: false,
            action_menu_open: false,
            action_menu_selected: 0,
            action_menu_items: crate::picker::FTS_ACTION_MENU_ITEMS.iter().map(|s| s.to_string()).collect(),
            drill_down_page_id: None,
            drill_down_page_title: None,
        });
        self.update_picker_preview();
    }

    /// Open the "Unlinked Mentions" picker for the current page.
    fn open_picker_unlinked_mentions(&mut self) {
        if let Some(ref index) = self.index {
            let text = self.buffer.text();
            let title = parser::parse(&text)
                .ok()
                .map(|d| d.frontmatter.title)
                .unwrap_or_default();
            if title.is_empty() {
                return;
            }
            let resolver = crate::resolver::Resolver::new(index);
            if let Ok(source) = UnlinkedMentionsSource::for_page_title(&resolver, &title) {
                self.active_picker = Some(ActivePicker {
                    kind: PickerKind::Default,
                    title: format!("Unlinked Mentions of: {title}"),
                    inner: Box::new(Picker::new(source)),
                    inline: false,
                    inline_trigger_len: 0,
                    is_embed: false,
                    filter_pills: Vec::new(),
                    typed_query: String::new(),
                    preview: None,
                    marked: HashSet::new(),
                    supports_batch_select: true,
                    action_menu_open: false,
                    action_menu_selected: 0,
                    action_menu_items: crate::picker::UNLINKED_ACTION_MENU_ITEMS.iter().map(|s| s.to_string()).collect(),
                    drill_down_page_id: None,
                    drill_down_page_title: None,
                });
                self.update_picker_preview();
            }
        }
    }

    /// Open the "New Page" template picker.
    fn open_picker_templates(&mut self) {
        let source = if let Some(ref root) = self.vault_root {
            TemplatePickerSource::from_vault(root)
        } else {
            TemplatePickerSource::from_vault(std::path::Path::new("."))
        };
        self.active_picker = Some(ActivePicker {
            kind: PickerKind::Default,
            title: "New Page".into(),
            inner: Box::new(Picker::new(source)),
            inline: false,
            inline_trigger_len: 0,
            is_embed: false,
            filter_pills: Vec::new(),
            typed_query: String::new(),
            preview: None,
            marked: HashSet::new(),
            supports_batch_select: false,
            action_menu_open: false,
            action_menu_selected: 0,
            action_menu_items: crate::picker::DEFAULT_ACTION_MENU_ITEMS.iter().map(|s| s.to_string()).collect(),
            drill_down_page_id: None,
            drill_down_page_title: None,
        });
    }

    /// Open a picker with a pre-built source.
    pub fn open_picker<T: Send + Sync + 'static>(&mut self, title: impl Into<String>, source: impl PickerSource<Item = T> + Send + Sync + 'static) {
        self.active_picker = Some(ActivePicker {
            kind: PickerKind::Default,
            title: title.into(),
            inner: Box::new(Picker::new(source)),
            inline: false,
            inline_trigger_len: 0,
            is_embed: false,
            filter_pills: Vec::new(),
            typed_query: String::new(),
            preview: None,
            marked: HashSet::new(),
            supports_batch_select: false,
            action_menu_open: false,
            action_menu_selected: 0,
            action_menu_items: crate::picker::DEFAULT_ACTION_MENU_ITEMS.iter().map(|s| s.to_string()).collect(),
            drill_down_page_id: None,
            drill_down_page_title: None,
        });
    }

    /// Close the active picker.
    pub fn close_picker(&mut self) {
        self.active_picker = None;
    }

    /// Promote marked unlinked mentions to wiki-links.
    /// For each marked mention, reads the source file, replaces the first
    /// occurrence of the page title with `[[page_id|title]]`, and writes back.
    fn batch_promote_unlinked_mentions(&mut self, marked: HashSet<usize>) {
        // Extract current page title and id from the editor's buffer.
        let text = self.buffer.text();
        let doc = match parser::parse(&text) {
            Ok(d) => d,
            Err(_) => return,
        };
        let page_id = doc.frontmatter.id.clone();
        let page_title = doc.frontmatter.title.clone();
        if page_id.is_empty() || page_title.is_empty() {
            return;
        }

        // Get promote data from the picker before we drop it.
        let promote_items: Vec<(std::path::PathBuf, String)> = self
            .active_picker
            .as_ref()
            .map(|ap| ap.inner.items_for_batch_promote(&marked))
            .unwrap_or_default();

        let vault_root = match self.vault_root.clone() {
            Some(r) => r,
            None => return,
        };
        let store = match LocalFileStore::new(vault_root) {
            Ok(s) => s,
            Err(_) => return,
        };

        let link = format!("[[{page_id}|{page_title}]]");
        for (source_path, _source_page_id) in &promote_items {
            // source_path is relative (e.g. "pages/Source-A.md"). Resolve to absolute.
            let abs_path = store.root().join(source_path);
            let content = match store.read(&abs_path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            // Case-insensitive find-and-replace of the first occurrence of the title.
            let lower_content = content.to_lowercase();
            let lower_title = page_title.to_lowercase();
            if let Some(pos) = lower_content.find(&lower_title) {
                let promoted = format!(
                    "{}{}{}",
                    &content[..pos],
                    &link,
                    &content[pos + page_title.len()..]
                );
                let _ = store.write(&abs_path, &promoted);

                // Re-index the modified file.
                if let Some(ref mut index) = self.index {
                    if let Ok(doc) = parser::parse(&promoted) {
                        let _ = index.index_document(source_path, &doc);
                    }
                }
            }
        }
    }

    /// Check whether the last two characters form `[[` (or `![[`) and open
    /// an inline link/embed picker if so.
    fn maybe_open_inline_picker(&mut self) {
        let text = self.buffer.text();
        // Cursor is *after* the second `[`.
        if self.cursor < 2 {
            return;
        }
        let bytes = text.as_bytes();
        if bytes[self.cursor - 1] != b'[' || bytes[self.cursor - 2] != b'[' {
            return;
        }
        let is_embed = self.cursor >= 3 && bytes[self.cursor - 3] == b'!';
        let trigger_len = if is_embed { 3 } else { 2 };
        let title = if is_embed { "Embed Page" } else { "Insert Link" };
        let source = if let Some(ref index) = self.index {
            FindPagesSource::from_index(index).unwrap_or_else(|_| FindPagesSource::empty())
        } else {
            FindPagesSource::empty()
        };
        self.active_picker = Some(ActivePicker {
            kind: PickerKind::Default,
            title: title.into(),
            inner: Box::new(Picker::new(source)),
            inline: true,
            inline_trigger_len: trigger_len,
            is_embed,
            filter_pills: Vec::new(),
            typed_query: String::new(),
            preview: None,
            marked: HashSet::new(),
            supports_batch_select: false,
            action_menu_open: false,
            action_menu_selected: 0,
            action_menu_items: crate::picker::DEFAULT_ACTION_MENU_ITEMS.iter().map(|s| s.to_string()).collect(),
            drill_down_page_id: None,
            drill_down_page_title: None,
        });
        self.update_picker_preview();
    }

    // ── Picker Enter dispatch ─────────────────────────────────────────

    /// Handle Enter on a non-inline picker: open selected page, execute
    /// command, or transition (search tags two-step flow).
    fn handle_picker_enter(&mut self) {
        let (kind, title, selected_value) = match self.active_picker.as_ref() {
            Some(ap) => (ap.kind, ap.title.clone(), ap.inner.selected_value()),
            None => return,
        };

        // Two-step tag picker: Enter transitions to filtered Find Page.
        if kind == PickerKind::SearchTags {
            if let Some((_id, tag_name)) = selected_value {
                self.active_picker = None;
                self.open_picker_find_page();
                if let Some(ref mut ap) = self.active_picker {
                    ap.filter_pills.push(("tag".into(), tag_name));
                    ap.sync_query();
                }
            } else {
                self.active_picker = None;
            }
            return;
        }

        // All Commands picker: execute the selected command.
        if title == "All Commands" {
            self.active_picker = None;
            if let Some((command_id, _)) = selected_value {
                let _ = self.dispatch_leader(&command_id);
            }
            return;
        }

        // New Page (template) picker: expand selected template into a new page.
        if title == "New Page" {
            if let Some((path_str, name)) = selected_value {
                self.active_picker = None;
                let tmpl_path = std::path::Path::new(&path_str);
                if let Ok(raw) = std::fs::read_to_string(tmpl_path) {
                    let parsed = template::parse(&name, &raw);
                    let expanded = template::expand(&parsed);

                    if let Some(ref root) = self.vault_root.clone() {
                        let pages_dir = root.join("pages");
                        let _ = std::fs::create_dir_all(&pages_dir);
                        let filename = format!("{}.md", store::sanitize_filename(&name));
                        let page_path = pages_dir.join(&filename);
                        let _ = std::fs::write(&page_path, &expanded.content);

                        let mut buf = Buffer::from_str(&expanded.content);
                        buf.file_path = Some(page_path.clone());
                        buf.dirty = false;
                        self.switch_to_buffer(buf);

                        // Re-index the new page.
                        if let Some(ref mut index) = self.index {
                            let rel_path = std::path::Path::new("pages").join(&filename);
                            if let Ok(doc) = parser::parse(&expanded.content) {
                                let _ = index.index_document(&rel_path, &doc);
                            }
                        }
                    } else {
                        let mut buf = Buffer::from_str(&expanded.content);
                        buf.dirty = true;
                        self.switch_to_buffer(buf);
                    }

                    // Store tab stops for Tab navigation.
                    self.tab_stops = expanded.tab_stops;
                    self.tab_stop_index = 0;
                    // Position cursor at the first tab stop if available.
                    if let Some(ts) = self.tab_stops.first() {
                        self.cursor = ts.start;
                        self.mode = Mode::Insert;
                    }
                }
            } else {
                self.active_picker = None;
            }
            return;
        }

        // Switch Buffer picker: open by file path.
        if title == "Switch Buffer" {
            if let Some((path_str, _title)) = selected_value {
                let path = PathBuf::from(&path_str);
                if let Some(buf) = self.take_or_read_buffer(&path) {
                    self.switch_to_buffer(buf);
                }
            }
            self.active_picker = None;
            return;
        }

        // All other pickers: selected_value is (page_id, title).
        // Look up the file path from the index and open it.
        if let Some((page_id, _display)) = selected_value {
            self.open_page_by_id(&page_id);
        }
        self.active_picker = None;
    }

    fn selected_picker_action(&self) -> Option<String> {
        let ap = self.active_picker.as_ref()?;
        ap.action_menu_items.get(ap.action_menu_selected).cloned()
    }

    fn open_selection_in_split(&mut self, title: &str, selected_value: Option<(String, String)>) {
        self.save_current_pane_state();
        let new_pane = self.window_layout.split_vertical();
        self.pane_states.insert(new_pane, PaneState::default());
        self.load_pane_state(new_pane);

        if title == "Switch Buffer" {
            if let Some((path_str, _)) = selected_value {
                let path = PathBuf::from(path_str);
                if let Some(buf) = self.take_or_read_buffer(&path) {
                    self.switch_to_buffer(buf);
                }
            }
            return;
        }

        if let Some((page_id, _)) = selected_value {
            self.open_page_by_id(&page_id);
        }
    }

    fn close_buffer_path(&mut self, path: &std::path::Path) {
        if self.buffer.file_path.as_deref() == Some(path) {
            if let Some(next) = self.open_buffers.pop() {
                self.switch_to_buffer(next);
            } else {
                self.buffer = Buffer::from_str("");
                self.cursor = 0;
                self.scroll_offset = 0;
            }
            return;
        }
        self.open_buffers
            .retain(|buf| buf.file_path.as_deref() != Some(path));
        for pane in self.pane_states.values_mut() {
            if pane.buffer.file_path.as_deref() == Some(path) {
                pane.buffer = Buffer::from_str("");
                pane.cursor = 0;
                pane.scroll_offset = 0;
            }
        }
    }

    fn save_buffer_path(&mut self, path: &std::path::Path) -> bool {
        if self.buffer.file_path.as_deref() == Some(path) {
            if std::fs::write(path, self.buffer.text()).is_ok() {
                self.buffer.dirty = false;
                return true;
            }
            return false;
        }
        if let Some(buf) = self
            .open_buffers
            .iter_mut()
            .find(|buf| buf.file_path.as_deref() == Some(path))
        {
            if std::fs::write(path, buf.text()).is_ok() {
                buf.dirty = false;
                return true;
            }
            return false;
        }
        if let Some((_pane_id, pane)) = self
            .pane_states
            .iter_mut()
            .find(|(_id, pane)| pane.buffer.file_path.as_deref() == Some(path))
        {
            if std::fs::write(path, pane.buffer.text()).is_ok() {
                pane.buffer.dirty = false;
                return true;
            }
            return false;
        }
        false
    }

    fn execute_picker_action(&mut self) -> KeyResult {
        let (kind, title, selected_value, action) = match self.active_picker.as_ref() {
            Some(ap) => (
                ap.kind,
                ap.title.clone(),
                ap.inner.selected_value(),
                self.selected_picker_action(),
            ),
            None => return KeyResult::Handled,
        };
        let Some(action) = action else {
            return KeyResult::Handled;
        };

        match action.as_str() {
            "Open" | "Open at line" => {
                self.handle_picker_enter();
            }
            "Promote to link" => {
                // Single-item promote: reuse batch promote with just the selected item.
                if let Some(ref ap) = self.active_picker {
                    if let Some(sel_idx) = ap.inner.selected_index() {
                        let results = ap.inner.results();
                        if let Some(m) = results.get(sel_idx) {
                            let mut marked = HashSet::new();
                            marked.insert(m.source_index);
                            self.batch_promote_unlinked_mentions(marked);
                        }
                    }
                }
                self.active_picker = None;
            }
            "Open in split" => {
                if kind == PickerKind::SearchTags || title == "All Commands" {
                    self.handle_picker_enter();
                    return KeyResult::Handled;
                }
                self.active_picker = None;
                self.open_selection_in_split(&title, selected_value);
            }
            "Copy link" => {
                if let Some((page_id, display)) = selected_value {
                    let link = format!("[[{page_id}|{display}]]");
                    self.last_copied_text = Some(link.clone());
                    self.transient_message = Some(format!("Copied {link}"));
                }
                if let Some(ap) = self.active_picker.as_mut() {
                    ap.action_menu_open = false;
                }
            }
            "Copy page ID" => {
                if let Some((page_id, _)) = selected_value {
                    self.last_copied_text = Some(page_id.clone());
                    self.transient_message = Some(format!("Copied {page_id}"));
                }
                if let Some(ap) = self.active_picker.as_mut() {
                    ap.action_menu_open = false;
                }
            }
            "Close buffer" => {
                if let Some((path_str, _)) = selected_value {
                    self.close_buffer_path(std::path::Path::new(&path_str));
                    self.open_picker_buffers();
                    if let Some(ap) = self.active_picker.as_mut() {
                        ap.action_menu_open = false;
                    }
                }
            }
            "Save" => {
                if let Some((path_str, _)) = selected_value {
                    let ok = self.save_buffer_path(std::path::Path::new(&path_str));
                    self.transient_message = Some(if ok {
                        "Saved buffer".into()
                    } else {
                        "Failed to save buffer".into()
                    });
                    self.open_picker_buffers();
                    if let Some(ap) = self.active_picker.as_mut() {
                        ap.action_menu_open = false;
                    }
                }
            }
            _ => {
                if let Some(ap) = self.active_picker.as_mut() {
                    ap.action_menu_open = false;
                }
            }
        }
        KeyResult::Handled
    }

    /// Load a page into the editor buffer by looking up its path from the index.
    fn open_page_by_id(&mut self, page_id: &str) {
        let path = self
            .index
            .as_ref()
            .and_then(|idx| idx.page_for_id(page_id).ok().flatten())
            .map(|p| p.path);

        let Some(path) = path else { return };

        // If we have a vault root, resolve relative paths.
        let abs_path = if path.is_absolute() {
            path
        } else if let Some(ref root) = self.vault_root {
            root.join(&path)
        } else {
            path
        };

        if let Some(buf) = self.take_or_read_buffer(&abs_path) {
            self.switch_to_buffer(buf);
        }
    }

    fn refresh_fulltext_search_source(&mut self) {
        let query = match self.active_picker.as_ref() {
            Some(ap) if ap.title == "Search" => {
                let mut parts: Vec<String> = ap
                    .filter_pills
                    .iter()
                    .map(|(_, label)| label.clone())
                    .collect();
                if !ap.typed_query.is_empty() {
                    parts.push(ap.typed_query.clone());
                }
                parts.join(" ")
            }
            _ => return,
        };

        // Prefer the background FTS worker (non-blocking).
        if let Some(ref worker) = self.fts_worker {
            worker.search(&query);
            return;
        }

        // Fallback: synchronous search (no worker available, e.g. in tests).
        let source = if let Some(ref index) = self.index {
            FullTextSearchSource::from_index(index, &query)
                .unwrap_or_else(|_| FullTextSearchSource::empty())
        } else {
            FullTextSearchSource::empty()
        };
        if let Some(ap) = self.active_picker.as_mut() {
            if ap.title == "Search" {
                ap.inner = Box::new(Picker::new(source));
                ap.sync_query();
            }
        }
    }

    /// Initialize the FTS background worker from the current index.
    /// Called once after index is set up.
    pub fn init_fts_worker(&mut self) {
        if let Some(ref index) = self.index {
            if let Ok(worker) = FtsWorker::spawn(index.db_path()) {
                self.fts_worker = Some(worker);
            }
        }
    }

    /// Poll the FTS worker for completed results and apply them to the
    /// active Search picker. Call this from the event loop (non-blocking).
    pub fn poll_fts_results(&mut self) {
        let result = match self.fts_worker.as_ref() {
            Some(worker) => worker.rx.try_recv().ok(),
            None => return,
        };
        let Some(result) = result else { return };

        // Discard stale results.
        if result.generation <= self.fts_applied_generation {
            return;
        }
        self.fts_applied_generation = result.generation;

        if let Some(ref mut ap) = self.active_picker {
            if ap.title == "Search" {
                ap.inner = Box::new(Picker::new(result.source));
                ap.sync_query();
                // Refresh preview for the new results.
            }
        }
        self.update_picker_preview();
    }

    // ── Picker preview ──────────────────────────────────────────────────

    /// Generate preview content for the currently selected picker item.
    fn update_picker_preview(&mut self) {
        // Buffer picker has its own preview source (in-memory buffer content).
        if matches!(self.active_picker.as_ref(), Some(ap) if ap.title == "Switch Buffer") {
            self.update_buffer_picker_preview();
            return;
        }

        let (preview_lines, page_id, inline) = match self.active_picker.as_ref() {
            Some(ap) => (
                ap.inner.selected_preview_lines(),
                ap.inner.selected_value().map(|(pid, _)| pid),
                ap.inline,
            ),
            None => return,
        };

        // Skip preview for inline pickers (too small).
        if inline {
            if let Some(ap) = self.active_picker.as_mut() {
                ap.preview = None;
            }
            return;
        }

        if let Some(lines) = preview_lines {
            let mut hl_ctx = crate::highlight::HighlightContext::new();
            let rendered = lines
                .iter()
                .enumerate()
                .map(|(i, line)| RenderedLine {
                    text: line.to_string(),
                    spans: crate::highlight::highlight_line(line, &mut hl_ctx),
                    line_number: Some(i),
                })
                .collect();
            if let Some(ap) = self.active_picker.as_mut() {
                ap.preview = Some(rendered);
            }
            return;
        }

        let Some(page_id) = page_id else {
            if let Some(ap) = self.active_picker.as_mut() {
                ap.preview = None;
            }
            return;
        };

        let content = self
            .index
            .as_ref()
            .and_then(|idx| idx.content_for_page_id(&page_id).ok().flatten());

        let preview = content.map(|text| {
            let max_lines = 20;
            let mut hl_ctx = crate::highlight::HighlightContext::new();
            text.lines()
                .take(max_lines)
                .enumerate()
                .map(|(i, line)| {
                    let spans = crate::highlight::highlight_line(line, &mut hl_ctx);
                    RenderedLine {
                        text: line.to_string(),
                        spans,
                        line_number: Some(i),
                    }
                })
                .collect()
        });

        if let Some(ap) = self.active_picker.as_mut() {
            ap.preview = preview;
        }
    }

    /// Generate preview for the Switch Buffer picker from embedded buffer content.
    fn update_buffer_picker_preview(&mut self) {
        let lines = match self.active_picker.as_ref() {
            Some(ap) if ap.title == "Switch Buffer" => ap.inner.selected_preview_lines(),
            _ => return,
        };

        let preview = lines.map(|text_lines| {
            let mut hl_ctx = crate::highlight::HighlightContext::new();
            text_lines
                .iter()
                .enumerate()
                .map(|(i, line)| {
                    let spans = crate::highlight::highlight_line(line, &mut hl_ctx);
                    RenderedLine {
                        text: line.to_string(),
                        spans,
                        line_number: Some(i),
                    }
                })
                .collect()
        });

        if let Some(ap) = self.active_picker.as_mut() {
            ap.preview = preview;
        }
    }

    // ── Create page from picker ─────────────────────────────────────────

    /// Create a new page from the current picker query text.
    /// Returns `(page_id, title)` on success.
    fn create_page_from_query(&mut self, title: &str) -> Option<(String, String)> {
        let title = title.trim();
        if title.is_empty() {
            return None;
        }
        let fm = Frontmatter::new(title);
        let page_id = fm.id.clone();
        let content = format!(
            "---\nid: {}\ntitle: \"{}\"\ntags: []\n---\n\n",
            fm.id, fm.title,
        );

        // Write file to disk if vault_root is available.
        if let Some(ref root) = self.vault_root {
            if let Ok(s) = LocalFileStore::new(root.clone()) {
                let filename = format!("{}.md", store::sanitize_filename(title));
                let path = s.pages_dir().join(&filename);
                let _ = s.write(&path, &content);

                // Re-index the new page.
                if let Some(ref mut index) = self.index {
                    let rel_path = std::path::Path::new("pages").join(&filename);
                    if let Ok(doc) = parser::parse(&content) {
                        let _ = index.index_document(&rel_path, &doc);
                    }
                }
            }
        }

        Some((page_id, title.to_string()))
    }

    // ── Picker key handling ─────────────────────────────────────────────

    fn handle_picker(&mut self, key: Key) -> KeyResult {
        // When the action menu is open, intercept keys for menu navigation.
        if let Some(ref mut ap) = self.active_picker {
            if ap.action_menu_open {
                match key {
                    Key::Escape => {
                        ap.action_menu_open = false;
                        return KeyResult::Handled;
                    }
                    Key::Char('j') | Key::Down => {
                        let len = ap.action_menu_items.len();
                        if len > 0 {
                            ap.action_menu_selected = (ap.action_menu_selected + 1) % len;
                        }
                        return KeyResult::Handled;
                    }
                    Key::Char('k') | Key::Up => {
                        let len = ap.action_menu_items.len();
                        if len > 0 {
                            ap.action_menu_selected = if ap.action_menu_selected == 0 {
                                len - 1
                            } else {
                                ap.action_menu_selected - 1
                            };
                        }
                        return KeyResult::Handled;
                    }
                    Key::Enter => {
                        return self.execute_picker_action();
                    }
                    _ => return KeyResult::Handled,
                }
            }
        }

        match key {
            Key::Escape => {
                self.active_picker = None;
                KeyResult::Handled
            }
            Key::Tab => {
                if let Some(ref mut ap) = self.active_picker {
                    if ap.supports_batch_select {
                        // Toggle mark on the selected item (batch select).
                        if let Some(sel_idx) = ap.inner.selected_index() {
                            let results = ap.inner.results();
                            if let Some(m) = results.get(sel_idx) {
                                let source_index = m.source_index;
                                if !ap.marked.remove(&source_index) {
                                    ap.marked.insert(source_index);
                                }
                            }
                        }
                        ap.inner.move_down();
                    } else {
                        // Toggle action menu.
                        ap.action_menu_open = !ap.action_menu_open;
                        ap.action_menu_selected = 0;
                    }
                }
                KeyResult::Handled
            }
            Key::Enter => {
                // Batch promote: if marks exist on a batch-select picker, promote all.
                if let Some(ref ap) = self.active_picker {
                    if ap.supports_batch_select && !ap.marked.is_empty() {
                        // Collect marked source indices and gather promote data.
                        let marked = ap.marked.clone();
                        let results = ap.inner.results();
                        let promote_data: Vec<(usize, String)> = results
                            .iter()
                            .filter(|m| marked.contains(&m.source_index))
                            .map(|m| (m.source_index, m.text.clone()))
                            .collect();
                        // We need the DynPicker to provide items for promote.
                        // Extract the items via the inner picker's results to get source_index.
                        // The actual promote uses source items from the picker source.
                        // We must collect all info before dropping the borrow.
                        drop(promote_data);
                        self.batch_promote_unlinked_mentions(marked);
                        self.active_picker = None;
                        return KeyResult::Handled;
                    }
                }
                // For inline pickers, insert formatted link and remove trigger chars.
                if let Some(ref ap) = self.active_picker {
                    if ap.inline {
                        let trigger_len = ap.inline_trigger_len;
                        let is_embed = ap.is_embed;
                        let in_drilldown = ap.drill_down_page_id.is_some();
                        let in_capture_bar = self.capture_bar_mode.is_some();
                        if let Some((selected_id, selected_text)) = ap.inner.selected_value() {
                            if in_capture_bar {
                                let link = format!("[[{selected_id}|{selected_text}]]");
                                let del_start = self.capture_bar_input.len().saturating_sub(trigger_len);
                                self.capture_bar_input.truncate(del_start);
                                self.capture_bar_input.push_str(&link);
                                self.active_picker = None;
                                return KeyResult::Handled;
                            }
                            if is_embed && !in_drilldown {
                                // Enter drill-down mode: show sections/blocks of the selected page.
                                let page_id = selected_id.clone();
                                let page_title = selected_text.clone();
                                let content = self
                                    .index
                                    .as_ref()
                                    .and_then(|idx| idx.content_for_page_id(&page_id).ok().flatten())
                                    .unwrap_or_default();
                                let source = DrillDownSource::from_content(&page_id, &page_title, &content);
                                if let Some(ref mut ap) = self.active_picker {
                                    ap.title = format!("Embed: {page_title}");
                                    ap.inner = Box::new(Picker::new(source));
                                    ap.drill_down_page_id = Some(page_id);
                                    ap.drill_down_page_title = Some(page_title);
                                    ap.typed_query.clear();
                                    ap.filter_pills.clear();
                                }
                                return KeyResult::Handled;
                            } else if is_embed && in_drilldown {
                                // Insert embed with optional sub-id.
                                let dd_page_id = ap.drill_down_page_id.clone().unwrap_or_default();
                                let dd_page_title = ap.drill_down_page_title.clone().unwrap_or_default();
                                let link = if selected_id.is_empty() {
                                    // Whole page embed.
                                    format!("![[{dd_page_id}|{dd_page_title}]]")
                                } else {
                                    format!("![[{dd_page_id}#{selected_id}|{selected_text}]]")
                                };
                                let del_start = self.cursor.saturating_sub(trigger_len);
                                self.buffer.delete(del_start..self.cursor);
                                self.cursor = del_start;
                                self.buffer.insert(self.cursor, &link);
                                self.cursor += link.len();
                            } else {
                                // Regular link insert.
                                let link = format!("[[{selected_id}|{selected_text}]]");
                                let del_start = self.cursor.saturating_sub(trigger_len);
                                self.buffer.delete(del_start..self.cursor);
                                self.cursor = del_start;
                                self.buffer.insert(self.cursor, &link);
                                self.cursor += link.len();
                            }
                        }
                        self.active_picker = None;
                        return KeyResult::Handled;
                    }
                }
                // Non-inline picker Enter: open selected page or execute action.
                self.handle_picker_enter();
                KeyResult::Handled
            }
            Key::AltEnter => {
                // Create a new page from the picker query text.
                let (query, inline, trigger_len, is_embed, title) = match self.active_picker.as_ref() {
                    Some(ap) => {
                        let t = &ap.title;
                        let is_create_picker = t == "Find Page"
                            || t == "Insert Link"
                            || t == "Embed Page";
                        if !is_create_picker {
                            self.active_picker = None;
                            return KeyResult::Handled;
                        }
                        (
                            ap.typed_query.clone(),
                            ap.inline,
                            ap.inline_trigger_len,
                            ap.is_embed,
                            t.clone(),
                        )
                    }
                    None => return KeyResult::Handled,
                };

                if let Some((page_id, page_title)) = self.create_page_from_query(&query) {
                    if inline {
                        let link = if is_embed {
                            format!("![[{page_id}|{page_title}]]")
                        } else {
                            format!("[[{page_id}|{page_title}]]")
                        };
                        let del_start = self.cursor.saturating_sub(trigger_len);
                        self.buffer.delete(del_start..self.cursor);
                        self.cursor = del_start;
                        self.buffer.insert(self.cursor, &link);
                        self.cursor += link.len();
                    } else if title == "Find Page" {
                        // Open the new page in the editor buffer.
                        if let Some(ref root) = self.vault_root {
                            let filename = format!("{}.md", store::sanitize_filename(&page_title));
                            let path = root.join("pages").join(&filename);
                            if let Some(buf) = self.take_or_read_buffer(&path) {
                                self.switch_to_buffer(buf);
                            }
                        }
                    }
                }
                self.active_picker = None;
                KeyResult::Handled
            }
            Key::Ctrl('j') | Key::Ctrl('n') | Key::Down => {
                if let Some(ref mut picker) = self.active_picker {
                    picker.inner.move_down();
                }
                self.update_picker_preview();
                KeyResult::Handled
            }
            Key::Ctrl('k') | Key::Ctrl('p') | Key::Up => {
                if let Some(ref mut picker) = self.active_picker {
                    picker.inner.move_up();
                }
                self.update_picker_preview();
                KeyResult::Handled
            }
            Key::Ctrl('t') => {
                if let Some(ref mut ap) = self.active_picker {
                    ap.extract_filter("tag");
                }
                self.refresh_fulltext_search_source();
                self.update_picker_preview();
                KeyResult::Handled
            }
            Key::Backspace | Key::Left => {
                // In drill-down mode with empty query, go back to page picker.
                if let Some(ref ap) = self.active_picker {
                    if ap.drill_down_page_id.is_some()
                        && ap.typed_query.is_empty()
                        && ap.filter_pills.is_empty()
                    {
                        let is_embed = ap.is_embed;
                        let source = if let Some(ref index) = self.index {
                            FindPagesSource::from_index(index)
                                .unwrap_or_else(|_| FindPagesSource::empty())
                        } else {
                            FindPagesSource::empty()
                        };
                        if let Some(ref mut ap) = self.active_picker {
                            ap.title = if is_embed { "Embed Page" } else { "Insert Link" }.into();
                            ap.inner = Box::new(Picker::new(source));
                            ap.drill_down_page_id = None;
                            ap.drill_down_page_title = None;
                            ap.typed_query.clear();
                            ap.filter_pills.clear();
                        }
                        return KeyResult::Handled;
                    }
                }
                // For Left arrow without drill-down context, ignore (don't delete chars).
                if key == Key::Left {
                    return KeyResult::Handled;
                }
                if let Some(ref mut ap) = self.active_picker {
                    ap.pop_char();
                }
                self.refresh_fulltext_search_source();
                self.update_picker_preview();
                KeyResult::Handled
            }
            Key::Char(c) => {
                if let Some(ref mut ap) = self.active_picker {
                    ap.push_char(c);
                }
                self.refresh_fulltext_search_source();
                self.update_picker_preview();
                KeyResult::Handled
            }
            _ => KeyResult::Handled,
        }
    }

    // ── Timeline helpers ────────────────────────────────────────────────

    /// Parse the current buffer's frontmatter to extract the page ID.
    fn current_page_id(&self) -> Option<String> {
        let text = self.buffer.text();
        let doc = parser::parse(&text).ok()?;
        let id = doc.frontmatter.id;
        if id.is_empty() { None } else { Some(id) }
    }

    /// Open the timeline overlay for the current page.
    fn open_timeline(&mut self) {
        let page_id = self.current_page_id();
        let entries = match (&self.index, page_id) {
            (Some(index), Some(pid)) => {
                let svc = TimelineService::new(index);
                svc.entries_for_page_id(&pid).unwrap_or_default()
            }
            _ => Vec::new(),
        };
        self.timeline_selected = 0;
        self.timeline_entries = entries;
        self.timeline_active = true;
    }

    /// Close the timeline overlay.
    fn close_timeline(&mut self) {
        self.timeline_active = false;
        self.timeline_entries.clear();
        self.timeline_selected = 0;
    }

    fn handle_timeline(&mut self, key: Key) -> KeyResult {
        match key {
            Key::Escape => {
                self.close_timeline();
                KeyResult::Handled
            }
            Key::Enter => {
                // Stub: just close for now (jump-to-source is a future todo).
                self.close_timeline();
                KeyResult::Handled
            }
            Key::Char('j') | Key::Down => {
                if !self.timeline_entries.is_empty() {
                    self.timeline_selected =
                        (self.timeline_selected + 1) % self.timeline_entries.len();
                }
                KeyResult::Handled
            }
            Key::Char('k') | Key::Up => {
                if !self.timeline_entries.is_empty() {
                    let len = self.timeline_entries.len();
                    self.timeline_selected = if self.timeline_selected == 0 {
                        len - 1
                    } else {
                        self.timeline_selected - 1
                    };
                }
                KeyResult::Handled
            }
            _ => KeyResult::Handled,
        }
    }

    // ── Agenda overlay ──────────────────────────────────────────────────

    fn open_agenda(&mut self) {
        let today = chrono::Local::now().date_naive();
        let view = match &self.index {
            Some(idx) => crate::agenda::scan_vault(idx, today).unwrap_or_else(|_| {
                crate::agenda::AgendaView {
                    overdue: Vec::new(),
                    today: Vec::new(),
                    upcoming: Vec::new(),
                }
            }),
            None => crate::agenda::AgendaView {
                overdue: Vec::new(),
                today: Vec::new(),
                upcoming: Vec::new(),
            },
        };
        self.agenda_view = Some(view);
        self.agenda_selected = 0;
        self.agenda_active = true;
    }

    fn close_agenda(&mut self) {
        self.agenda_active = false;
        self.agenda_view = None;
        self.agenda_selected = 0;
    }

    /// Total number of items in the flat agenda list.
    fn agenda_total_items(&self) -> usize {
        match &self.agenda_view {
            Some(v) => v.overdue.len() + v.today.len() + v.upcoming.len(),
            None => 0,
        }
    }

    /// Get the agenda item at a flat index (overdue ++ today ++ upcoming).
    fn agenda_item_at(&self, idx: usize) -> Option<&crate::agenda::AgendaItem> {
        let v = self.agenda_view.as_ref()?;
        let o = v.overdue.len();
        let t = v.today.len();
        if idx < o {
            Some(&v.overdue[idx])
        } else if idx < o + t {
            Some(&v.today[idx - o])
        } else if idx < o + t + v.upcoming.len() {
            Some(&v.upcoming[idx - o - t])
        } else {
            None
        }
    }

    /// Translate a body-relative line index (from indexed content) to a file
    /// line index by counting frontmatter lines in the actual file.
    fn body_line_to_file_line(store: &LocalFileStore, path: &std::path::Path, body_line: usize) -> usize {
        let content = match store.read(path) {
            Ok(c) => c,
            Err(_) => return body_line,
        };
        // Count lines before the body starts (frontmatter + blank line after).
        let mut in_frontmatter = false;
        let mut fm_end_line = 0;
        for (i, line) in content.lines().enumerate() {
            if i == 0 && line.trim() == "---" {
                in_frontmatter = true;
                continue;
            }
            if in_frontmatter {
                if line.trim() == "---" {
                    fm_end_line = i + 1; // line after closing ---
                    break;
                }
            }
        }
        // Skip blank lines between frontmatter and body
        let lines: Vec<&str> = content.lines().collect();
        let mut body_start = fm_end_line;
        while body_start < lines.len() && lines[body_start].trim().is_empty() {
            body_start += 1;
        }
        body_start + body_line
    }

    fn handle_agenda(&mut self, key: Key) -> KeyResult {
        let total = self.agenda_total_items();
        match key {
            Key::Escape | Key::Char('q') => {
                self.close_agenda();
            }
            Key::Char('j') | Key::Down => {
                if total > 0 && self.agenda_selected + 1 < total {
                    self.agenda_selected += 1;
                }
            }
            Key::Char('k') | Key::Up => {
                if self.agenda_selected > 0 {
                    self.agenda_selected -= 1;
                }
            }
            Key::Enter => {
                if let Some(item) = self.agenda_item_at(self.agenda_selected).cloned() {
                    // Compute file-relative line before closing (need vault_root).
                    let file_line = if let Some(root) = self.vault_root.clone() {
                        if let Ok(store) = LocalFileStore::new(root) {
                            Self::body_line_to_file_line(&store, &item.path, item.line)
                        } else {
                            item.line
                        }
                    } else {
                        item.line
                    };
                    self.close_agenda();
                    self.open_page_by_id(&item.page_id);
                    let line_byte = self.buffer.line_to_byte(file_line);
                    self.cursor = line_byte;
                }
            }
            Key::Char('x') => {
                if let Some(item) = self.agenda_item_at(self.agenda_selected).cloned() {
                    if let Some(root) = self.vault_root.clone() {
                        if let Ok(store) = LocalFileStore::new(root) {
                            // The agenda line is relative to indexed content (body only).
                            // Compute the frontmatter line offset in the actual file.
                            let file_line = Self::body_line_to_file_line(&store, &item.path, item.line);
                            let _ = crate::agenda::toggle_task(&store, &item.path, file_line);
                            // Refresh agenda
                            let today = chrono::Local::now().date_naive();
                            if let Some(ref idx) = self.index {
                                if let Ok(view) = crate::agenda::scan_vault(idx, today) {
                                    let new_total = view.overdue.len() + view.today.len() + view.upcoming.len();
                                    self.agenda_view = Some(view);
                                    if self.agenda_selected >= new_total && new_total > 0 {
                                        self.agenda_selected = new_total - 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Key::Char('s') => {
                self.transient_message = Some("Reschedule: date picker not yet implemented".into());
            }
            Key::Char('o') => {
                // Open source in split
                if let Some(item) = self.agenda_item_at(self.agenda_selected).cloned() {
                    let file_line = if let Some(root) = self.vault_root.clone() {
                        if let Ok(store) = LocalFileStore::new(root) {
                            Self::body_line_to_file_line(&store, &item.path, item.line)
                        } else {
                            item.line
                        }
                    } else {
                        item.line
                    };
                    self.close_agenda();
                    let new_pane = self.window_layout.split_vertical();
                    self.save_current_pane_state();
                    self.pane_states.insert(new_pane, PaneState::default());
                    self.load_pane_state(new_pane);
                    self.open_page_by_id(&item.page_id);
                    let line_byte = self.buffer.line_to_byte(file_line);
                    self.cursor = line_byte;
                }
            }
            _ => {}
        }
        KeyResult::Handled
    }

    // ── Journal helpers ─────────────────────────────────────────────────

    fn with_store<F>(&mut self, f: F)
    where
        F: FnOnce(&mut Self, &LocalFileStore),
    {
        if let Some(root) = self.vault_root.clone() {
            if let Ok(store) = LocalFileStore::new(root) {
                f(self, &store);
            }
        }
    }

    fn open_journal_today(&mut self) {
        self.with_store(|ed, store| {
            let svc = JournalService::new(store, store.journal_dir());
            let path = svc.today_path();
            ed.load_journal_file(&path, store);
        });
    }

    fn open_journal_prev(&mut self) {
        self.with_store(|ed, store| {
            let current_date = ed.current_journal_date();
            if let Some(date) = current_date {
                let prev = journal::prev_date(date);
                let svc = JournalService::new(store, store.journal_dir());
                let path = svc.path_for_date(prev);
                ed.load_journal_file(&path, store);
            }
        });
    }

    fn open_journal_next(&mut self) {
        self.with_store(|ed, store| {
            let current_date = ed.current_journal_date();
            if let Some(date) = current_date {
                let next = journal::next_date(date);
                let svc = JournalService::new(store, store.journal_dir());
                let path = svc.path_for_date(next);
                ed.load_journal_file(&path, store);
            }
        });
    }

    fn load_journal_file(&mut self, path: &std::path::Path, store: &LocalFileStore) {
        // Check open_buffers first.
        if let Some(buf) = self.take_or_read_buffer(path) {
            self.switch_to_buffer(buf);
            return;
        }
        // Journal page doesn't exist on disk yet — create it.
        let date_str = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown");
        let frontmatter = format!("---\ndate: {date_str}\n---\n");
        let _ = store.write(path, &frontmatter);
        let mut buf = Buffer::from_str(&frontmatter);
        buf.file_path = Some(path.to_path_buf());
        self.switch_to_buffer(buf);
    }

    fn current_journal_date(&self) -> Option<chrono::NaiveDate> {
        self.buffer
            .file_path
            .as_ref()
            .and_then(|p| p.file_stem())
            .and_then(|s| s.to_str())
            .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
    }

    // ── Capture bar ─────────────────────────────────────────────────────

    fn handle_capture_bar(&mut self, key: Key) -> KeyResult {
        match key {
            Key::Escape => {
                self.capture_bar_mode = None;
                self.capture_bar_input.clear();
                KeyResult::Handled
            }
            Key::Enter => {
                let text = self.capture_bar_input.clone();
                let mode = self.capture_bar_mode.take();
                self.capture_bar_input.clear();
                if !text.is_empty() {
                    if let Some(m) = mode {
                        self.commit_capture_bar(&text, m);
                    }
                }
                KeyResult::Handled
            }
            Key::Backspace => {
                self.capture_bar_input.pop();
                KeyResult::Handled
            }
            Key::Char(c) => {
                self.capture_bar_input.push(c);
                if c == '[' && self.capture_bar_input.ends_with("[[") {
                    self.open_capture_link_picker();
                }
                KeyResult::Handled
            }
            _ => KeyResult::Handled,
        }
    }

    fn open_capture_link_picker(&mut self) {
        let source = if let Some(ref index) = self.index {
            FindPagesSource::from_index(index).unwrap_or_else(|_| FindPagesSource::empty())
        } else {
            FindPagesSource::empty()
        };
        self.active_picker = Some(ActivePicker {
            kind: PickerKind::Default,
            title: "Insert Link".into(),
            inner: Box::new(Picker::new(source)),
            inline: true,
            inline_trigger_len: 2,
            is_embed: false,
            filter_pills: Vec::new(),
            typed_query: String::new(),
            preview: None,
            marked: HashSet::new(),
            supports_batch_select: false,
            action_menu_open: false,
            action_menu_selected: 0,
            action_menu_items: crate::picker::DEFAULT_ACTION_MENU_ITEMS.iter().map(|s| s.to_string()).collect(),
            drill_down_page_id: None,
            drill_down_page_title: None,
        });
    }

    fn commit_capture_bar(&mut self, text: &str, mode: CaptureBarMode) {
        if let Some(root) = self.vault_root.clone() {
            if let Ok(store) = LocalFileStore::new(root) {
                let svc = JournalService::new(&store, store.journal_dir());
                let result = match mode {
                    CaptureBarMode::Append => svc.quick_append_text(text),
                    CaptureBarMode::Task => svc.quick_append_task(text),
                };
                self.transient_message = Some(match result {
                    Ok(_) => format!(
                        "✓ Added to {} journal",
                        chrono::Local::now().format("%b %-d")
                    ),
                    Err(err) => format!("Failed to add to journal: {err}"),
                });
            }
        }
    }

    // ── Normal mode ─────────────────────────────────────────────────────

    fn handle_normal(&mut self, key: Key) -> KeyResult {
        // Awaiting text-object identifier (third key in e.g. `diw`)
        if let Some((op, kind, _n)) = self.awaiting_text_obj.take() {
            self.dot_record_push(&key);
            if let Key::Char(obj) = key {
                if let Some((start, end)) = self.text_object_range(kind, obj) {
                    if start < end {
                        match op {
                            'd' => {
                                let deleted = self.buffer.text()[start..end].to_string();
                                self.store_register(&deleted);
                                self.buffer.delete(start..end);
                                self.cursor = start;
                                self.clamp_cursor();
                                self.dot_record_finish();
                            }
                            'c' => {
                                let deleted = self.buffer.text()[start..end].to_string();
                                self.store_register(&deleted);
                                self.buffer.delete(start..end);
                                self.cursor = start;
                                self.mode = Mode::Insert;
                            }
                            'y' => {
                                let yanked = self.buffer.text()[start..end].to_string();
                                self.store_register(&yanked);
                                self.cursor = start;
                                self.dot_record_finish();
                            }
                            _ => { self.dot_record_finish(); }
                        }
                    } else {
                        self.dot_record_finish();
                    }
                } else {
                    self.dot_record_finish();
                }
            }
            return KeyResult::Handled;
        }

        // Count prefix accumulation
        if let Key::Char(c @ '1'..='9') = key {
            if self.pending_op.is_none() || self.count.is_some() {
                let digit = (c as u32 - '0' as u32) as usize;
                self.count = Some(self.count.unwrap_or(0) * 10 + digit);
                return KeyResult::Pending;
            }
        }
        if let Key::Char('0') = key {
            if self.count.is_some() {
                self.count = Some(self.count.unwrap() * 10);
                return KeyResult::Pending;
            }
        }

        let n = self.count.take().unwrap_or(1);

        // If we have a pending operator, the next key is a motion
        if let Some(op) = self.pending_op.take() {
            return self.execute_operator(op, &key, n);
        }

        match key {
            // Dot repeat
            Key::Char('.') => {
                if let Some(keys) = self.last_change.clone() {
                    self.is_replaying = true;
                    self.feed_keys(&keys);
                    self.is_replaying = false;
                }
                return KeyResult::Handled;
            }

            // Mode transitions
            Key::Char('i') => {
                self.dot_record_start(&key);
                self.mode = Mode::Insert;
                KeyResult::Handled
            }
            Key::Char('a') => {
                self.dot_record_start(&key);
                self.move_right(1); // move past current char
                self.mode = Mode::Insert;
                KeyResult::Handled
            }
            Key::Char('I') => {
                self.dot_record_start(&key);
                self.move_to_line_start();
                self.move_to_first_non_blank();
                self.mode = Mode::Insert;
                KeyResult::Handled
            }
            Key::Char('A') => {
                self.dot_record_start(&key);
                self.move_to_line_end_insert();
                self.mode = Mode::Insert;
                KeyResult::Handled
            }
            Key::Char('o') => {
                self.dot_record_start(&key);
                self.open_line_below();
                self.mode = Mode::Insert;
                KeyResult::Handled
            }
            Key::Char('O') => {
                self.dot_record_start(&key);
                self.open_line_above();
                self.mode = Mode::Insert;
                KeyResult::Handled
            }
            Key::Char('v') => {
                self.visual_anchor = self.cursor;
                self.mode = Mode::Visual;
                KeyResult::Handled
            }
            Key::Char(':') => {
                self.mode = Mode::Command;
                self.command_input.clear();
                KeyResult::Handled
            }

            // Motions
            Key::Char('h') | Key::Left => {
                self.move_left(n);
                KeyResult::Handled
            }
            Key::Char('l') | Key::Right => {
                self.move_right(n);
                KeyResult::Handled
            }
            Key::Char('j') | Key::Down => {
                self.move_down(n);
                KeyResult::Handled
            }
            Key::Char('k') | Key::Up => {
                self.move_up(n);
                KeyResult::Handled
            }
            Key::Char('w') => {
                for _ in 0..n {
                    self.move_word_forward();
                }
                KeyResult::Handled
            }
            Key::Char('b') => {
                for _ in 0..n {
                    self.move_word_backward();
                }
                KeyResult::Handled
            }
            Key::Char('e') => {
                for _ in 0..n {
                    self.move_word_end();
                }
                KeyResult::Handled
            }
            Key::Char('0') => {
                self.move_to_line_start();
                KeyResult::Handled
            }
            Key::Char('$') => {
                self.move_to_line_end();
                KeyResult::Handled
            }
            Key::Char('^') => {
                self.move_to_line_start();
                self.move_to_first_non_blank();
                KeyResult::Handled
            }
            Key::Char('G') => {
                let last_line = self.buffer.line_count().saturating_sub(1);
                let target = if n > 1 {
                    // Explicit count means go-to-line
                    (n - 1).min(last_line)
                } else {
                    last_line
                };
                self.cursor = self.buffer.line_to_byte(target);
                self.ensure_scroll();
                KeyResult::Handled
            }
            Key::Char('g') => {
                // Wait for second char
                self.pending_op = Some('g');
                self.count = Some(n);
                KeyResult::Pending
            }

            // Operators
            Key::Char('d') | Key::Char('c') | Key::Char('y') => {
                if let Key::Char(c) = key {
                    self.dot_record_start(&key);
                    self.pending_op = Some(c);
                    self.count = Some(n);
                }
                KeyResult::Pending
            }

            // Single-key actions
            Key::Char('x') => {
                self.dot_record_start(&key);
                for _ in 0..n {
                    self.delete_char_at_cursor();
                }
                self.dot_record_finish();
                KeyResult::Handled
            }
            Key::Char('X') => {
                self.dot_record_start(&key);
                for _ in 0..n {
                    self.delete_char_before_cursor();
                }
                self.dot_record_finish();
                KeyResult::Handled
            }
            Key::Char('r') => {
                self.dot_record_start(&key);
                // 'r' needs next char — store as pending
                self.pending_op = Some('r');
                self.count = Some(n);
                KeyResult::Pending
            }
            Key::Char('u') => {
                for _ in 0..n {
                    if !self.buffer.undo() {
                        break;
                    }
                }
                self.clamp_cursor();
                KeyResult::Handled
            }
            Key::Ctrl('r') => {
                for _ in 0..n {
                    if !self.buffer.redo() {
                        break;
                    }
                }
                self.clamp_cursor();
                KeyResult::Handled
            }
            Key::Char('p') => {
                self.dot_record_start(&key);
                let content = self.read_register();
                if !content.is_empty() {
                    // Paste after cursor
                    let insert_pos = (self.cursor + 1).min(self.buffer.text().len());
                    self.buffer.insert(insert_pos, &content);
                    self.cursor = insert_pos + content.len() - 1;
                    self.clamp_cursor();
                }
                self.dot_record_finish();
                KeyResult::Handled
            }
            Key::Char('P') => {
                self.dot_record_start(&key);
                let content = self.read_register();
                if !content.is_empty() {
                    self.buffer.insert(self.cursor, &content);
                    self.cursor = self.cursor + content.len() - 1;
                    self.clamp_cursor();
                }
                self.dot_record_finish();
                KeyResult::Handled
            }
            // Register prefix: "x sets pending register
            Key::Char('"') => {
                self.pending_op = Some('"');
                self.count = Some(n);
                KeyResult::Pending
            }
            // Macro recording / stop
            Key::Char('q') => {
                if self.macro_recording.is_some() {
                    // Stop recording
                    self.macro_recording = None;
                } else {
                    // Start: wait for register name
                    self.pending_op = Some('Q');
                    self.count = Some(n);
                }
                KeyResult::Handled
            }
            // Macro replay: @a / @@
            Key::Char('@') => {
                self.pending_op = Some('@');
                self.count = Some(n);
                KeyResult::Pending
            }
            // Set mark: ma
            Key::Char('m') => {
                self.pending_op = Some('m');
                self.count = Some(n);
                KeyResult::Pending
            }
            // Jump to mark line: 'a
            Key::Char('\'') => {
                self.pending_op = Some('\'');
                self.count = Some(n);
                KeyResult::Pending
            }
            // Jump to mark exact: `a
            Key::Char('`') => {
                self.pending_op = Some('`');
                self.count = Some(n);
                KeyResult::Pending
            }
            Key::Char('J') => {
                self.dot_record_start(&key);
                for _ in 0..n {
                    self.join_lines();
                }
                self.dot_record_finish();
                KeyResult::Handled
            }
            Key::Char(' ') => {
                // Enter leader (SPC-prefix) mode.
                self.leader_active = true;
                self.leader_state.reset();
                self.last_leader_action = None;
                KeyResult::Pending
            }
            Key::Escape => {
                self.pending_op = None;
                self.awaiting_text_obj = None;
                self.count = None;
                self.pending_register = None;
                KeyResult::Handled
            }
            _ => KeyResult::Handled,
        }
    }

    fn execute_operator(&mut self, op: char, motion: &Key, n: usize) -> KeyResult {
        // Record the motion key for dot-repeat
        self.dot_record_push(motion);

        // Handle `"x` — register prefix: set pending_register, then re-dispatch
        if op == '"' {
            if let Key::Char(reg) = motion {
                self.pending_register = Some(*reg);
            }
            return KeyResult::Handled;
        }

        // Handle `Q` (qa — start macro recording)
        if op == 'Q' {
            if let Key::Char(reg @ 'a'..='z') = motion {
                self.macro_recording = Some(*reg);
                self.macro_registers.insert(*reg, Vec::new());
            }
            return KeyResult::Handled;
        }

        // Handle `@` (macro replay)
        if op == '@' {
            if let Key::Char(reg) = motion {
                let target = if *reg == '@' {
                    self.last_macro
                } else {
                    Some(*reg)
                };
                if let Some(r) = target {
                    self.last_macro = Some(r);
                    if let Some(keys) = self.macro_registers.get(&r).cloned() {
                        let was_replaying = self.is_replaying;
                        self.is_replaying = true;
                        for _ in 0..n {
                            self.feed_keys(&keys);
                        }
                        self.is_replaying = was_replaying;
                    }
                }
            }
            return KeyResult::Handled;
        }

        // Handle `m` (set mark)
        if op == 'm' {
            if let Key::Char(reg @ 'a'..='z') = motion {
                self.marks.insert(*reg, self.cursor);
            }
            return KeyResult::Handled;
        }

        // Handle `'` (jump to mark line)
        if op == '\'' {
            if let Key::Char(reg @ 'a'..='z') = motion {
                if let Some(&pos) = self.marks.get(reg) {
                    self.cursor = pos.min(self.buffer.text().len().saturating_sub(1));
                    let (row, _) = self.cursor_row_col();
                    self.cursor = self.buffer.line_to_byte(row);
                    self.move_to_first_non_blank();
                    self.ensure_scroll();
                }
            }
            return KeyResult::Handled;
        }

        // Handle `` ` `` (jump to mark exact position)
        if op == '`' {
            if let Key::Char(reg @ 'a'..='z') = motion {
                if let Some(&pos) = self.marks.get(reg) {
                    self.cursor = pos.min(self.buffer.text().len().saturating_sub(1));
                    self.clamp_cursor();
                    self.ensure_scroll();
                }
            }
            return KeyResult::Handled;
        }

        // Handle 'g' prefix (gg)
        if op == 'g' {
            if let Key::Char('g') = motion {
                let target = if n > 1 { (n - 1).min(self.buffer.line_count().saturating_sub(1)) } else { 0 };
                self.cursor = self.buffer.line_to_byte(target);
                self.ensure_scroll();
                return KeyResult::Handled;
            }
            return KeyResult::Handled;
        }

        // Handle 'r' (replace char)
        if op == 'r' {
            if let Key::Char(ch) = motion {
                let text = self.buffer.text();
                let len = text.len();
                if self.cursor < len && text.as_bytes()[self.cursor] != b'\n' {
                    self.buffer.replace(self.cursor..self.cursor + 1, &ch.to_string());
                }
                self.dot_record_finish();
                return KeyResult::Handled;
            }
            self.dot_record_finish();
            return KeyResult::Handled;
        }

        // dd / cc / yy — line-wise double tap
        if let Key::Char(c) = motion {
            if *c == op {
                for _ in 0..n {
                    match op {
                        'd' => {
                            let (row, _) = self.cursor_row_col();
                            let line_start = self.buffer.line_to_byte(row);
                            let line = self.buffer.line(row).unwrap_or_default();
                            let end = line_start + line.len();
                            let deleted = self.buffer.text()[line_start..end.min(self.buffer.text().len())].to_string();
                            self.store_register(&deleted);
                            self.delete_current_line();
                        }
                        'c' => {
                            let (row, _) = self.cursor_row_col();
                            let line_start = self.buffer.line_to_byte(row);
                            let line = self.buffer.line(row).unwrap_or_default();
                            let content_len = line.trim_end_matches('\n').trim_end_matches('\r').len();
                            let deleted = self.buffer.text()[line_start..line_start + content_len].to_string();
                            self.store_register(&deleted);
                            self.delete_current_line_content();
                            self.mode = Mode::Insert;
                        }
                        'y' => {
                            let (row, _) = self.cursor_row_col();
                            let line_start = self.buffer.line_to_byte(row);
                            let line = self.buffer.line(row).unwrap_or_default();
                            let end = line_start + line.len();
                            let yanked = self.buffer.text()[line_start..end.min(self.buffer.text().len())].to_string();
                            self.store_register(&yanked);
                        }
                        _ => {}
                    }
                }
                if op != 'c' {
                    self.dot_record_finish();
                }
                return KeyResult::Handled;
            }
        }

        // Text objects: `i`/`a` as motion starts a text-object sequence
        if let Key::Char(k @ ('i' | 'a')) = motion {
            self.awaiting_text_obj = Some((op, *k, n));
            return KeyResult::Pending;
        }

        // Operator + motion: compute range then apply
        let start = self.cursor;
        match motion {
            Key::Char('w') => {
                for _ in 0..n {
                    self.move_word_forward();
                }
            }
            Key::Char('b') => {
                for _ in 0..n {
                    self.move_word_backward();
                }
            }
            Key::Char('e') => {
                for _ in 0..n {
                    self.move_word_end();
                }
                // 'e' motion for operators is inclusive
                let text = self.buffer.text();
                if self.cursor < text.len() {
                    self.cursor += 1;
                }
            }
            Key::Char('$') => {
                self.move_to_line_end();
                let text = self.buffer.text();
                if self.cursor < text.len() && text.as_bytes()[self.cursor] != b'\n' {
                    self.cursor += 1;
                }
            }
            Key::Char('0') => {
                self.move_to_line_start();
            }
            Key::Char('l') | Key::Right => {
                self.move_right(n);
            }
            Key::Char('h') | Key::Left => {
                self.move_left(n);
            }
            Key::Char('j') | Key::Down => {
                // Line-wise: delete from current line start through n lines
                let (row, _) = self.cursor_row_col();
                let end_row = (row + n).min(self.buffer.line_count().saturating_sub(1));
                let line_start = self.buffer.line_to_byte(row);
                let end_line_start = self.buffer.line_to_byte(end_row);
                let end_text = self.buffer.line(end_row).unwrap_or_default();
                let line_end = end_line_start + end_text.len();
                self.cursor = line_start;
                let text_len = self.buffer.text().len();
                let range_end = line_end.min(text_len);
                if range_end > line_start {
                    let deleted = self.buffer.text()[line_start..range_end].to_string();
                    self.store_register(&deleted);
                    self.buffer.delete(line_start..range_end);
                }
                self.clamp_cursor();
                self.dot_record_finish();
                return KeyResult::Handled;
            }
            _ => {
                // Unknown motion — cancel
                return KeyResult::Handled;
            }
        }
        let end = self.cursor;

        let (range_start, range_end) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };

        if range_start == range_end {
            self.cursor = start;
            return KeyResult::Handled;
        }

        match op {
            'd' => {
                let deleted = self.buffer.text()[range_start..range_end].to_string();
                self.store_register(&deleted);
                self.buffer.delete(range_start..range_end);
                self.cursor = range_start;
                self.clamp_cursor();
                self.dot_record_finish();
            }
            'c' => {
                let deleted = self.buffer.text()[range_start..range_end].to_string();
                self.store_register(&deleted);
                self.buffer.delete(range_start..range_end);
                self.cursor = range_start;
                self.mode = Mode::Insert;
                // Recording continues into Insert mode; finishes on Escape.
            }
            'y' => {
                let yanked = self.buffer.text()[range_start..range_end].to_string();
                self.store_register(&yanked);
                self.cursor = range_start;
                self.dot_record_finish();
            }
            _ => {
                self.cursor = start;
            }
        }
        KeyResult::Handled
    }

    // ── Insert mode ─────────────────────────────────────────────────────

    fn handle_insert(&mut self, key: Key) -> KeyResult {
        // Record all insert-mode keys for dot-repeat.
        self.dot_record_push(&key);

        match key {
            Key::Escape => {
                self.dot_record_finish();
                self.mode = Mode::Normal;
                // Move cursor back one if possible (Vim behavior)
                if self.cursor > 0 {
                    let (_row, col) = self.cursor_row_col();
                    if col > 0 {
                        self.cursor -= 1;
                    }
                }
                KeyResult::Handled
            }
            Key::Char(c) => {
                self.buffer.insert(self.cursor, &c.to_string());
                self.cursor += c.len_utf8();
                self.ensure_scroll();
                // Detect `[[` or `![[` to trigger inline picker.
                if c == '[' {
                    self.maybe_open_inline_picker();
                }
                KeyResult::Handled
            }
            Key::Enter => {
                self.buffer.insert(self.cursor, "\n");
                self.cursor += 1;
                self.ensure_scroll();
                KeyResult::Handled
            }
            Key::Backspace => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                    self.buffer.delete(self.cursor..self.cursor + 1);
                }
                KeyResult::Handled
            }
            Key::Tab => {
                if !self.tab_stops.is_empty() && self.tab_stop_index < self.tab_stops.len() {
                    let ts = &self.tab_stops[self.tab_stop_index];
                    self.cursor = ts.start;
                    self.tab_stop_index += 1;
                    if self.tab_stop_index >= self.tab_stops.len() {
                        self.tab_stops.clear();
                        self.tab_stop_index = 0;
                    }
                } else {
                    self.buffer.insert(self.cursor, "    ");
                    self.cursor += 4;
                }
                KeyResult::Handled
            }
            Key::Left => {
                self.move_left(1);
                KeyResult::Handled
            }
            Key::Right => {
                self.move_right(1);
                KeyResult::Handled
            }
            Key::Up => {
                self.move_up(1);
                KeyResult::Handled
            }
            Key::Down => {
                self.move_down(1);
                KeyResult::Handled
            }
            _ => KeyResult::Handled,
        }
    }

    // ── Visual mode ─────────────────────────────────────────────────────

    fn handle_visual(&mut self, key: Key) -> KeyResult {
        match key {
            Key::Escape | Key::Char('v') => {
                self.mode = Mode::Normal;
                KeyResult::Handled
            }
            Key::Char('d') | Key::Char('x') => {
                let (start, end) = self.visual_range();
                self.buffer.delete(start..end);
                self.cursor = start;
                self.clamp_cursor();
                self.mode = Mode::Normal;
                KeyResult::Handled
            }
            Key::Char('c') => {
                let (start, end) = self.visual_range();
                self.buffer.delete(start..end);
                self.cursor = start;
                self.mode = Mode::Insert;
                KeyResult::Handled
            }
            Key::Char('y') => {
                // Yank — no register system yet
                self.mode = Mode::Normal;
                KeyResult::Handled
            }
            // Motions in visual mode move cursor (anchor stays)
            Key::Char('h') | Key::Left => { self.move_left(1); KeyResult::Handled }
            Key::Char('l') | Key::Right => { self.move_right(1); KeyResult::Handled }
            Key::Char('j') | Key::Down => { self.move_down(1); KeyResult::Handled }
            Key::Char('k') | Key::Up => { self.move_up(1); KeyResult::Handled }
            Key::Char('w') => { self.move_word_forward(); KeyResult::Handled }
            Key::Char('b') => { self.move_word_backward(); KeyResult::Handled }
            Key::Char('e') => { self.move_word_end(); KeyResult::Handled }
            Key::Char('0') => { self.move_to_line_start(); KeyResult::Handled }
            Key::Char('$') => { self.move_to_line_end(); KeyResult::Handled }
            Key::Char('G') => {
                let last = self.buffer.line_count().saturating_sub(1);
                self.cursor = self.buffer.line_to_byte(last);
                self.ensure_scroll();
                KeyResult::Handled
            }
            _ => KeyResult::Handled,
        }
    }

    fn visual_range(&self) -> (usize, usize) {
        let a = self.visual_anchor;
        let b = self.cursor;
        if a <= b {
            // Include the character at b
            let text = self.buffer.text();
            let end = if b < text.len() { b + 1 } else { b };
            (a, end)
        } else {
            let text = self.buffer.text();
            let end = if a < text.len() { a + 1 } else { a };
            (b, end)
        }
    }

    // ── Command mode ────────────────────────────────────────────────────

    fn handle_command(&mut self, key: Key) -> KeyResult {
        match key {
            Key::Escape => {
                self.mode = Mode::Normal;
                self.command_input.clear();
                KeyResult::Handled
            }
            Key::Enter => {
                let result = self.execute_command();
                self.mode = Mode::Normal;
                self.command_input.clear();
                result
            }
            Key::Backspace => {
                if self.command_input.pop().is_none() {
                    self.mode = Mode::Normal;
                }
                KeyResult::Handled
            }
            Key::Char(c) => {
                self.command_input.push(c);
                KeyResult::Handled
            }
            _ => KeyResult::Handled,
        }
    }

    fn execute_command(&mut self) -> KeyResult {
        let cmd = self.command_input.trim().to_string();
        match cmd.as_str() {
            "q" | "q!" => KeyResult::Quit,
            "w" => KeyResult::Save,
            "wq" | "x" => KeyResult::SaveAndQuit,
            "rebuild-index" => {
                self.rebuild_index();
                KeyResult::Handled
            }
            "theme" => {
                self.cycle_theme();
                KeyResult::Handled
            }
            _ if cmd.starts_with("theme ") => {
                let name = cmd.strip_prefix("theme ").unwrap().trim();
                self.set_theme_by_name(name);
                KeyResult::Handled
            }
            _ => {
                // Try :N for go-to-line
                if let Ok(line_num) = cmd.parse::<usize>() {
                    if line_num > 0 {
                        let target = (line_num - 1).min(self.buffer.line_count().saturating_sub(1));
                        self.cursor = self.buffer.line_to_byte(target);
                        self.ensure_scroll();
                    }
                }
                KeyResult::Handled
            }
        }
    }

    pub fn rebuild_index(&mut self) {
        let (Some(index), Some(root)) = (self.index.as_mut(), self.vault_root.as_ref()) else {
            return;
        };
        let dirs = [root.join("pages"), root.join("journal")];
        for dir in &dirs {
            let entries = match std::fs::read_dir(dir) {
                Ok(e) => e,
                Err(_) => continue,
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map_or(true, |e| e != "md") {
                    continue;
                }
                let content = match std::fs::read_to_string(&path) {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                let doc = match parser::parse(&content) {
                    Ok(d) => d,
                    Err(_) => continue,
                };
                let _ = index.index_document(&path, &doc);
            }
        }
    }

    // ── Theme switching ─────────────────────────────────────────────────

    /// Cycle to the next built-in theme.
    pub fn cycle_theme(&mut self) {
        let themes = Theme::all_builtin();
        let current_name = self.theme.name;
        let idx = themes.iter().position(|t| t.name == current_name).unwrap_or(0);
        let next = (idx + 1) % themes.len();
        self.theme = themes.into_iter().nth(next).unwrap();
    }

    /// Set theme by name. Falls back to cycling if name not found.
    pub fn set_theme_by_name(&mut self, name: &str) {
        let themes = Theme::all_builtin();
        if let Some(t) = themes.into_iter().find(|t| t.name == name) {
            self.theme = t;
        }
    }

    // ── Motion helpers ──────────────────────────────────────────────────

    fn move_left(&mut self, n: usize) {
        let (row, col) = self.cursor_row_col();
        let new_col = col.saturating_sub(n);
        let line_start = self.buffer.line_to_byte(row);
        self.cursor = line_start + new_col;
    }

    fn move_right(&mut self, n: usize) {
        let (row, col) = self.cursor_row_col();
        let line = self.buffer.line(row).unwrap_or_default();
        let line_len = line.trim_end_matches('\n').trim_end_matches('\r').len();
        // In normal mode, can't go past last char; in insert mode, can be at line_len
        let max_col = if self.mode == Mode::Insert {
            line_len
        } else {
            line_len.saturating_sub(1)
        };
        let new_col = (col + n).min(max_col);
        let line_start = self.buffer.line_to_byte(row);
        self.cursor = line_start + new_col;
    }

    fn move_down(&mut self, n: usize) {
        let (row, col) = self.cursor_row_col();
        let target_row = (row + n).min(self.buffer.line_count().saturating_sub(1));
        let line_start = self.buffer.line_to_byte(target_row);
        let line = self.buffer.line(target_row).unwrap_or_default();
        let line_len = line.trim_end_matches('\n').trim_end_matches('\r').len();
        let clamped_col = col.min(line_len.saturating_sub(1).max(0));
        self.cursor = line_start + clamped_col;
        self.ensure_scroll();
    }

    fn move_up(&mut self, n: usize) {
        let (row, col) = self.cursor_row_col();
        let target_row = row.saturating_sub(n);
        let line_start = self.buffer.line_to_byte(target_row);
        let line = self.buffer.line(target_row).unwrap_or_default();
        let line_len = line.trim_end_matches('\n').trim_end_matches('\r').len();
        let clamped_col = col.min(line_len.saturating_sub(1).max(0));
        self.cursor = line_start + clamped_col;
        self.ensure_scroll();
    }

    fn move_word_forward(&mut self) {
        let text = self.buffer.text();
        let bytes = text.as_bytes();
        let len = bytes.len();
        if self.cursor >= len {
            return;
        }

        let mut i = self.cursor;
        // Skip current word chars
        if i < len && is_word_char(bytes[i]) {
            while i < len && is_word_char(bytes[i]) {
                i += 1;
            }
        } else if i < len && is_punct(bytes[i]) {
            while i < len && is_punct(bytes[i]) {
                i += 1;
            }
        } else {
            i += 1;
        }
        // Skip whitespace
        while i < len && bytes[i].is_ascii_whitespace() && bytes[i] != b'\n' {
            i += 1;
        }
        // Skip newlines (move to next line's first non-blank)
        if i < len && bytes[i] == b'\n' {
            i += 1;
            while i < len && bytes[i] == b' ' {
                // don't skip past content
                break;
            }
        }

        self.cursor = i.min(len);
        self.ensure_scroll();
    }

    fn move_word_backward(&mut self) {
        let text = self.buffer.text();
        let bytes = text.as_bytes();
        if self.cursor == 0 {
            return;
        }

        let mut i = self.cursor;
        // Skip trailing whitespace
        while i > 0 && bytes[i - 1].is_ascii_whitespace() {
            i -= 1;
        }
        // Skip word or punct
        if i > 0 && is_word_char(bytes[i - 1]) {
            while i > 0 && is_word_char(bytes[i - 1]) {
                i -= 1;
            }
        } else if i > 0 && is_punct(bytes[i - 1]) {
            while i > 0 && is_punct(bytes[i - 1]) {
                i -= 1;
            }
        } else if i > 0 {
            i -= 1;
        }

        self.cursor = i;
        self.ensure_scroll();
    }

    fn move_word_end(&mut self) {
        let text = self.buffer.text();
        let bytes = text.as_bytes();
        let len = bytes.len();
        if self.cursor + 1 >= len {
            return;
        }

        let mut i = self.cursor + 1;
        // Skip whitespace
        while i < len && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        // Skip to end of word
        if i < len && is_word_char(bytes[i]) {
            while i + 1 < len && is_word_char(bytes[i + 1]) {
                i += 1;
            }
        } else if i < len && is_punct(bytes[i]) {
            while i + 1 < len && is_punct(bytes[i + 1]) {
                i += 1;
            }
        }

        self.cursor = i.min(len.saturating_sub(1));
        self.ensure_scroll();
    }

    fn move_to_line_start(&mut self) {
        let (row, _) = self.cursor_row_col();
        self.cursor = self.buffer.line_to_byte(row);
    }

    fn move_to_line_end(&mut self) {
        let (row, _) = self.cursor_row_col();
        let line = self.buffer.line(row).unwrap_or_default();
        let content_len = line.trim_end_matches('\n').trim_end_matches('\r').len();
        let line_start = self.buffer.line_to_byte(row);
        self.cursor = line_start + content_len.saturating_sub(1).max(0);
    }

    fn move_to_line_end_insert(&mut self) {
        let (row, _) = self.cursor_row_col();
        let line = self.buffer.line(row).unwrap_or_default();
        let content_len = line.trim_end_matches('\n').trim_end_matches('\r').len();
        let line_start = self.buffer.line_to_byte(row);
        self.cursor = line_start + content_len;
    }

    fn move_to_first_non_blank(&mut self) {
        let (row, _) = self.cursor_row_col();
        let line = self.buffer.line(row).unwrap_or_default();
        let line_start = self.buffer.line_to_byte(row);
        let offset = line.chars().take_while(|c| c.is_whitespace() && *c != '\n').count();
        self.cursor = line_start + offset;
    }

    // ── Edit helpers ────────────────────────────────────────────────────

    fn delete_char_at_cursor(&mut self) {
        let text = self.buffer.text();
        if self.cursor < text.len() && text.as_bytes()[self.cursor] != b'\n' {
            self.buffer.delete(self.cursor..self.cursor + 1);
            self.clamp_cursor();
        }
    }

    fn delete_char_before_cursor(&mut self) {
        if self.cursor > 0 {
            let (_row, col) = self.cursor_row_col();
            if col > 0 {
                self.cursor -= 1;
                self.buffer.delete(self.cursor..self.cursor + 1);
            }
        }
    }

    fn delete_current_line(&mut self) {
        let (row, _) = self.cursor_row_col();
        let line_start = self.buffer.line_to_byte(row);
        let line = self.buffer.line(row).unwrap_or_default();
        let end = line_start + line.len();
        // If the line includes a trailing newline, delete that too
        let text = self.buffer.text();
        if end <= text.len() && end > line_start {
            self.buffer.delete(line_start..end);
        }
        self.cursor = line_start;
        self.clamp_cursor();
    }

    fn delete_current_line_content(&mut self) {
        let (row, _) = self.cursor_row_col();
        let line_start = self.buffer.line_to_byte(row);
        let line = self.buffer.line(row).unwrap_or_default();
        let content_len = line.trim_end_matches('\n').trim_end_matches('\r').len();
        if content_len > 0 {
            self.buffer.delete(line_start..line_start + content_len);
        }
        self.cursor = line_start;
    }

    fn open_line_below(&mut self) {
        let (row, _) = self.cursor_row_col();
        let line = self.buffer.line(row).unwrap_or_default();
        let line_start = self.buffer.line_to_byte(row);
        let insert_pos = line_start + line.len();
        // If line doesn't end with newline, we need to add before our new line
        if !line.ends_with('\n') {
            self.buffer.insert(insert_pos, "\n");
            self.cursor = insert_pos + 1;
        } else {
            self.buffer.insert(insert_pos, "\n");
            self.cursor = insert_pos; // after existing newline
            // Actually the newline was at insert_pos-1, so our new \n goes after it
            self.cursor = insert_pos + 1;
        }
        // More precise: position cursor at start of new blank line
        let new_line_start = self.buffer.line_to_byte(row + 1);
        self.cursor = new_line_start;
        self.ensure_scroll();
    }

    fn open_line_above(&mut self) {
        let (row, _) = self.cursor_row_col();
        let line_start = self.buffer.line_to_byte(row);
        self.buffer.insert(line_start, "\n");
        self.cursor = line_start;
        self.ensure_scroll();
    }

    fn join_lines(&mut self) {
        let (row, _) = self.cursor_row_col();
        if row + 1 >= self.buffer.line_count() {
            return;
        }
        // Find end of current line (before newline)
        let line = self.buffer.line(row).unwrap_or_default();
        let line_start = self.buffer.line_to_byte(row);
        let content_len = line.trim_end_matches('\n').trim_end_matches('\r').len();
        let join_pos = line_start + content_len;

        // Delete from join_pos to start of next line's content
        let next_line_start = self.buffer.line_to_byte(row + 1);
        let next_line = self.buffer.line(row + 1).unwrap_or_default();
        let next_content_offset = next_line
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .count();

        let delete_end = next_line_start + next_content_offset;
        if delete_end > join_pos {
            self.buffer.delete(join_pos..delete_end);
            // Insert a single space
            self.buffer.insert(join_pos, " ");
            self.cursor = join_pos;
        }
    }

    // ── Cursor utilities ────────────────────────────────────────────────

    fn clamp_cursor(&mut self) {
        let text = self.buffer.text();
        let len = text.len();
        if len == 0 {
            self.cursor = 0;
            return;
        }
        if self.cursor >= len {
            self.cursor = len.saturating_sub(1);
        }
        // In normal mode, don't sit on a newline if we can avoid it
        if self.mode == Mode::Normal && self.cursor > 0 {
            let bytes = text.as_bytes();
            if self.cursor < len && bytes[self.cursor] == b'\n' && self.cursor > 0 {
                self.cursor -= 1;
            }
        }
        self.ensure_scroll();
    }

    fn ensure_scroll(&mut self) {
        let (row, _) = self.cursor_row_col();
        if row < self.scroll_offset {
            self.scroll_offset = row;
        }
        if row >= self.scroll_offset + self.viewport_height {
            self.scroll_offset = row - self.viewport_height + 1;
        }
    }

    // ── Public accessors ────────────────────────────────────────────────

    /// Current buffer text as a String.
    pub fn text(&self) -> String {
        self.buffer.text().to_string()
    }

    /// Cursor position as (row, col) in the document.
    pub fn cursor_row_col(&self) -> (usize, usize) {
        let len = self.buffer.rope.len_bytes();
        let byte_pos = self.cursor.min(len);
        let line_idx = self.buffer.byte_to_line(byte_pos);
        let line_start = self.buffer.line_to_byte(line_idx);
        let col = byte_pos - line_start;
        (line_idx, col)
    }

    /// Pending keys formatted as a display string.
    pub fn pending_display(&self) -> String {
        if self.leader_active {
            let prefix = self.leader_state.prefix();
            if prefix.is_empty() {
                return "SPC".to_string();
            }
            return format!("SPC {}", prefix.join(" "));
        }
        let mut out = String::new();
        if let Some(n) = self.count {
            if n > 1 {
                out.push_str(&n.to_string());
            }
        }
        if let Some(op) = self.pending_op {
            out.push(op);
        }
        out
    }

    /// Check links in the current buffer against the index and return
    /// `Diagnostic` entries for any that cannot be resolved.
    pub fn link_diagnostics(&self, index: &SqliteIndex) -> Vec<Diagnostic> {
        let text = self.text();
        let doc = match crate::parser::parse(&text) {
            Ok(doc) => doc,
            Err(_) => return Vec::new(),
        };

        let fm_lines = frontmatter_line_count(&text);

        let mut diagnostics = Vec::new();
        for (block_idx, block) in doc.blocks.iter().enumerate() {
            let line = fm_lines + block_idx;
            for link in &block.links {
                let page_id = link.page_id.trim();
                if page_id.is_empty() {
                    continue;
                }
                match index.page_for_id(page_id) {
                    Ok(Some(_)) => {}
                    _ => {
                        diagnostics.push(Diagnostic {
                            line,
                            start: link.span.start,
                            end: link.span.end,
                            kind: DiagnosticKind::BrokenLink,
                            message: format!("broken link: page '{page_id}' not found"),
                        });
                    }
                }
            }
        }
        diagnostics
    }

    /// Produce a UI-agnostic RenderFrame for the current state.
    pub fn render(&self) -> RenderFrame {
        let focused_id = self.window_layout.focused();
        let pane_ids = self.window_layout.pane_ids();

        let mut panes = Vec::with_capacity(pane_ids.len());
        for &pane_id in &pane_ids {
            let is_focused = pane_id == focused_id;
            let pane = if is_focused {
                self.render_pane_from_editor(true)
            } else if let Some(ps) = self.pane_states.get(&pane_id) {
                Self::render_pane_from_state(ps, self.viewport_height, false)
            } else {
                Self::render_pane_from_state(&PaneState::default(), self.viewport_height, false)
            };
            panes.push(pane);
        }

        let command_line = if self.mode == Mode::Command {
            Some(format!(":{}", self.command_input))
        } else {
            self.transient_message.clone()
        };

        let which_key = if self.leader_active {
            self.leader_registry
                .query(self.leader_state.prefix().iter().map(String::as_str))
                .or_else(|| {
                    if self.leader_state.prefix().is_empty() {
                        self.leader_registry.query(std::iter::empty::<&str>())
                    } else {
                        None
                    }
                })
        } else {
            None
        };

        let capture_bar = self.capture_bar_mode.as_ref().map(|mode| {
            let prefix = match mode {
                CaptureBarMode::Append => "Append: ",
                CaptureBarMode::Task => "Task: ",
            };
            format!("{}{}", prefix, self.capture_bar_input)
        });

        let picker_frame = self.active_picker.as_ref().map(|ap| {
            let results = ap.inner.results();
            let selected = ap.inner.selected_index();
            let total = results.len();
            let marked_count = ap.marked.len();
            let items: Vec<RenderPickerItem> = results
                .iter()
                .enumerate()
                .map(|(i, m)| RenderPickerItem {
                    text: m.text.clone(),
                    match_indices: m.match_indices.clone(),
                    marginalia: m.marginalia.clone(),
                    selected: Some(i) == selected,
                    marked: ap.marked.contains(&m.source_index),
                })
                .collect();
            let shown = items.len();
            let result_count = if marked_count > 0 {
                format!("{marked_count} marked · {shown} of {total}")
            } else {
                format!("{shown} of {total}")
            };
            PickerFrame {
                title: ap.title.clone(),
                query: ap.typed_query.clone(),
                filters: ap
                    .filter_pills
                    .iter()
                    .map(|(kind, label)| crate::render::FilterPill {
                        kind: kind.clone(),
                        label: label.clone(),
                    })
                    .collect(),
                items,
                result_count,
                preview: ap.preview.clone(),
                inline: ap.inline,
                action_menu: if ap.action_menu_open {
                    Some(ap.action_menu_items.clone())
                } else {
                    None
                },
                action_menu_selected: if ap.action_menu_open {
                    Some(ap.action_menu_selected)
                } else {
                    None
                },
            }
        });

        let diagnostics = match self.index.as_ref() {
            Some(idx) => self.link_diagnostics(idx),
            None => Vec::new(),
        };

        // Add BrokenLink styled spans to the focused pane.
        if !diagnostics.is_empty() {
            if let Some(focused_pane) = panes.iter_mut().find(|p| p.focused) {
                for diag in &diagnostics {
                    if let Some(rendered) = focused_pane
                        .lines
                        .iter_mut()
                        .find(|l| l.line_number == Some(diag.line))
                    {
                        rendered.spans.push(StyledSpan {
                            start: diag.start,
                            end: diag.end,
                            style: Style::BrokenLink,
                        });
                    }
                }
            }
        }

        let undo_tree_frame = if self.undo_tree_active {
            let branches = self.buffer.undo_branches();
            let current_node = self.buffer.undo_current_node();
            let entries = if branches.is_empty() {
                vec![UndoTreeEntry {
                    branch_index: 0,
                    current: true,
                }]
            } else {
                branches
                    .iter()
                    .map(|&id| UndoTreeEntry {
                        branch_index: id,
                        current: id == current_node,
                    })
                    .collect()
            };
            Some(UndoTreeFrame {
                title: "Undo Tree".to_string(),
                entries,
                selected: self.undo_tree_selected,
            })
        } else {
            None
        };

        let agenda_frame = if self.agenda_active {
            self.agenda_view.as_ref().map(|view| {
                use crate::render::{AgendaFrame, AgendaSection, AgendaRenderItem};
                let make_items = |items: &[crate::agenda::AgendaItem]| -> Vec<AgendaRenderItem> {
                    items.iter().map(|i| AgendaRenderItem {
                        text: i.text.clone(),
                        page_title: i.page_title.clone(),
                        date: i.timestamp.date.format("%Y-%m-%d").to_string(),
                        completed: i.completed,
                        tags: i.tags.clone(),
                    }).collect()
                };
                let mut sections = Vec::new();
                if !view.overdue.is_empty() {
                    sections.push(AgendaSection { label: "Overdue".into(), items: make_items(&view.overdue) });
                }
                if !view.today.is_empty() {
                    sections.push(AgendaSection { label: "Today".into(), items: make_items(&view.today) });
                }
                if !view.upcoming.is_empty() {
                    sections.push(AgendaSection { label: "Upcoming".into(), items: make_items(&view.upcoming) });
                }
                let total = view.overdue.len() + view.today.len() + view.upcoming.len();
                AgendaFrame {
                    title: "Agenda".into(),
                    sections,
                    selected: self.agenda_selected,
                    total_items: total,
                }
            })
        } else {
            None
        };

        RenderFrame {
            panes,
            picker: picker_frame,
            which_key,
            diagnostics,
            command_line,
            capture_bar,
            undo_tree: undo_tree_frame,
            agenda: agenda_frame,
        }
    }

    /// Render the focused pane from live editor state.
    fn render_pane_from_editor(&self, focused: bool) -> PaneFrame {
        let total_lines = self.buffer.line_count();
        let (cursor_row, cursor_col) = self.cursor_row_col();

        let end_line = (self.scroll_offset + self.viewport_height).min(total_lines);
        let mut lines = Vec::with_capacity(self.viewport_height);

        // Build highlight context by scanning from document start
        let mut hl_ctx = crate::highlight::HighlightContext::new();
        for i in 0..self.scroll_offset {
            if let Some(line_text) = self.buffer.line(i) {
                let display = line_text.trim_end_matches('\n').trim_end_matches('\r');
                crate::highlight::highlight_line(display, &mut hl_ctx);
            }
        }

        for i in self.scroll_offset..end_line {
            let line_text = self.buffer.line(i).unwrap_or_default();
            let display_text = line_text
                .trim_end_matches('\n')
                .trim_end_matches('\r')
                .to_string();
            let spans = crate::highlight::highlight_line(&display_text, &mut hl_ctx);
            lines.push(RenderedLine {
                text: display_text,
                spans,
                line_number: Some(i),
            });
        }

        for _ in lines.len()..self.viewport_height {
            lines.push(RenderedLine {
                text: "~".into(),
                spans: Vec::new(),
                line_number: None,
            });
        }

        let cursor_shape = match self.mode {
            Mode::Normal => CursorShape::Block,
            Mode::Insert => CursorShape::Bar,
            Mode::Visual => CursorShape::Block,
            Mode::Command => CursorShape::Bar,
        };

        let filename = self
            .buffer
            .file_path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "[No Name]".into());

        let (row, col) = self.cursor_row_col();

        PaneFrame {
            lines,
            cursor: CursorState {
                row: cursor_row.saturating_sub(self.scroll_offset),
                col: cursor_col,
                shape: cursor_shape,
            },
            status_bar: StatusBar {
                mode: self.mode.to_string(),
                filename,
                dirty: self.buffer.dirty,
                position: format!("{}:{}", row + 1, col + 1),
                pending_keys: self.pending_display(),
                filetype: "markdown".into(),
            },
            focused,
        }
    }

    /// Render a non-focused pane from its stored state.
    fn render_pane_from_state(ps: &PaneState, viewport_height: usize, focused: bool) -> PaneFrame {
        let total_lines = ps.buffer.line_count();
        let end_line = (ps.scroll_offset + viewport_height).min(total_lines);
        let mut lines = Vec::with_capacity(viewport_height);

        for i in ps.scroll_offset..end_line {
            let line_text = ps.buffer.line(i).unwrap_or_default();
            let display_text = line_text
                .trim_end_matches('\n')
                .trim_end_matches('\r')
                .to_string();
            lines.push(RenderedLine {
                text: display_text,
                spans: Vec::new(),
                line_number: Some(i),
            });
        }

        for _ in lines.len()..viewport_height {
            lines.push(RenderedLine {
                text: "~".into(),
                spans: Vec::new(),
                line_number: None,
            });
        }

        let filename = ps
            .buffer
            .file_path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "[No Name]".into());

        PaneFrame {
            lines,
            cursor: CursorState {
                row: 0,
                col: 0,
                shape: CursorShape::Block,
            },
            status_bar: StatusBar {
                mode: "NORMAL".into(),
                filename,
                dirty: ps.buffer.dirty,
                position: "1:1".into(),
                pending_keys: String::new(),
                filetype: "markdown".into(),
            },
            focused,
        }
    }

    // ── Text objects ────────────────────────────────────────────────────

    /// Compute (start, end) byte range for a text object.
    /// `kind` is 'i' (inner) or 'a' (around), `obj` is the object key.
    fn text_object_range(&self, kind: char, obj: char) -> Option<(usize, usize)> {
        match obj {
            'w' => Some(self.text_object_word(kind)),
            '(' | ')' | 'b' => self.text_object_delimited(kind, b'(', b')'),
            '[' | ']' => self.text_object_delimited(kind, b'[', b']'),
            '{' | '}' | 'B' => self.text_object_delimited(kind, b'{', b'}'),
            '"' => self.text_object_quote(kind, b'"'),
            '\'' => self.text_object_quote(kind, b'\''),
            '`' => self.text_object_quote(kind, b'`'),
            // Bloom-specific text objects
            'l' => self.text_object_bloom_link(kind),
            'e' => self.text_object_bloom_embed(kind),
            '#' => self.text_object_bloom_tag(kind),
            '@' => self.text_object_bloom_timestamp(kind),
            'h' => self.text_object_bloom_heading_section(kind),
            _ => None,
        }
    }

    fn text_object_word(&self, kind: char) -> (usize, usize) {
        let text = self.buffer.text();
        let bytes = text.as_bytes();
        let len = bytes.len();
        if len == 0 {
            return (0, 0);
        }
        let pos = self.cursor.min(len.saturating_sub(1));

        // Find start/end of the current word
        let mut start = pos;
        let mut end = pos;

        if is_word_char(bytes[pos]) {
            while start > 0 && is_word_char(bytes[start - 1]) {
                start -= 1;
            }
            while end < len && is_word_char(bytes[end]) {
                end += 1;
            }
        } else if bytes[pos].is_ascii_whitespace() {
            while start > 0 && bytes[start - 1].is_ascii_whitespace() && bytes[start - 1] != b'\n' {
                start -= 1;
            }
            while end < len && bytes[end].is_ascii_whitespace() && bytes[end] != b'\n' {
                end += 1;
            }
        } else {
            // punctuation
            while start > 0 && is_punct(bytes[start - 1]) {
                start -= 1;
            }
            while end < len && is_punct(bytes[end]) {
                end += 1;
            }
        }

        if kind == 'a' {
            // Include trailing whitespace, or leading if no trailing
            let saved_end = end;
            while end < len && bytes[end].is_ascii_whitespace() && bytes[end] != b'\n' {
                end += 1;
            }
            if end == saved_end {
                // No trailing space — consume leading space
                while start > 0 && bytes[start - 1].is_ascii_whitespace() && bytes[start - 1] != b'\n' {
                    start -= 1;
                }
            }
        }

        (start, end)
    }

    fn text_object_delimited(&self, kind: char, open: u8, close: u8) -> Option<(usize, usize)> {
        let text = self.buffer.text();
        let bytes = text.as_bytes();
        let len = bytes.len();
        let pos = self.cursor;

        // Search backward for matching open delimiter (handle nesting)
        let mut depth = 0i32;
        let mut open_pos = None;
        let mut i = pos;
        loop {
            if bytes[i] == close && i != pos {
                depth += 1;
            } else if bytes[i] == open {
                if depth == 0 {
                    open_pos = Some(i);
                    break;
                }
                depth -= 1;
            }
            if i == 0 {
                break;
            }
            i -= 1;
        }
        let open_idx = open_pos?;

        // Search forward for matching close delimiter
        depth = 0;
        let mut close_pos = None;
        for j in (open_idx + 1)..len {
            if bytes[j] == open {
                depth += 1;
            } else if bytes[j] == close {
                if depth == 0 {
                    close_pos = Some(j);
                    break;
                }
                depth -= 1;
            }
        }
        let close_idx = close_pos?;

        if kind == 'i' {
            Some((open_idx + 1, close_idx))
        } else {
            Some((open_idx, close_idx + 1))
        }
    }

    /// Quote text object: find matching quote on the same line.
    fn text_object_quote(&self, kind: char, quote: u8) -> Option<(usize, usize)> {
        let text = self.buffer.text();
        let bytes = text.as_bytes();
        let pos = self.cursor;
        // Find line boundaries.
        let line_start = text[..pos].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let line_end = text[pos..].find('\n').map(|i| pos + i).unwrap_or(text.len());
        let line = &bytes[line_start..line_end];
        let rel = pos - line_start;
        // Find all quote positions on this line.
        let positions: Vec<usize> = line.iter().enumerate()
            .filter(|(_, b)| **b == quote)
            .map(|(i, _)| i)
            .collect();
        // Find the pair that contains cursor.
        for pair in positions.chunks(2) {
            if pair.len() == 2 && pair[0] <= rel && rel <= pair[1] {
                return if kind == 'i' {
                    Some((line_start + pair[0] + 1, line_start + pair[1]))
                } else {
                    Some((line_start + pair[0], line_start + pair[1] + 1))
                };
            }
        }
        None
    }

    /// Bloom text object: `il` / `al` — content within `[[...]]`.
    fn text_object_bloom_link(&self, kind: char) -> Option<(usize, usize)> {
        let text = self.buffer.text();
        let pos = self.cursor;
        // Search backward for `[[`.
        let before = &text[..pos.min(text.len())];
        let open = before.rfind("[[")?;
        // Search forward from open for `]]`.
        let after_open = open + 2;
        let close = text[after_open..].find("]]").map(|i| after_open + i)?;
        // Cursor must be within the range.
        if pos > close + 1 { return None; }
        if kind == 'i' {
            Some((after_open, close))
        } else {
            Some((open, close + 2))
        }
    }

    /// Bloom text object: `ie` / `ae` — content within `![[...]]`.
    fn text_object_bloom_embed(&self, kind: char) -> Option<(usize, usize)> {
        let text = self.buffer.text();
        let pos = self.cursor;
        let before = &text[..pos.min(text.len())];
        // Search for `![[`.
        let open = before.rfind("![[")?;
        let after_open = open + 3;
        let close = text[after_open..].find("]]").map(|i| after_open + i)?;
        if pos > close + 1 { return None; }
        if kind == 'i' {
            Some((after_open, close))
        } else {
            Some((open, close + 2))
        }
    }

    /// Bloom text object: `i#` / `a#` — tag name (after `#`).
    fn text_object_bloom_tag(&self, kind: char) -> Option<(usize, usize)> {
        let text = self.buffer.text();
        let bytes = text.as_bytes();
        let pos = self.cursor.min(bytes.len().saturating_sub(1));
        // Walk backward to find the `#` that starts this tag.
        let mut hash_pos = pos;
        loop {
            if bytes[hash_pos] == b'#' { break; }
            if hash_pos == 0 { return None; }
            if bytes[hash_pos].is_ascii_whitespace() { return None; }
            hash_pos -= 1;
        }
        // Walk forward from hash to find end of tag name.
        let mut end = hash_pos + 1;
        while end < bytes.len()
            && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'-' || bytes[end] == b'_')
        {
            end += 1;
        }
        if end == hash_pos + 1 { return None; } // empty tag
        if kind == 'i' {
            Some((hash_pos + 1, end))
        } else {
            Some((hash_pos, end))
        }
    }

    /// Bloom text object: `i@` / `a@` — timestamp like `@due(2026-03-05)`.
    fn text_object_bloom_timestamp(&self, kind: char) -> Option<(usize, usize)> {
        let text = self.buffer.text();
        let pos = self.cursor;
        let before = &text[..pos.min(text.len())];
        // Search backward for `@` followed by a word and `(`.
        let at_pos = before.rfind('@')?;
        let after_at = &text[at_pos..];
        let paren_open = after_at.find('(')?;
        let paren_close = after_at.find(')')?;
        if paren_close <= paren_open { return None; }
        let abs_end = at_pos + paren_close + 1;
        if pos > abs_end { return None; }
        if kind == 'i' {
            // Inner: just the date inside parens.
            Some((at_pos + paren_open + 1, at_pos + paren_close))
        } else {
            // Around: `@due(2026-03-05)` in full.
            Some((at_pos, abs_end))
        }
    }

    /// Bloom text object: `ih` / `ah` — heading section.
    /// `ih`: content under heading (excluding heading line).
    /// `ah`: heading line + all content until next same-or-higher-level heading.
    fn text_object_bloom_heading_section(&self, kind: char) -> Option<(usize, usize)> {
        let text = self.buffer.text();
        let lines: Vec<&str> = text.lines().collect();
        if lines.is_empty() { return None; }
        let (cursor_line, _) = self.cursor_row_col();

        // Find the heading line at or above cursor.
        let mut heading_line = None;
        let mut heading_level = 0usize;
        for i in (0..=cursor_line.min(lines.len() - 1)).rev() {
            if let Some(level) = heading_level_of(lines[i]) {
                heading_line = Some(i);
                heading_level = level;
                break;
            }
        }
        let heading_line = heading_line?;

        // Find end: next heading of same or higher level, or EOF.
        let mut end_line = lines.len();
        for i in (heading_line + 1)..lines.len() {
            if let Some(level) = heading_level_of(lines[i]) {
                if level <= heading_level {
                    end_line = i;
                    break;
                }
            }
        }

        // Convert line ranges to byte offsets.
        let heading_byte = self.buffer.line_to_byte(heading_line);
        let content_start_byte = if heading_line + 1 < lines.len() {
            self.buffer.line_to_byte(heading_line + 1)
        } else {
            text.len()
        };
        let end_byte = if end_line < lines.len() {
            self.buffer.line_to_byte(end_line)
        } else {
            text.len()
        };

        if kind == 'i' {
            Some((content_start_byte, end_byte))
        } else {
            Some((heading_byte, end_byte))
        }
    }
    // ── Session persistence helpers ────────────────────────────────────

    /// Collect current editor state into a serialisable `SessionData`.
    pub fn save_session_data(&self) -> crate::session::SessionData {
        let mut buffers = Vec::new();
        let mut cursors = std::collections::HashMap::new();
        let mut scroll_offsets = std::collections::HashMap::new();

        // Current (active) buffer first.
        if let Some(ref p) = self.buffer.file_path {
            let key = p.to_string_lossy().to_string();
            buffers.push(key.clone());
            cursors.insert(key.clone(), self.cursor);
            scroll_offsets.insert(key, self.scroll_offset);
        }

        // Background buffers.
        for buf in &self.open_buffers {
            if let Some(ref p) = buf.file_path {
                let key = p.to_string_lossy().to_string();
                if !buffers.contains(&key) {
                    buffers.push(key);
                }
            }
        }

        crate::session::SessionData {
            active_buffer: self.buffer.file_path.as_ref().map(|p| p.to_string_lossy().to_string()),
            buffers,
            cursors,
            scroll_offsets,
            layout: crate::session::SessionLayout::Single,
            theme: self.theme.name.to_string(),
        }
    }

    /// Restore editor state from previously saved `SessionData`.
    pub fn restore_session(&mut self, data: &crate::session::SessionData) {
        // Set theme.
        self.set_theme_by_name(&data.theme);

        // Load buffers.  The active buffer becomes self.buffer; others go
        // into open_buffers.
        let mut loaded: Vec<(String, crate::buffer::Buffer)> = Vec::new();
        for path_str in &data.buffers {
            let path = std::path::Path::new(path_str);
            if path.exists() {
                if let Ok(buf) = crate::buffer::Buffer::from_file(path) {
                    loaded.push((path_str.clone(), buf));
                }
            }
        }

        // Determine which buffer is active.
        let active_key = data.active_buffer.clone();
        let mut active_set = false;

        for (key, buf) in loaded {
            if !active_set && active_key.as_deref() == Some(key.as_str()) {
                self.buffer = buf;
                self.cursor = data.cursors.get(&key).copied().unwrap_or(0);
                self.scroll_offset = data.scroll_offsets.get(&key).copied().unwrap_or(0);
                active_set = true;
            } else {
                self.open_buffers.push(buf);
            }
        }
    }
}

fn heading_level_of(line: &str) -> Option<usize> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with('#') { return None; }
    let level = trimmed.chars().take_while(|&c| c == '#').count();
    if level > 0 && trimmed.len() > level && trimmed.as_bytes()[level] == b' ' {
        Some(level)
    } else {
        None
    }
}

fn is_word_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn is_punct(b: u8) -> bool {
    !b.is_ascii_alphanumeric() && b != b'_' && !b.is_ascii_whitespace()
}

/// Count lines consumed by YAML frontmatter (including the closing `---`).
fn frontmatter_line_count(text: &str) -> usize {
    let after_open = if let Some(s) = text.strip_prefix("---\n") {
        s
    } else if let Some(s) = text.strip_prefix("---\r\n") {
        s
    } else {
        return 0;
    };
    let mut count = 1; // the opening "---"
    for line in after_open.split_inclusive('\n') {
        count += 1;
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if trimmed == "---" {
            return count;
        }
    }
    0
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_keys(
        initial_text: &str,
        initial_cursor: usize,
        keys: &[Key],
        expected_text: &str,
        expected_cursor: usize,
        expected_mode: Mode,
    ) {
        let mut state = EditorState::new(initial_text);
        state.cursor = initial_cursor;
        state.feed_keys(keys);
        assert_eq!(
            state.text(),
            expected_text,
            "text mismatch after keys {keys:?}"
        );
        assert_eq!(
            state.cursor, expected_cursor,
            "cursor mismatch after keys {keys:?}"
        );
        assert_eq!(
            state.mode, expected_mode,
            "mode mismatch after keys {keys:?}"
        );
    }

    fn keys(s: &str) -> Vec<Key> {
        s.chars()
            .map(|c| match c {
                '\x1b' => Key::Escape,
                '\n' => Key::Enter,
                _ => Key::Char(c),
            })
            .collect()
    }

    // ── Mode transitions ────────────────────────────────────────────────

    #[test]
    fn normal_mode_i_enters_insert() {
        assert_keys("hello", 0, &keys("i"), "hello", 0, Mode::Insert);
    }

    #[test]
    fn insert_mode_escape_returns_to_normal() {
        // i enters insert at pos 0, Esc returns to normal
        let mut state = EditorState::new("hello");
        state.feed_keys(&keys("i\x1b"));
        assert_eq!(state.mode, Mode::Normal);
    }

    #[test]
    fn normal_a_enters_insert_after_cursor() {
        let mut state = EditorState::new("hello");
        state.cursor = 0;
        state.feed_keys(&keys("a"));
        assert_eq!(state.mode, Mode::Insert);
        assert_eq!(state.cursor, 1);
    }

    #[test]
    fn normal_A_appends_at_eol() {
        let mut state = EditorState::new("hello");
        state.cursor = 0;
        state.feed_keys(&keys("A"));
        assert_eq!(state.mode, Mode::Insert);
        assert_eq!(state.cursor, 5); // after 'o'
    }

    #[test]
    fn normal_o_opens_line_below() {
        let mut state = EditorState::new("hello\nworld");
        state.cursor = 0;
        state.feed_keys(&keys("o"));
        assert_eq!(state.mode, Mode::Insert);
        assert!(state.text().starts_with("hello\n"));
        assert!(state.text().contains("\nworld"));
    }

    #[test]
    fn normal_colon_enters_command_mode() {
        let mut state = EditorState::new("test");
        state.feed_keys(&keys(":"));
        assert_eq!(state.mode, Mode::Command);
    }

    #[test]
    fn normal_v_enters_visual_mode() {
        let mut state = EditorState::new("test");
        state.feed_keys(&keys("v"));
        assert_eq!(state.mode, Mode::Visual);
    }

    // ── Motions ─────────────────────────────────────────────────────────

    #[test]
    fn h_moves_left() {
        let mut state = EditorState::new("hello");
        state.cursor = 3;
        state.feed_keys(&keys("h"));
        assert_eq!(state.cursor, 2);
    }

    #[test]
    fn l_moves_right() {
        let mut state = EditorState::new("hello");
        state.cursor = 0;
        state.feed_keys(&keys("l"));
        assert_eq!(state.cursor, 1);
    }

    #[test]
    fn j_moves_down() {
        let mut state = EditorState::new("hello\nworld");
        state.cursor = 0;
        state.feed_keys(&keys("j"));
        assert_eq!(state.cursor_row_col().0, 1);
    }

    #[test]
    fn k_moves_up() {
        let mut state = EditorState::new("hello\nworld");
        state.cursor = 6; // start of "world"
        state.feed_keys(&keys("k"));
        assert_eq!(state.cursor_row_col().0, 0);
    }

    #[test]
    fn w_moves_to_next_word() {
        let mut state = EditorState::new("hello world");
        state.cursor = 0;
        state.feed_keys(&keys("w"));
        assert_eq!(state.cursor, 6);
    }

    #[test]
    fn b_moves_to_prev_word() {
        let mut state = EditorState::new("hello world");
        state.cursor = 6;
        state.feed_keys(&keys("b"));
        assert_eq!(state.cursor, 0);
    }

    #[test]
    fn zero_moves_to_line_start() {
        let mut state = EditorState::new("hello");
        state.cursor = 3;
        state.feed_keys(&keys("0"));
        assert_eq!(state.cursor, 0);
    }

    #[test]
    fn dollar_moves_to_line_end() {
        let mut state = EditorState::new("hello");
        state.cursor = 0;
        state.feed_keys(&keys("$"));
        assert_eq!(state.cursor, 4); // last char index
    }

    #[test]
    fn gg_goes_to_first_line() {
        let mut state = EditorState::new("aaa\nbbb\nccc");
        state.cursor = 8; // on "ccc"
        state.feed_keys(&keys("gg"));
        assert_eq!(state.cursor_row_col().0, 0);
    }

    #[test]
    fn G_goes_to_last_line() {
        let mut state = EditorState::new("aaa\nbbb\nccc");
        state.cursor = 0;
        state.feed_keys(&keys("G"));
        assert_eq!(state.cursor_row_col().0, 2);
    }

    #[test]
    fn count_prefix_3j_moves_down_3() {
        let mut state = EditorState::new("a\nb\nc\nd\ne");
        state.cursor = 0;
        state.feed_keys(&keys("3j"));
        assert_eq!(state.cursor_row_col().0, 3);
    }

    // ── Single-key actions ──────────────────────────────────────────────

    #[test]
    fn x_deletes_char_at_cursor() {
        assert_keys("hello", 0, &keys("x"), "ello", 0, Mode::Normal);
    }

    #[test]
    fn count_3x_deletes_three_chars() {
        assert_keys("hello", 0, &keys("3x"), "lo", 0, Mode::Normal);
    }

    #[test]
    fn u_undoes_last_change() {
        let mut state = EditorState::new("hello");
        state.feed_keys(&keys("x")); // delete 'h'
        assert_eq!(state.text(), "ello");
        state.feed_keys(&keys("u")); // undo
        assert_eq!(state.text(), "hello");
    }

    #[test]
    fn ctrl_r_redoes() {
        let mut state = EditorState::new("hello");
        state.feed_keys(&keys("x")); // delete 'h'
        state.feed_keys(&keys("u")); // undo
        state.feed_keys(&[Key::Ctrl('r')]); // redo
        assert_eq!(state.text(), "ello");
    }

    // ── Operators ───────────────────────────────────────────────────────

    #[test]
    fn dw_deletes_word() {
        assert_keys("hello world", 0, &keys("dw"), "world", 0, Mode::Normal);
    }

    #[test]
    fn dd_deletes_line() {
        let mut state = EditorState::new("hello\nworld");
        state.cursor = 0;
        state.feed_keys(&keys("dd"));
        assert_eq!(state.text(), "world");
    }

    #[test]
    fn cw_changes_word() {
        let mut state = EditorState::new("hello world");
        state.cursor = 0;
        state.feed_keys(&keys("cw"));
        assert_eq!(state.mode, Mode::Insert);
        // "hello " deleted, cursor at start
        assert_eq!(state.text(), "world");
    }

    #[test]
    fn d_dollar_deletes_to_eol() {
        let mut state = EditorState::new("hello world");
        state.cursor = 5; // on space
        state.feed_keys(&keys("d$"));
        // Delete from pos 5 to end of content
        let text = state.text();
        assert_eq!(text, "hello");
    }

    // ── Insert mode editing ─────────────────────────────────────────────

    #[test]
    fn insert_typing_adds_text() {
        let mut state = EditorState::new("");
        state.feed_keys(&keys("i"));
        state.feed_keys(&keys("abc"));
        assert_eq!(state.text(), "abc");
        assert_eq!(state.cursor, 3);
    }

    #[test]
    fn insert_enter_splits_line() {
        let mut state = EditorState::new("helloworld");
        state.cursor = 0;
        state.feed_keys(&keys("i"));
        state.cursor = 5;
        state.feed_keys(&[Key::Enter]);
        assert_eq!(state.text(), "hello\nworld");
    }

    #[test]
    fn insert_backspace_deletes_behind() {
        let mut state = EditorState::new("hello");
        state.cursor = 0;
        state.feed_keys(&keys("i"));
        state.cursor = 3;
        state.feed_keys(&[Key::Backspace]);
        assert_eq!(state.text(), "helo");
        assert_eq!(state.cursor, 2);
    }

    // ── Visual mode ─────────────────────────────────────────────────────

    #[test]
    fn visual_d_deletes_selection() {
        let mut state = EditorState::new("hello world");
        state.cursor = 0;
        state.feed_keys(&keys("v")); // enter visual
        state.feed_keys(&keys("lllld")); // select "hello" then delete
        assert_eq!(state.text(), " world");
        assert_eq!(state.mode, Mode::Normal);
    }

    #[test]
    fn visual_escape_cancels() {
        let mut state = EditorState::new("hello");
        state.feed_keys(&keys("v\x1b"));
        assert_eq!(state.mode, Mode::Normal);
    }

    // ── Command mode ────────────────────────────────────────────────────

    #[test]
    fn command_q_returns_quit() {
        let mut state = EditorState::new("test");
        state.feed_keys(&keys(":"));
        let result = state.handle_key(Key::Char('q'));
        assert_eq!(result, KeyResult::Handled); // just typing 'q'
        let result = state.handle_key(Key::Enter);
        assert_eq!(result, KeyResult::Quit);
    }

    #[test]
    fn command_w_returns_save() {
        let mut state = EditorState::new("test");
        state.feed_keys(&keys(":"));
        state.feed_keys(&keys("w"));
        let result = state.handle_key(Key::Enter);
        assert_eq!(result, KeyResult::Save);
    }

    #[test]
    fn command_wq_returns_save_and_quit() {
        let mut state = EditorState::new("test");
        state.feed_keys(&keys(":"));
        state.feed_keys(&keys("wq"));
        let result = state.handle_key(Key::Enter);
        assert_eq!(result, KeyResult::SaveAndQuit);
    }

    #[test]
    fn command_escape_cancels() {
        let mut state = EditorState::new("test");
        state.feed_keys(&keys(":"));
        state.feed_keys(&keys("w\x1b"));
        assert_eq!(state.mode, Mode::Normal);
        assert!(state.command_input.is_empty());
    }

    #[test]
    fn command_goto_line() {
        let mut state = EditorState::new("aaa\nbbb\nccc");
        state.feed_keys(&keys(":"));
        state.feed_keys(&keys("3"));
        state.handle_key(Key::Enter);
        assert_eq!(state.cursor_row_col().0, 2);
    }

    #[test]
    fn command_theme_cycles_through_builtin_themes() {
        let mut state = EditorState::new("hello");
        assert_eq!(state.theme.name, "bloom-dark");
        state.feed_keys(&keys(":"));
        for c in "theme".chars() { state.handle_key(Key::Char(c)); }
        state.handle_key(Key::Enter);
        assert_eq!(state.theme.name, "bloom-dark-faded");

        state.feed_keys(&keys(":"));
        for c in "theme".chars() { state.handle_key(Key::Char(c)); }
        state.handle_key(Key::Enter);
        assert_eq!(state.theme.name, "bloom-light");

        state.feed_keys(&keys(":"));
        for c in "theme".chars() { state.handle_key(Key::Char(c)); }
        state.handle_key(Key::Enter);
        assert_eq!(state.theme.name, "bloom-light-faded");

        state.feed_keys(&keys(":"));
        for c in "theme".chars() { state.handle_key(Key::Char(c)); }
        state.handle_key(Key::Enter);
        assert_eq!(state.theme.name, "bloom-dark");
    }

    #[test]
    fn command_theme_name_selects_specific_theme() {
        let mut state = EditorState::new("hello");
        state.feed_keys(&keys(":"));
        for c in "theme bloom-light".chars() { state.handle_key(Key::Char(c)); }
        state.handle_key(Key::Enter);
        assert_eq!(state.theme.name, "bloom-light");
    }

    // ── Replace ─────────────────────────────────────────────────────────

    #[test]
    fn r_replaces_single_char() {
        let mut state = EditorState::new("hello");
        state.cursor = 0;
        state.feed_keys(&keys("rH"));
        assert_eq!(state.text(), "Hello");
        assert_eq!(state.mode, Mode::Normal);
    }

    // ── Join lines ──────────────────────────────────────────────────────

    #[test]
    fn J_joins_current_and_next_line() {
        let mut state = EditorState::new("hello\nworld");
        state.cursor = 0;
        state.feed_keys(&keys("J"));
        assert_eq!(state.text(), "hello world");
    }

    // ── RenderFrame tests ───────────────────────────────────────────────

    #[test]
    fn render_produces_frame_with_content() {
        let state = EditorState::new("Hello\nWorld\nThird line");
        let frame = state.render();
        let pane = frame.focused_pane().unwrap();

        assert_eq!(pane.lines[0].text, "Hello");
        assert_eq!(pane.lines[0].line_number, Some(0));
        assert_eq!(pane.lines[1].text, "World");
        assert_eq!(pane.lines[2].text, "Third line");
        assert_eq!(pane.lines[3].text, "~");
        assert!(pane.lines[3].line_number.is_none());
    }

    #[test]
    fn render_status_bar_reflects_state() {
        let state = EditorState::new("test");
        let frame = state.render();
        let status = frame.status().unwrap();

        assert_eq!(status.mode, "NORMAL");
        assert_eq!(status.filename, "[No Name]");
        assert!(!status.dirty);
        assert_eq!(status.position, "1:1");
        assert_eq!(status.filetype, "markdown");
    }

    #[test]
    fn render_cursor_shape_changes_with_mode() {
        use crate::render::CursorShape;

        let mut state = EditorState::new("test");
        let frame = state.render();
        assert_eq!(
            frame.focused_pane().unwrap().cursor.shape,
            CursorShape::Block
        );

        state.mode = Mode::Insert;
        let frame = state.render();
        assert_eq!(frame.focused_pane().unwrap().cursor.shape, CursorShape::Bar);
    }

    #[test]
    fn render_command_line_in_command_mode() {
        let mut state = EditorState::new("test");
        state.mode = Mode::Command;
        state.command_input = "w".into();
        let frame = state.render();
        assert_eq!(frame.command_line.as_deref(), Some(":w"));
    }

    #[test]
    fn render_tilde_fills_viewport() {
        let mut state = EditorState::new("one line");
        state.viewport_height = 5;
        let frame = state.render();
        let pane = frame.focused_pane().unwrap();
        assert_eq!(pane.lines.len(), 5);
        assert_eq!(pane.lines[0].text, "one line");
        for i in 1..5 {
            assert_eq!(pane.lines[i].text, "~");
        }
    }

    #[test]
    fn render_empty_buffer() {
        let mut state = EditorState::new("");
        state.viewport_height = 3;
        let frame = state.render();
        let pane = frame.focused_pane().unwrap();
        assert_eq!(pane.lines[0].text, "");
        assert_eq!(pane.lines[0].line_number, Some(0));
        assert_eq!(pane.lines[1].text, "~");
        assert_eq!(pane.lines[2].text, "~");
    }

    #[test]
    fn render_reflects_mode_after_key() {
        let mut state = EditorState::new("test");
        state.handle_key(Key::Char('i'));
        let frame = state.render();
        assert_eq!(frame.status().unwrap().mode, "INSERT");
    }

    #[test]
    fn render_reflects_cursor_movement() {
        let mut state = EditorState::new("hello\nworld");
        state.handle_key(Key::Char('j'));
        let frame = state.render();
        let pane = frame.focused_pane().unwrap();
        assert_eq!(pane.cursor.row, 1);
    }

    #[test]
    fn render_dirty_flag_after_edit() {
        let mut state = EditorState::new("hello");
        state.handle_key(Key::Char('x'));
        let frame = state.render();
        assert!(frame.status().unwrap().dirty);
    }

    // ── Leader / which-key tests ────────────────────────────────────────

    #[test]
    fn spc_enters_leader_mode() {
        let mut state = EditorState::new("hello");
        let result = state.handle_key(Key::Char(' '));
        assert_eq!(result, KeyResult::Pending);
        assert!(state.leader_active);
        assert_eq!(state.mode, Mode::Normal);
    }

    #[test]
    fn spc_unknown_key_resets_leader() {
        let mut state = EditorState::new("hello");
        state.handle_key(Key::Char(' '));
        let result = state.handle_key(Key::Char('z'));
        assert_eq!(result, KeyResult::Handled);
        assert!(!state.leader_active);
        assert!(state.last_leader_action.is_none());
    }

    #[test]
    fn spc_escape_cancels_leader() {
        let mut state = EditorState::new("hello");
        state.handle_key(Key::Char(' '));
        let result = state.handle_key(Key::Escape);
        assert_eq!(result, KeyResult::Handled);
        assert!(!state.leader_active);
        assert_eq!(state.mode, Mode::Normal);
    }

    #[test]
    fn which_key_frame_populated_during_leader_pending() {
        let mut state = EditorState::new("hello");
        state.handle_key(Key::Char(' '));
        let frame = state.render();
        assert!(frame.which_key.is_some(), "which_key should be Some when leader is pending");
        let wk = frame.which_key.unwrap();
        assert!(!wk.entries.is_empty());
        let keys: Vec<&str> = wk.entries.iter().map(|e| e.key.as_str()).collect();
        assert!(keys.contains(&"f"), "missing 'f' (file) group");
        assert!(keys.contains(&"w"), "missing 'w' (window) group");
    }

    #[test]
    fn leader_full_sequence_dispatches_action() {
        let mut state = EditorState::new("hello");
        state.handle_key(Key::Char(' '));
        let r = state.handle_key(Key::Char('f'));
        assert_eq!(r, KeyResult::Pending);
        assert!(state.leader_active);
        let r = state.handle_key(Key::Char('f'));
        assert_eq!(r, KeyResult::Handled);
        assert!(!state.leader_active);
        assert_eq!(state.last_leader_action, Some(LeaderAction::FindFile));
    }

    #[test]
    fn leader_spc_spc_dispatches_all_commands() {
        let mut state = EditorState::new("hello");
        state.handle_key(Key::Char(' '));
        let r = state.handle_key(Key::Char(' '));
        assert_eq!(r, KeyResult::Handled);
        assert_eq!(state.last_leader_action, Some(LeaderAction::AllCommands));
    }

    #[test]
    fn which_key_frame_none_when_not_in_leader() {
        let state = EditorState::new("hello");
        let frame = state.render();
        assert!(frame.which_key.is_none());
    }

    #[test]
    fn leader_non_char_key_resets() {
        let mut state = EditorState::new("hello");
        state.handle_key(Key::Char(' '));
        let r = state.handle_key(Key::Enter);
        assert_eq!(r, KeyResult::Handled);
        assert!(!state.leader_active);
    }

    // ── Picker wiring ───────────────────────────────────────────────────

    #[test]
    fn leader_ff_opens_find_page_picker() {
        let mut state = EditorState::new("hello");
        state.handle_key(Key::Char(' '));
        state.handle_key(Key::Char('f'));
        state.handle_key(Key::Char('f'));
        assert!(state.active_picker.is_some());
        assert_eq!(state.active_picker.as_ref().unwrap().title, "Find Page");
    }

    #[test]
    fn leader_spc_spc_opens_all_commands_picker() {
        let mut state = EditorState::new("hello");
        state.handle_key(Key::Char(' '));
        state.handle_key(Key::Char(' '));
        assert!(state.active_picker.is_some());
        assert_eq!(state.active_picker.as_ref().unwrap().title, "All Commands");
    }

    #[test]
    fn all_commands_picker_shows_preview_help() {
        let mut state = EditorState::new("hello");
        state.handle_key(Key::Char(' '));
        state.handle_key(Key::Char(' '));
        let frame = state.render();
        let picker = frame.picker.unwrap();
        let preview = picker.preview.unwrap();
        assert!(
            preview.iter().any(|line| line.text.contains("Command:")),
            "expected command help preview"
        );
    }

    #[test]
    fn all_commands_enter_executes_selected_command() {
        let mut state = EditorState::new("hello");
        state.handle_key(Key::Char(' '));
        state.handle_key(Key::Char(' '));
        for c in "Find file".chars() {
            state.handle_key(Key::Char(c));
        }
        state.handle_key(Key::Enter);
        assert_eq!(state.last_leader_action, Some(LeaderAction::FindFile));
        assert!(state.active_picker.is_some());
        assert_eq!(state.active_picker.as_ref().unwrap().title, "Find Page");
    }

    #[test]
    fn leader_bb_opens_switch_buffer_picker() {
        let mut state = EditorState::new("# Hello\nWorld");
        state.buffer.file_path = Some(PathBuf::from("/tmp/vault/journal/2026-03-01.md"));
        send_leader_seq(&mut state, &['b', 'b']);
        assert!(state.active_picker.is_some());
        let ap = state.active_picker.as_ref().unwrap();
        assert_eq!(ap.title, "Switch Buffer");
        // Current buffer should appear in results with journal label.
        let results = ap.inner.results();
        assert_eq!(results.len(), 1);
        assert!(results[0].text.contains("journal"));
        // Marginalia should show "active".
        assert!(results[0].marginalia.contains("active"));
        // Preview should be populated from buffer content.
        assert!(ap.preview.is_some());
        let preview = ap.preview.as_ref().unwrap();
        assert_eq!(preview[0].text, "# Hello");
    }

    #[test]
    fn leader_ss_opens_fulltext_search_picker() {
        let mut state = EditorState::new("hello");
        send_leader_seq(&mut state, &['s', 's']);
        assert!(state.active_picker.is_some());
        assert_eq!(state.active_picker.as_ref().unwrap().title, "Search");
    }

    #[test]
    fn leader_st_opens_search_tags_picker() {
        use tempfile::TempDir;
        let tmp = TempDir::new().unwrap();
        let _store = LocalFileStore::new(tmp.path().to_path_buf()).unwrap();
        let db_path = tmp.path().join(".index").join("core.db");
        let mut index = crate::index::SqliteIndex::open(&db_path).unwrap();
        let content = "---\nid: tt112233\ntitle: \"Tagged Page\"\ncreated: 2026-02-28T00:00:00Z\ntags: [rust]\n---\n\nSome #rust content.";
        let path = tmp.path().join("pages").join("Tagged Page.md");
        std::fs::write(&path, content).unwrap();
        let doc = crate::parser::parse(content).unwrap();
        index.index_document(&path, &doc).unwrap();

        let mut state = EditorState::new("hello");
        state.index = Some(index);
        send_leader_seq(&mut state, &['s', 't']);
        assert!(state.active_picker.is_some());
        assert_eq!(state.active_picker.as_ref().unwrap().title, "Search Tags");
        assert!(state.active_picker.as_ref().unwrap().preview.is_some());
    }

    #[test]
    fn leader_sj_opens_search_journal_picker() {
        use tempfile::TempDir;
        let tmp = TempDir::new().unwrap();
        let _store = LocalFileStore::new(tmp.path().to_path_buf()).unwrap();
        let db_path = tmp.path().join(".index").join("core.db");
        let index = crate::index::SqliteIndex::open(&db_path).unwrap();

        let mut state = EditorState::new("hello");
        state.index = Some(index);
        send_leader_seq(&mut state, &['s', 'j']);
        assert!(state.active_picker.is_some());
        assert_eq!(state.active_picker.as_ref().unwrap().title, "Search Journal");
    }

    #[test]
    fn leader_sl_opens_backlinks_picker() {
        use tempfile::TempDir;
        let tmp = TempDir::new().unwrap();
        let _store = LocalFileStore::new(tmp.path().to_path_buf()).unwrap();
        let db_path = tmp.path().join(".index").join("core.db");
        let index = crate::index::SqliteIndex::open(&db_path).unwrap();

        let mut state = EditorState::new("---\nid: abcd1234\ntitle: \"Test\"\n---\n\nBody");
        state.index = Some(index);
        send_leader_seq(&mut state, &['s', 'l']);
        assert!(state.active_picker.is_some());
        let title = &state.active_picker.as_ref().unwrap().title;
        assert!(title.starts_with("Backlinks"), "expected Backlinks title, got: {title}");
    }

    #[test]
    fn leader_su_opens_unlinked_mentions_picker() {
        use tempfile::TempDir;
        let tmp = TempDir::new().unwrap();
        let _store = LocalFileStore::new(tmp.path().to_path_buf()).unwrap();
        let db_path = tmp.path().join(".index").join("core.db");
        let index = crate::index::SqliteIndex::open(&db_path).unwrap();

        let mut state = EditorState::new("---\nid: abcd1234\ntitle: \"Test Page\"\n---\n\nBody");
        state.index = Some(index);
        send_leader_seq(&mut state, &['s', 'u']);
        assert!(state.active_picker.is_some());
        let title = &state.active_picker.as_ref().unwrap().title;
        assert!(title.contains("Unlinked Mentions"), "expected Unlinked Mentions title, got: {title}");
        assert!(state.active_picker.as_ref().unwrap().supports_batch_select);
    }

    #[test]
    fn fulltext_search_source_returns_hits_from_index() {
        use tempfile::TempDir;
        let tmp = TempDir::new().unwrap();
        let _store = LocalFileStore::new(tmp.path().to_path_buf()).unwrap();
        let db_path = tmp.path().join(".index").join("core.db");
        let mut index = crate::index::SqliteIndex::open(&db_path).unwrap();

        // Index a document with searchable content.
        let doc = crate::parser::parse(
            "---\nid: aabb1122\ntitle: \"Rope Buffers\"\ncreated: 2026-02-28T00:00:00Z\ntags: []\n---\n\nRopes are O(log n) for inserts."
        ).unwrap();
        let path = tmp.path().join("pages").join("Rope Buffers.md");
        index.index_document(&path, &doc).unwrap();

        let source = FullTextSearchSource::from_index(&index, "rope").unwrap();
        assert!(!source.items().is_empty());
        assert_eq!(source.items()[0].page_title, "Rope Buffers");
    }

    // ── Picker Enter behaviour tests ────────────────────────────────────

    /// Helper: create a vault with an indexed page, return (tmp, state).
    fn setup_vault_with_page(title: &str, content: &str) -> (tempfile::TempDir, EditorState) {
        use tempfile::TempDir;
        let tmp = TempDir::new().unwrap();
        let _store = LocalFileStore::new(tmp.path().to_path_buf()).unwrap();
        let db_path = tmp.path().join(".index").join("core.db");
        let mut index = crate::index::SqliteIndex::open(&db_path).unwrap();

        let filename = format!("{}.md", store::sanitize_filename(title));
        let path = tmp.path().join("pages").join(&filename);
        std::fs::write(&path, content).unwrap();
        let doc = crate::parser::parse(content).unwrap();
        index.index_document(&path, &doc).unwrap();

        let mut state = EditorState::new("initial buffer");
        state.vault_root = Some(tmp.path().to_path_buf());
        state.index = Some(index);
        (tmp, state)
    }

    #[test]
    fn find_page_enter_opens_selected_page() {
        let content = "---\nid: aabb1122\ntitle: \"Rope Buffers\"\ncreated: 2026-02-28T00:00:00Z\ntags: []\n---\n\nRopes are great.";
        let (_tmp, mut state) = setup_vault_with_page("Rope Buffers", content);

        // Open Find Page picker.
        send_leader_seq(&mut state, &['f', 'f']);
        assert!(state.active_picker.is_some());
        assert_eq!(state.active_picker.as_ref().unwrap().title, "Find Page");

        // Results should include our page.
        let results = state.active_picker.as_ref().unwrap().inner.results();
        assert!(!results.is_empty(), "expected at least one result");
        assert!(results.iter().any(|r| r.text.contains("Rope")));

        // Press Enter to open.
        state.handle_key(Key::Enter);
        assert!(state.active_picker.is_none(), "picker should close");
        assert!(state.text().contains("Ropes are great"), "buffer should contain page content");
        assert!(state.buffer.file_path.is_some());
    }

    #[test]
    fn search_enter_opens_selected_page() {
        let content = "---\nid: ccdd3344\ntitle: \"Search Target\"\ncreated: 2026-02-28T00:00:00Z\ntags: []\n---\n\nFindable content here.";
        let (_tmp, mut state) = setup_vault_with_page("Search Target", content);

        send_leader_seq(&mut state, &['s', 's']);
        assert!(state.active_picker.is_some());
        assert_eq!(state.active_picker.as_ref().unwrap().title, "Search");

        for c in "Findable".chars() {
            state.handle_key(Key::Char(c));
        }
        assert!(!state.active_picker.as_ref().unwrap().inner.is_empty());
        state.handle_key(Key::Enter);
        assert!(state.active_picker.is_none());
        assert!(state.text().contains("Findable content here"));
    }

    #[test]
    fn search_journal_enter_opens_journal_page() {
        use tempfile::TempDir;
        let tmp = TempDir::new().unwrap();
        let _store = LocalFileStore::new(tmp.path().to_path_buf()).unwrap();
        let db_path = tmp.path().join(".index").join("core.db");
        let mut index = crate::index::SqliteIndex::open(&db_path).unwrap();

        let content = "---\nid: jj112233\ntitle: \"2026-03-01\"\ncreated: 2026-03-01T00:00:00Z\ntags: []\n---\n\nJournal entry.";
        let path = tmp.path().join("journal").join("2026-03-01.md");
        std::fs::write(&path, content).unwrap();
        let doc = crate::parser::parse(content).unwrap();
        index.index_document(&path, &doc).unwrap();

        let mut state = EditorState::new("initial");
        state.vault_root = Some(tmp.path().to_path_buf());
        state.index = Some(index);

        send_leader_seq(&mut state, &['s', 'j']);
        assert!(state.active_picker.is_some());
        assert_eq!(state.active_picker.as_ref().unwrap().title, "Search Journal");

        state.handle_key(Key::Enter);
        assert!(state.active_picker.is_none());
        assert!(state.text().contains("Journal entry"));
    }

    #[test]
    fn switch_buffer_enter_opens_selected_buffer() {
        use tempfile::TempDir;
        let tmp = TempDir::new().unwrap();
        let _store = LocalFileStore::new(tmp.path().to_path_buf()).unwrap();

        let page_a = tmp.path().join("pages").join("A.md");
        let page_b = tmp.path().join("pages").join("B.md");
        std::fs::write(&page_a, "Page A content").unwrap();
        std::fs::write(&page_b, "Page B content").unwrap();

        // Start with page A open.
        let mut state = EditorState::new("Page A content");
        state.buffer.file_path = Some(page_a.clone());
        state.vault_root = Some(tmp.path().to_path_buf());

        // Add page B as a pane state buffer.
        let pane_id = state.window_layout.split_vertical();
        state.pane_states.insert(pane_id, crate::editor::PaneState {
            buffer: {
                let mut b = Buffer::from_str("Page B content");
                b.file_path = Some(page_b.clone());
                b
            },
            cursor: 0,
            scroll_offset: 0,
        });

        // Open buffer picker, navigate to page B, press Enter.
        send_leader_seq(&mut state, &['b', 'b']);
        assert!(state.active_picker.is_some());

        // Page B should be at index 1. Navigate down.
        state.handle_key(Key::Ctrl('n'));
        state.handle_key(Key::Enter);

        assert!(state.active_picker.is_none());
        assert_eq!(state.text(), "Page B content");
        assert_eq!(state.buffer.file_path.as_deref(), Some(page_b.as_path()));
    }

    #[test]
    fn search_tags_enter_transitions_to_filtered_find_page() {
        use tempfile::TempDir;
        let tmp = TempDir::new().unwrap();
        let _store = LocalFileStore::new(tmp.path().to_path_buf()).unwrap();
        let db_path = tmp.path().join(".index").join("core.db");
        let mut index = crate::index::SqliteIndex::open(&db_path).unwrap();

        let content = "---\nid: tt112233\ntitle: \"Tagged Page\"\ncreated: 2026-02-28T00:00:00Z\ntags: [rust]\n---\n\nSome #rust content.";
        let path = tmp.path().join("pages").join("Tagged Page.md");
        std::fs::write(&path, content).unwrap();
        let doc = crate::parser::parse(content).unwrap();
        index.index_document(&path, &doc).unwrap();

        let mut state = EditorState::new("initial");
        state.vault_root = Some(tmp.path().to_path_buf());
        state.index = Some(index);

        // Open Search Tags picker.
        send_leader_seq(&mut state, &['s', 't']);
        assert!(state.active_picker.is_some());
        assert_eq!(state.active_picker.as_ref().unwrap().title, "Search Tags");

        // Press Enter to transition to Find Page with tag filter.
        state.handle_key(Key::Enter);

        // Should now be a Find Page picker with a tag filter pill.
        assert!(state.active_picker.is_some());
        assert_eq!(state.active_picker.as_ref().unwrap().title, "Find Page");
        assert!(!state.active_picker.as_ref().unwrap().filter_pills.is_empty());
        assert_eq!(state.active_picker.as_ref().unwrap().filter_pills[0].0, "tag");
    }

    #[test]
    fn backlinks_enter_opens_source_page() {
        use tempfile::TempDir;
        let tmp = TempDir::new().unwrap();
        let _store = LocalFileStore::new(tmp.path().to_path_buf()).unwrap();
        let db_path = tmp.path().join(".index").join("core.db");
        let mut index = crate::index::SqliteIndex::open(&db_path).unwrap();

        // Target page.
        let target = "---\nid: tgt11111\ntitle: \"Target\"\ncreated: 2026-02-28T00:00:00Z\ntags: []\n---\n\nTarget page.";
        let target_path = tmp.path().join("pages").join("Target.md");
        std::fs::write(&target_path, target).unwrap();
        let target_doc = crate::parser::parse(target).unwrap();
        index.index_document(&target_path, &target_doc).unwrap();

        // Source page that links to target.
        let source = "---\nid: src22222\ntitle: \"Source\"\ncreated: 2026-02-28T00:00:00Z\ntags: []\n---\n\nSee [[tgt11111|Target]] for more.";
        let source_path = tmp.path().join("pages").join("Source.md");
        std::fs::write(&source_path, source).unwrap();
        let source_doc = crate::parser::parse(source).unwrap();
        index.index_document(&source_path, &source_doc).unwrap();

        // Open Target in the editor.
        let mut state = EditorState::new(target);
        state.buffer.file_path = Some(target_path);
        state.vault_root = Some(tmp.path().to_path_buf());
        state.index = Some(index);

        // Open backlinks picker.
        send_leader_seq(&mut state, &['s', 'l']);
        assert!(state.active_picker.is_some());
        let title = &state.active_picker.as_ref().unwrap().title;
        assert!(title.contains("Backlinks"), "got: {title}");

        // There should be one backlink from Source.
        let results = state.active_picker.as_ref().unwrap().inner.results();
        assert!(!results.is_empty(), "expected at least one backlink result");

        // Press Enter to open the source page.
        state.handle_key(Key::Enter);
        assert!(state.active_picker.is_none());
        assert!(state.text().contains("See [[tgt11111|Target]]"));
    }

    // ── Buffer list retention tests ─────────────────────────────────────

    #[test]
    fn opening_page_preserves_previous_buffer() {
        let content_a = "---\nid: aaaa1111\ntitle: \"Page A\"\ncreated: 2026-02-28T00:00:00Z\ntags: []\n---\n\nContent A.";
        let content_b = "---\nid: bbbb2222\ntitle: \"Page B\"\ncreated: 2026-02-28T00:00:00Z\ntags: []\n---\n\nContent B.";
        let (_tmp, mut state) = setup_vault_with_page("Page A", content_a);

        // Index page B too.
        let path_b = _tmp.path().join("pages").join("Page B.md");
        std::fs::write(&path_b, content_b).unwrap();
        let doc_b = crate::parser::parse(content_b).unwrap();
        state.index.as_mut().unwrap().index_document(&path_b, &doc_b).unwrap();

        // Open page A first.
        state.open_page_by_id("aaaa1111");
        assert!(state.text().contains("Content A"));

        // Open page B — page A should be preserved.
        state.open_page_by_id("bbbb2222");
        assert!(state.text().contains("Content B"));
        assert_eq!(state.open_buffers.len(), 1, "previous buffer should be stashed");
        assert_eq!(
            state.open_buffers[0].file_path.as_ref().unwrap().file_stem().unwrap().to_str().unwrap(),
            "Page A"
        );
    }

    #[test]
    fn buffer_picker_shows_all_visited_buffers() {
        let content_a = "---\nid: aaaa1111\ntitle: \"Page A\"\ncreated: 2026-02-28T00:00:00Z\ntags: []\n---\n\nA.";
        let content_b = "---\nid: bbbb2222\ntitle: \"Page B\"\ncreated: 2026-02-28T00:00:00Z\ntags: []\n---\n\nB.";
        let (_tmp, mut state) = setup_vault_with_page("Page A", content_a);

        let path_b = _tmp.path().join("pages").join("Page B.md");
        std::fs::write(&path_b, content_b).unwrap();
        let doc_b = crate::parser::parse(content_b).unwrap();
        state.index.as_mut().unwrap().index_document(&path_b, &doc_b).unwrap();

        // Visit A then B.
        state.open_page_by_id("aaaa1111");
        state.open_page_by_id("bbbb2222");

        // Open buffer picker — should show both.
        send_leader_seq(&mut state, &['b', 'b']);
        let results = state.active_picker.as_ref().unwrap().inner.results();
        assert_eq!(results.len(), 2, "should show current + background buffer");
    }

    #[test]
    fn switching_back_to_stashed_buffer_preserves_edits() {
        let content_a = "---\nid: aaaa1111\ntitle: \"Page A\"\ncreated: 2026-02-28T00:00:00Z\ntags: []\n---\n\nOriginal A.";
        let content_b = "---\nid: bbbb2222\ntitle: \"Page B\"\ncreated: 2026-02-28T00:00:00Z\ntags: []\n---\n\nB.";
        let (_tmp, mut state) = setup_vault_with_page("Page A", content_a);

        let path_b = _tmp.path().join("pages").join("Page B.md");
        std::fs::write(&path_b, content_b).unwrap();
        let doc_b = crate::parser::parse(content_b).unwrap();
        state.index.as_mut().unwrap().index_document(&path_b, &doc_b).unwrap();

        // Open A, make an edit.
        state.open_page_by_id("aaaa1111");
        let end = state.buffer.text().len();
        state.buffer.insert(end, "EDITED");
        assert!(state.text().contains("EDITED"));

        // Switch to B — A goes to open_buffers with the edit.
        state.open_page_by_id("bbbb2222");
        assert!(state.text().contains("B."));

        // Switch back to A — should still have the edit.
        state.open_page_by_id("aaaa1111");
        assert!(state.text().contains("EDITED"), "in-memory edit should be preserved");
    }

    #[test]
    fn picker_typing_updates_query() {
        use crate::picker::{FindPageItem, PickerSource};
        use std::borrow::Cow;

        struct TestPages {
            items: Vec<FindPageItem>,
        }
        impl PickerSource for TestPages {
            type Item = FindPageItem;
            fn items(&self) -> &[Self::Item] { &self.items }
            fn text(&self, item: &Self::Item) -> Cow<'_, str> { Cow::Owned(item.title.clone()) }
        }
        unsafe impl Send for TestPages {}
        unsafe impl Sync for TestPages {}

        let mut state = EditorState::new("hello");
        let source = TestPages {
            items: vec![
                FindPageItem { page_id: "a".into(), path: "a.md".into(), title: "Alpha".into(), tags: vec![], date_label: None },
                FindPageItem { page_id: "b".into(), path: "b.md".into(), title: "Beta".into(), tags: vec![], date_label: None },
                FindPageItem { page_id: "c".into(), path: "c.md".into(), title: "Gamma".into(), tags: vec![], date_label: None },
            ],
        };
        state.open_picker("Test", source);
        assert_eq!(state.active_picker.as_ref().unwrap().inner.results().len(), 3);

        // Type "al" to filter
        state.handle_key(Key::Char('a'));
        state.handle_key(Key::Char('l'));
        assert_eq!(state.active_picker.as_ref().unwrap().inner.query(), "al");
        let results = state.active_picker.as_ref().unwrap().inner.results();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].text, "Alpha");
    }

    #[test]
    fn picker_ctrl_j_moves_selection_down() {
        use crate::picker::{FindPageItem, PickerSource};
        use std::borrow::Cow;

        struct TestPages { items: Vec<FindPageItem> }
        impl PickerSource for TestPages {
            type Item = FindPageItem;
            fn items(&self) -> &[Self::Item] { &self.items }
            fn text(&self, item: &Self::Item) -> Cow<'_, str> { Cow::Owned(item.title.clone()) }
        }
        unsafe impl Send for TestPages {}
        unsafe impl Sync for TestPages {}

        let mut state = EditorState::new("hello");
        let source = TestPages {
            items: vec![
                FindPageItem { page_id: "a".into(), path: "a.md".into(), title: "Alpha".into(), tags: vec![], date_label: None },
                FindPageItem { page_id: "b".into(), path: "b.md".into(), title: "Beta".into(), tags: vec![], date_label: None },
            ],
        };
        state.open_picker("Test", source);
        assert_eq!(state.active_picker.as_ref().unwrap().inner.selected_index(), Some(0));

        state.handle_key(Key::Ctrl('j'));
        assert_eq!(state.active_picker.as_ref().unwrap().inner.selected_index(), Some(1));

        // Ctrl-K moves back up
        state.handle_key(Key::Ctrl('k'));
        assert_eq!(state.active_picker.as_ref().unwrap().inner.selected_index(), Some(0));

        // Ctrl-N also moves down (Emacs convention)
        state.handle_key(Key::Ctrl('n'));
        assert_eq!(state.active_picker.as_ref().unwrap().inner.selected_index(), Some(1));

        // Ctrl-P also moves up (Emacs convention)
        state.handle_key(Key::Ctrl('p'));
        assert_eq!(state.active_picker.as_ref().unwrap().inner.selected_index(), Some(0));
    }

    #[test]
    fn picker_escape_closes() {
        let mut state = EditorState::new("hello");
        state.handle_key(Key::Char(' '));
        state.handle_key(Key::Char('f'));
        state.handle_key(Key::Char('f'));
        assert!(state.active_picker.is_some());

        state.handle_key(Key::Escape);
        assert!(state.active_picker.is_none());
    }

    #[test]
    fn picker_enter_closes() {
        let mut state = EditorState::new("hello");
        state.handle_key(Key::Char(' '));
        state.handle_key(Key::Char('f'));
        state.handle_key(Key::Char('f'));
        assert!(state.active_picker.is_some());

        state.handle_key(Key::Enter);
        assert!(state.active_picker.is_none());
    }

    // ── Filter stacking ─────────────────────────────────────────────────

    #[test]
    fn picker_ctrl_t_extracts_tag_filter_pill() {
        use crate::picker::{FindPageItem, PickerSource};
        use std::borrow::Cow;

        struct TestPages { items: Vec<FindPageItem> }
        impl PickerSource for TestPages {
            type Item = FindPageItem;
            fn items(&self) -> &[Self::Item] { &self.items }
            fn text(&self, item: &Self::Item) -> Cow<'_, str> { Cow::Owned(item.title.clone()) }
        }
        unsafe impl Send for TestPages {}
        unsafe impl Sync for TestPages {}

        let mut state = EditorState::new("hello");
        let source = TestPages {
            items: vec![
                FindPageItem { page_id: "a".into(), path: "a.md".into(), title: "Alpha".into(), tags: vec![], date_label: None },
                FindPageItem { page_id: "b".into(), path: "b.md".into(), title: "Beta".into(), tags: vec![], date_label: None },
            ],
        };
        state.open_picker("Test", source);

        // Type "alpha" then Ctrl-T to convert to a filter pill
        for c in "alpha".chars() {
            state.handle_key(Key::Char(c));
        }
        assert_eq!(state.active_picker.as_ref().unwrap().typed_query, "alpha");

        state.handle_key(Key::Ctrl('t'));
        let ap = state.active_picker.as_ref().unwrap();
        assert_eq!(ap.filter_pills.len(), 1);
        assert_eq!(ap.filter_pills[0], ("tag".to_string(), "alpha".to_string()));
        assert_eq!(ap.typed_query, "");
    }

    #[test]
    fn picker_ctrl_t_ignored_on_empty_query() {
        let mut state = EditorState::new("hello");
        state.open_picker("Test", FindPagesSource::empty());

        state.handle_key(Key::Ctrl('t'));
        let ap = state.active_picker.as_ref().unwrap();
        assert!(ap.filter_pills.is_empty());
    }

    #[test]
    fn picker_backspace_removes_filter_pill_when_query_empty() {
        use crate::picker::{FindPageItem, PickerSource};
        use std::borrow::Cow;

        struct TestPages { items: Vec<FindPageItem> }
        impl PickerSource for TestPages {
            type Item = FindPageItem;
            fn items(&self) -> &[Self::Item] { &self.items }
            fn text(&self, item: &Self::Item) -> Cow<'_, str> { Cow::Owned(item.title.clone()) }
        }
        unsafe impl Send for TestPages {}
        unsafe impl Sync for TestPages {}

        let mut state = EditorState::new("hello");
        let source = TestPages {
            items: vec![
                FindPageItem { page_id: "a".into(), path: "a.md".into(), title: "Alpha".into(), tags: vec![], date_label: None },
            ],
        };
        state.open_picker("Test", source);

        // Create a filter pill
        for c in "rust".chars() {
            state.handle_key(Key::Char(c));
        }
        state.handle_key(Key::Ctrl('t'));
        assert_eq!(state.active_picker.as_ref().unwrap().filter_pills.len(), 1);
        assert_eq!(state.active_picker.as_ref().unwrap().typed_query, "");

        // Backspace on empty typed_query pops the pill, restoring its label
        state.handle_key(Key::Backspace);
        let ap = state.active_picker.as_ref().unwrap();
        assert!(ap.filter_pills.is_empty());
        assert_eq!(ap.typed_query, "rust");
    }

    #[test]
    fn picker_filter_pill_narrows_results() {
        use crate::picker::{FindPageItem, PickerSource};
        use std::borrow::Cow;

        struct TestPages { items: Vec<FindPageItem> }
        impl PickerSource for TestPages {
            type Item = FindPageItem;
            fn items(&self) -> &[Self::Item] { &self.items }
            fn text(&self, item: &Self::Item) -> Cow<'_, str> { Cow::Owned(item.title.clone()) }
        }
        unsafe impl Send for TestPages {}
        unsafe impl Sync for TestPages {}

        let mut state = EditorState::new("hello");
        let source = TestPages {
            items: vec![
                FindPageItem { page_id: "a".into(), path: "a.md".into(), title: "Alpha".into(), tags: vec![], date_label: None },
                FindPageItem { page_id: "b".into(), path: "b.md".into(), title: "Beta".into(), tags: vec![], date_label: None },
                FindPageItem { page_id: "c".into(), path: "c.md".into(), title: "Gamma".into(), tags: vec![], date_label: None },
            ],
        };
        state.open_picker("Test", source);
        assert_eq!(state.active_picker.as_ref().unwrap().inner.results().len(), 3);

        // Type "al" and convert to pill — only "Alpha" matches
        for c in "al".chars() {
            state.handle_key(Key::Char(c));
        }
        state.handle_key(Key::Ctrl('t'));
        let ap = state.active_picker.as_ref().unwrap();
        assert_eq!(ap.inner.results().len(), 1);
        assert_eq!(ap.inner.results()[0].text, "Alpha");
    }

    #[test]
    fn picker_render_shows_filter_pills() {
        use crate::picker::{FindPageItem, PickerSource};
        use std::borrow::Cow;

        struct TestPages { items: Vec<FindPageItem> }
        impl PickerSource for TestPages {
            type Item = FindPageItem;
            fn items(&self) -> &[Self::Item] { &self.items }
            fn text(&self, item: &Self::Item) -> Cow<'_, str> { Cow::Owned(item.title.clone()) }
        }
        unsafe impl Send for TestPages {}
        unsafe impl Sync for TestPages {}

        let mut state = EditorState::new("hello");
        let source = TestPages {
            items: vec![
                FindPageItem { page_id: "a".into(), path: "a.md".into(), title: "Alpha".into(), tags: vec![], date_label: None },
            ],
        };
        state.open_picker("Test", source);

        // Create a filter pill and type more
        for c in "test".chars() {
            state.handle_key(Key::Char(c));
        }
        state.handle_key(Key::Ctrl('t'));
        state.handle_key(Key::Char('x'));

        let frame = state.render();
        let pf = frame.picker.unwrap();
        assert_eq!(pf.query, "x");
        assert_eq!(pf.filters.len(), 1);
        assert_eq!(pf.filters[0].kind, "tag");
        assert_eq!(pf.filters[0].label, "test");
    }

    #[test]
    fn picker_render_frame_populated() {
        let mut state = EditorState::new("hello");
        // No picker → frame.picker is None
        assert!(state.render().picker.is_none());

        // Open picker
        state.handle_key(Key::Char(' '));
        state.handle_key(Key::Char('f'));
        state.handle_key(Key::Char('f'));
        let frame = state.render();
        assert!(frame.picker.is_some());
        let pf = frame.picker.unwrap();
        assert_eq!(pf.title, "Find Page");
        assert_eq!(pf.query, "");
    }

    // ── Orphan link diagnostics ──────────────────────────────────────────

    #[test]
    fn link_diagnostics_valid_link_no_diagnostic() {
        use crate::index::SqliteIndex;
        use crate::parser::parse;
        use std::path::Path;

        let tmp = tempfile::TempDir::new().unwrap();
        let db_path = tmp.path().join("idx.db");
        let mut index = SqliteIndex::open(&db_path).unwrap();

        let target_doc = parse("---\nid: target01\ntitle: \"Target\"\ntags: []\n---\n\nTarget body.\n").unwrap();
        index.index_document(Path::new("pages/target.md"), &target_doc).unwrap();

        let content = "---\nid: source01\ntitle: \"Source\"\ntags: []\n---\n\nSee [[target01|Target]] here.\n";
        let mut state = EditorState::new(content);
        state.index = Some(index);

        let frame = state.render();
        assert!(frame.diagnostics.is_empty(), "valid link should produce no diagnostics");
    }

    #[test]
    fn link_diagnostics_broken_link_produces_diagnostic() {
        use crate::index::SqliteIndex;
        use crate::render::DiagnosticKind;

        let tmp = tempfile::TempDir::new().unwrap();
        let db_path = tmp.path().join("idx.db");
        let index = SqliteIndex::open(&db_path).unwrap();

        let content = "---\nid: source01\ntitle: \"Source\"\ntags: []\n---\n\nSee [[missing99|Gone]] here.\n";
        let mut state = EditorState::new(content);
        state.index = Some(index);

        let frame = state.render();
        assert_eq!(frame.diagnostics.len(), 1);
        assert_eq!(frame.diagnostics[0].kind, DiagnosticKind::BrokenLink);
        assert!(frame.diagnostics[0].message.contains("missing99"));

        // BrokenLink styled span should also be present on the focused pane.
        let pane = frame.focused_pane().unwrap();
        let broken_spans: Vec<_> = pane
            .lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .filter(|s| s.style == crate::render::Style::BrokenLink)
            .collect();
        assert_eq!(broken_spans.len(), 1);
    }

    // ── Inline picker tests ─────────────────────────────────────────────

    /// Helper source that provides `value()` for inline picker tests.
    fn make_inline_test_picker(state: &mut EditorState) {
        use crate::picker::{ActivePicker, FindPageItem, Picker, PickerSource};
        use std::borrow::Cow;

        struct InlineTestPages { items: Vec<FindPageItem> }
        impl PickerSource for InlineTestPages {
            type Item = FindPageItem;
            fn items(&self) -> &[Self::Item] { &self.items }
            fn text(&self, item: &Self::Item) -> Cow<'_, str> { Cow::Owned(item.title.clone()) }
            fn value(&self, item: &Self::Item) -> Option<(String, String)> {
                Some((item.page_id.clone(), item.title.clone()))
            }
        }
        unsafe impl Send for InlineTestPages {}
        unsafe impl Sync for InlineTestPages {}

        let source = InlineTestPages {
            items: vec![
                FindPageItem { page_id: "pg001".into(), path: "pages/alpha.md".into(), title: "Alpha Page".into(), tags: vec![], date_label: None },
                FindPageItem { page_id: "pg002".into(), path: "pages/beta.md".into(), title: "Beta Page".into(), tags: vec![], date_label: None },
            ],
        };
        state.active_picker = Some(ActivePicker {
            kind: PickerKind::Default,
            title: "Insert Link".into(),
            inner: Box::new(Picker::new(source)),
            inline: true,
            inline_trigger_len: 2,
            is_embed: false,
            filter_pills: Vec::new(),
            typed_query: String::new(),
            preview: None,
            marked: HashSet::new(),
            supports_batch_select: false,
            action_menu_open: false,
            action_menu_selected: 0,
            action_menu_items: crate::picker::DEFAULT_ACTION_MENU_ITEMS.iter().map(|s| s.to_string()).collect(),
            drill_down_page_id: None,
            drill_down_page_title: None,
        });
    }

    #[test]
    fn inline_picker_triggers_on_double_bracket() {
        let mut state = EditorState::new("");
        // Enter insert mode and type `[[`
        state.handle_key(Key::Char('i'));
        state.handle_key(Key::Char('['));
        assert!(state.active_picker.is_none(), "single [ should not open picker");
        state.handle_key(Key::Char('['));
        assert!(state.active_picker.is_some(), "[[ should open inline picker");
        let ap = state.active_picker.as_ref().unwrap();
        assert!(ap.inline);
        assert_eq!(ap.inline_trigger_len, 2);
        assert!(!ap.is_embed);
        assert_eq!(ap.title, "Insert Link");
        // Render frame should mark picker as inline
        let frame = state.render();
        assert!(frame.picker.as_ref().unwrap().inline);
    }

    #[test]
    fn inline_picker_selection_inserts_link() {
        let mut state = EditorState::new("");
        // Enter insert mode and type `Hello [[`
        state.handle_key(Key::Char('i'));
        for c in "Hello [[".chars() {
            state.handle_key(Key::Char(c));
        }
        assert_eq!(state.text(), "Hello [[");
        // Replace the auto-opened (empty) picker with one that has items.
        make_inline_test_picker(&mut state);
        // Press Enter to select first item ("Alpha Page", id "pg001")
        state.handle_key(Key::Enter);
        assert!(state.active_picker.is_none());
        assert_eq!(state.text(), "Hello [[pg001|Alpha Page]]");
    }

    #[test]
    fn inline_picker_escape_closes_without_modifying_buffer() {
        let mut state = EditorState::new("");
        state.handle_key(Key::Char('i'));
        for c in "[[".chars() {
            state.handle_key(Key::Char(c));
        }
        assert!(state.active_picker.is_some());
        state.handle_key(Key::Escape);
        assert!(state.active_picker.is_none());
        // Buffer still has `[[` — user can delete manually.
        assert_eq!(state.text(), "[[");
    }

    #[test]
    fn inline_embed_picker_triggers_on_excl_double_bracket() {
        let mut state = EditorState::new("");
        state.handle_key(Key::Char('i'));
        for c in "![[".chars() {
            state.handle_key(Key::Char(c));
        }
        assert!(state.active_picker.is_some());
        let ap = state.active_picker.as_ref().unwrap();
        assert!(ap.inline);
        assert_eq!(ap.inline_trigger_len, 3);
        assert!(ap.is_embed);
        assert_eq!(ap.title, "Embed Page");
    }

    // ── Window command tests ────────────────────────────────────────────

    fn send_leader_seq(state: &mut EditorState, keys: &[char]) {
        state.handle_key(Key::Char(' '));
        for &k in keys {
            state.handle_key(Key::Char(k));
        }
    }

    #[test]
    fn window_split_creates_two_panes() {
        let mut state = EditorState::new("hello");
        assert_eq!(state.window_layout.pane_ids().len(), 1);
        send_leader_seq(&mut state, &['w', 'v']);
        assert_eq!(state.window_layout.pane_ids().len(), 2);
        let frame = state.render();
        assert_eq!(frame.panes.len(), 2);
        assert_eq!(frame.panes.iter().filter(|p| p.focused).count(), 1);
    }

    #[test]
    fn window_move_focus_changes_active() {
        let mut state = EditorState::new("hello");
        let first = state.window_layout.focused();
        send_leader_seq(&mut state, &['w', 'v']);
        let second = state.window_layout.focused();
        assert_ne!(first, second);
        send_leader_seq(&mut state, &['w', 'h']);
        assert_eq!(state.window_layout.focused(), first);
    }

    #[test]
    fn window_close_returns_to_one_pane() {
        let mut state = EditorState::new("hello");
        send_leader_seq(&mut state, &['w', 'v']);
        assert_eq!(state.window_layout.pane_ids().len(), 2);
        send_leader_seq(&mut state, &['w', 'd']);
        assert_eq!(state.window_layout.pane_ids().len(), 1);
        let frame = state.render();
        assert_eq!(frame.panes.len(), 1);
        assert!(frame.panes[0].focused);
    }

    #[test]
    fn window_render_frame_has_correct_pane_count() {
        let mut state = EditorState::new("test");
        let frame = state.render();
        assert_eq!(frame.panes.len(), 1);
        assert!(frame.focused_pane().is_some());

        send_leader_seq(&mut state, &['w', 's']);
        let frame = state.render();
        assert_eq!(frame.panes.len(), 2);

        send_leader_seq(&mut state, &['w', 'v']);
        let frame = state.render();
        assert_eq!(frame.panes.len(), 3);
    }

    #[test]
    fn window_maximize_toggle() {
        let mut state = EditorState::new("hello");
        send_leader_seq(&mut state, &['w', 'v']);
        assert!(state.window_layout.maximized().is_none());
        send_leader_seq(&mut state, &['w', 'm']);
        assert!(state.window_layout.maximized().is_some());
        send_leader_seq(&mut state, &['w', 'm']);
        assert!(state.window_layout.maximized().is_none());
    }

    #[test]
    fn window_balance() {
        let mut state = EditorState::new("hello");
        send_leader_seq(&mut state, &['w', 'v']);
        send_leader_seq(&mut state, &['w', '=']);
        for ratio in state.window_layout.split_ratios() {
            assert!((ratio - 0.5).abs() < 0.001);
        }
    }

    #[test]
    fn window_split_independent_buffers() {
        // 1. Create editor with initial content.
        let mut state = EditorState::new("file A content");
        let pane0 = state.window_layout.focused();

        // 2. SPC w v → vertical split (focus moves to new pane).
        send_leader_seq(&mut state, &['w', 'v']);

        // 3. Two panes exist after split.
        let pane_ids = state.window_layout.pane_ids();
        assert_eq!(pane_ids.len(), 2);
        let frame = state.render();
        assert_eq!(frame.panes.len(), 2);

        // The new pane is now focused.
        let pane1 = state.window_layout.focused();
        assert_ne!(pane0, pane1);

        // 4. Replace the live buffer with different content.
        state.buffer = Buffer::from_str("file B content");

        // 5. Render and verify: the focused pane shows the new content.
        let frame = state.render();
        assert_eq!(frame.panes.len(), 2);

        // The focused pane (pane1, index 1) should show the new content.
        let focused_pane = frame.focused_pane().unwrap();
        assert_eq!(focused_pane.lines[0].text, "file B content");

        // Known limitation: when buffer is replaced, the non-focused pane
        // loses its content because pane_states stores a snapshot from
        // the previous focus switch. The non-focused pane shows whatever
        // was saved in pane_states at split time.
        // TODO: Implement proper per-pane buffer storage for true
        // independent buffer support (each pane holds its own Buffer).
        assert!(!frame.panes[0].focused);
        assert!(frame.panes[1].focused);
    }

    // ── Capture bar tests ───────────────────────────────────────────────

    #[test]
    fn spc_j_a_activates_capture_bar_append() {
        let mut state = EditorState::new("hello");
        send_leader_seq(&mut state, &['j', 'a']);
        assert_eq!(state.capture_bar_mode, Some(CaptureBarMode::Append));
        assert_eq!(state.last_leader_action, Some(LeaderAction::JournalAppend));
        let frame = state.render();
        assert_eq!(frame.capture_bar, Some("Append: ".to_string()));
    }

    #[test]
    fn spc_j_t_activates_capture_bar_task() {
        let mut state = EditorState::new("hello");
        send_leader_seq(&mut state, &['j', 't']);
        assert_eq!(state.capture_bar_mode, Some(CaptureBarMode::Task));
        assert_eq!(state.last_leader_action, Some(LeaderAction::JournalTask));
        let frame = state.render();
        assert_eq!(frame.capture_bar, Some("Task: ".to_string()));
    }

    #[test]
    fn spc_j_x_still_activates_capture_bar_task() {
        let mut state = EditorState::new("hello");
        send_leader_seq(&mut state, &['j', 'x']);
        assert_eq!(state.capture_bar_mode, Some(CaptureBarMode::Task));
    }

    #[test]
    fn capture_bar_typing_shows_in_render() {
        let mut state = EditorState::new("hello");
        state.capture_bar_mode = Some(CaptureBarMode::Append);
        state.handle_key(Key::Char('t'));
        state.handle_key(Key::Char('e'));
        state.handle_key(Key::Char('s'));
        state.handle_key(Key::Char('t'));
        let frame = state.render();
        assert_eq!(frame.capture_bar, Some("Append: test".to_string()));
    }

    #[test]
    fn capture_bar_escape_cancels() {
        let mut state = EditorState::new("hello");
        state.capture_bar_mode = Some(CaptureBarMode::Append);
        state.handle_key(Key::Char('a'));
        state.handle_key(Key::Escape);
        assert!(state.capture_bar_mode.is_none());
        assert!(state.capture_bar_input.is_empty());
        let frame = state.render();
        assert!(frame.capture_bar.is_none());
    }

    #[test]
    fn capture_bar_backspace_deletes() {
        let mut state = EditorState::new("hello");
        state.capture_bar_mode = Some(CaptureBarMode::Append);
        state.handle_key(Key::Char('a'));
        state.handle_key(Key::Char('b'));
        state.handle_key(Key::Backspace);
        assert_eq!(state.capture_bar_input, "a");
    }

    #[test]
    fn capture_bar_enter_commits_append() {
        use tempfile::TempDir;
        let tmp = TempDir::new().unwrap();
        let store = LocalFileStore::new(tmp.path().to_path_buf()).unwrap();
        let mut state = EditorState::new("hello");
        state.vault_root = Some(tmp.path().to_path_buf());
        state.capture_bar_mode = Some(CaptureBarMode::Append);
        state.capture_bar_input = "my note".to_string();
        state.handle_key(Key::Enter);
        assert!(state.capture_bar_mode.is_none());
        assert!(state.capture_bar_input.is_empty());
        let today_path = store.today_journal_path();
        assert!(store.exists(&today_path));
        let content = store.read(&today_path).unwrap();
        assert!(content.contains("my note"));
        let frame = state.render();
        assert!(
            frame
                .command_line
                .as_deref()
                .unwrap_or_default()
                .contains("Added to"),
            "expected capture confirmation message"
        );
    }

    #[test]
    fn capture_bar_enter_commits_task() {
        use tempfile::TempDir;
        let tmp = TempDir::new().unwrap();
        let store = LocalFileStore::new(tmp.path().to_path_buf()).unwrap();
        let mut state = EditorState::new("hello");
        state.vault_root = Some(tmp.path().to_path_buf());
        state.capture_bar_mode = Some(CaptureBarMode::Task);
        state.capture_bar_input = "buy milk".to_string();
        state.handle_key(Key::Enter);
        let today_path = store.today_journal_path();
        let content = store.read(&today_path).unwrap();
        assert!(content.contains("- [ ] buy milk"));
        let frame = state.render();
        assert!(
            frame
                .command_line
                .as_deref()
                .unwrap_or_default()
                .contains("Added to"),
            "expected capture confirmation message"
        );
    }

    // ── Journal navigation tests ────────────────────────────────────────

    #[test]
    fn journal_today_loads_buffer() {
        use tempfile::TempDir;
        let tmp = TempDir::new().unwrap();
        let _store = LocalFileStore::new(tmp.path().to_path_buf()).unwrap();
        let mut state = EditorState::new("");
        state.vault_root = Some(tmp.path().to_path_buf());
        send_leader_seq(&mut state, &['j', 'j']);
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let fp = state.buffer.file_path.as_ref().unwrap();
        assert!(fp.to_str().unwrap().contains(&today));
        assert!(state.text().contains("---"));
    }

    #[test]
    fn journal_prev_next_navigate() {
        use tempfile::TempDir;
        let tmp = TempDir::new().unwrap();
        let _store = LocalFileStore::new(tmp.path().to_path_buf()).unwrap();
        let mut state = EditorState::new("");
        state.vault_root = Some(tmp.path().to_path_buf());
        send_leader_seq(&mut state, &['j', 'j']);
        let today = chrono::Local::now().date_naive();
        send_leader_seq(&mut state, &['j', 'p']);
        let yesterday = journal::prev_date(today);
        let fp = state.buffer.file_path.as_ref().unwrap();
        assert!(fp.to_str().unwrap().contains(&yesterday.format("%Y-%m-%d").to_string()));
        send_leader_seq(&mut state, &['j', 'n']);
        let fp = state.buffer.file_path.as_ref().unwrap();
        assert!(fp.to_str().unwrap().contains(&today.format("%Y-%m-%d").to_string()));
    }

    // ── Text object tests ───────────────────────────────────────────────

    #[test]
    fn diw_deletes_inner_word() {
        assert_keys("hello world", 1, &keys("diw"), " world", 0, Mode::Normal);
    }

    #[test]
    fn daw_deletes_word_and_trailing_space() {
        assert_keys("hello world", 1, &keys("daw"), "world", 0, Mode::Normal);
    }

    #[test]
    fn ci_paren_deletes_inner_and_enters_insert() {
        assert_keys("fn(arg)", 3, &keys("ci("), "fn()", 3, Mode::Insert);
    }

    #[test]
    fn da_bracket_deletes_including_delimiters() {
        assert_keys("x [foo] y", 3, &keys("da["), "x  y", 2, Mode::Normal);
    }

    // ── Bloom text object tests ─────────────────────────────────────────

    #[test]
    fn di_quote_deletes_inner_double_quote() {
        assert_keys(r#"say "hello" end"#, 6, &keys("di\""), r#"say "" end"#, 5, Mode::Normal);
    }

    #[test]
    fn da_brace_deletes_including_braces() {
        assert_keys("x {foo} y", 3, &keys("da{"), "x  y", 2, Mode::Normal);
    }

    #[test]
    fn cil_changes_inner_link() {
        assert_keys("See [[abc|Note]] here", 7, &keys("cil"), "See [[]] here", 6, Mode::Insert);
    }

    #[test]
    fn dal_deletes_around_link() {
        assert_keys("See [[abc|Note]] here", 7, &keys("dal"), "See  here", 4, Mode::Normal);
    }

    #[test]
    fn cie_changes_inner_embed() {
        assert_keys("X ![[pg1|Title]] Y", 8, &keys("cie"), "X ![[]] Y", 5, Mode::Insert);
    }

    #[test]
    fn dae_deletes_around_embed() {
        assert_keys("X ![[pg1|Title]] Y", 8, &keys("dae"), "X  Y", 2, Mode::Normal);
    }

    #[test]
    fn di_hash_deletes_inner_tag() {
        assert_keys("text #rust-lang end", 7, &keys("di#"), "text # end", 6, Mode::Normal);
    }

    #[test]
    fn da_hash_deletes_around_tag() {
        assert_keys("text #rust end", 7, &keys("da#"), "text  end", 5, Mode::Normal);
    }

    #[test]
    fn di_at_deletes_inner_timestamp() {
        assert_keys("task @due(2026-03-05) end", 14, &keys("di@"), "task @due() end", 10, Mode::Normal);
    }

    #[test]
    fn da_at_deletes_around_timestamp() {
        assert_keys("task @due(2026-03-05) end", 14, &keys("da@"), "task  end", 5, Mode::Normal);
    }

    #[test]
    fn dih_deletes_inner_heading_section() {
        let text = "# H1\nline1\nline2\n# H2\nother";
        let mut state = EditorState::new(text);
        state.cursor = 6; // on "line1"
        state.feed_keys(&keys("dih"));
        assert!(state.text().starts_with("# H1\n# H2"));
    }

    #[test]
    fn dah_deletes_around_heading_section() {
        let text = "# H1\nline1\nline2\n# H2\nother";
        let mut state = EditorState::new(text);
        state.cursor = 0; // on "# H1"
        state.feed_keys(&keys("dah"));
        assert_eq!(state.text(), "# H2\nother");
    }

    // ── Timeline tests ──────────────────────────────────────────────────

    #[test]
    fn spc_l_t_activates_timeline_view() {
        let mut state = EditorState::new("---\nid: abc12345\ntitle: \"Test\"\ntags: []\n---\n\nHello");
        assert!(!state.timeline_active);
        // SPC l t
        state.handle_key(Key::Char(' '));
        state.handle_key(Key::Char('l'));
        state.handle_key(Key::Char('t'));
        assert!(state.timeline_active);
        assert_eq!(state.last_leader_action, Some(LeaderAction::TimelineView));
    }

    #[test]
    fn timeline_escape_closes() {
        let mut state = EditorState::new("---\nid: abc12345\ntitle: \"Test\"\ntags: []\n---\n\nHello");
        state.handle_key(Key::Char(' '));
        state.handle_key(Key::Char('l'));
        state.handle_key(Key::Char('t'));
        assert!(state.timeline_active);
        state.handle_key(Key::Escape);
        assert!(!state.timeline_active);
    }

    #[test]
    fn timeline_no_index_shows_empty_state() {
        let mut state = EditorState::new("---\nid: abc12345\ntitle: \"Test\"\ntags: []\n---\n\nHello");
        assert!(state.index.is_none());
        state.handle_key(Key::Char(' '));
        state.handle_key(Key::Char('l'));
        state.handle_key(Key::Char('t'));
        assert!(state.timeline_active);
        assert!(state.timeline_entries.is_empty());
    }

    // ── Dot-repeat tests ────────────────────────────────────────────────

    #[test]
    fn dot_repeat_x_deletes_another_char() {
        let mut state = EditorState::new("abcd");
        state.cursor = 0;
        state.feed_keys(&keys("x"));   // deletes 'a' → "bcd", cursor 0
        assert_eq!(state.text(), "bcd");
        state.feed_keys(&keys("."));   // deletes 'b' → "cd", cursor 0
        assert_eq!(state.text(), "cd");
        assert_eq!(state.cursor, 0);
        assert_eq!(state.mode, Mode::Normal);
    }

    #[test]
    fn dot_repeat_dw_deletes_next_word() {
        let mut state = EditorState::new("one two three");
        state.cursor = 0;
        state.feed_keys(&keys("dw"));  // deletes "one " → "two three"
        assert_eq!(state.text(), "two three");
        state.feed_keys(&keys("."));   // deletes "two " → "three"
        assert_eq!(state.text(), "three");
        assert_eq!(state.mode, Mode::Normal);
    }

    #[test]
    fn dot_repeat_insert_text() {
        let mut state = EditorState::new("ab");
        state.cursor = 0;
        state.feed_keys(&keys("ihi\x1b"));  // insert "hi" → "hiab", cursor 1
        assert_eq!(state.text(), "hiab");
        state.cursor = 4; // move to end
        state.feed_keys(&keys("."));   // insert "hi" again → "hiabhi", cursor 5
        assert_eq!(state.text(), "hiabhi");
        assert_eq!(state.mode, Mode::Normal);
    }

    // ── Undo tree visualizer tests ──────────────────────────────────────

    #[test]
    fn spc_u_u_activates_undo_visualizer() {
        let mut state = EditorState::new("hello");
        assert!(!state.undo_tree_active);
        send_leader_seq(&mut state, &['u', 'u']);
        assert!(state.undo_tree_active);
        assert_eq!(state.last_leader_action, Some(LeaderAction::UndoVisualizer));
        let frame = state.render();
        assert!(frame.undo_tree.is_some());
    }

    #[test]
    fn undo_visualizer_escape_closes() {
        let mut state = EditorState::new("hello");
        send_leader_seq(&mut state, &['u', 'u']);
        assert!(state.undo_tree_active);
        state.handle_key(Key::Escape);
        assert!(!state.undo_tree_active);
        let frame = state.render();
        assert!(frame.undo_tree.is_none());
    }

    #[test]
    fn undo_visualizer_no_branches_shows_single_entry() {
        let mut state = EditorState::new("hello");
        send_leader_seq(&mut state, &['u', 'u']);
        let frame = state.render();
        let ut = frame.undo_tree.unwrap();
        assert_eq!(ut.entries.len(), 1);
        assert!(ut.entries[0].current);
    }

    // ── Create from picker tests ────────────────────────────────────────

    #[test]
    fn alt_enter_in_empty_picker_creates_page() {
        use tempfile::TempDir;
        let tmp = TempDir::new().unwrap();
        let _store = LocalFileStore::new(tmp.path().to_path_buf()).unwrap();
        let db_path = tmp.path().join(".index").join("core.db");
        let index = crate::index::SqliteIndex::open(&db_path).unwrap();

        let mut state = EditorState::new("hello");
        state.vault_root = Some(tmp.path().to_path_buf());
        state.index = Some(index);

        // Open "Find Page" picker with no results.
        state.open_picker_find_page();
        assert!(state.active_picker.is_some());
        assert_eq!(state.active_picker.as_ref().unwrap().title, "Find Page");

        // Type a query that matches nothing.
        for c in "Brand New Note".chars() {
            state.handle_key(Key::Char(c));
        }
        assert!(state.active_picker.as_ref().unwrap().inner.is_empty());

        // Press Alt+Enter to create.
        state.handle_key(Key::AltEnter);
        assert!(state.active_picker.is_none());

        // Buffer should now contain the new page frontmatter.
        let text = state.text();
        assert!(text.contains("title: \"Brand New Note\""));
        assert!(text.contains("id: "));
        assert!(text.contains("tags: []"));

        // File should exist on disk.
        let page_path = tmp.path().join("pages").join("Brand New Note.md");
        assert!(page_path.exists());
    }

    #[test]
    fn new_page_has_valid_frontmatter_with_uuid() {
        use tempfile::TempDir;
        let tmp = TempDir::new().unwrap();
        let _store = LocalFileStore::new(tmp.path().to_path_buf()).unwrap();

        let mut state = EditorState::new("hello");
        state.vault_root = Some(tmp.path().to_path_buf());

        state.open_picker_find_page();
        for c in "Test UUID".chars() {
            state.handle_key(Key::Char(c));
        }
        state.handle_key(Key::AltEnter);

        let text = state.text();
        // Extract the id from frontmatter.
        let id_line = text.lines().find(|l| l.starts_with("id: ")).unwrap();
        let id = id_line.strip_prefix("id: ").unwrap().trim();
        assert_eq!(id.len(), 16);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn inline_picker_alt_enter_inserts_link_to_new_page() {
        use tempfile::TempDir;
        let tmp = TempDir::new().unwrap();
        let _store = LocalFileStore::new(tmp.path().to_path_buf()).unwrap();

        let mut state = EditorState::new("");
        state.vault_root = Some(tmp.path().to_path_buf());

        // Enter insert mode and type `Hello [[`
        state.handle_key(Key::Char('i'));
        for c in "Hello [[".chars() {
            state.handle_key(Key::Char(c));
        }
        assert!(state.active_picker.is_some());
        let ap = state.active_picker.as_ref().unwrap();
        assert!(ap.inline);
        assert_eq!(ap.title, "Insert Link");

        // Type query for new page.
        for c in "New Topic".chars() {
            state.handle_key(Key::Char(c));
        }

        // Alt+Enter to create and insert link.
        state.handle_key(Key::AltEnter);
        assert!(state.active_picker.is_none());

        let text = state.text();
        // Should contain `[[<16-hex-id>|New Topic]]`
        assert!(text.starts_with("Hello [["));
        assert!(text.contains("|New Topic]]"));
        // Verify the page_id is 16 hex chars.
        let link_start = text.find("[[").unwrap() + 2;
        let pipe = text.find('|').unwrap();
        let page_id = &text[link_start..pipe];
        assert_eq!(page_id.len(), 16);
        assert!(page_id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn alt_enter_with_empty_query_does_not_create() {
        let mut state = EditorState::new("hello");
        state.open_picker_find_page();
        assert!(state.active_picker.is_some());
        // Press Alt+Enter without typing anything.
        state.handle_key(Key::AltEnter);
        // Picker should be closed but buffer unchanged.
        assert!(state.active_picker.is_none());
        assert_eq!(state.text(), "hello");
    }

    // ── Action menu tests ───────────────────────────────────────────────

    #[test]
    fn action_menu_tab_opens_and_escape_closes() {
        use crate::picker::{FindPageItem, PickerSource};
        use std::borrow::Cow;

        struct TestPages { items: Vec<FindPageItem> }
        impl PickerSource for TestPages {
            type Item = FindPageItem;
            fn items(&self) -> &[Self::Item] { &self.items }
            fn text(&self, item: &Self::Item) -> Cow<'_, str> { Cow::Owned(item.title.clone()) }
        }
        unsafe impl Send for TestPages {}
        unsafe impl Sync for TestPages {}

        let mut state = EditorState::new("hello");
        let source = TestPages {
            items: vec![
                FindPageItem { page_id: "a".into(), path: "a.md".into(), title: "Alpha".into(), tags: vec![], date_label: None },
            ],
        };
        state.open_picker("Test", source);
        assert!(!state.active_picker.as_ref().unwrap().action_menu_open);

        // Tab opens action menu
        state.handle_key(Key::Tab);
        assert!(state.active_picker.as_ref().unwrap().action_menu_open);
        assert_eq!(state.active_picker.as_ref().unwrap().action_menu_selected, 0);

        // Escape closes action menu but keeps picker open
        state.handle_key(Key::Escape);
        assert!(state.active_picker.is_some());
        assert!(!state.active_picker.as_ref().unwrap().action_menu_open);
    }

    #[test]
    fn action_menu_jk_navigates() {
        use crate::picker::{FindPageItem, PickerSource};
        use std::borrow::Cow;

        struct TestPages { items: Vec<FindPageItem> }
        impl PickerSource for TestPages {
            type Item = FindPageItem;
            fn items(&self) -> &[Self::Item] { &self.items }
            fn text(&self, item: &Self::Item) -> Cow<'_, str> { Cow::Owned(item.title.clone()) }
        }
        unsafe impl Send for TestPages {}
        unsafe impl Sync for TestPages {}

        let mut state = EditorState::new("hello");
        let source = TestPages {
            items: vec![
                FindPageItem { page_id: "a".into(), path: "a.md".into(), title: "Alpha".into(), tags: vec![], date_label: None },
            ],
        };
        state.open_picker("Test", source);

        // Open action menu
        state.handle_key(Key::Tab);
        assert_eq!(state.active_picker.as_ref().unwrap().action_menu_selected, 0);

        // j moves down
        state.handle_key(Key::Char('j'));
        assert_eq!(state.active_picker.as_ref().unwrap().action_menu_selected, 1);

        // j again
        state.handle_key(Key::Char('j'));
        assert_eq!(state.active_picker.as_ref().unwrap().action_menu_selected, 2);

        // k moves up
        state.handle_key(Key::Char('k'));
        assert_eq!(state.active_picker.as_ref().unwrap().action_menu_selected, 1);

        // k wraps to last
        state.handle_key(Key::Char('k'));
        assert_eq!(state.active_picker.as_ref().unwrap().action_menu_selected, 0);
        state.handle_key(Key::Char('k'));
        let len = state.active_picker.as_ref().unwrap().action_menu_items.len();
        assert_eq!(state.active_picker.as_ref().unwrap().action_menu_selected, len - 1);
    }

    #[test]
    fn action_menu_enter_closes_picker() {
        use crate::picker::{FindPageItem, PickerSource};
        use std::borrow::Cow;

        struct TestPages { items: Vec<FindPageItem> }
        impl PickerSource for TestPages {
            type Item = FindPageItem;
            fn items(&self) -> &[Self::Item] { &self.items }
            fn text(&self, item: &Self::Item) -> Cow<'_, str> { Cow::Owned(item.title.clone()) }
        }
        unsafe impl Send for TestPages {}
        unsafe impl Sync for TestPages {}

        let mut state = EditorState::new("hello");
        let source = TestPages {
            items: vec![
                FindPageItem { page_id: "a".into(), path: "a.md".into(), title: "Alpha".into(), tags: vec![], date_label: None },
            ],
        };
        state.open_picker("Test", source);

        // Open action menu and press Enter
        state.handle_key(Key::Tab);
        assert!(state.active_picker.as_ref().unwrap().action_menu_open);
        state.handle_key(Key::Enter);
        assert!(state.active_picker.is_none());
    }

    #[test]
    fn action_menu_open_in_split_creates_new_pane() {
        let content = "---\nid: aabb1122\ntitle: \"Rope Buffers\"\ncreated: 2026-02-28T00:00:00Z\ntags: []\n---\n\nRopes are great.";
        let (_tmp, mut state) = setup_vault_with_page("Rope Buffers", content);

        send_leader_seq(&mut state, &['f', 'f']);
        assert_eq!(state.window_layout.pane_ids().len(), 1);

        state.handle_key(Key::Tab);
        state.handle_key(Key::Char('j')); // Open in split
        state.handle_key(Key::Enter);

        assert!(state.active_picker.is_none());
        assert_eq!(state.window_layout.pane_ids().len(), 2);
        assert!(state.text().contains("Ropes are great"));
    }

    #[test]
    fn capture_bar_double_bracket_opens_link_picker_and_inserts_link() {
        use tempfile::TempDir;
        let tmp = TempDir::new().unwrap();
        let _store = LocalFileStore::new(tmp.path().to_path_buf()).unwrap();
        let db_path = tmp.path().join(".index").join("core.db");
        let mut index = crate::index::SqliteIndex::open(&db_path).unwrap();
        let content = "---\nid: link1234\ntitle: \"Link Target\"\ncreated: 2026-02-28T00:00:00Z\ntags: []\n---\n\nBody.";
        let path = tmp.path().join("pages").join("Link Target.md");
        std::fs::write(&path, content).unwrap();
        let doc = crate::parser::parse(content).unwrap();
        index.index_document(&path, &doc).unwrap();

        let mut state = EditorState::new("hello");
        state.index = Some(index);
        send_leader_seq(&mut state, &['j', 'a']);
        state.handle_key(Key::Char('['));
        state.handle_key(Key::Char('['));
        assert!(state.active_picker.is_some());
        assert!(state.active_picker.as_ref().unwrap().inline);

        state.handle_key(Key::Enter);
        assert!(state.active_picker.is_none());
        assert!(state.capture_bar_input.contains("[[link1234|Link Target]]"));
    }

    // ── Named registers ─────────────────────────────────────────────────

    #[test]
    fn register_yank_and_paste() {
        // "ayiw yanks inner word into register 'a', then "ap pastes it
        let mut state = EditorState::new("hello world");
        state.cursor = 0;
        // "ayiw — yank inner word into register a
        state.feed_keys(&keys("\"ayiw"));
        assert_eq!(state.registers.get(&'a'), Some(&"hello".to_string()));
        assert_eq!(state.text(), "hello world");
        // Move to end of "world"
        state.feed_keys(&keys("$"));
        // "ap — paste from register a after cursor
        state.feed_keys(&keys("\"ap"));
        assert!(state.text().contains("hello"));
        assert!(state.text().contains("worldhello"));
    }

    #[test]
    fn unnamed_register_paste() {
        // diw deletes inner word into unnamed register, then p pastes it
        let mut state = EditorState::new("hello world");
        state.cursor = 0;
        state.feed_keys(&keys("diw"));
        assert_eq!(state.text(), " world");
        assert_eq!(state.registers.get(&'"'), Some(&"hello".to_string()));
        // Move to end, paste
        state.feed_keys(&keys("$p"));
        assert!(state.text().contains("hello"));
    }

    // ── Macros ──────────────────────────────────────────────────────────

    #[test]
    fn macro_record_and_replay() {
        // qa + iX<Esc> + q records macro, @a replays it
        let mut state = EditorState::new("hello");
        state.cursor = 0;
        // qa starts recording into register a
        state.feed_keys(&keys("qa"));
        assert_eq!(state.macro_recording, Some('a'));
        // iX<Esc> — insert X then return to normal
        state.feed_keys(&keys("iX\x1b"));
        // q stops recording
        state.feed_keys(&keys("q"));
        assert_eq!(state.macro_recording, None);
        assert!(!state.macro_registers.get(&'a').unwrap().is_empty());
        let text_after_record = state.text().to_string();
        assert!(text_after_record.contains("X"));

        // Move to end, @a replays the macro
        state.feed_keys(&keys("$"));
        state.feed_keys(&keys("@a"));
        assert!(state.text().len() > text_after_record.len());
    }

    #[test]
    fn macro_repeat_last() {
        // @a then @@ replays the same macro
        let mut state = EditorState::new("abc");
        state.cursor = 0;
        // Record macro: insert Z
        state.feed_keys(&keys("qaiZ\x1bq"));
        let after_record = state.text().to_string();
        // @a plays macro a
        state.feed_keys(&keys("$@a"));
        let after_first = state.text().to_string();
        assert!(after_first.len() > after_record.len());
        // @@ replays last macro (a)
        state.feed_keys(&keys("$@@"));
        assert!(state.text().len() > after_first.len());
        assert_eq!(state.last_macro, Some('a'));
    }

    // ── Marks ───────────────────────────────────────────────────────────

    #[test]
    fn mark_set_and_jump() {
        // ma sets mark, move away, 'a jumps to line start of mark
        let mut state = EditorState::new("line one\nline two\nline three");
        // Position cursor on "line two"
        state.cursor = 9; // start of "line two"
        state.feed_keys(&keys("ma"));
        assert_eq!(state.marks.get(&'a'), Some(&9));
        // Move to line three
        state.feed_keys(&keys("j"));
        assert!(state.cursor > 9);
        // 'a jumps to line containing mark a (start of line)
        state.feed_keys(&keys("'a"));
        assert_eq!(state.cursor, 9);
    }

    #[test]
    fn mark_backtick_exact() {
        // `a jumps to exact byte position
        let mut state = EditorState::new("hello world stuff");
        state.cursor = 6; // 'w' in "world"
        state.feed_keys(&keys("ma"));
        assert_eq!(state.marks.get(&'a'), Some(&6));
        // Move away
        state.feed_keys(&keys("0"));
        assert_eq!(state.cursor, 0);
        // `a jumps to exact position 6
        state.feed_keys(&keys("`a"));
        assert_eq!(state.cursor, 6);
    }

    // ── Template picker wiring ──────────────────────────────────────────

    #[test]
    fn spc_n_opens_template_picker() {
        let mut state = EditorState::new("hello");
        // SPC n p dispatches NewPage
        state.handle_key(Key::Char(' '));
        state.handle_key(Key::Char('n'));
        state.handle_key(Key::Char('p'));
        assert_eq!(state.last_leader_action, Some(LeaderAction::NewPage));
        assert!(state.active_picker.is_some());
        assert_eq!(state.active_picker.as_ref().unwrap().title, "New Page");
    }

    #[test]
    fn template_picker_enter_creates_page() {
        use tempfile::TempDir;
        let tmp = TempDir::new().unwrap();
        let _store = LocalFileStore::new(tmp.path().to_path_buf()).unwrap();

        // Create a template file.
        let tpl_dir = tmp.path().join("templates");
        std::fs::create_dir_all(&tpl_dir).unwrap();
        std::fs::write(
            tpl_dir.join("note.tmpl"),
            "# ${1:Title}\n\n${2:Body}\n",
        )
        .unwrap();

        let mut state = EditorState::new("hello");
        state.vault_root = Some(tmp.path().to_path_buf());

        // Open template picker.
        state.open_picker_templates();
        assert!(state.active_picker.is_some());
        assert_eq!(state.active_picker.as_ref().unwrap().title, "New Page");

        // The picker should have one item ("note").
        let results = state.active_picker.as_ref().unwrap().inner.results();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].text, "note");

        // Press Enter to select the template.
        state.handle_key(Key::Enter);

        // Picker should be closed.
        assert!(state.active_picker.is_none());

        // New page file should exist.
        let page_path = tmp.path().join("pages").join("note.md");
        assert!(page_path.exists());

        // Buffer should contain expanded template content.
        let text = state.text();
        assert!(text.contains("# Title"));
        assert!(text.contains("Body"));
        // No unexpanded placeholders should remain.
        assert!(!text.contains("${"));

        // Tab stops should be set.
        assert!(!state.tab_stops.is_empty());
        assert_eq!(state.mode, Mode::Insert);
    }

    // ── Agenda overlay tests ────────────────────────────────────────────

    #[test]
    fn spc_a_a_opens_agenda() {
        let mut state = EditorState::new("hello");
        assert!(!state.agenda_active);
        // SPC a a
        send_leader_seq(&mut state, &['a', 'a']);
        assert!(state.agenda_active);
        assert!(state.agenda_view.is_some());
        assert_eq!(state.last_leader_action, Some(LeaderAction::AgendaView));
        let frame = state.render();
        assert!(frame.agenda.is_some());
    }

    #[test]
    fn agenda_jk_navigates() {
        let mut state = EditorState::new("hello");
        // Manually set up an agenda view with items
        state.agenda_view = Some(crate::agenda::AgendaView {
            overdue: vec![crate::agenda::AgendaItem {
                page_id: "p1".into(),
                path: std::path::PathBuf::from("a.md"),
                page_title: "Page A".into(),
                line: 0,
                text: "- [ ] task1 @due(2025-01-01)".into(),
                timestamp: crate::agenda::AgendaTimestamp {
                    kind: crate::agenda::TimestampKind::Due,
                    date: chrono::NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
                },
                completed: false,
                tags: vec![],
            }],
            today: vec![crate::agenda::AgendaItem {
                page_id: "p2".into(),
                path: std::path::PathBuf::from("b.md"),
                page_title: "Page B".into(),
                line: 0,
                text: "- [ ] task2 @due(2025-06-15)".into(),
                timestamp: crate::agenda::AgendaTimestamp {
                    kind: crate::agenda::TimestampKind::Due,
                    date: chrono::NaiveDate::from_ymd_opt(2025, 6, 15).unwrap(),
                },
                completed: false,
                tags: vec![],
            }],
            upcoming: vec![],
        });
        state.agenda_active = true;
        state.agenda_selected = 0;

        // j moves down
        state.handle_key(Key::Char('j'));
        assert_eq!(state.agenda_selected, 1);

        // k moves back up
        state.handle_key(Key::Char('k'));
        assert_eq!(state.agenda_selected, 0);

        // k at 0 stays at 0
        state.handle_key(Key::Char('k'));
        assert_eq!(state.agenda_selected, 0);
    }

    #[test]
    fn agenda_escape_closes() {
        let mut state = EditorState::new("hello");
        send_leader_seq(&mut state, &['a', 'a']);
        assert!(state.agenda_active);
        state.handle_key(Key::Escape);
        assert!(!state.agenda_active);
        assert!(state.agenda_view.is_none());
        let frame = state.render();
        assert!(frame.agenda.is_none());
    }

    #[test]
    fn agenda_x_toggles_task() {
        use crate::store::LocalFileStore;
        let tmp = tempfile::TempDir::new().unwrap();
        let store = LocalFileStore::new(tmp.path().to_path_buf()).unwrap();
        let page_path = store.pages_dir().join("task.md");
        store.write(&page_path, "---\nid: t001\ntitle: \"Tasks\"\ntags: []\n---\n\n- [ ] buy milk @due(2025-06-15)\n").unwrap();

        let db_path = tmp.path().join(".index").join("core.db");
        let mut index = crate::index::SqliteIndex::open(&db_path).unwrap();
        let doc = crate::parser::parse(&std::fs::read_to_string(&page_path).unwrap()).unwrap();
        index.index_document(&page_path, &doc).unwrap();

        let mut state = EditorState::new("hello");
        state.vault_root = Some(tmp.path().to_path_buf());
        state.index = Some(index);

        // Open agenda
        send_leader_seq(&mut state, &['a', 'a']);
        assert!(state.agenda_active);
        let total = state.agenda_total_items();
        assert!(total > 0);

        // Toggle task
        state.handle_key(Key::Char('x'));

        // The file should now have [x]
        let content = std::fs::read_to_string(&page_path).unwrap();
        assert!(content.contains("- [x] buy milk"));
    }

    #[test]
    fn agenda_enter_opens_source_page() {
        use crate::store::LocalFileStore;
        let tmp = tempfile::TempDir::new().unwrap();
        let store = LocalFileStore::new(tmp.path().to_path_buf()).unwrap();
        let page_path = store.pages_dir().join("nav.md");
        store.write(&page_path, "---\nid: n001\ntitle: \"Nav\"\ntags: []\n---\n\nline0\n- [ ] task @due(2025-06-15)\nline2\n").unwrap();

        let db_path = tmp.path().join(".index").join("core.db");
        let mut index = crate::index::SqliteIndex::open(&db_path).unwrap();
        let doc = crate::parser::parse(&std::fs::read_to_string(&page_path).unwrap()).unwrap();
        index.index_document(&page_path, &doc).unwrap();

        let mut state = EditorState::new("hello");
        state.vault_root = Some(tmp.path().to_path_buf());
        state.index = Some(index);

        // Open agenda
        send_leader_seq(&mut state, &['a', 'a']);
        assert!(state.agenda_active);

        // Press Enter to jump to source
        state.handle_key(Key::Enter);
        assert!(!state.agenda_active);
        // The buffer should now contain the page content
        assert!(state.text().contains("task @due(2025-06-15)"));
    }
}
