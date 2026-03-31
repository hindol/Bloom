use crate::types::{PaneId, UndoNodeId};
use chrono::NaiveDate;
use serde::Serialize;
use std::time::Instant;

// Re-exported from parser::traits (shared between parser and render layers).
pub use bloom_md::parser::traits::{Style, StyledSpan};

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
    pub context_strip: Option<ContextStripFrame>,
    pub temporal_strip: Option<TemporalStripFrame>,
    pub dialog: Option<DialogFrame>,
    pub view: Option<ViewFrame>,
    pub notifications: Vec<Notification>,
    pub scrolloff: usize,
    pub word_wrap: bool,
    pub wrap_indicator: String,
    /// Active theme name — frontends resolve to a palette each frame for live preview.
    pub theme_name: String,
    /// Split tree snapshot — the GUI computes pixel rects from ratios + font
    /// metrics; the TUI uses [`PaneRectFrame`] cell coords instead.
    pub layout_tree: super::LayoutTree,
    /// Text that should be written to the system clipboard this frame.
    pub clipboard_text: Option<String>,
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
    /// Total number of lines in the buffer (for scroll/position indicators).
    pub total_lines: usize,
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
    Dashboard(DashboardFrame),
}

// ---------------------------------------------------------------------------
// Dashboard (empty state)
// ---------------------------------------------------------------------------

/// Render data for the dashboard shown when no buffers are open.
#[derive(Serialize)]
pub struct DashboardFrame {
    pub recent_pages: Vec<DashboardRecentPage>,
    pub open_tasks: usize,
    pub pages_edited_today: usize,
    pub journal_entries_today: usize,
    pub tip: String,
}

/// One entry in the dashboard's "Recent Pages" section.
#[derive(Serialize)]
pub struct DashboardRecentPage {
    pub title: String,
    pub time_ago: String,
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
    /// Days in the current month that have journal entries (shown with ◆).
    pub journal_days: Vec<u32>,
    /// Today's date for highlighting.
    pub today: NaiveDate,
    /// Year and month being displayed.
    pub year: i32,
    pub month: u32,
}

// ---------------------------------------------------------------------------
// Context Strip (temporal navigation panel — 3 lines above status bar)
// ---------------------------------------------------------------------------

/// A reusable strip showing a selected item plus its neighbors.
/// Used for journal day-hopping, page history, and day activity.
#[derive(Serialize)]
pub struct ContextStripFrame {
    pub prev: Option<ContextStripDay>,
    pub current: ContextStripDay,
    pub next: Option<ContextStripDay>,
}

/// Summary info for one day in the context strip.
#[derive(Serialize)]
pub struct ContextStripDay {
    /// Date label (e.g., "Mar 10 Mon").
    pub label: String,
    /// Summary line (e.g., "5 items · #rust #editors").
    pub stats: String,
    /// First task or first content line (e.g., "[ ] Fix parser bug").
    pub first_line: String,
}

// ---------------------------------------------------------------------------
// Temporal strip — unified horizontal timeline for history views
// ---------------------------------------------------------------------------

/// Horizontal timeline strip for page history, block history, and day activity.
#[derive(Serialize)]
pub struct TemporalStripFrame {
    pub items: Vec<StripNode>,
    pub selected: usize,
    pub mode: TemporalMode,
    pub compact: bool,
    /// Preview content: diff lines for page history, empty for block history.
    pub preview_lines: Vec<DiffLine>,
    pub title: String,
    /// For BlockHistory: the buffer line to replace with inline diff.
    pub block_line: Option<usize>,
    /// For BlockHistory: word-diff segments for the inline preview.
    pub block_diff_segments: Vec<DiffSegment>,
}

impl TemporalStripFrame {
    /// Height of the drawer area (below status bar).
    pub fn drawer_height(&self) -> u16 {
        if self.compact {
            4
        } else {
            6
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct StripNode {
    pub label: String,
    pub detail: Option<String>,
    pub kind: StripNodeKind,
    /// Number of branches at this node (0 = linear, 2+ = fork).
    pub branch_count: usize,
    /// True if this node is dimmed (block content unchanged from older neighbor).
    pub skip: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum StripNodeKind {
    /// Recent undo tree node (●).
    UndoNode,
    /// Git commit (○).
    GitCommit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum TemporalMode {
    PageHistory,
    BlockHistory,
    DayActivity,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiffLine {
    /// Segments with word-level diff styling.
    pub segments: Vec<DiffSegment>,
    pub kind: DiffLineKind,
    /// Line number in the old (historical) version. None for added lines.
    pub old_line: Option<usize>,
    /// Line number in the new (current) version. None for removed lines.
    pub new_line: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiffSegment {
    pub text: String,
    pub kind: DiffLineKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum DiffLineKind {
    Context,
    Added,    // in current version (green)
    Removed,  // in historical version (red)
    Modified, // inline word diff — single line with mixed red/green segments
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
    /// True if this line contains a `^=` mirror marker.
    pub is_mirror: bool,
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
    /// Right-aligned hint text (e.g., "↵:calendar  [d/]d").
    pub right_hints: Option<String>,
}

#[derive(Serialize)]
pub enum StatusBarContent {
    /// Default: mode, title, dirty flag, cursor position, etc.
    Normal(NormalStatus),
    /// Active when user presses `:` (Command mode).
    CommandLine(CommandLineSlot),
    /// Active during quick capture, such as SPC j a or SPC x a.
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
            right_hints: None,
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
    /// Wall clock time for display in `:messages` buffer.
    #[serde(skip)]
    pub wall_time: chrono::DateTime<chrono::Local>,
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
// View frame (Live Views overlay)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct ViewFrame {
    pub title: String,
    pub query: String,
    /// Column headers (from BQL result columns).
    pub columns: Vec<String>,
    pub rows: Vec<ViewRow>,
    pub selected: usize,
    pub total: usize,
    pub error: Option<String>,
    pub is_prompt: bool,
    pub query_cursor: usize,
}

#[derive(Serialize)]
pub enum ViewRow {
    /// A section header (from `group` clause).
    SectionHeader(String),
    /// A data row with cell values.
    Data {
        cells: Vec<String>,
        is_task: bool,
        task_done: bool,
    },
}

// ---------------------------------------------------------------------------
// Completion (shared by command line and pickers)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct Completion {
    pub text: String,
    pub description: String,
}
