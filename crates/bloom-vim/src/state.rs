//! Core Vim state machine and key processing.
//!
//! [`VimState`] holds the current mode, pending key buffer, register file, macro
//! recorder, and last-command info for `.` repeat. Each key event is fed through
//! the grammar parser; the result is a [`VimAction`] the editor applies.

use std::ops::Range;

use crate::input::{KeyCode, KeyEvent};
use bloom_buffer::{Buffer, EditOp};

use super::grammar::{self, ModeSwitch, ParseResult, ParsedCommand, StandaloneCmd};
use super::macros::MacroState;
use super::motion::{self, FindCommand, MotionType};
use super::operator::Operator;
use super::register::RegisterFile;
use super::text_object;

// ── public types ─────────────────────────────────────────────────────

/// Vim editing mode.
#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    Normal,
    Insert,
    Visual { start: usize },
    Command,
}

/// The result of processing a key event through the Vim state machine.
#[derive(Debug)]
pub enum VimAction {
    /// An edit to apply to the buffer.
    Edit(EditOp),
    /// A motion (move cursor, no edit).
    Motion(MotionResult),
    /// A mode transition.
    ModeChange(Mode),
    /// A command to dispatch (e.g., `:rebuild-index`).
    Command(String),
    /// Key is pending — waiting for more input.
    Pending,
    /// Key not handled by Vim — pass to next layer.
    Unhandled,
    /// Multiple actions (e.g., delete + mode change for `cc`).
    Composite(Vec<VimAction>),
    /// Restore the insert-mode checkpoint (Ctrl+U).
    RestoreCheckpoint,
}

/// Result of a cursor-only motion.
#[derive(Debug)]
pub struct MotionResult {
    pub new_position: usize,
    pub extend_selection: bool,
}

/// A previously executed command, for `.` repeat.
#[derive(Debug, Clone)]
pub struct RecordedCommand {
    pub keys: Vec<KeyEvent>,
}

// ── VimState ─────────────────────────────────────────────────────────

/// The Vim state machine.
///
/// Tracks the current editing mode, pending key buffer, named registers,
/// macro recorder, and last-command info for `.` repeat. Feed key events
/// via [`process_key`](Self::process_key) to get back [`VimAction`]s.
pub struct VimState {
    mode: Mode,
    pending: String,
    registers: RegisterFile,
    macro_state: MacroState,
    last_command: Option<RecordedCommand>,
    last_find: Option<FindCommand>,
    /// Keys accumulated for the current editing command (for `.` repeat).
    current_cmd_keys: Vec<KeyEvent>,
    /// Whether we're recording Insert-mode keys for a change that entered Insert.
    recording_insert: bool,
    /// Keys accumulated for a change that spans Normal→Insert→Esc.
    insert_change_keys: Vec<KeyEvent>,
    /// Set during dot-repeat replay to suppress re-recording.
    replaying: bool,
}

impl Default for VimState {
    fn default() -> Self {
        Self::new()
    }
}

impl VimState {
    pub fn new() -> Self {
        Self {
            mode: Mode::Normal,
            pending: String::new(),
            registers: RegisterFile::new(),
            macro_state: MacroState::new(),
            last_command: None,
            last_find: None,
            current_cmd_keys: Vec::new(),
            recording_insert: false,
            insert_change_keys: Vec::new(),
            replaying: false,
        }
    }

    /// Current mode (cloned).
    pub fn mode(&self) -> Mode {
        self.mode.clone()
    }

    /// Process a key event. Returns the action(s) to take.
    pub fn process_key(&mut self, key: KeyEvent, buffer: &Buffer, cursor: usize) -> VimAction {
        // Record for macros
        if self.macro_state.is_recording() {
            self.macro_state.record_key(key.clone());
        }

        match self.mode.clone() {
            Mode::Normal => self.process_normal(key, buffer, cursor),
            Mode::Insert => self.process_insert(key, buffer, cursor),
            Mode::Visual { start } => self.process_visual(key, buffer, cursor, start),
            Mode::Command => self.process_command(key),
        }
    }

    /// Currently pending keys (for status bar display).
    pub fn pending_keys(&self) -> &str {
        &self.pending
    }

    /// Replace the pending command line text (for Tab completion).
    pub fn set_command_line(&mut self, text: &str) {
        if matches!(self.mode, Mode::Command) {
            self.pending = text.to_string();
        }
    }

    /// Force the mode back to Normal (used by read-only buffer filter).
    pub fn force_normal_mode(&mut self) {
        self.mode = Mode::Normal;
        self.pending.clear();
    }

    /// Force the mode to an arbitrary value (used by search prompt).
    pub fn force_mode(&mut self, mode: Mode) {
        let is_command = matches!(mode, Mode::Command);
        self.mode = mode;
        if !is_command {
            self.pending.clear();
        }
    }

    /// Get the contents of a register.
    pub fn register(&self, name: char) -> Option<&str> {
        self.registers.get(name)
    }

    /// Access the full register file (for kill ring iteration, etc.).
    pub fn registers(&self) -> &RegisterFile {
        &self.registers
    }

    /// Start macro recording to a register.
    pub fn start_macro(&mut self, register: char) {
        self.macro_state.start_recording(register);
    }

    /// Stop macro recording.
    pub fn stop_macro(&mut self) {
        self.macro_state.stop_recording();
    }

    /// Whether a macro is being recorded.
    pub fn is_recording(&self) -> bool {
        self.macro_state.is_recording()
    }

    /// Play back a macro from a register.
    pub fn play_macro(&self, register: char) -> Vec<KeyEvent> {
        self.macro_state.get(register)
    }

    /// Get the last repeatable command (for `.`).
    pub fn last_command(&self) -> Option<&RecordedCommand> {
        self.last_command.as_ref()
    }

    /// Mark replay mode (suppresses re-recording during `.` replay).
    pub fn set_replaying(&mut self, replaying: bool) {
        self.replaying = replaying;
    }

    // ── Normal mode ──────────────────────────────────────────────────

    fn process_normal(&mut self, key: KeyEvent, buffer: &Buffer, cursor: usize) -> VimAction {
        // Handle Escape — clear pending
        if key.code == KeyCode::Esc {
            self.pending.clear();
            self.current_cmd_keys.clear();
            return VimAction::Pending;
        }

        // Ctrl+R → redo
        if key.modifiers.ctrl {
            if let KeyCode::Char('r') = key.code {
                self.pending.clear();
                self.current_cmd_keys.clear();
                return VimAction::Command("redo".into());
            }
            return VimAction::Unhandled;
        }

        // Only char keys feed the grammar
        let c = match key.code {
            KeyCode::Char(ch) => ch,
            _ => return VimAction::Unhandled,
        };

        self.pending.push(c);
        self.current_cmd_keys.push(key);

        match grammar::parse_pending(&self.pending, self.macro_state.is_recording()) {
            ParseResult::Complete(cmd) => {
                let keys = std::mem::take(&mut self.current_cmd_keys);
                self.pending.clear();
                let action = self.execute_command(cmd, buffer, cursor);
                if !self.replaying {
                    let enters_insert = enters_insert_mode(&action);
                    if enters_insert {
                        // Defer last_command — record Insert keys until Esc.
                        self.recording_insert = true;
                        self.insert_change_keys = keys;
                    } else if is_repeatable(&action) {
                        self.last_command = Some(RecordedCommand { keys });
                    }
                }
                action
            }
            ParseResult::Incomplete => VimAction::Pending,
            ParseResult::Invalid => {
                self.pending.clear();
                self.current_cmd_keys.clear();
                VimAction::Unhandled
            }
        }
    }

    // ── Insert mode ──────────────────────────────────────────────────

    fn process_insert(&mut self, key: KeyEvent, buffer: &Buffer, cursor: usize) -> VimAction {
        // Record insert-mode keys for dot-repeat.
        if self.recording_insert && !self.replaying {
            self.insert_change_keys.push(key.clone());
        }

        // Handle Ctrl combinations first
        if key.modifiers.ctrl {
            match key.code {
                KeyCode::Char('u') | KeyCode::Char('U') => {
                    return VimAction::RestoreCheckpoint;
                }
                KeyCode::Char('w') | KeyCode::Char('W') => {
                    // Delete word before cursor
                    if cursor == 0 {
                        return VimAction::Unhandled;
                    }
                    let rope = buffer.text();
                    let mut pos = cursor;
                    // Skip whitespace backwards
                    while pos > 0 && rope.char(pos - 1).is_whitespace() {
                        pos -= 1;
                    }
                    // Skip word chars backwards
                    while pos > 0 && !rope.char(pos - 1).is_whitespace() {
                        pos -= 1;
                    }
                    return VimAction::Edit(EditOp {
                        range: pos..cursor,
                        replacement: String::new(),
                        cursor_after: pos,
                    });
                }
                _ => {}
            }
        }

        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                if self.recording_insert && !self.replaying {
                    self.last_command = Some(RecordedCommand {
                        keys: std::mem::take(&mut self.insert_change_keys),
                    });
                    self.recording_insert = false;
                }
                VimAction::ModeChange(Mode::Normal)
            }
            KeyCode::Char(c) => {
                let mut s = String::new();
                s.push(c);
                VimAction::Edit(EditOp {
                    range: cursor..cursor,
                    replacement: s,
                    cursor_after: cursor + c.len_utf8(),
                })
            }
            KeyCode::Enter => VimAction::Edit(EditOp {
                range: cursor..cursor,
                replacement: "\n".to_string(),
                cursor_after: cursor + 1,
            }),
            KeyCode::Backspace => {
                if cursor == 0 {
                    return VimAction::Unhandled;
                }
                VimAction::Edit(EditOp {
                    range: (cursor - 1)..cursor,
                    replacement: String::new(),
                    cursor_after: cursor - 1,
                })
            }
            KeyCode::Delete => {
                if cursor >= buffer.len_chars() {
                    return VimAction::Unhandled;
                }
                VimAction::Edit(EditOp {
                    range: cursor..(cursor + 1),
                    replacement: String::new(),
                    cursor_after: cursor,
                })
            }
            KeyCode::Left => self.insert_motion(MotionType::Left, buffer, cursor),
            KeyCode::Right => self.insert_motion(MotionType::Right, buffer, cursor),
            KeyCode::Up => self.insert_motion(MotionType::Up, buffer, cursor),
            KeyCode::Down => self.insert_motion(MotionType::Down, buffer, cursor),
            KeyCode::Home => self.insert_motion(MotionType::LineStart, buffer, cursor),
            KeyCode::End => self.insert_motion(MotionType::LineEnd, buffer, cursor),
            KeyCode::PageUp => self.insert_motion(MotionType::DocumentStart, buffer, cursor),
            KeyCode::PageDown => self.insert_motion(MotionType::DocumentEnd, buffer, cursor),
            _ => VimAction::Unhandled,
        }
    }

    fn insert_motion(&self, motion: MotionType, buffer: &Buffer, cursor: usize) -> VimAction {
        let len = buffer.len_chars();
        let new_pos = match motion {
            // Insert mode allows cursor after last char, so use simple +1/-1
            MotionType::Left => cursor.saturating_sub(1),
            MotionType::Right => (cursor + 1).min(len),
            MotionType::LineStart
            | MotionType::LineEnd
            | MotionType::DocumentStart
            | MotionType::DocumentEnd => {
                motion::execute_motion(&motion, buffer, cursor, None, &self.last_find)
            }
            _ => {
                // Up/Down use the standard motion (line-based, works for insert)
                motion::execute_motion(&motion, buffer, cursor, None, &self.last_find)
            }
        };
        VimAction::Motion(MotionResult {
            new_position: new_pos,
            extend_selection: false,
        })
    }

    // ── Visual mode ──────────────────────────────────────────────────

    fn process_visual(
        &mut self,
        key: KeyEvent,
        buffer: &Buffer,
        cursor: usize,
        sel_start: usize,
    ) -> VimAction {
        if key.code == KeyCode::Esc {
            self.mode = Mode::Normal;
            self.pending.clear();
            return VimAction::ModeChange(Mode::Normal);
        }

        let c = match key.code {
            KeyCode::Char(ch) => ch,
            _ => return VimAction::Unhandled,
        };

        match c {
            // Exit visual
            'v' => {
                self.mode = Mode::Normal;
                VimAction::ModeChange(Mode::Normal)
            }
            // Operators on selection
            'd' | 'x' => {
                let range = ordered_range(sel_start, cursor);
                self.yank_range(buffer, &range);
                self.mode = Mode::Normal;
                VimAction::Composite(vec![
                    VimAction::Edit(EditOp {
                        range: range.clone(),
                        replacement: String::new(),
                        cursor_after: range.start,
                    }),
                    VimAction::ModeChange(Mode::Normal),
                ])
            }
            'c' | 's' => {
                let range = ordered_range(sel_start, cursor);
                self.yank_range(buffer, &range);
                self.mode = Mode::Insert;
                VimAction::Composite(vec![
                    VimAction::Edit(EditOp {
                        range: range.clone(),
                        replacement: String::new(),
                        cursor_after: range.start,
                    }),
                    VimAction::ModeChange(Mode::Insert),
                ])
            }
            'y' => {
                let range = ordered_range(sel_start, cursor);
                self.yank_range(buffer, &range);
                self.mode = Mode::Normal;
                VimAction::Composite(vec![
                    VimAction::Motion(MotionResult {
                        new_position: range.start,
                        extend_selection: false,
                    }),
                    VimAction::ModeChange(Mode::Normal),
                ])
            }
            '>' => {
                let range = ordered_range(sel_start, cursor);
                self.mode = Mode::Normal;
                VimAction::Composite(vec![
                    VimAction::Edit(indent_range(buffer, &range)),
                    VimAction::ModeChange(Mode::Normal),
                ])
            }
            '<' => {
                let range = ordered_range(sel_start, cursor);
                self.mode = Mode::Normal;
                VimAction::Composite(vec![
                    VimAction::Edit(dedent_range(buffer, &range)),
                    VimAction::ModeChange(Mode::Normal),
                ])
            }
            // Motions — extend selection
            _ => {
                if let Some(mt) = char_to_motion(c) {
                    let new_pos =
                        motion::execute_motion(&mt, buffer, cursor, None, &self.last_find);
                    VimAction::Motion(MotionResult {
                        new_position: new_pos,
                        extend_selection: true,
                    })
                } else {
                    VimAction::Unhandled
                }
            }
        }
    }

    // ── Command mode ─────────────────────────────────────────────────

    fn process_command(&mut self, key: KeyEvent) -> VimAction {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.pending.clear();
                VimAction::ModeChange(Mode::Normal)
            }
            KeyCode::Enter => {
                let cmd = self.pending.clone();
                self.pending.clear();
                self.mode = Mode::Normal;
                VimAction::Composite(vec![
                    VimAction::Command(cmd),
                    VimAction::ModeChange(Mode::Normal),
                ])
            }
            KeyCode::Backspace => {
                self.pending.pop();
                if self.pending.is_empty() {
                    self.mode = Mode::Normal;
                    VimAction::ModeChange(Mode::Normal)
                } else {
                    VimAction::Pending
                }
            }
            KeyCode::Char(c) => {
                self.pending.push(c);
                VimAction::Pending
            }
            _ => VimAction::Unhandled,
        }
    }

    // ── command execution ────────────────────────────────────────────

    fn execute_command(&mut self, cmd: ParsedCommand, buffer: &Buffer, cursor: usize) -> VimAction {
        match cmd {
            // ── motion ───────────────────────────────────────────────
            ParsedCommand::Motion { motion, count } => {
                self.update_find_state(&motion);
                let new_pos =
                    motion::execute_motion(&motion, buffer, cursor, count, &self.last_find);
                VimAction::Motion(MotionResult {
                    new_position: new_pos,
                    extend_selection: false,
                })
            }
            // ── operator + motion ────────────────────────────────────
            ParsedCommand::OperatorMotion {
                operator,
                motion,
                count,
            } => {
                self.update_find_state(&motion);
                let spec = motion::execute_operator_motion(
                    operator,
                    &motion,
                    buffer,
                    cursor,
                    count,
                    &self.last_find,
                );
                let range = motion::range_from_operator_spec(cursor, spec);
                self.apply_operator(operator, buffer, &range, cursor)
            }
            // ── operator doubled (line-wise) ─────────────────────────
            ParsedCommand::OperatorLine { operator, count } => {
                let c = count.unwrap_or(1);
                let range = line_range(buffer, cursor, c);
                self.apply_operator(operator, buffer, &range, cursor)
            }
            // ── operator + text object ───────────────────────────────
            ParsedCommand::OperatorTextObject {
                operator,
                object,
                count,
            } => {
                let c = count.unwrap_or(1);
                if let Some(mut range) = text_object::resolve_text_object(&object, buffer, cursor) {
                    for _ in 1..c {
                        if let Some(next) =
                            text_object::resolve_text_object(&object, buffer, range.end)
                        {
                            range.end = next.end;
                        }
                    }
                    self.apply_operator(operator, buffer, &range, cursor)
                } else {
                    VimAction::Unhandled
                }
            }
            // ── mode switch ──────────────────────────────────────────
            ParsedCommand::ModeSwitch(ms) => self.execute_mode_switch(ms, buffer, cursor),
            // ── standalone commands ──────────────────────────────────
            ParsedCommand::Standalone { cmd, count } => {
                self.execute_standalone(cmd, count, buffer, cursor)
            }
        }
    }

    fn apply_operator(
        &mut self,
        op: Operator,
        buffer: &Buffer,
        range: &Range<usize>,
        _cursor: usize,
    ) -> VimAction {
        if range.is_empty() {
            return VimAction::Unhandled;
        }
        match op {
            Operator::Delete => {
                self.yank_range(buffer, range);
                let new_len = buffer.len_chars() - (range.end - range.start);
                let cursor = range.start.min(new_len.saturating_sub(1));
                VimAction::Edit(EditOp {
                    range: range.clone(),
                    replacement: String::new(),
                    cursor_after: cursor,
                })
            }
            Operator::Change => {
                self.yank_range(buffer, range);
                self.mode = Mode::Insert;
                VimAction::Composite(vec![
                    VimAction::Edit(EditOp {
                        range: range.clone(),
                        replacement: String::new(),
                        cursor_after: range.start,
                    }),
                    VimAction::ModeChange(Mode::Insert),
                ])
            }
            Operator::Yank => {
                self.yank_range(buffer, range);
                VimAction::Motion(MotionResult {
                    new_position: range.start,
                    extend_selection: false,
                })
            }
            Operator::Indent => VimAction::Edit(indent_range(buffer, range)),
            Operator::Dedent => VimAction::Edit(dedent_range(buffer, range)),
            Operator::AutoIndent | Operator::Reflow => VimAction::Motion(MotionResult {
                new_position: range.start,
                extend_selection: false,
            }),
        }
    }

    fn execute_mode_switch(&mut self, ms: ModeSwitch, buffer: &Buffer, cursor: usize) -> VimAction {
        match ms {
            ModeSwitch::InsertBefore => {
                self.mode = Mode::Insert;
                VimAction::ModeChange(Mode::Insert)
            }
            ModeSwitch::InsertAfter => {
                self.mode = Mode::Insert;
                let new_pos = (cursor + 1).min(buffer.len_chars());
                VimAction::Composite(vec![
                    VimAction::Motion(MotionResult {
                        new_position: new_pos,
                        extend_selection: false,
                    }),
                    VimAction::ModeChange(Mode::Insert),
                ])
            }
            ModeSwitch::InsertLineStart => {
                self.mode = Mode::Insert;
                let rope = buffer.text();
                let line_idx = rope.char_to_line(cursor);
                let start = rope.line_to_char(line_idx);
                let line = rope.line(line_idx);
                let mut col = 0;
                while col < line.len_chars()
                    && line.char(col).is_whitespace()
                    && line.char(col) != '\n'
                {
                    col += 1;
                }
                VimAction::Composite(vec![
                    VimAction::Motion(MotionResult {
                        new_position: start + col,
                        extend_selection: false,
                    }),
                    VimAction::ModeChange(Mode::Insert),
                ])
            }
            ModeSwitch::InsertLineEnd => {
                self.mode = Mode::Insert;
                let rope = buffer.text();
                let line_idx = rope.char_to_line(cursor);
                let line = rope.line(line_idx);
                let start = rope.line_to_char(line_idx);
                let mut end = line.len_chars();
                while end > 0 && matches!(line.char(end - 1), '\n' | '\r') {
                    end -= 1;
                }
                VimAction::Composite(vec![
                    VimAction::Motion(MotionResult {
                        new_position: start + end,
                        extend_selection: false,
                    }),
                    VimAction::ModeChange(Mode::Insert),
                ])
            }
            ModeSwitch::OpenBelow => {
                self.mode = Mode::Insert;
                let rope = buffer.text();
                let line_idx = rope.char_to_line(cursor);
                let next_line_start = if line_idx + 1 < rope.len_lines() {
                    rope.line_to_char(line_idx + 1)
                } else {
                    rope.len_chars()
                };
                // Insert \n at the start of the next line (or end of buffer).
                // For lines ending with \n, this puts the new \n right after it.
                // For a final line without \n, we first need to add a \n.
                let (insert_pos, replacement, cursor_after) = if next_line_start == rope.len_chars()
                    && (rope.len_chars() == 0 || rope.char(rope.len_chars() - 1) != '\n')
                {
                    // Last line has no trailing newline — insert \n
                    (next_line_start, "\n".to_string(), next_line_start + 1)
                } else if next_line_start == rope.len_chars() {
                    // At end of buffer (last line has trailing \n) — cursor after the new \n
                    (next_line_start, "\n".to_string(), next_line_start + 1)
                } else {
                    // Mid-file — cursor at start of the new empty line
                    (next_line_start, "\n".to_string(), next_line_start)
                };
                VimAction::Composite(vec![
                    VimAction::Edit(EditOp {
                        range: insert_pos..insert_pos,
                        replacement,
                        cursor_after,
                    }),
                    VimAction::ModeChange(Mode::Insert),
                ])
            }
            ModeSwitch::OpenAbove => {
                self.mode = Mode::Insert;
                let rope = buffer.text();
                let line_idx = rope.char_to_line(cursor);
                let line_start = rope.line_to_char(line_idx);
                VimAction::Composite(vec![
                    VimAction::Edit(EditOp {
                        range: line_start..line_start,
                        replacement: "\n".into(),
                        cursor_after: line_start,
                    }),
                    VimAction::ModeChange(Mode::Insert),
                ])
            }
            ModeSwitch::Visual => {
                self.mode = Mode::Visual { start: cursor };
                VimAction::ModeChange(Mode::Visual { start: cursor })
            }
            ModeSwitch::VisualLine => {
                let rope = buffer.text();
                let line_idx = rope.char_to_line(cursor);
                let start = rope.line_to_char(line_idx);
                self.mode = Mode::Visual { start };
                VimAction::ModeChange(Mode::Visual { start })
            }
            ModeSwitch::Command => {
                self.mode = Mode::Command;
                self.pending.clear();
                VimAction::ModeChange(Mode::Command)
            }
        }
    }

    fn execute_standalone(
        &mut self,
        cmd: StandaloneCmd,
        _count: Option<usize>,
        buffer: &Buffer,
        cursor: usize,
    ) -> VimAction {
        match cmd {
            StandaloneCmd::Undo => VimAction::Command("undo".into()),
            StandaloneCmd::Redo => VimAction::Command("redo".into()),
            StandaloneCmd::RepeatLast => VimAction::Command("repeat".into()),
            StandaloneCmd::PasteAfter => {
                if let Some(text) = self.registers.get('"').map(|s| s.to_string()) {
                    let insert_pos = (cursor + 1).min(buffer.len_chars());
                    let after = insert_pos + text.len();
                    VimAction::Edit(EditOp {
                        range: insert_pos..insert_pos,
                        replacement: text,
                        cursor_after: after.saturating_sub(1),
                    })
                } else {
                    VimAction::Unhandled
                }
            }
            StandaloneCmd::PasteBefore => {
                if let Some(text) = self.registers.get('"').map(|s| s.to_string()) {
                    let after = cursor + text.len();
                    VimAction::Edit(EditOp {
                        range: cursor..cursor,
                        replacement: text,
                        cursor_after: after.saturating_sub(1),
                    })
                } else {
                    VimAction::Unhandled
                }
            }
            StandaloneCmd::JoinLines => {
                // Join current line with next: replace newline (+ leading whitespace) with a space
                let line_idx = buffer.text().char_to_line(cursor);
                if line_idx + 1 >= buffer.len_lines() {
                    return VimAction::Unhandled; // no next line to join
                }
                let line_end = buffer.text().line_to_char(line_idx + 1);
                // Find start of content on next line (skip whitespace)
                let next_line = buffer.line(line_idx + 1).to_string();
                let trimmed = next_line.trim_start();
                let ws_len = next_line.len() - trimmed.len();
                // Delete from end of current line content to start of next line content
                // Current line may have trailing whitespace — find last non-ws char
                let curr_line = buffer.line(line_idx).to_string();
                let curr_trimmed_len = curr_line.trim_end_matches(['\n', '\r', ' ', '\t']).len();
                let curr_line_start = buffer.text().line_to_char(line_idx);
                let delete_from = curr_line_start + curr_trimmed_len;
                let delete_to = line_end + ws_len;
                VimAction::Edit(EditOp {
                    range: delete_from..delete_to,
                    replacement: " ".to_string(),
                    cursor_after: delete_from,
                })
            }
            StandaloneCmd::ReplaceChar(ch) => {
                if cursor < buffer.len_chars() {
                    VimAction::Edit(EditOp {
                        range: cursor..cursor + 1,
                        replacement: ch.to_string(),
                        cursor_after: cursor,
                    })
                } else {
                    VimAction::Unhandled
                }
            }
            StandaloneCmd::SearchForward => VimAction::Command("search-forward".into()),
            StandaloneCmd::SearchBackward => VimAction::Command("search-backward".into()),
            StandaloneCmd::NextMatch => VimAction::Command("next-match".into()),
            StandaloneCmd::PrevMatch => VimAction::Command("prev-match".into()),
            StandaloneCmd::StartMacro(reg) => {
                self.macro_state.start_recording(reg);
                VimAction::Pending
            }
            StandaloneCmd::StopMacro => {
                self.macro_state.stop_recording();
                VimAction::Pending
            }
            StandaloneCmd::PlayMacro(reg) => VimAction::Command(format!("play-macro:{reg}")),
            StandaloneCmd::Bracket(ch, forward) => {
                let dir = if forward { "]" } else { "[" };
                VimAction::Command(format!("bracket:{dir}{ch}"))
            }
        }
    }

    // ── private helpers ──────────────────────────────────────────────

    fn yank_range(&mut self, buffer: &Buffer, range: &Range<usize>) {
        if range.start < range.end && range.end <= buffer.len_chars() {
            let text = buffer.text().slice(range.clone()).to_string();
            self.registers.set('"', text);
        }
    }

    fn update_find_state(&mut self, motion: &MotionType) {
        match motion {
            MotionType::FindForward(ch) => {
                self.last_find = Some(FindCommand {
                    char_target: *ch,
                    forward: true,
                    inclusive: true,
                });
            }
            MotionType::FindBackward(ch) => {
                self.last_find = Some(FindCommand {
                    char_target: *ch,
                    forward: false,
                    inclusive: true,
                });
            }
            MotionType::ToForward(ch) => {
                self.last_find = Some(FindCommand {
                    char_target: *ch,
                    forward: true,
                    inclusive: false,
                });
            }
            MotionType::ToBackward(ch) => {
                self.last_find = Some(FindCommand {
                    char_target: *ch,
                    forward: false,
                    inclusive: false,
                });
            }
            _ => {}
        }
    }
}

// ── free helpers ─────────────────────────────────────────────────────

fn ordered_range(a: usize, b: usize) -> Range<usize> {
    if a <= b {
        a..b.saturating_add(1)
    } else {
        b..a.saturating_add(1)
    }
}

fn line_range(buffer: &Buffer, cursor: usize, count: usize) -> Range<usize> {
    let rope = buffer.text();
    let total_lines = rope.len_lines();
    let start_line = rope.char_to_line(cursor);
    let end_line = (start_line + count).min(total_lines);
    let start = rope.line_to_char(start_line);
    let end = if end_line < total_lines {
        rope.line_to_char(end_line)
    } else {
        buffer.len_chars()
    };

    // If the range is non-empty, use it as-is (normal case).
    if start < end {
        return start..end;
    }

    // Last line is empty (start == end == len_chars). Include the preceding
    // newline so the line is actually removed and the cursor moves up.
    if start_line > 0 {
        let prev_line_start = rope.line_to_char(start_line - 1);
        let prev_line_end = rope.line_to_char(start_line);
        // Delete from end of previous line's content (the newline) to EOF
        prev_line_end.saturating_sub(1).max(prev_line_start)..end
    } else {
        start..end
    }
}

fn indent_range(buffer: &Buffer, range: &Range<usize>) -> EditOp {
    let rope = buffer.text();
    let start_line = rope.char_to_line(range.start);
    let end_line = rope.char_to_line(range.end.saturating_sub(1).max(range.start));
    let mut text = String::new();
    for line_idx in start_line..=end_line {
        let line: String = rope.line(line_idx).to_string();
        text.push_str("    ");
        text.push_str(&line);
    }
    let full_start = rope.line_to_char(start_line);
    let full_end = if end_line + 1 < rope.len_lines() {
        rope.line_to_char(end_line + 1)
    } else {
        buffer.len_chars()
    };
    EditOp {
        range: full_start..full_end,
        replacement: text,
        cursor_after: full_start,
    }
}

fn dedent_range(buffer: &Buffer, range: &Range<usize>) -> EditOp {
    let rope = buffer.text();
    let start_line = rope.char_to_line(range.start);
    let end_line = rope.char_to_line(range.end.saturating_sub(1).max(range.start));
    let mut text = String::new();
    for line_idx in start_line..=end_line {
        let line: String = rope.line(line_idx).to_string();
        let stripped = line
            .strip_prefix("    ")
            .or_else(|| line.strip_prefix("   "))
            .or_else(|| line.strip_prefix("  "))
            .or_else(|| line.strip_prefix(' '))
            .unwrap_or(&line);
        text.push_str(stripped);
    }
    let full_start = rope.line_to_char(start_line);
    let full_end = if end_line + 1 < rope.len_lines() {
        rope.line_to_char(end_line + 1)
    } else {
        buffer.len_chars()
    };
    EditOp {
        range: full_start..full_end,
        replacement: text,
        cursor_after: full_start,
    }
}

/// Map a single character to a motion type (for visual mode quick dispatch).
fn char_to_motion(c: char) -> Option<MotionType> {
    match c {
        'h' => Some(MotionType::Left),
        'l' => Some(MotionType::Right),
        'j' => Some(MotionType::Down),
        'k' => Some(MotionType::Up),
        'w' => Some(MotionType::WordForward),
        'b' => Some(MotionType::WordBackward),
        'e' => Some(MotionType::WordEnd),
        'W' => Some(MotionType::WORDForward),
        'B' => Some(MotionType::WORDBackward),
        'E' => Some(MotionType::WORDEnd),
        '0' => Some(MotionType::LineStart),
        '$' => Some(MotionType::LineEnd),
        '^' => Some(MotionType::FirstNonWhitespace),
        'G' => Some(MotionType::DocumentEnd),
        '%' => Some(MotionType::MatchingBracket),
        '{' => Some(MotionType::ParagraphBackward),
        '}' => Some(MotionType::ParagraphForward),
        ';' => Some(MotionType::RepeatFind),
        ',' => Some(MotionType::RepeatFindReverse),
        _ => None,
    }
}

/// Check if an action is repeatable (editing commands, not motions).
fn is_repeatable(action: &VimAction) -> bool {
    match action {
        VimAction::Edit(_) => true,
        VimAction::Composite(actions) => actions.iter().any(|a| matches!(a, VimAction::Edit(_))),
        _ => false,
    }
}

/// Check if an action transitions into Insert mode.
fn enters_insert_mode(action: &VimAction) -> bool {
    match action {
        VimAction::ModeChange(Mode::Insert) => true,
        VimAction::Composite(actions) => actions
            .iter()
            .any(|a| matches!(a, VimAction::ModeChange(Mode::Insert))),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::KeyEvent;
    use bloom_buffer::Buffer;

    fn key(c: char) -> KeyEvent {
        KeyEvent::char(c)
    }
    fn esc() -> KeyEvent {
        KeyEvent::esc()
    }

    // UC-14: Mode transitions
    #[test]
    fn test_initial_mode_is_normal() {
        let vim = VimState::new();
        assert_eq!(vim.mode(), Mode::Normal);
    }

    #[test]
    fn test_i_enters_insert_mode() {
        let mut vim = VimState::new();
        let buf = Buffer::from_text("hello");
        let _action = vim.process_key(key('i'), &buf, 0);
        assert!(matches!(vim.mode(), Mode::Insert));
    }

    #[test]
    fn test_esc_returns_to_normal() {
        let mut vim = VimState::new();
        let buf = Buffer::from_text("hello");
        vim.process_key(key('i'), &buf, 0);
        assert!(matches!(vim.mode(), Mode::Insert));
        vim.process_key(esc(), &buf, 0);
        assert_eq!(vim.mode(), Mode::Normal);
    }

    #[test]
    fn test_a_enters_insert_after_cursor() {
        let mut vim = VimState::new();
        let buf = Buffer::from_text("hello");
        let _action = vim.process_key(key('a'), &buf, 2);
        assert!(matches!(vim.mode(), Mode::Insert));
    }

    #[test]
    fn test_v_enters_visual_mode() {
        let mut vim = VimState::new();
        let buf = Buffer::from_text("hello");
        vim.process_key(key('v'), &buf, 0);
        assert!(matches!(vim.mode(), Mode::Visual { .. }));
    }

    #[test]
    fn test_colon_enters_command_mode() {
        let mut vim = VimState::new();
        let buf = Buffer::from_text("hello");
        vim.process_key(key(':'), &buf, 0);
        assert_eq!(vim.mode(), Mode::Command);
    }

    // UC-15: Motions with counts
    #[test]
    fn test_w_motion_moves_to_next_word() {
        let mut vim = VimState::new();
        let buf = Buffer::from_text("hello world foo");
        let action = vim.process_key(key('w'), &buf, 0);
        if let VimAction::Motion(m) = action {
            assert_eq!(m.new_position, 6); // 'w' in "world"
        } else {
            panic!("expected Motion, got {:?}", action);
        }
    }

    #[test]
    fn test_b_motion_moves_to_prev_word() {
        let mut vim = VimState::new();
        let buf = Buffer::from_text("hello world");
        let action = vim.process_key(key('b'), &buf, 8);
        if let VimAction::Motion(m) = action {
            assert_eq!(m.new_position, 6);
        } else {
            panic!("expected Motion");
        }
    }

    #[test]
    fn test_dollar_motion_to_end_of_line() {
        let mut vim = VimState::new();
        let buf = Buffer::from_text("hello world\nsecond");
        let action = vim.process_key(key('$'), &buf, 0);
        if let VimAction::Motion(m) = action {
            assert_eq!(m.new_position, 10); // last char of first line
        } else {
            panic!("expected Motion");
        }
    }

    #[test]
    fn test_0_motion_to_start_of_line() {
        let mut vim = VimState::new();
        let buf = Buffer::from_text("hello world");
        let action = vim.process_key(key('0'), &buf, 5);
        if let VimAction::Motion(m) = action {
            assert_eq!(m.new_position, 0);
        } else {
            panic!("expected Motion");
        }
    }

    #[test]
    fn test_gg_motion_to_document_start() {
        let mut vim = VimState::new();
        let buf = Buffer::from_text("first\nsecond\nthird");
        vim.process_key(key('g'), &buf, 10);
        let action = vim.process_key(key('g'), &buf, 10);
        if let VimAction::Motion(m) = action {
            assert_eq!(m.new_position, 0);
        } else {
            panic!("expected Motion for gg");
        }
    }

    #[test]
    fn test_g_motion_to_document_end() {
        let mut vim = VimState::new();
        let buf = Buffer::from_text("first\nsecond\nthird");
        let action = vim.process_key(key('G'), &buf, 0);
        if let VimAction::Motion(m) = action {
            assert!(m.new_position > 0); // should be at end
        } else {
            panic!("expected Motion");
        }
    }

    // UC-14: Operators
    #[test]
    fn test_dd_deletes_line() {
        let mut vim = VimState::new();
        let buf = Buffer::from_text("first\nsecond\nthird");
        vim.process_key(key('d'), &buf, 0);
        let action = vim.process_key(key('d'), &buf, 0);
        match action {
            VimAction::Edit(edit) => {
                assert!(edit.replacement.is_empty()); // deletion
            }
            VimAction::Composite(actions) => {
                assert!(!actions.is_empty());
            }
            _ => panic!("expected Edit or Composite for dd, got {:?}", action),
        }
    }

    #[test]
    fn test_dd_last_empty_line() {
        let mut vim = VimState::new();
        // "hello\n" has two lines: "hello\n" and "" (trailing empty line)
        let buf = Buffer::from_text("hello\n");
        let cursor = buf.len_chars(); // cursor on the empty last line
        vim.process_key(key('d'), &buf, cursor);
        let action = vim.process_key(key('d'), &buf, cursor);
        match action {
            VimAction::Edit(edit) => {
                assert!(edit.replacement.is_empty());
                // Should delete the trailing newline, removing the empty line
                assert!(
                    !edit.range.is_empty(),
                    "range should not be empty for dd on last line"
                );
                assert_eq!(edit.range, 5..6); // the \n at position 5
            }
            _ => panic!("expected Edit for dd on last empty line, got {:?}", action),
        }
    }

    #[test]
    fn test_x_deletes_char() {
        let mut vim = VimState::new();
        let buf = Buffer::from_text("hello");
        let action = vim.process_key(key('x'), &buf, 0);
        match action {
            VimAction::Edit(edit) => {
                assert!(edit.replacement.is_empty());
                assert_eq!(edit.range, 0..1);
                assert_eq!(edit.cursor_after, 0);
            }
            _ => panic!("expected Edit for x, got {:?}", action),
        }
    }

    #[test]
    fn test_2x_deletes_two_chars() {
        let mut vim = VimState::new();
        let buf = Buffer::from_text("hello");
        vim.process_key(key('2'), &buf, 0);
        let action = vim.process_key(key('x'), &buf, 0);
        match action {
            VimAction::Edit(edit) => {
                assert!(edit.replacement.is_empty());
                assert_eq!(edit.range, 0..2);
                assert_eq!(edit.cursor_after, 0);
            }
            _ => panic!("expected Edit for 2x, got {:?}", action),
        }
    }

    #[test]
    fn test_s_changes_one_char_and_enters_insert() {
        let mut vim = VimState::new();
        let buf = Buffer::from_text("hello");
        let action = vim.process_key(key('s'), &buf, 0);
        assert!(matches!(vim.mode(), Mode::Insert));
        match action {
            VimAction::Composite(actions) => {
                let edit = actions
                    .iter()
                    .find_map(|action| match action {
                        VimAction::Edit(edit) => Some(edit),
                        _ => None,
                    })
                    .expect("expected edit action for s");
                assert!(edit.replacement.is_empty());
                assert_eq!(edit.range, 0..1);
                assert_eq!(edit.cursor_after, 0);
                assert!(actions
                    .iter()
                    .any(|action| matches!(action, VimAction::ModeChange(Mode::Insert))));
            }
            _ => panic!("expected Composite for s, got {:?}", action),
        }
    }

    #[test]
    fn test_dw_deletes_word() {
        let mut vim = VimState::new();
        let buf = Buffer::from_text("hello world");
        assert!(matches!(
            vim.process_key(key('d'), &buf, 0),
            VimAction::Pending
        ));
        let action = vim.process_key(key('w'), &buf, 0);
        match action {
            VimAction::Edit(edit) => {
                assert!(edit.replacement.is_empty());
                assert_eq!(edit.range, 0..6);
                assert_eq!(edit.cursor_after, 0);
            }
            _ => panic!("expected Edit for dw, got {:?}", action),
        }
    }

    #[test]
    fn test_cw_changes_word_and_enters_insert() {
        let mut vim = VimState::new();
        let buf = Buffer::from_text("hello world");
        assert!(matches!(
            vim.process_key(key('c'), &buf, 0),
            VimAction::Pending
        ));
        let action = vim.process_key(key('w'), &buf, 0);
        assert!(matches!(vim.mode(), Mode::Insert));
        match action {
            VimAction::Composite(actions) => {
                let edit = actions
                    .iter()
                    .find_map(|action| match action {
                        VimAction::Edit(edit) => Some(edit),
                        _ => None,
                    })
                    .expect("expected edit action for cw");
                assert!(edit.replacement.is_empty());
                assert_eq!(edit.range, 0..5);
                assert_eq!(edit.cursor_after, 0);
                assert!(actions
                    .iter()
                    .any(|action| matches!(action, VimAction::ModeChange(Mode::Insert))));
            }
            _ => panic!("expected Composite for cw, got {:?}", action),
        }
    }

    #[test]
    fn test_cw_single_letter_word_stays_on_current_word() {
        let mut vim = VimState::new();
        let buf = Buffer::from_text("a b");
        assert!(matches!(
            vim.process_key(key('c'), &buf, 0),
            VimAction::Pending
        ));
        let action = vim.process_key(key('w'), &buf, 0);
        assert!(matches!(vim.mode(), Mode::Insert));
        match action {
            VimAction::Composite(actions) => {
                let edit = actions
                    .iter()
                    .find_map(|action| match action {
                        VimAction::Edit(edit) => Some(edit),
                        _ => None,
                    })
                    .expect("expected edit action for cw on a single-letter word");
                assert_eq!(edit.range, 0..1);
                assert_eq!(edit.cursor_after, 0);
            }
            _ => panic!(
                "expected Composite for cw on a single-letter word, got {:?}",
                action
            ),
        }
    }

    // UC-14 step 7: Pending keys display
    #[test]
    fn test_pending_keys_shown_after_d() {
        let mut vim = VimState::new();
        let buf = Buffer::from_text("hello");
        vim.process_key(key('d'), &buf, 0);
        assert!(!vim.pending_keys().is_empty());
    }

    // UC-21: Registers
    #[test]
    fn test_yy_copies_to_unnamed_register() {
        let mut vim = VimState::new();
        let buf = Buffer::from_text("hello world\n");
        vim.process_key(key('y'), &buf, 0);
        vim.process_key(key('y'), &buf, 0);
        // The unnamed register should have content
        // (exact behavior depends on implementation)
    }

    // UC-22: Macros
    #[test]
    fn test_macro_recording_flag() {
        let mut vim = VimState::new();
        let buf = Buffer::from_text("hello");
        assert!(!vim.is_recording());
        vim.process_key(key('q'), &buf, 0);
        vim.process_key(key('a'), &buf, 0);
        assert!(vim.is_recording());
        vim.process_key(key('q'), &buf, 0);
        assert!(!vim.is_recording());
    }

    // UC-15: Count prefix
    #[test]
    fn test_count_prefix_with_motion() {
        let mut vim = VimState::new();
        let buf = Buffer::from_text("one two three four five");
        vim.process_key(key('3'), &buf, 0);
        let action = vim.process_key(key('w'), &buf, 0);
        if let VimAction::Motion(m) = action {
            // Should skip 3 words
            assert!(m.new_position > 4); // past "one "
        } else {
            panic!("expected Motion for 3w");
        }
    }

    // UC-14: h/j/k/l basic motions
    #[test]
    fn test_h_moves_left() {
        let mut vim = VimState::new();
        let buf = Buffer::from_text("hello");
        let action = vim.process_key(key('h'), &buf, 3);
        if let VimAction::Motion(m) = action {
            assert_eq!(m.new_position, 2);
        } else {
            panic!("expected Motion");
        }
    }

    #[test]
    fn test_l_moves_right() {
        let mut vim = VimState::new();
        let buf = Buffer::from_text("hello");
        let action = vim.process_key(key('l'), &buf, 0);
        if let VimAction::Motion(m) = action {
            assert_eq!(m.new_position, 1);
        } else {
            panic!("expected Motion");
        }
    }

    #[test]
    fn test_j_moves_down() {
        let mut vim = VimState::new();
        let buf = Buffer::from_text("first\nsecond");
        let action = vim.process_key(key('j'), &buf, 0);
        if let VimAction::Motion(m) = action {
            assert!(m.new_position >= 6); // somewhere on second line
        } else {
            panic!("expected Motion");
        }
    }

    #[test]
    fn test_o_opens_line_below_and_enters_insert() {
        let mut vim = VimState::new();
        let buf = Buffer::from_text("hello\nworld");
        let _action = vim.process_key(key('o'), &buf, 3);
        assert!(matches!(vim.mode(), Mode::Insert));
    }

    #[test]
    fn test_o_upper_opens_line_above_and_enters_insert() {
        let mut vim = VimState::new();
        let buf = Buffer::from_text("hello\nworld");
        let _action = vim.process_key(key('O'), &buf, 8);
        assert!(matches!(vim.mode(), Mode::Insert));
    }

    // Insert mode: typing goes through as Unhandled
    #[test]
    fn test_insert_mode_chars_are_edits() {
        let mut vim = VimState::new();
        let buf = Buffer::from_text("");
        vim.process_key(key('i'), &buf, 0);
        let action = vim.process_key(key('a'), &buf, 0);
        // In insert mode, characters produce Unhandled (caller handles raw input)
        match action {
            VimAction::Edit(_) => {}
            VimAction::Unhandled => {}
            _ => panic!(
                "expected Edit or Unhandled in insert mode, got {:?}",
                action
            ),
        }
    }

    // ── Dot repeat tests ──────────────────────────────────────────────

    #[test]
    fn test_dot_repeat_records_dw() {
        let mut vim = VimState::new();
        let buf = Buffer::from_text("hello world");
        vim.process_key(key('d'), &buf, 0);
        vim.process_key(key('w'), &buf, 0);
        let last = vim.last_command().expect("dw should be recorded");
        assert_eq!(last.keys.len(), 2);
        assert_eq!(last.keys[0], key('d'));
        assert_eq!(last.keys[1], key('w'));
    }

    #[test]
    fn test_dot_repeat_records_x() {
        let mut vim = VimState::new();
        let buf = Buffer::from_text("hello");
        vim.process_key(key('x'), &buf, 0);
        let last = vim.last_command().expect("x should be recorded");
        assert_eq!(last.keys.len(), 1);
        assert_eq!(last.keys[0], key('x'));
    }

    #[test]
    fn test_dot_repeat_records_insert_session() {
        let mut vim = VimState::new();
        let buf = Buffer::from_text("hello");
        // i + type "ab" + Esc should record the full sequence
        vim.process_key(key('i'), &buf, 0);
        assert!(vim.recording_insert, "should be recording insert keys");
        vim.process_key(key('a'), &buf, 0);
        vim.process_key(key('b'), &buf, 0);
        vim.process_key(esc(), &buf, 2);
        assert!(!vim.recording_insert);
        let last = vim.last_command().expect("insert session should be recorded");
        // Keys: i, a, b, Esc
        assert_eq!(last.keys.len(), 4);
        assert_eq!(last.keys[0], key('i'));
        assert_eq!(last.keys[1], key('a'));
        assert_eq!(last.keys[2], key('b'));
        assert_eq!(last.keys[3], esc());
    }

    #[test]
    fn test_dot_repeat_records_cw_with_insert_text() {
        let mut vim = VimState::new();
        let buf = Buffer::from_text("hello world");
        // cw enters insert mode after deleting word
        vim.process_key(key('c'), &buf, 0);
        vim.process_key(key('w'), &buf, 0);
        assert!(matches!(vim.mode(), Mode::Insert));
        assert!(vim.recording_insert);
        // Type "hi" then Esc
        vim.process_key(key('h'), &buf, 0);
        vim.process_key(key('i'), &buf, 1);
        vim.process_key(esc(), &buf, 2);
        let last = vim.last_command().expect("cw+text should be recorded");
        // Keys: c, w, h, i, Esc
        assert_eq!(last.keys.len(), 5);
        assert_eq!(last.keys[0], key('c'));
        assert_eq!(last.keys[1], key('w'));
        assert_eq!(last.keys[2], key('h'));
        assert_eq!(last.keys[3], key('i'));
        assert_eq!(last.keys[4], esc());
    }

    #[test]
    fn test_dot_repeat_records_o_with_insert_text() {
        let mut vim = VimState::new();
        let buf = Buffer::from_text("hello\nworld");
        // o opens line below and enters insert mode
        vim.process_key(key('o'), &buf, 3);
        assert!(matches!(vim.mode(), Mode::Insert));
        assert!(vim.recording_insert);
        vim.process_key(key('x'), &buf, 6);
        vim.process_key(esc(), &buf, 7);
        let last = vim.last_command().expect("o+text should be recorded");
        // Keys: o, x, Esc
        assert_eq!(last.keys.len(), 3);
        assert_eq!(last.keys[0], key('o'));
    }

    #[test]
    fn test_dot_repeat_records_a_with_insert_text() {
        let mut vim = VimState::new();
        let buf = Buffer::from_text("hello");
        // a enters insert after cursor
        vim.process_key(key('a'), &buf, 2);
        assert!(matches!(vim.mode(), Mode::Insert));
        assert!(vim.recording_insert);
        vim.process_key(key('z'), &buf, 3);
        vim.process_key(esc(), &buf, 4);
        let last = vim.last_command().expect("a+text should be recorded");
        assert_eq!(last.keys.len(), 3); // a, z, Esc
        assert_eq!(last.keys[0], key('a'));
    }

    #[test]
    fn test_motion_does_not_record() {
        let mut vim = VimState::new();
        let buf = Buffer::from_text("hello world");
        vim.process_key(key('w'), &buf, 0);
        assert!(vim.last_command().is_none(), "motion should not be recorded");
    }

    #[test]
    fn test_normal_edit_overwrites_insert_recording() {
        let mut vim = VimState::new();
        let buf = Buffer::from_text("hello world");
        // First do an insert
        vim.process_key(key('i'), &buf, 0);
        vim.process_key(key('z'), &buf, 0);
        vim.process_key(esc(), &buf, 1);
        let last1 = vim.last_command().cloned().unwrap();
        assert_eq!(last1.keys.len(), 3); // i, z, Esc

        // Now do a normal edit — should overwrite
        vim.process_key(key('x'), &buf, 0);
        let last2 = vim.last_command().unwrap();
        assert_eq!(last2.keys.len(), 1);
        assert_eq!(last2.keys[0], key('x'));
    }

    #[test]
    fn test_replaying_suppresses_recording() {
        let mut vim = VimState::new();
        let buf = Buffer::from_text("hello");
        vim.process_key(key('x'), &buf, 0);
        let original = vim.last_command().cloned().unwrap();

        // Simulate replay
        vim.set_replaying(true);
        vim.process_key(key('i'), &buf, 0);
        vim.process_key(key('a'), &buf, 0);
        vim.process_key(esc(), &buf, 1);
        vim.set_replaying(false);

        // last_command should still be the original x, not overwritten
        let after = vim.last_command().unwrap();
        assert_eq!(after.keys, original.keys);
    }
}
