use bloom_buffer::EditOp;
use crate::types::*;
use bloom_vim::state::Mode;
use crate::window::{Direction, SplitDirection};
use chrono::NaiveDate;

use super::platform::platform_shortcut;

// ---------------------------------------------------------------------------
// Action — the central action type for the whole editor
// ---------------------------------------------------------------------------

/// The central action enum for the editor.
///
/// Every user interaction ultimately resolves to one or more `Action` values
/// that [`BloomEditor`](crate::BloomEditor) executes: edits, motions, mode
/// changes, picker operations, window management, and more.
#[derive(Debug, Clone)]
pub enum Action {
    // Buffer edits
    Edit(EditOp),
    Motion(MotionResult),
    ModeChange(Mode),

    // Commands
    OpenPage(PageId),
    OpenJournal(NaiveDate),
    OpenPicker(PickerKind),
    ClosePicker,
    PickerInput(PickerInputAction),

    QuickCapture(QuickCaptureKind),
    SubmitQuickCapture(String),
    CancelQuickCapture,

    SplitWindow(SplitDirection),
    NavigateWindow(Direction),
    CloseWindow,
    CloseOtherWindows,
    ResizeWindow(ResizeOp),
    SwapWindow,
    RotateLayout,
    MoveBuffer(Direction),

    Save,
    Quit,
    Undo,
    Redo,
    ToggleTask,
    FollowLink,
    CopyToClipboard(String),

    OpenTimeline(PageId),
    OpenAgenda,
    OpenUndoTree,
    OpenPageHistory,
    OpenDatePicker(DatePickerPurpose),
    DialogResponse(usize),

    Refactor(RefactorOp),
    TemplateAdvance,
    RebuildIndex,
    ToggleMcp,

    Noop,
}

// ---------------------------------------------------------------------------
// Supporting types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct MotionResult {
    pub new_position: usize,
    pub extend_selection: bool,
}

/// Identifies which picker to open (pages, buffers, tags, templates, etc.).
#[derive(Debug, Clone)]
pub enum PickerKind {
    FindPage,
    SwitchBuffer,
    Search,
    Journal,
    Tags,
    Backlinks(PageId),
    UnlinkedMentions(PageId),
    AllCommands,
    InlineLink,
    Templates,
    Theme,
}

#[derive(Debug, Clone)]
pub enum PickerInputAction {
    UpdateQuery(String),
    MoveSelection(i32),
    Select,
    ToggleMark,
    Cancel,
}

#[derive(Debug, Clone)]
pub enum QuickCaptureKind {
    Note,
    Task,
}

#[derive(Debug, Clone)]
pub enum DatePickerPurpose {
    InsertDue,
    InsertStart,
    InsertAt,
    Reschedule(Task),
    JumpToJournal,
}

#[derive(Debug, Clone)]
pub enum RefactorOp {
    SplitPage,
    MergePages,
    MoveBlock,
}

#[derive(Debug, Clone)]
pub enum ResizeOp {
    IncreaseWidth,
    DecreaseWidth,
    IncreaseHeight,
    DecreaseHeight,
}

// ---------------------------------------------------------------------------
// EditorContext & KeymapConfig
// ---------------------------------------------------------------------------

pub struct EditorContext<'a> {
    pub mode: Mode,
    pub buffer: &'a bloom_buffer::Buffer,
    pub cursor: usize,
    pub picker_open: bool,
    pub quick_capture_open: bool,
    pub template_mode_active: bool,
    pub active_pane: PaneId,
}

#[derive(Default)]
pub struct KeymapConfig {
    // placeholder for user keymap overrides
}

// ---------------------------------------------------------------------------
// KeymapDispatcher
// ---------------------------------------------------------------------------

pub struct KeymapDispatcher {
    #[allow(dead_code)]
    config: KeymapConfig,
}

impl KeymapDispatcher {
    pub fn new(_config: &KeymapConfig) -> Self {
        KeymapDispatcher {
            config: KeymapConfig::default(),
        }
    }

    /// Process key through priority chain:
    /// 1. picker open → route to picker input
    /// 2. quick capture open → route to quick capture
    /// 3. platform shortcuts (Ctrl+S, Ctrl+Q, …)
    /// 4. everything else → return empty (caller handles vim + which-key)
    pub fn dispatch(&mut self, key: KeyEvent, context: &EditorContext) -> Vec<Action> {
        // 1. Picker input
        if context.picker_open {
            return self.dispatch_picker(&key);
        }

        // 2. Quick capture input
        if context.quick_capture_open {
            return self.dispatch_quick_capture(&key);
        }

        // 3. Platform shortcuts
        if let Some(action) = platform_shortcut(&key) {
            return vec![action];
        }

        // 4. Template mode: Tab advances
        if context.template_mode_active
            && key.code == KeyCode::Tab
            && key.modifiers == Modifiers::none()
        {
            return vec![Action::TemplateAdvance];
        }

        // Caller handles vim + which-key
        vec![]
    }

    fn dispatch_picker(&self, key: &KeyEvent) -> Vec<Action> {
        // Ctrl+key shortcuts (checked before bare-char branch)
        if key.modifiers.ctrl {
            return match &key.code {
                // Ctrl+N / Ctrl+J  → next result
                KeyCode::Char('n') | KeyCode::Char('j') => {
                    vec![Action::PickerInput(PickerInputAction::MoveSelection(1))]
                }
                // Ctrl+P / Ctrl+K  → previous result
                KeyCode::Char('p') | KeyCode::Char('k') => {
                    vec![Action::PickerInput(PickerInputAction::MoveSelection(-1))]
                }
                // Ctrl+G           → close picker (Emacs-style cancel)
                KeyCode::Char('g') => vec![Action::ClosePicker],
                // Ctrl+U           → clear search input
                KeyCode::Char('u') => {
                    vec![Action::PickerInput(PickerInputAction::UpdateQuery(
                        String::new(),
                    ))]
                }
                _ => vec![],
            };
        }

        match &key.code {
            KeyCode::Esc => vec![Action::ClosePicker],
            KeyCode::Enter => vec![Action::PickerInput(PickerInputAction::Select)],
            KeyCode::Up => vec![Action::PickerInput(PickerInputAction::MoveSelection(-1))],
            KeyCode::Down => vec![Action::PickerInput(PickerInputAction::MoveSelection(1))],
            KeyCode::Tab => vec![Action::PickerInput(PickerInputAction::ToggleMark)],
            KeyCode::Char(c)
                if key.modifiers == Modifiers::none() || key.modifiers == Modifiers::shift() =>
            {
                vec![Action::PickerInput(PickerInputAction::UpdateQuery(
                    c.to_string(),
                ))]
            }
            KeyCode::Backspace => {
                // Backspace in picker: send empty update to signal deletion
                vec![Action::PickerInput(PickerInputAction::UpdateQuery(
                    String::new(),
                ))]
            }
            _ => vec![],
        }
    }

    fn dispatch_quick_capture(&self, key: &KeyEvent) -> Vec<Action> {
        match &key.code {
            KeyCode::Esc => vec![Action::CancelQuickCapture],
            KeyCode::Enter if key.modifiers.ctrl => {
                // Ctrl+Enter submits (content managed by caller)
                vec![Action::SubmitQuickCapture(String::new())]
            }
            _ => vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bloom_buffer::Buffer;

    fn make_context(buf: &Buffer) -> EditorContext<'_> {
        EditorContext {
            mode: Mode::Normal,
            buffer: buf,
            cursor: 0,
            picker_open: false,
            quick_capture_open: false,
            template_mode_active: false,
            active_pane: PaneId(0),
        }
    }

    #[test]
    fn platform_save() {
        let config = KeymapConfig::default();
        let mut d = KeymapDispatcher::new(&config);
        let buf = Buffer::from_text("");
        let ctx = make_context(&buf);
        let actions = d.dispatch(KeyEvent::ctrl('s'), &ctx);
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], Action::Save));
    }

    #[test]
    fn picker_esc_closes() {
        let config = KeymapConfig::default();
        let mut d = KeymapDispatcher::new(&config);
        let buf = Buffer::from_text("");
        let ctx = EditorContext {
            picker_open: true,
            ..make_context(&buf)
        };
        let actions = d.dispatch(KeyEvent::esc(), &ctx);
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], Action::ClosePicker));
    }

    #[test]
    fn quick_capture_esc_cancels() {
        let config = KeymapConfig::default();
        let mut d = KeymapDispatcher::new(&config);
        let buf = Buffer::from_text("");
        let ctx = EditorContext {
            quick_capture_open: true,
            ..make_context(&buf)
        };
        let actions = d.dispatch(KeyEvent::esc(), &ctx);
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], Action::CancelQuickCapture));
    }

    #[test]
    fn template_tab_advances() {
        let config = KeymapConfig::default();
        let mut d = KeymapDispatcher::new(&config);
        let buf = Buffer::from_text("");
        let ctx = EditorContext {
            template_mode_active: true,
            ..make_context(&buf)
        };
        let actions = d.dispatch(KeyEvent::tab(), &ctx);
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], Action::TemplateAdvance));
    }

    #[test]
    fn picker_ctrl_j_moves_down() {
        let config = KeymapConfig::default();
        let mut d = KeymapDispatcher::new(&config);
        let buf = Buffer::from_text("");
        let ctx = EditorContext {
            picker_open: true,
            ..make_context(&buf)
        };
        let actions = d.dispatch(KeyEvent::ctrl('j'), &ctx);
        assert!(matches!(
            actions[0],
            Action::PickerInput(PickerInputAction::MoveSelection(1))
        ));
    }

    #[test]
    fn picker_ctrl_k_moves_up() {
        let config = KeymapConfig::default();
        let mut d = KeymapDispatcher::new(&config);
        let buf = Buffer::from_text("");
        let ctx = EditorContext {
            picker_open: true,
            ..make_context(&buf)
        };
        let actions = d.dispatch(KeyEvent::ctrl('k'), &ctx);
        assert!(matches!(
            actions[0],
            Action::PickerInput(PickerInputAction::MoveSelection(-1))
        ));
    }

    #[test]
    fn picker_ctrl_n_moves_down() {
        let config = KeymapConfig::default();
        let mut d = KeymapDispatcher::new(&config);
        let buf = Buffer::from_text("");
        let ctx = EditorContext {
            picker_open: true,
            ..make_context(&buf)
        };
        let actions = d.dispatch(KeyEvent::ctrl('n'), &ctx);
        assert!(matches!(
            actions[0],
            Action::PickerInput(PickerInputAction::MoveSelection(1))
        ));
    }

    #[test]
    fn picker_ctrl_p_moves_up() {
        let config = KeymapConfig::default();
        let mut d = KeymapDispatcher::new(&config);
        let buf = Buffer::from_text("");
        let ctx = EditorContext {
            picker_open: true,
            ..make_context(&buf)
        };
        let actions = d.dispatch(KeyEvent::ctrl('p'), &ctx);
        assert!(matches!(
            actions[0],
            Action::PickerInput(PickerInputAction::MoveSelection(-1))
        ));
    }

    #[test]
    fn picker_ctrl_g_closes() {
        let config = KeymapConfig::default();
        let mut d = KeymapDispatcher::new(&config);
        let buf = Buffer::from_text("");
        let ctx = EditorContext {
            picker_open: true,
            ..make_context(&buf)
        };
        let actions = d.dispatch(KeyEvent::ctrl('g'), &ctx);
        assert!(matches!(actions[0], Action::ClosePicker));
    }

    #[test]
    fn picker_ctrl_u_clears_input() {
        let config = KeymapConfig::default();
        let mut d = KeymapDispatcher::new(&config);
        let buf = Buffer::from_text("");
        let ctx = EditorContext {
            picker_open: true,
            ..make_context(&buf)
        };
        let actions = d.dispatch(KeyEvent::ctrl('u'), &ctx);
        assert!(matches!(
            actions[0],
            Action::PickerInput(PickerInputAction::UpdateQuery(_))
        ));
    }

    #[test]
    fn unknown_key_returns_empty() {
        let config = KeymapConfig::default();
        let mut d = KeymapDispatcher::new(&config);
        let buf = Buffer::from_text("");
        let ctx = make_context(&buf);
        let actions = d.dispatch(KeyEvent::char('j'), &ctx);
        assert!(actions.is_empty());
    }
}
