use crate::types::{PaneId, UndoNodeId};
use chrono::NaiveDate;
use std::time::Instant;

// Re-exported from parser::traits (shared between parser and render layers).
pub use crate::parser::traits::{Style, StyledSpan};

// ---------------------------------------------------------------------------
// Top-level render frame
// ---------------------------------------------------------------------------

/// Complete snapshot of everything a frontend needs to paint one frame.
///
/// Produced by [`BloomEditor::render`](crate::BloomEditor::render) each tick.
/// Contains pane layout, cursor state, modal overlays (picker, agenda,
/// which-key drawer), and the current notification stack.
pub struct RenderFrame {
    pub panes: Vec<PaneFrame>,
    pub maximized: bool,
    pub hidden_pane_count: usize,
    pub picker: Option<PickerFrame>,
    pub inline_menu: Option<InlineMenuFrame>,
    pub which_key: Option<WhichKeyFrame>,
    pub date_picker: Option<DatePickerFrame>,
    pub dialog: Option<DialogFrame>,
    pub notifications: Vec<Notification>,
    pub scrolloff: usize,
}

// ---------------------------------------------------------------------------
// Pane
// ---------------------------------------------------------------------------

/// A single editor pane within the render frame.
///
/// Contains the visible lines, cursor state, scroll offset, status bar, and
/// layout rect. Frontends iterate over panes to render the editor area.
pub struct PaneFrame {
    pub id: PaneId,
    pub kind: PaneKind,
    pub visible_lines: Vec<RenderedLine>,
    pub cursor: CursorState,
    pub scroll_offset: usize,
    pub is_active: bool,
    pub title: String,
    pub dirty: bool,
    pub status_bar: StatusBarFrame,
    /// Cell rect computed by core layout — TUI reads this directly.
    pub rect: PaneRectFrame,
    /// BQL query result blocks embedded in this pane's content.
    pub query_blocks: Vec<QueryResultBlock>,
}

/// Pane positioning info for the TUI renderer.
#[derive(Debug, Clone, Copy, Default)]
pub struct PaneRectFrame {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub content_height: u16,
    pub total_height: u16,
}

pub enum PaneKind {
    Editor,
    UndoTree(UndoTreeFrame),
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
// Inline menu (shared: command completion, link picker, tag completion)
// ---------------------------------------------------------------------------

pub struct InlineMenuFrame {
    pub items: Vec<InlineMenuItem>,
    pub selected: usize,
    pub anchor: InlineMenuAnchor,
    pub hint: Option<String>,
}

pub struct InlineMenuItem {
    pub id: Option<String>,
    pub label: String,
    pub right: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InlineMenuAnchor {
    /// Above the status bar (command completion, tag add/remove)
    CommandLine,
    /// Below a specific cursor position (inline link, inline tag)
    Cursor { line: usize, col: usize },
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

/// A BQL query result block embedded in page content.
#[derive(Debug, Clone)]
pub struct QueryResultBlock {
    /// Screen line index where results start (within visible_lines).
    pub screen_line_start: usize,
    /// Number of result rows rendered.
    pub row_count: usize,
    /// Column headers for the result table.
    pub columns: Vec<String>,
    /// Result rows with display values and optional block IDs for actions.
    pub result_rows: Vec<QueryResultRow>,
    /// Error message if the query failed to parse/compile/execute.
    pub error: Option<String>,
}

/// A single row in a BQL query result.
#[derive(Debug, Clone)]
pub struct QueryResultRow {
    /// Display values for each column.
    pub cells: Vec<String>,
    /// Page ID for jump-to-source (if available).
    pub page_id: Option<String>,
    /// Block ID for targeted actions like toggle (if available).
    pub block_id: Option<String>,
    /// Line number in source page (for cursor positioning on jump).
    pub source_line: Option<usize>,
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
// Status bar (slot-based)
// ---------------------------------------------------------------------------

/// Global status bar — exactly one line at the bottom of the screen.
/// Different modes populate different slots; rendering is centralized.
pub struct StatusBarFrame {
    pub content: StatusBarContent,
    pub mode: String,
}

pub enum StatusBarContent {
    /// Default: mode, title, dirty flag, cursor position, etc.
    Normal(NormalStatus),
    /// Active when user presses `:` (Command mode).
    CommandLine(CommandLineSlot),
    /// Active during SPC j a / SPC j t.
    QuickCapture(QuickCaptureSlot),
}

pub struct NormalStatus {
    pub title: String,
    pub dirty: bool,
    pub line: usize,
    pub column: usize,
    pub pending_keys: String,
    pub recording_macro: Option<char>,
    pub mcp: McpIndicator,
    pub indexing: bool,
}

pub struct CommandLineSlot {
    pub input: String,
    pub cursor_pos: usize,
    pub error: Option<String>,
}

pub struct QuickCaptureSlot {
    pub prompt: String,
    pub input: String,
    pub cursor_pos: usize,
}

#[derive(Clone, Default)]
pub enum McpIndicator {
    #[default]
    Off,
    Idle,
    Editing {
        tick: u8,
    },
}

impl Default for StatusBarFrame {
    fn default() -> Self {
        Self {
            content: StatusBarContent::Normal(NormalStatus {
                title: String::new(),
                dirty: false,
                line: 0,
                column: 0,
                pending_keys: String::new(),
                recording_macro: None,
                mcp: McpIndicator::Off,
                indexing: false,
            }),
            mode: String::from("NORMAL"),
        }
    }
}

// ---------------------------------------------------------------------------
// Notification
// ---------------------------------------------------------------------------

/// A transient notification shown to the user.
///
/// Info (4 s) and Warning (8 s) auto-expire; Errors persist until dismissed.
#[derive(Clone)]
pub struct Notification {
    pub message: String,
    pub level: NotificationLevel,
    /// None means the notification persists until dismissed (used for errors).
    pub expires_at: Option<Instant>,
    pub created_at: Instant,
}

/// Severity level for notifications — determines color coding and expiry.
#[derive(Clone, PartialEq)]
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
    /// Minimum query length before showing results (0 = show immediately).
    pub min_query_len: usize,
    /// Whether the query text is selected (visual highlight, typing replaces).
    pub query_selected: bool,
    /// Hint for the TUI to use a wider layout (e.g. search results with context).
    pub wide: bool,
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
    CommandLine,
}

// ---------------------------------------------------------------------------
// Completion (shared by command line and pickers)
// ---------------------------------------------------------------------------

pub struct Completion {
    pub text: String,
    pub description: String,
}
