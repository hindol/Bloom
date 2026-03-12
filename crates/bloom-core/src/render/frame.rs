use crate::types::{PaneId, UndoNodeId};
use chrono::NaiveDate;
use serde::Serialize;
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
#[derive(Serialize)]
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
#[derive(Serialize)]
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
}

/// Pane positioning info for the TUI renderer.
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct PaneRectFrame {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub content_height: u16,
    pub total_height: u16,
}

#[derive(Serialize)]
pub enum PaneKind {
    Editor,
    UndoTree(UndoTreeFrame),
    Timeline(TimelineFrame),
    PageHistory(PageHistoryFrame),
    SetupWizard(SetupWizardFrame),
}

// ---------------------------------------------------------------------------
// Undo tree
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct UndoTreeFrame {
    pub nodes: Vec<UndoTreeNode>,
    pub selected: UndoNodeId,
    pub preview: Option<String>,
}

#[derive(Serialize)]
pub struct UndoTreeNode {
    pub id: UndoNodeId,
    pub depth: usize,
    pub branch: usize,
    pub description: String,
    pub is_current: bool,
}

// ---------------------------------------------------------------------------
// Page history (git time travel)
// ---------------------------------------------------------------------------

/// Render data for the page history split pane (`SPC H h`).
#[derive(Serialize)]
pub struct PageHistoryFrame {
    pub page_title: String,
    pub entries: Vec<PageHistoryEntryFrame>,
    pub selected_index: usize,
    pub total_versions: usize,
}

/// One entry in the page history list.
#[derive(Serialize)]
pub struct PageHistoryEntryFrame {
    pub oid: String,
    pub date: String,
    pub diff_stat: String,
    pub description: String,
}

// ---------------------------------------------------------------------------
// Timeline
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct TimelineFrame {
    pub target_title: String,
    pub entries: Vec<TimelineEntryFrame>,
    pub selected_index: usize,
}

#[derive(Serialize)]
pub struct TimelineEntryFrame {
    pub source_title: String,
    pub date: NaiveDate,
    pub context: String,
    pub expanded: bool,
}

// ---------------------------------------------------------------------------
// Inline menu (shared: command completion, link picker, tag completion)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct InlineMenuFrame {
    pub items: Vec<InlineMenuItem>,
    pub selected: usize,
    pub anchor: InlineMenuAnchor,
    pub hint: Option<String>,
}

#[derive(Serialize)]
pub struct InlineMenuItem {
    pub id: Option<String>,
    pub label: String,
    pub right: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum InlineMenuAnchor {
    /// Above the status bar (command completion, tag add/remove)
    CommandLine,
    /// Below a specific cursor position (inline link, inline tag)
    Cursor { line: usize, col: usize },
}

// ---------------------------------------------------------------------------
// Setup wizard
// ---------------------------------------------------------------------------

#[derive(Serialize)]
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

#[derive(Serialize)]
pub enum SetupStep {
    Welcome,
    ChooseVaultLocation,
    ImportChoice,
    ImportPath,
    ImportRunning,
    Complete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ImportChoice {
    No,
    Yes,
}

#[derive(Serialize)]
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

#[derive(Serialize)]
pub struct WizardStats {
    pub pages: usize,
    pub journals: usize,
}

// ---------------------------------------------------------------------------
// Date picker
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct DatePickerFrame {
    pub selected_date: NaiveDate,
    pub month_view: Vec<Vec<Option<u32>>>,
    pub prompt: String,
}

// ---------------------------------------------------------------------------
// Dialog
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct DialogFrame {
    pub message: String,
    pub choices: Vec<String>,
    pub selected: usize,
}

// ---------------------------------------------------------------------------
// Lines & cursor
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct RenderedLine {
    pub source: LineSource,
    pub text: String,
    pub spans: Vec<StyledSpan>,
}

/// Where a rendered line originated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum LineSource {
    /// A line from the text buffer at the given line index (0-based).
    Buffer(usize),
    /// A tilde line beyond the end of the buffer.
    BeyondEof,
}

impl LineSource {
    /// Returns the buffer line index if this is a Buffer line.
    pub fn buffer_line(&self) -> Option<usize> {
        match self {
            LineSource::Buffer(n) => Some(*n),
            _ => None,
        }
    }
}

#[derive(Serialize)]
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

#[derive(Serialize)]
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
#[derive(Serialize)]
pub struct StatusBarFrame {
    pub content: StatusBarContent,
    pub mode: String,
}

#[derive(Serialize)]
pub enum StatusBarContent {
    /// Default: mode, title, dirty flag, cursor position, etc.
    Normal(NormalStatus),
    /// Active when user presses `:` (Command mode).
    CommandLine(CommandLineSlot),
    /// Active during SPC j a / SPC j t.
    QuickCapture(QuickCaptureSlot),
}

#[derive(Serialize)]
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

#[derive(Serialize)]
pub struct CommandLineSlot {
    pub input: String,
    pub cursor_pos: usize,
    pub ghost_text: Option<String>,
    pub error: Option<String>,
}

#[derive(Serialize)]
pub struct QuickCaptureSlot {
    pub prompt: String,
    pub input: String,
    pub cursor_pos: usize,
}

#[derive(Clone, Default, Serialize)]
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
#[derive(Clone, Serialize)]
pub struct Notification {
    pub message: String,
    pub level: NotificationLevel,
    /// None means the notification persists until dismissed (used for errors).
    #[serde(skip)]
    pub expires_at: Option<Instant>,
    #[serde(skip)]
    pub created_at: Instant,
}

/// Severity level for notifications — determines color coding and expiry.
#[derive(Clone, PartialEq, Serialize)]
pub enum NotificationLevel {
    Info,
    Warning,
    Error,
}

// ---------------------------------------------------------------------------
// Picker
// ---------------------------------------------------------------------------

#[derive(Serialize)]
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

#[derive(Serialize)]
pub struct PickerRow {
    pub label: String,
    pub middle: Option<String>,
    pub right: Option<String>,
}

// ---------------------------------------------------------------------------
// Which-key
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct WhichKeyFrame {
    pub entries: Vec<WhichKeyEntry>,
    pub prefix: String,
    pub context: WhichKeyContext,
}

#[derive(Serialize)]
pub struct WhichKeyEntry {
    pub key: String,
    pub label: String,
    pub is_group: bool,
}

#[derive(Serialize)]
pub enum WhichKeyContext {
    Leader,
    VimOperator { operator: String },
    CommandLine,
}

// ---------------------------------------------------------------------------
// Completion (shared by command line and pickers)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct Completion {
    pub text: String,
    pub description: String,
}
