use crate::types::{PaneId, UndoNodeId};
use chrono::NaiveDate;
use std::time::Instant;

// Re-exported from parser::traits (shared between parser and render layers).
pub use crate::parser::traits::{Style, StyledSpan};

// ---------------------------------------------------------------------------
// Top-level render frame
// ---------------------------------------------------------------------------

pub struct RenderFrame {
    pub panes: Vec<PaneFrame>,
    pub maximized: bool,
    pub hidden_pane_count: usize,
    pub picker: Option<PickerFrame>,
    pub which_key: Option<WhichKeyFrame>,
    pub command_line: Option<CommandLineFrame>,
    pub quick_capture: Option<QuickCaptureFrame>,
    pub date_picker: Option<DatePickerFrame>,
    pub dialog: Option<DialogFrame>,
    pub notification: Option<Notification>,
}

// ---------------------------------------------------------------------------
// Pane
// ---------------------------------------------------------------------------

pub struct PaneFrame {
    pub id: PaneId,
    pub kind: PaneKind,
    pub visible_lines: Vec<RenderedLine>,
    pub cursor: CursorState,
    pub scroll_offset: usize,
    pub is_active: bool,
    pub title: String,
    pub dirty: bool,
    pub status_bar: StatusBar,
}

pub enum PaneKind {
    Editor,
    UndoTree(UndoTreeFrame),
    Agenda(AgendaFrame),
    Timeline(TimelineFrame),
    SetupWizard(SetupWizardFrame),
}

// ---------------------------------------------------------------------------
// Undo tree
// ---------------------------------------------------------------------------

pub struct UndoTreeFrame {
    pub nodes: Vec<UndoTreeNode>,
    pub selected: UndoNodeId,
    pub preview: Option<String>,
}

pub struct UndoTreeNode {
    pub id: UndoNodeId,
    pub depth: usize,
    pub branch: usize,
    pub description: String,
    pub is_current: bool,
}

// ---------------------------------------------------------------------------
// Agenda
// ---------------------------------------------------------------------------

pub struct AgendaFrame {
    pub overdue: Vec<AgendaItem>,
    pub today: Vec<AgendaItem>,
    pub upcoming: Vec<AgendaItem>,
    pub selected_index: usize,
    pub total_open: usize,
    pub total_pages: usize,
}

pub struct AgendaItem {
    pub task_text: String,
    pub source_page: String,
    pub date: Option<NaiveDate>,
    pub tags: Vec<String>,
}

// ---------------------------------------------------------------------------
// Timeline
// ---------------------------------------------------------------------------

pub struct TimelineFrame {
    pub target_title: String,
    pub entries: Vec<TimelineEntryFrame>,
    pub selected_index: usize,
}

pub struct TimelineEntryFrame {
    pub source_title: String,
    pub date: NaiveDate,
    pub context: String,
    pub expanded: bool,
}

// ---------------------------------------------------------------------------
// Setup wizard
// ---------------------------------------------------------------------------

pub struct SetupWizardFrame {
    pub step: SetupStep,
    pub vault_path: String,
    pub vault_path_cursor: usize,
    pub logseq_path: String,
    pub logseq_path_cursor: usize,
    pub import_choice: ImportChoice,
    pub import_progress: Option<ImportProgress>,
    pub stats: WizardStats,
    pub error: Option<String>,
}

pub enum SetupStep {
    Welcome,
    ChooseVaultLocation,
    ImportChoice,
    ImportPath,
    ImportRunning,
    Complete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportChoice {
    No,
    Yes,
}

pub struct ImportProgress {
    pub done: usize,
    pub total: usize,
    pub pages_imported: usize,
    pub journals_imported: usize,
    pub links_resolved: usize,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
    pub finished: bool,
}

pub struct WizardStats {
    pub pages: usize,
    pub journals: usize,
}

// ---------------------------------------------------------------------------
// Date picker
// ---------------------------------------------------------------------------

pub struct DatePickerFrame {
    pub selected_date: NaiveDate,
    pub month_view: Vec<Vec<Option<u32>>>,
    pub prompt: String,
}

// ---------------------------------------------------------------------------
// Dialog
// ---------------------------------------------------------------------------

pub struct DialogFrame {
    pub message: String,
    pub choices: Vec<String>,
    pub selected: usize,
}

// ---------------------------------------------------------------------------
// Lines & cursor
// ---------------------------------------------------------------------------

pub struct RenderedLine {
    pub line_number: usize,
    pub text: String,
    pub spans: Vec<StyledSpan>,
}

pub struct CursorState {
    pub line: usize,
    pub column: usize,
    pub shape: CursorShape,
}

impl Default for CursorState {
    fn default() -> Self {
        Self {
            line: 0,
            column: 0,
            shape: CursorShape::Block,
        }
    }
}

pub enum CursorShape {
    Block,
    Bar,
    Underline,
}

// ---------------------------------------------------------------------------
// Status bar
// ---------------------------------------------------------------------------

pub struct StatusBar {
    pub mode: String,
    pub title: String,
    pub dirty: bool,
    pub line: usize,
    pub column: usize,
    pub pending_keys: String,
    pub recording_macro: Option<char>,
    pub mcp: McpIndicator,
}

#[derive(Clone, Default)]
pub enum McpIndicator {
    #[default]
    Off,
    Idle,
    Editing { tick: u8 },
}

impl Default for StatusBar {
    fn default() -> Self {
        Self {
            mode: String::from("NORMAL"),
            title: String::new(),
            dirty: false,
            line: 0,
            column: 0,
            pending_keys: String::new(),
            recording_macro: None,
            mcp: McpIndicator::Off,
        }
    }
}

// ---------------------------------------------------------------------------
// Quick capture
// ---------------------------------------------------------------------------

pub struct QuickCaptureFrame {
    pub prompt: String,
    pub input: String,
    pub cursor_pos: usize,
}

// ---------------------------------------------------------------------------
// Notification
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct Notification {
    pub message: String,
    pub level: NotificationLevel,
    pub expires_at: Instant,
}

#[derive(Clone)]
pub enum NotificationLevel {
    Info,
    Warning,
    Error,
}

// ---------------------------------------------------------------------------
// Picker
// ---------------------------------------------------------------------------

pub struct PickerFrame {
    pub title: String,
    pub query: String,
    pub results: Vec<PickerRow>,
    pub selected_index: usize,
    pub filters: Vec<String>,
    pub preview: Option<String>,
    pub total_count: usize,
    pub filtered_count: usize,
    pub status_noun: String,
}

pub struct PickerRow {
    pub label: String,
    pub middle: Option<String>,
    pub right: Option<String>,
}

// ---------------------------------------------------------------------------
// Which-key
// ---------------------------------------------------------------------------

pub struct WhichKeyFrame {
    pub entries: Vec<WhichKeyEntry>,
    pub prefix: String,
    pub context: WhichKeyContext,
}

pub struct WhichKeyEntry {
    pub key: String,
    pub label: String,
    pub is_group: bool,
}

pub enum WhichKeyContext {
    Leader,
    VimOperator { operator: String },
}

// ---------------------------------------------------------------------------
// Command line
// ---------------------------------------------------------------------------

pub struct CommandLineFrame {
    pub input: String,
    pub cursor_pos: usize,
    pub completions: Vec<Completion>,
    pub selected_completion: Option<usize>,
    pub error: Option<String>,
}

pub struct Completion {
    pub text: String,
    pub description: String,
}