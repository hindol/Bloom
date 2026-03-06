# Bloom 🌱 — Module API Surfaces

> Public trait/struct signatures for every bloom-core module.
> Designed to satisfy all use cases in [USE_CASES.md](USE_CASES.md).
> These are contracts — not implementations. Parameters, return types, and error cases are specified; internals are not.

---

## Shared Types (`types.rs`)

```rust
pub struct PageId(pub [u8; 4]);           // 8-char hex UUID (4 bytes)
pub struct PaneId(pub u64);               // unique pane identifier
pub struct BlockId(pub String);            // e.g., "rope-perf"
pub struct TagName(pub String);            // e.g., "rust"
pub type Version = u64;                    // monotonically increasing per buffer
pub type UndoNodeId = u64;                 // identifies a node in the undo tree

/// Keyboard input types used throughout the dispatch pipeline.
pub struct KeyEvent {
    pub code: KeyCode,
    pub modifiers: Modifiers,
}

pub enum KeyCode {
    Char(char),
    Enter, Esc, Tab, Backspace, Delete,
    Up, Down, Left, Right,
    Home, End, PageUp, PageDown,
    F(u8),
}

pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
}

#[derive(Clone)]
pub struct PageMeta {
    pub id: PageId,
    pub title: String,
    pub created: NaiveDate,
    pub tags: Vec<TagName>,
    pub path: PathBuf,
}

#[derive(Clone)]
pub struct LinkTarget {
    pub page: PageId,
    pub section: Option<BlockId>,          // heading or block within the page
    pub display_hint: String,
}

pub enum Timestamp {
    Due(NaiveDate),
    Start(NaiveDate),
    At(NaiveDateTime),
}

pub struct Task {
    pub text: String,
    pub done: bool,
    pub timestamps: Vec<Timestamp>,
    pub source_page: PageId,
    pub line: usize,
}

pub enum BloomError {
    PageNotFound(String),
    AmbiguousMatch { query: String, candidates: Vec<String> },
    TextNotFound { old_text: String },
    AmbiguousText { old_text: String, count: usize },
    ReadOnly,
    MergeConflict(PathBuf),
    IoError(std::io::Error),
    IndexError(String),
    ParseError(String),
    InvalidPageId(String),
    PaneTooSmall,
    LastPane,
    ConfigError(String),
    WatcherError,
}
```

---

## Buffer (`buffer/`)

```rust
/// A rope-based text buffer with undo support.
pub struct Buffer {
    // internal: rope, undo_tree, version, dirty flag
}

impl Buffer {
    pub fn from_text(text: &str) -> Self;
    pub fn text(&self) -> &Rope;
    pub fn len_chars(&self) -> usize;
    pub fn len_lines(&self) -> usize;
    pub fn line(&self, idx: usize) -> RopeSlice;
    pub fn version(&self) -> Version;
    pub fn is_dirty(&self) -> bool;

    // Editing — all edits go through these, all create undo nodes
    pub fn insert(&mut self, char_idx: usize, text: &str);
    pub fn delete(&mut self, range: Range<usize>);
    pub fn replace(&mut self, range: Range<usize>, text: &str);

    // Search within buffer
    pub fn find_text(&self, needle: &str) -> Vec<Range<usize>>;

    // Edit groups — batch multiple edits into one undo node
    pub fn begin_edit_group(&mut self);
    pub fn end_edit_group(&mut self);
    pub fn restore_edit_group_checkpoint(&mut self) -> bool;

    // Undo / redo
    pub fn undo(&mut self) -> bool;               // returns false if at root
    pub fn redo(&mut self) -> bool;               // returns false if at tip
    pub fn undo_tree(&self) -> &UndoTree;
    pub fn restore_state(&mut self, node_id: UndoNodeId);

    // Mark clean (after save)
    pub fn mark_clean(&mut self);
}

/// Branching undo tree. RAM-only (not persisted).
pub struct UndoTree { /* internal */ }

impl UndoTree {
    pub fn current(&self) -> UndoNodeId;
    pub fn parent(&self, node: UndoNodeId) -> Option<UndoNodeId>;
    pub fn children(&self, node: UndoNodeId) -> &[UndoNodeId];
    pub fn branches(&self) -> Vec<Vec<UndoNodeId>>;  // for visualization
    pub fn node_info(&self, node: UndoNodeId) -> UndoNodeInfo;
}

pub struct UndoNodeInfo {
    pub id: UndoNodeId,
    pub timestamp: Instant,
    pub description: String,   // e.g., "insert 'hello'", "delete word"
}
```

**Use cases served:** UC-14–UC-23 (all editing), UC-18–UC-19 (undo tree), UC-68 (MCP edit via find_text + replace).

---

## Vim State Machine (`vim/`)

```rust
pub enum Mode {
    Normal,
    Insert,
    Visual { start: usize },       // start position of selection
    Command,
}

pub struct VimState {
    // internal: mode, pending keys, registers, macros, last command
}

/// The result of processing a key event through the Vim state machine.
pub enum VimAction {
    /// An edit to apply to the buffer.
    Edit(EditOp),
    /// A motion (move cursor, no edit).
    Motion(MotionResult),
    /// A mode transition.
    ModeChange(Mode),
    /// A command to dispatch (e.g., :rebuild-index).
    Command(String),
    /// Key is pending — waiting for more input (e.g., `d` waiting for motion).
    Pending,
    /// Key not handled by Vim — pass to next layer.
    Unhandled,
    /// Composite: mode change + edit (e.g., `cc` = delete line + enter insert).
    Composite(Vec<VimAction>),
    /// Restore buffer to a prior edit-group checkpoint.
    RestoreCheckpoint,
}

pub struct EditOp {
    pub range: Range<usize>,
    pub replacement: String,        // empty for delete, non-empty for change/insert
    pub cursor_after: usize,
}

pub struct MotionResult {
    pub new_position: usize,
    pub extend_selection: bool,     // true in Visual mode
}

impl VimState {
    pub fn new() -> Self;
    pub fn mode(&self) -> Mode;

    /// Process a key event. Returns the action(s) to take.
    /// The caller (keymap dispatch) applies edits to the buffer and cursor.
    pub fn process_key(
        &mut self,
        key: KeyEvent,
        buffer: &Buffer,
        cursor: usize,
    ) -> VimAction;

    /// What keys are currently pending (for status bar display).
    pub fn pending_keys(&self) -> &str;

    /// Get the contents of a register.
    pub fn register(&self, name: char) -> Option<&str>;

    /// Start/stop macro recording.
    pub fn start_macro(&mut self, register: char);
    pub fn stop_macro(&mut self);
    pub fn is_recording(&self) -> bool;
    pub fn play_macro(&self, register: char) -> Vec<KeyEvent>;

    /// Get the last repeatable command (for `.` repeat).
    pub fn last_command(&self) -> Option<&RecordedCommand>;
}
```

**Use cases served:** UC-14–UC-17 (editing/motions), UC-22–UC-23 (macros/dot repeat), UC-87–UC-88 (which-key during Vim grammar).

---

## Parser (`parser/`)

```rust
/// Trait for parsing document formats. Concrete: BloomMarkdownParser.
pub trait DocumentParser: Send + Sync {
    /// Parse a full document into a structured representation.
    fn parse(&self, text: &str) -> Document;

    /// Parse frontmatter only (fast path for indexing).
    fn parse_frontmatter(&self, text: &str) -> Option<Frontmatter>;

    /// Generate styled spans for a single line (for highlighting).
    fn highlight_line(&self, line: &str, context: &LineContext) -> Vec<StyledSpan>;

    /// Serialize a frontmatter struct back to YAML.
    fn serialize_frontmatter(&self, fm: &Frontmatter) -> String;
}

pub struct Document {
    pub frontmatter: Option<Frontmatter>,
    pub sections: Vec<Section>,
    pub links: Vec<ParsedLink>,
    pub tags: Vec<ParsedTag>,
    pub tasks: Vec<ParsedTask>,
    pub timestamps: Vec<ParsedTimestamp>,
    pub block_ids: Vec<ParsedBlockId>,
}

pub struct Frontmatter {
    pub id: Option<PageId>,
    pub title: Option<String>,
    pub created: Option<NaiveDate>,
    pub tags: Vec<TagName>,
    pub extra: HashMap<String, serde_yaml::Value>,  // preserved unknown keys
}

pub struct Section {
    pub level: u8,                  // 1-6
    pub title: String,
    pub block_id: Option<BlockId>,
    pub line_range: Range<usize>,   // line numbers in the document
}

pub struct StyledSpan {
    pub range: Range<usize>,        // byte range within the line
    pub style: Style,
}

pub enum Style {
    // Basic text
    Normal,
    Heading { level: u8 },
    Bold, Italic,
    Code, CodeBlock,

    // Links & references
    LinkText,                       // visible link text
    LinkChrome,                     // surrounding syntax: [[ ]] | etc.
    Tag,

    // Timestamps (broken into sub-spans)
    TimestampKeyword,               // @due, @start, @at
    TimestampDate,                  // the date string itself
    TimestampOverdue,               // overdue date highlight
    TimestampParens,                // surrounding ( )

    // Block IDs
    BlockId,
    BlockIdCaret,                   // the ^ prefix

    // Lists & tasks
    ListMarker,
    CheckboxUnchecked,
    CheckboxChecked,
    CheckedTaskText,                // dimmed text of completed tasks

    // Block quotes
    Blockquote,
    BlockquoteMarker,               // the > character

    // Tables
    TablePipe,                      // | separators
    TableAlignmentRow,              // :--- style alignment rows

    // Frontmatter (broken into sub-spans)
    Frontmatter,
    FrontmatterKey,
    FrontmatterTitle,
    FrontmatterId,
    FrontmatterDate,
    FrontmatterTags,

    // Diagnostics & search
    BrokenLink,
    SyntaxNoise,                    // Tier 3 markers: **, *, ##, [[ ]] etc.
    SearchMatch,                    // highlighted search result
    SearchMatchCurrent,             // the currently-focused search match
}

/// Context for line-level highlighting (is this inside a code block? frontmatter?)
pub struct LineContext {
    pub in_code_block: bool,
    pub in_frontmatter: bool,
    pub code_fence_lang: Option<String>,
}
```

**Use cases served:** UC-16 (text objects need parsed structure), UC-24–UC-25 (link syntax), UC-35 (tag parsing), UC-41 (task parsing), UC-79 (broken link detection via parsed links).

---

## Index (`index/`)

```rust
/// SQLite-backed index for search, backlinks, tags, and unlinked mentions.
/// Read-only queries are called from the UI thread.
/// Mutations are called from the indexer thread.
/// SQLite WAL mode supports concurrent readers + single writer.
pub struct Index {
    // internal: rusqlite connection
}

impl Index {
    pub fn open(path: &Path) -> Result<Self, BloomError>;

    /// Full rebuild — drops and recreates all index data.
    /// Used by :rebuild-index command for recovery.
    pub fn rebuild(&mut self, pages: &[IndexEntry]) -> Result<RebuildStats, BloomError>;

    /// Incremental update — only process changed/deleted files.
    /// Called by the indexer thread on startup and on file change events.
    pub fn incremental_update(
        &mut self,
        changed: &[IndexEntry],
        deleted: &[PageId],
    ) -> Result<RebuildStats, BloomError>;

    // Fingerprint queries (for incremental indexing)
    pub fn get_fingerprint(&self, path: &Path) -> Option<FileFingerprint>;
    pub fn set_fingerprint(&mut self, path: &Path, fp: &FileFingerprint);

    // Page queries (read-only, called from UI thread)
    pub fn find_page_by_title(&self, title: &str) -> Option<PageMeta>;
    pub fn find_page_by_id(&self, id: &PageId) -> Option<PageMeta>;
    pub fn find_page_fuzzy(&self, query: &str) -> Vec<PageMeta>;
    pub fn list_pages(&self, filter: Option<&TagName>) -> Vec<PageMeta>;

    // Link queries
    pub fn backlinks_to(&self, page: &PageId) -> Vec<Backlink>;
    pub fn forward_links_from(&self, page: &PageId) -> Vec<LinkTarget>;
    pub fn orphaned_links(&self, page: &PageId) -> Vec<OrphanedLink>;

    // Unlinked mentions
    pub fn unlinked_mentions(&self, page_title: &str) -> Vec<UnlinkedMention>;

    // Tag queries
    pub fn all_tags(&self) -> Vec<(TagName, usize)>;
    pub fn pages_with_tag(&self, tag: &TagName) -> Vec<PageMeta>;

    // Full-text search (FTS5)
    pub fn search(&self, query: &str, filters: &SearchFilters) -> Vec<SearchResult>;

    // Task queries (for agenda)
    pub fn all_open_tasks(&self) -> Vec<Task>;
    pub fn tasks_filtered(&self, filters: &AgendaFilters) -> Vec<Task>;

    // Mutation (called by indexer thread, within a transaction)
    pub fn index_page(&mut self, entry: &IndexEntry) -> Result<(), BloomError>;
    pub fn remove_page(&mut self, id: &PageId) -> Result<(), BloomError>;
    pub fn rename_tag(&mut self, old: &TagName, new: &TagName) -> Result<usize, BloomError>;
}

/// File fingerprint for incremental indexing.
/// Stored in SQLite alongside index data.
pub struct FileFingerprint {
    pub mtime_secs: i64,
    pub size_bytes: u64,
}

pub struct Backlink {
    pub source_page: PageMeta,
    pub context: String,
    pub line: usize,
}

pub struct UnlinkedMention {
    pub source_page: PageMeta,
    pub context: String,
    pub line: usize,
    pub match_range: Range<usize>,
}

pub struct SearchResult {
    pub page: PageMeta,
    pub line: usize,
    pub line_text: String,
    pub score: f64,
}

pub struct SearchFilters {
    pub tags: Vec<TagName>,
    pub date_range: Option<(NaiveDate, NaiveDate)>,
    pub links_to: Option<PageId>,
    pub task_status: Option<bool>,
}

pub struct AgendaFilters {
    pub tags: Vec<TagName>,
    pub page: Option<PageId>,
    pub date_range: Option<(NaiveDate, NaiveDate)>,
}

pub struct IndexEntry {
    pub meta: PageMeta,
    pub content: String,
    pub links: Vec<LinkTarget>,
    pub tags: Vec<TagName>,
    pub tasks: Vec<Task>,
    pub block_ids: Vec<(BlockId, usize)>,
}

pub struct RebuildStats {
    pub pages: usize,
    pub links: usize,
    pub tags: usize,
}
```

**Use cases served:** UC-08 (find page), UC-27 (backlinks), UC-28 (unlinked mentions), UC-34–UC-36 (tags), UC-37–UC-40 (search/filters), UC-43–UC-46 (agenda), UC-65–UC-66 (MCP search/read), UC-76 (rebuild index), UC-79 (broken links).

---

## Indexer Orchestrator (`index/`)

The indexer is the background thread that coordinates NoteStore, DocumentParser, and Index to build and maintain the search index without blocking the UI.

```rust
/// Background indexer that runs on its own OS thread.
/// Coordinates file scanning, parsing, and index updates.
pub struct Indexer {
    // internal: vault root, parser, index connection
}

/// Result sent from the indexer thread to the UI thread on completion.
pub struct IndexComplete {
    pub stats: RebuildStats,
    pub timing: IndexTiming,
}

pub struct IndexTiming {
    pub scan_ms: u64,       // Phase 1: stat files, compare fingerprints
    pub read_parse_ms: u64, // Phase 2: read + parse changed files (parallel)
    pub write_ms: u64,      // Phase 3: batch SQLite writes
    pub total_ms: u64,
    pub files_scanned: usize,
    pub files_changed: usize,
}

impl Indexer {
    pub fn new(
        vault_root: PathBuf,
        index_path: PathBuf,
    ) -> Result<Self, BloomError>;

    /// Run a full index build (used on first launch or :rebuild-index).
    pub fn full_rebuild(&mut self) -> Result<IndexComplete, BloomError>;

    /// Run an incremental update (used on startup with existing index
    /// and on file change events from the watcher).
    pub fn incremental_update(&mut self) -> Result<IndexComplete, BloomError>;
}
```

**Lifecycle:**

1. `init_vault()` spawns the indexer thread, which runs `incremental_update()`
2. The UI is immediately usable — index-dependent features return empty results
3. On completion, the indexer sends `IndexComplete` via channel to the UI thread
4. The UI shows "Index ready: N files in Xms" and index-dependent features light up
5. File watcher events trigger incremental re-indexing of changed files

**Use cases served:** All index-dependent UCs (search, backlinks, tags, agenda) — the indexer ensures the index is built and maintained without blocking the UI thread.

---

## Linker (`linker/`)

```rust
/// Resolves links, updates display hints, detects orphans.
pub struct Linker {
    // internal: reference to Index
}

impl Linker {
    pub fn resolve(&self, link: &ParsedLink) -> LinkResolution;
    pub fn update_display_hints(&self, old_title: &str, new_title: &str, page_id: &PageId)
        -> Vec<HintUpdate>;
    pub fn promote_unlinked_mention(&self, mention: &UnlinkedMention, target: &PageId)
        -> TextEdit;
    pub fn batch_promote(&self, mentions: &[UnlinkedMention], target: &PageId)
        -> Vec<TextEdit>;
}

pub enum LinkResolution {
    Resolved { page: PageMeta, section: Option<Section> },
    Orphaned { display_hint: String },
}

pub struct HintUpdate {
    pub file_path: PathBuf,
    pub old_text: String,           // e.g., "[[uuid|Old Title]]"
    pub new_text: String,           // e.g., "[[uuid|New Title]]"
}

pub struct TextEdit {
    pub file_path: PathBuf,
    pub range: Range<usize>,        // byte range in file
    pub new_text: String,
}
```

**Use cases served:** UC-09 (rename → hint update), UC-26 (follow link → resolve), UC-28 (promote mentions), UC-79 (broken link detection).

---

## Store (`store/`)

```rust
/// Trait for reading/writing/listing notes. Concrete: LocalFileStore.
pub trait NoteStore: Send + Sync {
    fn read(&self, path: &Path) -> Result<String, BloomError>;
    fn write(&self, path: &Path, content: &str) -> Result<(), BloomError>;
    fn delete(&self, path: &Path) -> Result<(), BloomError>;
    fn rename(&self, from: &Path, to: &Path) -> Result<(), BloomError>;
    fn list_pages(&self) -> Result<Vec<PathBuf>, BloomError>;
    fn list_journals(&self) -> Result<Vec<PathBuf>, BloomError>;
    fn exists(&self, path: &Path) -> bool;
    fn watch(&self) -> crossbeam::Receiver<FileEvent>;
}

pub enum FileEvent {
    Created(PathBuf),
    Modified(PathBuf),
    Deleted(PathBuf),
    Renamed { from: PathBuf, to: PathBuf },
}

/// Atomic disk writer. Receives write requests via channel, debounces, writes atomically.
pub struct DiskWriter {
    // internal: crossbeam sender, debounce timer
}

impl DiskWriter {
    pub fn new(debounce_ms: u64) -> (Self, crossbeam::Sender<WriteRequest>);
    pub fn start(self);   // runs on a dedicated OS thread
}

pub struct WriteRequest {
    pub path: PathBuf,
    pub content: String,
}
```

**Use cases served:** UC-01 (open journal), UC-07 (create page), UC-10 (delete page), UC-74 (import), UC-80–UC-81 (external file changes), UC-83 (file adoption), UC-86 (auto-save/crash recovery).

---

## Picker (`picker/`)

```rust
/// Generic fuzzy picker. Parameterized over the item type.
pub struct Picker<T: PickerItem> {
    // internal: query, results, selected index, filters, source
}

pub trait PickerItem: Clone {
    fn match_text(&self) -> &str;          // text to fuzzy-match against
    fn display(&self) -> PickerRow;        // how to render this item
    fn preview(&self) -> Option<String>;   // preview content (if any)
}

pub struct PickerColumn {
    pub text: String,
    pub style: ColumnStyle,                // Normal, Faded
}

pub struct PickerRow {
    pub label: String,
    pub middle: Option<PickerColumn>,
    pub right: Option<PickerColumn>,
}

impl<T: PickerItem> Picker<T> {
    pub fn new(items: Vec<T>) -> Self;
    pub fn set_query(&mut self, query: &str);
    pub fn results(&self) -> &[T];
    pub fn selected(&self) -> Option<&T>;
    pub fn selected_index(&self) -> usize;

    pub fn move_selection(&mut self, delta: i32);     // +1 = down, -1 = up
    pub fn select_first(&mut self);
    pub fn select_last(&mut self);

    // Filters
    pub fn add_filter(&mut self, filter: PickerFilter);
    pub fn remove_filter(&mut self, index: usize);
    pub fn clear_filters(&mut self);
    pub fn active_filters(&self) -> &[PickerFilter];

    // Batch selection
    pub fn toggle_mark(&mut self);                    // Tab on current item
    pub fn marked_items(&self) -> Vec<&T>;
}

pub enum PickerFilter {
    Tag(TagName),
    DateRange(NaiveDate, NaiveDate),
    LinksTo(PageId),
    TaskStatus(bool),
}

/// Frame data for rendering the picker.
pub struct PickerFrame {
    pub title: String,
    pub query: String,
    pub results: Vec<PickerRow>,
    pub selected_index: usize,
    pub filters: Vec<String>,              // display strings for filter pills
    pub preview: Option<String>,
    pub total_count: usize,
    pub filtered_count: usize,
    pub status_noun: String,                // e.g., "pages", "commands", "tags"
}
```

**Use cases served:** UC-08 (find page), UC-11 (switch buffer), UC-24–UC-25 (inline link picker), UC-27–UC-28 (backlinks/unlinked mentions), UC-34 (tags), UC-37–UC-40 (search), UC-62 (split page heading picker), UC-89 (all commands).

---

## Which-Key (`which_key/`)

```rust
pub struct WhichKeyTree {
    // internal: tree of key→action mappings
}

impl WhichKeyTree {
    pub fn new() -> Self;
    pub fn register(&mut self, keys: &str, label: &str, action: ActionId);
    pub fn set_group_label(&mut self, key: &str, label: &str);
    pub fn lookup(&self, prefix: &[KeyEvent]) -> WhichKeyLookup;
}

pub enum WhichKeyLookup {
    /// Exact match — execute this action.
    Action(ActionId),
    /// Prefix match — show these next keys.
    Prefix(Vec<WhichKeyEntry>),
    /// No match.
    NoMatch,
}

pub struct WhichKeyEntry {
    pub key: String,
    pub label: String,
    pub is_group: bool,             // true = has sub-keys, false = terminal action
}

/// Frame data for rendering the which-key popup.
pub struct WhichKeyFrame {
    pub entries: Vec<WhichKeyEntry>,
    pub prefix: String,             // e.g., "SPC f" — what's been typed so far
    pub context: WhichKeyContext,
}

pub enum WhichKeyContext {
    /// Leader key sequence (SPC → ...).
    Leader,
    /// Pending Vim operator (d, c, y, >, <, = etc.).
    /// Entries include both motions and text objects.
    VimOperator { operator: String },
    /// The `:` command line is active.
    CommandLine,
}
```

**Use cases served:** UC-87–UC-88 (which-key popups).

### Command Line

```rust
/// Frame data for rendering the `:` command line.
pub struct CommandLineFrame {
    pub input: String,
    pub cursor_pos: usize,
    pub completions: Vec<Completion>,
    pub selected_completion: Option<usize>,  // index into completions, if cycling
    pub error: Option<String>,               // error message from last command
}

pub struct Completion {
    pub text: String,              // the command or argument name
    pub description: String,       // brief help text
}

/// Registry of all available `:` commands.
pub struct CommandRegistry {
    // internal: HashMap<String, CommandDef>
}

pub struct CommandDef {
    pub name: String,
    pub aliases: Vec<String>,       // e.g., "w" for "write", "q" for "quit"
    pub description: String,
    pub args: CommandArgs,
}

pub enum CommandArgs {
    None,
    Required(String),               // label, e.g., "<path>"
    Optional(String),               // label, e.g., "<name>?"
}

impl CommandRegistry {
    pub fn new() -> Self;
    pub fn register(&mut self, def: CommandDef);
    pub fn find(&self, name: &str) -> Option<&CommandDef>;
    pub fn complete(&self, prefix: &str) -> Vec<Completion>;
    pub fn complete_args(&self, command: &str, arg_prefix: &str) -> Vec<Completion>;
}
```

**Use cases served:** UC-20 (command mode), UC-74 (`:import-logseq`), UC-76 (`:rebuild-index`).

---

## Render (`render/`)

```rust
/// UI-agnostic snapshot of everything to draw. Produced by BloomEditor::render().
pub struct RenderFrame {
    pub panes: Vec<PaneFrame>,
    pub maximized: bool,
    pub hidden_pane_count: usize,       // 0 when not maximized
    pub picker: Option<PickerFrame>,
    pub which_key: Option<WhichKeyFrame>,
    pub command_line: Option<CommandLineFrame>,
    pub quick_capture: Option<QuickCaptureFrame>,
    pub date_picker: Option<DatePickerFrame>,
    pub dialog: Option<DialogFrame>,
    pub notification: Option<Notification>,
}

pub struct PaneFrame {
    pub id: PaneId,
    pub kind: PaneKind,
    pub visible_lines: Vec<RenderedLine>,
    pub cursor: CursorState,
    pub scroll_offset: usize,       // first visible line
    pub is_active: bool,
    pub title: String,              // page title or "Timeline: X" etc.
    pub dirty: bool,
    pub status_bar: StatusBar,      // per-pane: active pane gets full bar, inactive gets compact
}

pub enum PaneKind {
    Editor,                         // normal page editing
    UndoTree(UndoTreeFrame),        // undo tree visualizer
    Agenda(AgendaFrame),            // agenda view
    Timeline(TimelineFrame),        // timeline view
    SetupWizard(SetupWizardFrame),  // first-launch wizard
}

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

pub struct SetupWizardFrame {
    pub step: SetupStep,
    pub vault_path: String,
    pub vault_path_cursor: usize,
    pub logseq_path: String,
    pub logseq_path_cursor: usize,
    pub import_choice: Option<bool>,        // true = import, false = skip
    pub import_progress: Option<(usize, usize)>,  // (done, total)
    pub stats: Option<RebuildStats>,
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

/// Date picker popup (for @due, @start, @at, reschedule).
pub struct DatePickerFrame {
    pub selected_date: NaiveDate,
    pub month_view: Vec<Vec<Option<u32>>>,  // calendar grid
    pub prompt: String,
}

/// Confirmation/choice dialog (for external file change, delete, etc.)
pub struct DialogFrame {
    pub message: String,
    pub choices: Vec<String>,
    pub selected: usize,
}

pub struct RenderedLine {
    pub line_number: usize,
    pub spans: Vec<StyledSpan>,
}

pub struct CursorState {
    pub line: usize,
    pub column: usize,
    pub shape: CursorShape,
}

pub enum CursorShape {
    Block,       // Normal mode
    Bar,         // Insert mode
    Underline,   // Replace mode
}

pub struct StatusBar {
    pub mode: String,               // "NORMAL", "INSERT", "VISUAL", "COMMAND"
    pub filename: String,
    pub dirty: bool,
    pub line: usize,
    pub column: usize,
    pub pending_keys: String,       // e.g., "d" when waiting for motion
    pub recording_macro: Option<char>,  // e.g., Some('a') when recording @a
    pub mcp_status: Option<String>,
}

pub struct QuickCaptureFrame {
    pub prompt: String,             // "📓 Append to journal > " or "☐ Append task > "
    pub input: String,
    pub cursor_pos: usize,
}

pub struct Notification {
    pub message: String,
    pub level: NotificationLevel,
    pub expires_at: Instant,
}

pub enum NotificationLevel {
    Info,      // "✓ Added to Mar 2 journal"
    Warning,   // "Merge conflict detected"
    Error,     // "Page not found"
}
```

**Use cases served:** ALL use cases — every UC ends with a verifiable `RenderFrame` state.

---

## Keymap (`keymap/`)

```rust
/// Central input dispatch. Priority: platform → vim → insert → which-key.
pub struct KeymapDispatcher {
    // internal: vim_state, which_key_tree, platform_shortcuts, user_overrides
}

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
    ResizeWindow(ResizeOp),

    Save,
    Quit,
    Undo,
    Redo,
    ToggleTask,
    FollowLink,
    CopyToClipboard(String),

    OpenTimeline(PageId),           // toggle: opens in split, or closes if already open
    OpenAgenda,                     // toggle: opens in split, or closes if already open
    OpenUndoTree,                   // toggle: opens in split, or closes if already open
    OpenDatePicker(DatePickerPurpose),
    DialogResponse(usize),          // index of chosen option

    Refactor(RefactorOp),
    TemplateAdvance,                // Tab pressed while template mode active
    RebuildIndex,
    ToggleMcp,

    // For testing/debugging
    Noop,
}

pub enum PickerKind {
    FindPage, SwitchBuffer, Search, Journal, Tags,
    Backlinks(PageId), UnlinkedMentions(PageId),
    AllCommands, InlineLink, Templates, Theme,
}

pub enum PickerInputAction {
    UpdateQuery(String),
    MoveSelection(i32),
    Select,
    ToggleMark,
    Cancel,
}

pub enum DatePickerPurpose {
    InsertDue,                      // SPC i d → insert @due(date)
    InsertStart,                    // SPC i s → insert @start(date)
    InsertAt,                       // SPC i a → insert @at(date)
    Reschedule(Task),               // reschedule a task in agenda
    JumpToJournal,                  // SPC j d → open journal by date
}

impl KeymapDispatcher {
    pub fn new(config: &KeymapConfig) -> Self;

    /// Process a key event through the priority chain.
    /// Returns the action(s) to execute.
    pub fn dispatch(&mut self, key: KeyEvent, context: &EditorContext) -> Vec<Action>;
}

pub struct EditorContext {
    pub mode: Mode,
    pub buffer: &Buffer,           // read-only ref for Vim to compute motions
    pub cursor: usize,
    pub picker_open: bool,
    pub quick_capture_open: bool,
    pub template_mode_active: bool,
    pub active_pane: PaneId,
}
```

**Use cases served:** UC-01–UC-06 (journal keys), UC-07–UC-13 (page mgmt keys), UC-14–UC-23 (editing), UC-24–UC-31 (linking keys), UC-32–UC-36 (tag keys), UC-37–UC-40 (search keys), UC-41–UC-47 (task/agenda keys), UC-48–UC-51 (timeline keys), UC-52–UC-57 (window keys), UC-87–UC-90 (discoverability).

---

## Journal (`journal/`)

```rust
pub struct Journal {
    // internal: vault root path
}

impl Journal {
    pub fn new(vault_root: &Path) -> Self;

    /// Get the path for a journal page by date.
    pub fn path_for_date(&self, date: NaiveDate) -> PathBuf;

    /// Get today's date.
    pub fn today() -> NaiveDate;

    /// Append a line to a journal page. Creates the file if needed.
    pub fn append(&self, date: NaiveDate, line: &str, store: &dyn NoteStore,
                  parser: &dyn DocumentParser) -> Result<(), BloomError>;

    /// Append a task line to a journal page.
    pub fn append_task(&self, date: NaiveDate, text: &str, store: &dyn NoteStore,
                       parser: &dyn DocumentParser) -> Result<(), BloomError>;

    /// List all journal dates that have files.
    pub fn all_dates(&self, store: &dyn NoteStore) -> Result<Vec<NaiveDate>, BloomError>;

    /// Navigate: next/previous journal date from a given date.
    pub fn next_date(&self, from: NaiveDate, store: &dyn NoteStore) -> Option<NaiveDate>;
    pub fn prev_date(&self, from: NaiveDate, store: &dyn NoteStore) -> Option<NaiveDate>;
}
```

**Use cases served:** UC-01–UC-06 (all daily workflow), UC-69 (MCP add to journal).

---

## Agenda (`agenda/`)

```rust
pub struct Agenda {
    // internal: reference to Index
}

pub struct AgendaView {
    pub overdue: Vec<Task>,
    pub today: Vec<Task>,
    pub upcoming: Vec<Task>,
    pub total_open: usize,
    pub total_pages: usize,
}

impl Agenda {
    pub fn new() -> Self;

    /// Build the agenda view from the index.
    pub fn build(&self, today: NaiveDate, index: &Index,
                 filters: &AgendaFilters) -> AgendaView;

    /// Reschedule a task (update @due timestamp in source file).
    pub fn reschedule(&self, task: &Task, new_date: NaiveDate,
                      buffer_mgr: &mut BufferManager) -> Result<(), BloomError>;
}
```

**Use cases served:** UC-43–UC-46 (agenda), UC-70 (MCP toggle task, via Index).

---

## Timeline (`timeline/`)

```rust
pub struct Timeline {
    // internal
}

pub struct TimelineEntry {
    pub source_page: PageMeta,
    pub date: NaiveDate,
    pub context: String,             // excerpt around the link
    pub link_line: usize,
}

pub struct TimelineView {
    pub target_page: PageMeta,
    pub entries: Vec<TimelineEntry>,
}

impl Timeline {
    pub fn new() -> Self;

    /// Build a timeline for a page using backlinks from the index.
    pub fn build(&self, page: &PageId, index: &Index) -> TimelineView;
}
```

**Use cases served:** UC-48–UC-51 (timeline).

---

## Template (`template/`)

```rust
pub struct TemplateEngine {
    // internal: templates dir path
}

pub struct Template {
    pub name: String,
    pub description: String,
    pub content: String,
    pub placeholders: Vec<Placeholder>,
}

pub struct Placeholder {
    pub index: usize,              // 1-based tab-stop order (0 = final cursor)
    pub description: String,       // e.g., "Meeting Title"
    pub occurrences: Vec<Range<usize>>,  // all positions of this placeholder in content
}

impl TemplateEngine {
    pub fn new(templates_dir: &Path) -> Self;

    /// List all available templates (built-in + user).
    pub fn list(&self) -> Vec<Template>;

    /// Expand a template: fill ${AUTO}, ${DATE}, ${TITLE}, return content with tab-stop placeholders.
    pub fn expand(&self, template: &Template, title: &str,
                  values: &HashMap<usize, String>) -> ExpandedTemplate;

    /// Get tab-stop positions in expanded content (for cursor jumping).
    pub fn tab_stops(&self, expanded: &ExpandedTemplate) -> Vec<TabStop>;
}

pub struct ExpandedTemplate {
    pub content: String,
    pub tab_stops: Vec<TabStop>,
}

pub struct TabStop {
    pub index: usize,                      // 0 = final cursor, 1+ = numbered stops
    pub ranges: Vec<Range<usize>>,         // all occurrences in content (for mirroring)
    pub default_text: String,              // description text if user skips
}

/// Runtime state for an active template session.
/// Lives in BloomEditor alongside VimState — independent lifecycle.
pub struct TemplateModeState {
    // internal: tab_stops, current_stop_index, typed text per stop
}

impl TemplateModeState {
    /// Create from an expanded template's tab stops.
    pub fn new(tab_stops: Vec<TabStop>) -> Self;

    /// Is template mode currently active?
    pub fn is_active(&self) -> bool;

    /// Current tab stop the user is filling (None if mode ended).
    pub fn current_stop(&self) -> Option<&TabStop>;

    /// Advance to the next tab stop. Returns the mirroring edits to apply
    /// (search-and-replace for all other occurrences of the current stop).
    /// Returns None if advancing past the last numbered stop (→ $0 or exit).
    pub fn advance(&mut self, typed_text: &str) -> TemplateAdvanceResult;

    /// End template mode (called after $0 or when user navigates away).
    pub fn deactivate(&mut self);
}

pub enum TemplateAdvanceResult {
    /// Move cursor to next tab stop. Apply these mirror edits first.
    NextStop {
        cursor_target: Range<usize>,        // where to place cursor (first range of next stop)
        mirror_edits: Vec<MirrorEdit>,      // replace other occurrences of previous stop
    },
    /// Move cursor to $0 (final position). Template mode ends.
    FinalCursor {
        cursor_target: usize,               // char position for $0
        mirror_edits: Vec<MirrorEdit>,
    },
    /// No $0 defined, template mode ends. No cursor move.
    Done {
        mirror_edits: Vec<MirrorEdit>,
    },
}

pub struct MirrorEdit {
    pub range: Range<usize>,                // byte range of the other occurrence
    pub new_text: String,                   // the text the user typed (replaces default)
}
```

**Use cases served:** UC-07 (create from template), UC-58–UC-61 (templates), UC-67 (MCP create with template).

---

## Refactor (`refactor/`)

```rust
pub struct Refactor {
    // internal
}

impl Refactor {
    pub fn new() -> Self;

    /// Extract a section into a new page. Returns edits to apply.
    pub fn split_page(
        &self,
        source_page: &PageId,
        section: &Section,
        new_title: &str,
        index: &Index,
        parser: &dyn DocumentParser,
    ) -> Result<SplitResult, BloomError>;

    /// Merge a source page into a target page.
    pub fn merge_pages(
        &self,
        source: &PageId,
        target: &PageId,
        index: &Index,
    ) -> Result<MergeResult, BloomError>;

    /// Move a block from one page to another.
    pub fn move_block(
        &self,
        block_id: &BlockId,
        from_page: &PageId,
        to_page: &PageId,
        index: &Index,
    ) -> Result<MoveResult, BloomError>;
}

pub struct SplitResult {
    pub new_page_content: String,        // content for the new page
    pub source_edits: Vec<TextEdit>,     // remove section, insert link
    pub link_updates: Vec<TextEdit>,     // update links in other files
}

pub struct MergeResult {
    pub target_edits: Vec<TextEdit>,     // append content
    pub link_redirects: Vec<TextEdit>,   // update links from source → target
    pub file_to_delete: PathBuf,
}

pub struct MoveResult {
    pub source_edits: Vec<TextEdit>,
    pub target_edits: Vec<TextEdit>,
    pub link_updates: Vec<TextEdit>,     // if block had links pointing to it
}
```

**Use cases served:** UC-62–UC-64 (split, merge, move).

---

## Window (`window/`)

```rust
pub struct WindowManager {
    // internal: tree of panes
}

pub enum SplitDirection { Vertical, Horizontal }
pub enum Direction { Left, Right, Up, Down }

impl WindowManager {
    pub fn new() -> Self;
    pub fn active_pane(&self) -> PaneId;
    pub fn pane_count(&self) -> usize;
    pub fn is_maximized(&self) -> bool;
    pub fn hidden_pane_count(&self) -> usize;     // 0 when not maximized

    /// Split the active pane. Fails if pane is too small or not an editor pane.
    pub fn split(&mut self, direction: SplitDirection) -> Result<PaneId, BloomError>;
    pub fn close(&mut self, pane: PaneId) -> bool;   // false if last pane
    pub fn close_others(&mut self);

    /// Navigate to the nearest spatial neighbor in the given direction.
    /// Uses cursor_line to pick the closest pane when multiple candidates exist.
    pub fn navigate(&mut self, direction: Direction, cursor_line: usize);

    /// Resize the active pane. No-op if minimum size (20 cols / 5 lines) would be violated.
    pub fn resize(&mut self, pane: PaneId, delta: i32, axis: SplitDirection);
    pub fn balance(&mut self);
    pub fn maximize_toggle(&mut self);
    pub fn swap_with_next(&mut self);
    pub fn rotate_layout(&mut self);
    pub fn move_buffer(&mut self, direction: Direction);

    /// Open a special view in a new split. If a pane of that kind already exists, close it (toggle).
    pub fn open_special_view(&mut self, kind: PaneKind, direction: SplitDirection) -> PaneId;

    /// Find an open pane of a given kind (for toggle detection).
    pub fn find_pane_by_kind(&self, kind: &PaneKind) -> Option<PaneId>;

    /// Produce the layout tree for rendering.
    pub fn layout(&self) -> LayoutTree;
}

pub enum LayoutTree {
    Leaf(PaneId),
    Split { direction: SplitDirection, children: Vec<(f32, LayoutTree)> },  // (ratio, subtree)
}
```

**Use cases served:** UC-52–UC-57 (all window management).

---

## Config (`config/`)

```rust
pub struct Config {
    pub startup: StartupConfig,
    pub font: FontConfig,
    pub theme: ThemeConfig,
    pub mcp: McpConfig,
    pub autosave_debounce_ms: u64,
    pub which_key_timeout_ms: u64,
}

pub struct StartupConfig {
    pub mode: StartupMode,    // Restore, Journal, Blank
}

pub struct FontConfig {
    pub family: String,
    pub size: u16,
    pub line_height: f32,
}

pub struct ThemeConfig {
    pub name: String,
    pub overrides: HashMap<String, String>,  // slot → hex colour
}

pub enum McpMode {
    ReadOnly,
    ReadWrite,
}

pub struct McpConfig {
    pub enabled: bool,
    pub mode: McpMode,
    pub exclude_paths: Vec<String>,  // glob patterns
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, BloomError>;
    pub fn defaults() -> Self;
}
```

**Use cases served:** UC-71–UC-72 (MCP config), UC-77–UC-78 (session startup), UC-73 (setup).

---

## Session (`session/`)

```rust
pub struct SessionState {
    pub buffers: Vec<SessionBuffer>,
    pub layout: LayoutTree,
    pub active_pane: PaneId,
}

pub struct SessionBuffer {
    pub page_path: PathBuf,
    pub cursor: CursorState,
    pub scroll_offset: usize,
    pub pane: PaneId,
}

impl SessionState {
    pub fn save(&self, path: &Path) -> Result<(), BloomError>;
    pub fn load(path: &Path) -> Result<Self, BloomError>;
}
```

**Use cases served:** UC-77 (session restore).

---

## Vault (`vault/`)

```rust
pub struct Vault {
    pub root: PathBuf,
}

impl Vault {
    /// Create a new vault with full directory structure.
    pub fn create(root: &Path) -> Result<Self, BloomError>;

    /// Open an existing vault.
    pub fn open(root: &Path) -> Result<Self, BloomError>;

    /// Generate a unique PageId (8-char hex, collision-checked against index).
    pub fn generate_id(&self, index: &Index) -> PageId;

    /// Derive a filename from a title (sanitized, case-collision-aware).
    pub fn filename_for_title(&self, title: &str, id: &PageId) -> String;

    /// Adopt an unrecognized .md file (add frontmatter).
    pub fn adopt_file(&self, path: &Path, parser: &dyn DocumentParser,
                      store: &dyn NoteStore) -> Result<PageMeta, BloomError>;

    /// Check if a file has merge conflict markers.
    pub fn has_merge_conflicts(&self, content: &str) -> bool;

    /// Generate .gitignore content.
    pub fn gitignore_content() -> &'static str;
}
```

**Use cases served:** UC-73 (setup wizard), UC-83 (file adoption), UC-82 (merge conflicts), UC-84–UC-85 (UUID/filename collisions).

---

## UUID (`uuid.rs`)

```rust
/// Generate a random 8-char hex string (4 bytes).
pub fn generate_hex_id() -> PageId;

/// Check if an ID collides with any existing page.
pub fn is_unique(id: &PageId, index: &Index) -> bool;

/// Generate a unique ID, retrying on collision.
pub fn generate_unique_id(index: &Index) -> PageId;
```

**Use cases served:** UC-84 (UUID collision).

---

## BloomEditor — The Orchestrator (`lib.rs`)

```rust
/// The top-level editor. Wires all modules together.
/// Frontends (TUI, GUI) and MCP server interact with this.
pub struct BloomEditor {
    // internal: all modules, buffer manager, channels
}

/// Tracks all open buffers (files loaded into memory).
pub struct BufferManager {
    // internal: HashMap<PageId, Buffer>
}

pub struct BufferInfo {
    pub page_id: PageId,
    pub title: String,
    pub path: PathBuf,
    pub dirty: bool,
    pub last_focused: Instant,
}

impl BufferManager {
    pub fn open(&mut self, path: &Path, content: &str) -> &mut Buffer;
    pub fn get(&self, page_id: &PageId) -> Option<&Buffer>;
    pub fn get_mut(&mut self, page_id: &PageId) -> Option<&mut Buffer>;
    pub fn close(&mut self, page_id: &PageId);
    pub fn open_buffers(&self) -> Vec<BufferInfo>;
    pub fn is_open(&self, page_id: &PageId) -> bool;
}

impl BloomEditor {
    pub fn new(config: Config) -> Result<Self, BloomError>;

    /// Process a key event from the UI.
    pub fn handle_key(&mut self, key: KeyEvent) -> Vec<Action>;

    /// Produce the current render frame.
    pub fn render(&self) -> RenderFrame;

    /// Tick — called periodically (e.g., 60fps). Handles timers, debounce, notifications.
    pub fn tick(&mut self, now: Instant);

    // Startup & setup
    pub fn startup(&mut self) -> Result<(), BloomError>;
    pub fn needs_setup(&self) -> bool;
    pub fn start_wizard(&mut self);
    pub fn wizard_active(&self) -> bool;
    pub fn init_vault(&mut self, path: &Path) -> Result<(), BloomError>;
    pub fn resize(&mut self, width: u16, height: u16);

    // Theme
    pub fn theme(&self) -> &ThemeConfig;
    pub fn set_theme(&mut self, name: &str);
    pub fn cycle_theme(&mut self);

    // Buffer management
    pub fn open_page(&mut self, id: &PageId) -> Result<(), BloomError>;
    pub fn open_page_with_content(&mut self, id: &PageId, content: &str) -> Result<(), BloomError>;
    pub fn create_page(&mut self, title: &str, template: Option<&str>) -> Result<PageId, BloomError>;
    pub fn close_buffer(&mut self, pane: PaneId) -> Result<(), BloomError>;
    pub fn save_current(&mut self) -> Result<(), BloomError>;

    /// Apply a batch of text edits across multiple files atomically.
    /// Used by refactoring ops, tag rename, and display hint updates.
    pub fn apply_edits(&mut self, edits: Vec<TextEdit>) -> Result<(), BloomError>;

    // Session
    pub fn save_session(&self) -> Result<(), BloomError>;
    pub fn restore_session(&mut self) -> Result<(), BloomError>;

    // Index
    pub fn rebuild_index(&mut self) -> Result<RebuildStats, BloomError>;
}
```

**Use cases served:** ALL — this is the entry point for every UC.

---

## UC → API Call Mapping (Selected Examples)

### UC-02: Quick-capture a thought

```
1. KeymapDispatcher::dispatch(SPC j a) → Action::QuickCapture(Note)
2. BloomEditor sets quick_capture_open = true
3. RenderFrame includes QuickCaptureFrame { prompt: "📓 ...", input: "" }
4. User types text → KeymapDispatcher routes to quick_capture input buffer
5. User presses Enter → Action::SubmitQuickCapture(text)
6. Journal::append(today, text, store, parser)
7. RenderFrame includes Notification { "✓ Added to Mar 2 journal" }
```

### UC-24: Create a link while writing

```
1. In Insert mode, user types `[[`
2. KeymapDispatcher detects `[[` trigger → Action::OpenPicker(InlineLink)
3. Picker<PageMeta> created from Index::find_page_fuzzy()
4. User types query → Picker::set_query() → nucleo fuzzy match
5. User selects page → Action::PickerSelect
6. Linker formats `[[page_id|title]]`
7. Buffer::insert(cursor, formatted_link)
8. Action::ClosePicker
```

### UC-28: Batch promote unlinked mentions

```
1. KeymapDispatcher::dispatch(SPC s u) → Action::OpenPicker(UnlinkedMentions)
2. Index::unlinked_mentions(current_page_title) → Vec<UnlinkedMention>
3. Picker<UnlinkedMention> populated
4. User marks items with Tab → Picker::toggle_mark()
5. User presses Enter → Picker::marked_items()
6. Linker::batch_promote(marked, target_page_id) → Vec<TextEdit>
7. For each TextEdit: open buffer, apply edit (or write to disk if not open)
8. Notification: "Promoted 3 unlinked mentions"
```

### UC-43: Open agenda

```
1. KeymapDispatcher::dispatch(SPC a a) → Action::OpenAgenda
2. Agenda::build(today, index, default_filters) → AgendaView
3. AgendaView contains overdue/today/upcoming task groups
4. RenderFrame includes agenda pane with formatted task list
```

### UC-68: LLM edits a note (search-and-replace)

```
1. MCP receives edit_note(title, old_text, new_text)
2. Index::find_page_fuzzy(title) → PageMeta
3. BufferManager::open_or_load(page_meta.path) → &mut Buffer
4. Buffer::find_text(old_text) → Vec<Range>
5. If 0 matches → Err(TextNotFound)
6. If >1 match → Err(AmbiguousText { count })
7. If 1 match → Buffer::replace(range, new_text)
8. UndoTree gets a new node
9. Notification: "MCP: edited Text Editor Theory"
10. Auto-save triggers after debounce
```

### UC-82: Git merge conflict detection

```
1. FileEvent::Modified(path) received from store watcher
2. NoteStore::read(path) → content
3. Vault::has_merge_conflicts(content) → true
4. BloomEditor marks buffer as degraded (no indexing, no link resolution)
5. Notification { level: Warning, "Merge conflict detected..." }
6. User resolves conflicts, saves
7. Vault::has_merge_conflicts(content) → false
8. Index::index_page(entry) — re-indexed normally
```

### UC-58: Use a built-in template (full chain)

```
 1. KeymapDispatcher::dispatch(SPC n) → Action::OpenPicker(Templates)
 2. TemplateEngine::list() → Vec<Template>
 3. Picker<Template> populated with names and descriptions
 4. User selects "Meeting notes" → picker prompts for title (QuickCaptureFrame)
 5. User types "Sprint Retrospective" → Action::SubmitQuickCapture("Sprint Retrospective")
 6. TemplateEngine::expand(template, "Sprint Retrospective", {}) → ExpandedTemplate
    - ${AUTO} → generated UUID
    - ${DATE} → "2026-03-03"
    - ${TITLE} → "Sprint Retrospective"
    - ${1:Attendees}, ${2:Topics}, ${3:Action item}, $0 left as tab stops
 7. Buffer::from_text(expanded.content)
 8. TemplateModeState::new(expanded.tab_stops) — template mode activated
 9. Cursor placed at tab_stops[0].ranges[0].start (first occurrence of $1)
10. User types "Alice, Bob" (Insert mode, normal buffer editing)
11. User presses Tab → KeymapDispatcher sees template_mode_active
    → Action::TemplateAdvance
12. TemplateModeState::advance("Alice, Bob") → NextStop {
        cursor_target: Range for $2,
        mirror_edits: [replace other ${1:Attendees} occurrences with "Alice, Bob"]
    }
13. BloomEditor applies mirror_edits via Buffer::replace for each MirrorEdit
14. Cursor moves to $2
15. (Repeat for $2, $3...)
16. After $3, Tab → TemplateModeState::advance(...) → FinalCursor { cursor at $0 }
17. TemplateModeState::deactivate() — template mode ends
18. Next Tab → normal tab character (KeymapDispatcher, no template_mode_active)
```

---

## Related Documents

| Document | Contents |
|----------|----------|
| [USE_CASES.md](USE_CASES.md) | All 90 use cases these APIs serve |
| [CRATE_STRUCTURE.md](CRATE_STRUCTURE.md) | Module layout and dependency rules |
| [ARCHITECTURE.md](ARCHITECTURE.md) | Threading model, RenderFrame, data safety |
| [GOALS.md](GOALS.md) | Feature goals |
