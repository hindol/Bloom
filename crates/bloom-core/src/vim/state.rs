use std::ops::Range;

use crate::buffer::{Buffer, EditOp};
use crate::types::{KeyCode, KeyEvent};

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

pub struct VimState {
    mode: Mode,
    pending: String,
    registers: RegisterFile,
    macro_state: MacroState,
    last_command: Option<RecordedCommand>,
    last_find: Option<FindCommand>,
    /// Keys accumulated for the current editing command (for `.` repeat).
    current_cmd_keys: Vec<KeyEvent>,
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
        }
    }

    /// Current mode (cloned).
    pub fn mode(&self) -> Mode {
        self.mode.clone()
    }

    /// Process a key event. Returns the action(s) to take.
    pub fn process_key(
        &mut self,
        key: KeyEvent,
        buffer: &Buffer,
        cursor: usize,
    ) -> VimAction {
        // Record for macros
        if self.macro_state.is_recording() {
            self.macro_state.record_key(key.clone());
        }

        match self.mode.clone() {
            Mode::Normal => self.process_normal(key, buffer, cursor),
            Mode::Insert => self.process_insert(key),
            Mode::Visual { start } => self.process_visual(key, buffer, cursor, start),
            Mode::Command => self.process_command(key),
        }
    }

    /// Currently pending keys (for status bar display).
    pub fn pending_keys(&self) -> &str {
        &self.pending
    }

    /// Get the contents of a register.
    pub fn register(&self, name: char) -> Option<&str> {
        self.registers.get(name)
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

    // ── Normal mode ──────────────────────────────────────────────────

    fn process_normal(
        &mut self,
        key: KeyEvent,
        buffer: &Buffer,
        cursor: usize,
    ) -> VimAction {
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
                // Store as last command for repeatable editing commands
                if is_repeatable(&action) {
                    self.last_command = Some(RecordedCommand { keys });
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

    fn process_insert(&mut self, key: KeyEvent) -> VimAction {
        if key.code == KeyCode::Esc {
            self.mode = Mode::Normal;
            return VimAction::ModeChange(Mode::Normal);
        }
        VimAction::Unhandled
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

    fn execute_command(
        &mut self,
        cmd: ParsedCommand,
        buffer: &Buffer,
        cursor: usize,
    ) -> VimAction {
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
                let target =
                    motion::execute_motion(&motion, buffer, cursor, count, &self.last_find);
                let range = ordered_range(cursor, target);
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
                if let Some(mut range) = text_object::resolve_text_object(&object, buffer, cursor)
                {
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
                VimAction::Edit(EditOp {
                    range: range.clone(),
                    replacement: String::new(),
                    cursor_after: range.start,
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
            Operator::AutoIndent | Operator::Reflow => {
                VimAction::Motion(MotionResult {
                    new_position: range.start,
                    extend_selection: false,
                })
            }
        }
    }

    fn execute_mode_switch(
        &mut self,
        ms: ModeSwitch,
        buffer: &Buffer,
        cursor: usize,
    ) -> VimAction {
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
                let line = rope.line(line_idx);
                let line_end = rope.line_to_char(line_idx) + line.len_chars();
                let insert_pos = if line.len_chars() > 0
                    && line.char(line.len_chars() - 1) == '\n'
                {
                    line_end
                } else {
                    line_end
                };
                VimAction::Composite(vec![
                    VimAction::Edit(EditOp {
                        range: insert_pos..insert_pos,
                        replacement: "\n".into(),
                        cursor_after: insert_pos + 1,
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
            StandaloneCmd::PlayMacro(reg) => {
                VimAction::Command(format!("play-macro:{reg}"))
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
        a..b + 1
    } else {
        b..a + 1
    }
}

fn line_range(buffer: &Buffer, cursor: usize, count: usize) -> Range<usize> {
    let rope = buffer.text();
    let start_line = rope.char_to_line(cursor);
    let end_line = (start_line + count).min(rope.len_lines());
    let start = rope.line_to_char(start_line);
    let end = if end_line < rope.len_lines() {
        rope.line_to_char(end_line)
    } else {
        buffer.len_chars()
    };
    start..end
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