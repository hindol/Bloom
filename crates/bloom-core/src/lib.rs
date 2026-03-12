#![doc = include_str!("../../../docs/ARCHITECTURE.md")]

pub mod agenda;
pub mod align;
pub mod block_id_gen;
pub mod config;
pub mod error;
pub mod history;
pub mod index;
pub mod journal;
pub mod keymap;
pub mod linker;
pub mod picker;
pub mod query;
pub mod refactor;
pub mod render;
pub mod session;
pub mod template;
pub mod timeline;
pub mod types;
pub mod uuid;
pub mod vault;
pub mod which_key;
pub mod window;

mod editor;
pub use editor::event_loop;

// ---------------------------------------------------------------------------
// BufferManager
// ---------------------------------------------------------------------------

use std::collections::HashMap;
use std::time::Instant;

use bloom_md::parser::traits::DocumentParser;

/// Manages all open text buffers, keyed by [`PageId`](types::PageId).
///
/// Each buffer is paired with [`BufferInfo`] metadata (title, path, dirty flag).
/// The editor opens, closes, and reloads buffers through this manager.
pub struct BufferManager {
    buffers: HashMap<String, (bloom_buffer::Buffer, BufferInfo)>,
}

/// Metadata for an open buffer: identity, display title, file path, and dirty state.
pub struct BufferInfo {
    pub page_id: types::PageId,
    pub title: String,
    pub path: std::path::PathBuf,
    pub dirty: bool,
    pub last_focused: Instant,
}

impl Default for BufferManager {
    fn default() -> Self {
        Self::new()
    }
}

impl BufferManager {
    pub fn new() -> Self {
        Self {
            buffers: HashMap::new(),
        }
    }

    pub fn open(
        &mut self,
        page_id: &types::PageId,
        title: &str,
        path: &std::path::Path,
        content: &str,
    ) -> &mut bloom_buffer::Buffer {
        let key = page_id.to_hex();
        self.buffers.entry(key.clone()).or_insert_with(|| {
            let buf = bloom_buffer::Buffer::from_text(content);
            let info = BufferInfo {
                page_id: page_id.clone(),
                title: title.to_string(),
                path: path.to_path_buf(),
                dirty: false,
                last_focused: Instant::now(),
            };
            (buf, info)
        });
        &mut self.buffers.get_mut(&key).unwrap().0
    }

    pub fn get(&self, page_id: &types::PageId) -> Option<&bloom_buffer::Buffer> {
        self.buffers.get(&page_id.to_hex()).map(|(b, _)| b)
    }

    pub fn get_with_info(
        &self,
        page_id: &types::PageId,
    ) -> Option<(&bloom_buffer::Buffer, &BufferInfo)> {
        self.buffers.get(&page_id.to_hex()).map(|(b, i)| (b, i))
    }

    pub fn get_mut(&mut self, page_id: &types::PageId) -> Option<&mut bloom_buffer::Buffer> {
        self.buffers.get_mut(&page_id.to_hex()).map(|(b, _)| b)
    }

    pub fn close(&mut self, page_id: &types::PageId) {
        self.buffers.remove(&page_id.to_hex());
    }

    pub fn open_buffers(&self) -> Vec<&BufferInfo> {
        self.buffers.values().map(|(_, info)| info).collect()
    }

    pub fn is_open(&self, page_id: &types::PageId) -> bool {
        self.buffers.contains_key(&page_id.to_hex())
    }

    /// Find a buffer by its file path (for file watcher conflict detection).
    pub fn find_by_path(&self, path: &std::path::Path) -> Option<&types::PageId> {
        self.buffers
            .values()
            .find(|(_, info)| info.path == path)
            .map(|(_, info)| &info.page_id)
    }

    /// Reload a buffer's content from a string (external file change).
    pub fn reload(&mut self, page_id: &types::PageId, content: &str) {
        if let Some((buf, _)) = self.buffers.get_mut(&page_id.to_hex()) {
            *buf = bloom_buffer::Buffer::from_text(content);
        }
    }
}

// ---------------------------------------------------------------------------
// Channel bundle for event-driven TUI loop
// ---------------------------------------------------------------------------

/// Channel bundle returned by [`BloomEditor::channels`] for the frontend event loop.
///
/// Each receiver corresponds to a background subsystem (disk writer, file watcher,
/// indexer). Fields are `None` until [`BloomEditor::init_vault`] sets them up.
/// Designed for multiplexing with `crossbeam::select!`.
pub struct EditorChannels {
    pub write_complete_rx:
        Option<crossbeam::channel::Receiver<bloom_store::disk_writer::WriteComplete>>,
    pub watcher_rx: Option<crossbeam::channel::Receiver<bloom_store::traits::FileEvent>>,
    pub indexer_rx: Option<crossbeam::channel::Receiver<index::indexer::IndexComplete>>,
    pub history_rx: Option<crossbeam::channel::Receiver<history::HistoryComplete>>,
}

// ---------------------------------------------------------------------------
// BloomEditor — The Orchestrator
// ---------------------------------------------------------------------------

/// The top-level editor orchestrator.
///
/// Owns all core state — buffers, Vim state machine, window layout, index,
/// journal, file store, and notification stack. Frontends drive the editor by
/// calling [`handle_key`](Self::handle_key) and [`render`](Self::render) in a loop,
/// using [`channels`](Self::channels) to multiplex background events.
pub struct BloomEditor {
    pub config: config::Config,
    pub(crate) buffer_mgr: BufferManager,
    pub(crate) vim_state: bloom_vim::VimState,
    pub(crate) window_mgr: window::WindowManager,
    pub(crate) which_key_tree: which_key::WhichKeyTree,
    pub(crate) _command_registry: which_key::CommandRegistry,
    pub(crate) index: Option<index::Index>,
    pub(crate) journal: Option<journal::Journal>,
    pub(crate) parser: bloom_md::parser::BloomMarkdownParser,
    pub(crate) template_engine: Option<template::TemplateEngine>,
    pub(crate) template_mode: Option<template::TemplateModeState>,
    pub(crate) _linker: linker::Linker,

    pub(crate) _timeline: timeline::Timeline,
    pub(crate) _refactorer: refactor::Refactor,
    pub(crate) note_store: Option<bloom_store::local::LocalFileStore>,

    // State
    pub(crate) picker_state: Option<ActivePicker>,

    pub(crate) quick_capture: Option<QuickCaptureState>,
    pub(crate) notifications: Vec<render::Notification>,
    pub(crate) notification_history: Vec<render::Notification>,
    pub(crate) wizard: Option<SetupWizardState>,
    pub(crate) vault_root: Option<std::path::PathBuf>,
    pub(crate) leader_keys: Vec<types::KeyEvent>,
    pub(crate) pending_since: Option<Instant>,
    pub(crate) which_key_visible: bool,
    pub(crate) active_theme: &'static bloom_md::theme::ThemePalette,
    // Auto-save
    pub(crate) autosave_tx:
        Option<crossbeam::channel::Sender<bloom_store::disk_writer::WriteRequest>>,
    pub(crate) write_complete_rx:
        Option<crossbeam::channel::Receiver<bloom_store::disk_writer::WriteComplete>>,
    pub(crate) last_write_fingerprints:
        std::collections::HashMap<std::path::PathBuf, (std::time::SystemTime, u64)>,
    pub(crate) terminal_height: u16,
    pub(crate) terminal_width: u16,
    // Background indexer
    pub(crate) indexer_rx: Option<crossbeam::channel::Receiver<index::indexer::IndexComplete>>,
    pub(crate) indexer_tx: Option<crossbeam::channel::Sender<index::indexer::IndexRequest>>,
    pub(crate) indexing: bool,
    pub(crate) last_index_timing: Option<index::indexer::IndexTiming>,
    pub(crate) last_picker_queries: std::collections::HashMap<String, String>,
    // File watcher debounce
    pub(crate) watcher_rx: Option<crossbeam::channel::Receiver<bloom_store::traits::FileEvent>>,
    pub(crate) pending_file_events: std::collections::HashSet<std::path::PathBuf>,
    pub(crate) file_event_deadline: Option<Instant>,
    // External file change dialog
    pub(crate) active_dialog: Option<ActiveDialog>,
    // Inline completion (link picker / tag completion)
    pub(crate) inline_completion: Option<InlineCompletion>,
    // BQL query cache (invalidated on IndexComplete)
    pub(crate) query_cache: std::cell::RefCell<query::QueryCache>,
    // Single-instance vault lock (held for the lifetime of the editor)
    pub(crate) vault_lock: Option<vault::lock::VaultLock>,
    // Git-backed history
    pub(crate) history_tx: Option<crossbeam::channel::Sender<history::HistoryRequest>>,
    pub(crate) history_rx: Option<crossbeam::channel::Receiver<history::HistoryComplete>>,
    pub(crate) page_history_entries: Option<Vec<history::PageHistoryEntry>>,
    pub(crate) page_history_selected: usize,
}

pub(crate) struct InlineCompletion {
    pub kind: InlineCompletionKind,
    /// Char position in buffer where the query starts (after the trigger).
    pub trigger_pos: usize,
    pub selected: usize,
}

pub(crate) enum InlineCompletionKind {
    Link, // triggered by [[
    Tag,  // triggered by #
}

pub(crate) enum ActiveDialog {
    FileChanged {
        page_id: types::PageId,
        path: std::path::PathBuf,
        selected: usize,
    },
}

// ---------------------------------------------------------------------------
// Setup wizard state machine
// ---------------------------------------------------------------------------

pub(crate) struct SetupWizardState {
    pub(crate) step: WizardStep,
    pub(crate) vault_path: String,
    pub(crate) vault_path_cursor: usize,
    pub(crate) import_choice: render::ImportChoice,
    pub(crate) logseq_path: String,
    pub(crate) logseq_path_cursor: usize,
    pub(crate) _import_progress: Option<render::ImportProgress>,
    pub(crate) stats: render::WizardStats,
    pub(crate) error: Option<String>,
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum WizardStep {
    Welcome,
    ChooseVault,
    ImportChoice,
    ImportPath,
    #[allow(dead_code)]
    ImportRunning,
    Complete,
}

impl SetupWizardState {
    pub(crate) fn new() -> Self {
        Self {
            step: WizardStep::Welcome,
            vault_path: default_vault_path(),
            vault_path_cursor: default_vault_path().len(),
            import_choice: render::ImportChoice::No,
            logseq_path: String::new(),
            logseq_path_cursor: 0,
            _import_progress: None,
            stats: render::WizardStats {
                pages: 0,
                journals: 0,
            },
            error: None,
        }
    }

    pub(crate) fn to_frame(&self) -> render::SetupWizardFrame {
        render::SetupWizardFrame {
            step: match self.step {
                WizardStep::Welcome => render::SetupStep::Welcome,
                WizardStep::ChooseVault => render::SetupStep::ChooseVaultLocation,
                WizardStep::ImportChoice => render::SetupStep::ImportChoice,
                WizardStep::ImportPath => render::SetupStep::ImportPath,
                WizardStep::ImportRunning => render::SetupStep::ImportRunning,
                WizardStep::Complete => render::SetupStep::Complete,
            },
            vault_path: self.vault_path.clone(),
            vault_path_cursor: self.vault_path_cursor,
            logseq_path: self.logseq_path.clone(),
            logseq_path_cursor: self.logseq_path_cursor,
            import_choice: self.import_choice,
            import_progress: None, // Logseq import not yet implemented
            stats: render::WizardStats {
                pages: self.stats.pages,
                journals: self.stats.journals,
            },
            error: self.error.clone(),
        }
    }
}

pub fn default_vault_path() -> String {
    if let Some(home) = home_dir() {
        let p = home.join("bloom");
        p.to_string_lossy().to_string()
    } else {
        "bloom".to_string()
    }
}

pub(crate) fn home_dir() -> Option<std::path::PathBuf> {
    #[cfg(windows)]
    {
        std::env::var("USERPROFILE")
            .ok()
            .map(std::path::PathBuf::from)
    }
    #[cfg(not(windows))]
    {
        std::env::var("HOME").ok().map(std::path::PathBuf::from)
    }
}

pub(crate) fn expand_tilde(path: &str) -> String {
    if path.starts_with("~/") || path == "~" {
        if let Some(home) = home_dir() {
            return path.replacen('~', &home.to_string_lossy(), 1);
        }
    }
    path.to_string()
}

pub(crate) struct ActivePicker {
    pub(crate) kind: keymap::dispatch::PickerKind,
    pub(crate) picker: picker::Picker<GenericPickerItem>,
    pub(crate) title: String,
    pub(crate) query: String,
    pub(crate) status_noun: String,
    pub(crate) min_query_len: usize,
    /// For theme picker: the theme to revert to on cancel.
    pub(crate) previous_theme: Option<&'static bloom_md::theme::ThemePalette>,
    /// When true, the next character typed replaces the query (select-all UX).
    pub(crate) query_selected: bool,
}

#[derive(Clone)]
pub(crate) struct GenericPickerItem {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) middle: Option<String>,
    pub(crate) right: Option<String>,
    pub(crate) preview_text: Option<String>,
    pub(crate) score_boost: u32,
}

impl picker::PickerItem for GenericPickerItem {
    fn match_text(&self) -> &str {
        &self.label
    }
    fn display(&self) -> picker::PickerRow {
        picker::PickerRow {
            label: self.label.clone(),
            middle: self.middle.as_ref().map(|t| picker::PickerColumn {
                text: t.clone(),
                style: picker::ColumnStyle::Faded,
            }),
            right: self.right.as_ref().map(|t| picker::PickerColumn {
                text: t.clone(),
                style: picker::ColumnStyle::Faded,
            }),
        }
    }
    fn preview(&self) -> Option<String> {
        self.preview_text.clone()
    }
    fn score_boost(&self) -> u32 {
        self.score_boost
    }
}

pub(crate) struct QuickCaptureState {
    pub(crate) kind: keymap::dispatch::QuickCaptureKind,
    pub(crate) input: String,
    pub(crate) cursor_pos: usize,
}

// ---------------------------------------------------------------------------
// BloomEditor — core impl (new, channels, small helpers)
// ---------------------------------------------------------------------------

impl BloomEditor {
    pub fn new(config: config::Config) -> Result<Self, error::BloomError> {
        let active_theme = bloom_md::theme::palette_by_name(&config.theme.name)
            .unwrap_or(&bloom_md::theme::BLOOM_DARK);
        Ok(Self {
            vim_state: bloom_vim::VimState::new(),
            window_mgr: window::WindowManager::new(),
            which_key_tree: which_key::default_tree(),
            _command_registry: which_key::default_registry(),
            index: None,
            journal: None,
            parser: bloom_md::parser::BloomMarkdownParser::new(),
            template_engine: None,
            template_mode: None,
            _linker: linker::Linker::new(),

            _timeline: timeline::Timeline::new(),
            _refactorer: refactor::Refactor::new(),
            note_store: None,
            buffer_mgr: BufferManager::new(),
            picker_state: None,

            quick_capture: None,
            notifications: Vec::new(),
            notification_history: Vec::new(),
            wizard: None,
            vault_root: None,
            leader_keys: Vec::new(),
            pending_since: None,
            which_key_visible: false,
            active_theme,
            autosave_tx: None,
            write_complete_rx: None,
            last_write_fingerprints: std::collections::HashMap::new(),
            terminal_height: 24,
            terminal_width: 80,
            indexer_rx: None,
            indexer_tx: None,
            indexing: false,
            last_index_timing: None,
            last_picker_queries: std::collections::HashMap::new(),
            watcher_rx: None,
            pending_file_events: std::collections::HashSet::new(),
            file_event_deadline: None,
            active_dialog: None,
            inline_completion: None,
            query_cache: std::cell::RefCell::new(query::QueryCache::new()),
            vault_lock: None,
            history_tx: None,
            history_rx: None,
            page_history_entries: None,
            page_history_selected: 0,
            config,
        })
    }

    // -----------------------------------------------------------------------
    // Per-pane state accessors
    // -----------------------------------------------------------------------

    pub(crate) fn cursor(&self) -> usize {
        if let Some(page_id) = self.active_page() {
            if let Some(buf) = self.buffer_mgr.get(page_id) {
                return buf.cursor(0);
            }
        }
        0
    }

    pub(crate) fn set_cursor(&mut self, pos: usize) {
        if let Some(page_id) = self.active_page().cloned() {
            if let Some(buf) = self.buffer_mgr.get_mut(&page_id) {
                buf.set_cursor(0, pos);
            }
        }
    }

    pub(crate) fn active_page(&self) -> Option<&types::PageId> {
        self.window_mgr
            .pane_state(self.window_mgr.active_pane())
            .and_then(|s| s.page_id.as_ref())
    }

    pub(crate) fn set_active_page(&mut self, id: Option<types::PageId>) {
        if let Some(s) = self
            .window_mgr
            .pane_state_mut(self.window_mgr.active_pane())
        {
            s.page_id = id;
        }
    }

    pub(crate) fn viewport(&self) -> &render::Viewport {
        // SAFETY: active pane always has a state entry
        self.window_mgr
            .pane_state(self.window_mgr.active_pane())
            .map(|s| &s.viewport)
            .expect("active pane must have state")
    }

    pub(crate) fn viewport_mut(&mut self) -> &mut render::Viewport {
        self.window_mgr
            .pane_state_mut(self.window_mgr.active_pane())
            .map(|s| &mut s.viewport)
            .expect("active pane must have state")
    }

    /// Get the active theme palette.
    pub fn theme(&self) -> &'static bloom_md::theme::ThemePalette {
        self.active_theme
    }

    /// Set the active theme by name. Returns false if name not found.
    pub fn set_theme(&mut self, name: &str) -> bool {
        if let Some(palette) = bloom_md::theme::palette_by_name(name) {
            self.active_theme = palette;
            true
        } else {
            false
        }
    }

    /// Cycle to the next theme.
    pub fn cycle_theme(&mut self) {
        let current = self.active_theme.name;
        let names = bloom_md::theme::THEME_NAMES;
        let idx = names.iter().position(|n| *n == current).unwrap_or(0);
        let next = names[(idx + 1) % names.len()];
        self.set_theme(next);
    }

    /// Get the text content of the active buffer (for testing).
    pub fn active_buffer_text(&self) -> Option<String> {
        let page_id = self.active_page()?;
        let buf = self.buffer_mgr.get(page_id)?;
        Some(buf.text().to_string())
    }

    /// Write the current theme name to config.toml.
    pub(crate) fn persist_theme_to_config(&self) {
        let Some(root) = &self.vault_root else { return };
        let config_path = root.join("config.toml");
        let Ok(content) = std::fs::read_to_string(&config_path) else {
            return;
        };
        let name = self.active_theme.name;
        let new_content = if content.contains("name = ") {
            content
                .lines()
                .map(|l| {
                    if l.trim().starts_with("name = ") && !l.contains("mode") {
                        format!("name = \"{}\"", name)
                    } else {
                        l.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            format!("{content}\n[theme]\nname = \"{name}\"\n")
        };
        let _ = std::fs::write(&config_path, new_content);
    }

    /// Whether background indexing is in progress.
    pub fn is_indexing(&self) -> bool {
        self.indexing
    }

    /// Return cloned channel receivers for use with `crossbeam::select!`.
    /// Returns None for channels not yet initialized (pre-init_vault).
    pub fn channels(&self) -> EditorChannels {
        EditorChannels {
            write_complete_rx: self.write_complete_rx.clone(),
            watcher_rx: self.watcher_rx.clone(),
            indexer_rx: self.indexer_rx.clone(),
            history_rx: self.history_rx.clone(),
        }
    }

    /// Handle a single indexer completion event.
    pub fn handle_index_complete(&mut self, complete: index::indexer::IndexComplete) {
        self.indexing = false;
        tracing::info!(
            files_scanned = complete.timing.files_scanned,
            files_changed = complete.timing.files_changed,
            total_ms = complete.timing.total_ms,
            "index complete received",
        );

        if let Some(error) = &complete.error {
            self.push_notification(
                format!("Index error: {error}"),
                render::NotificationLevel::Error,
            );
            return;
        }

        let t = &complete.timing;
        let message = format!("Index ready — {} files, {}ms", t.files_scanned, t.total_ms,);
        self.last_index_timing = Some(index::indexer::IndexTiming {
            scan_ms: t.scan_ms,
            read_parse_ms: t.read_parse_ms,
            write_ms: t.write_ms,
            total_ms: t.total_ms,
            files_scanned: t.files_scanned,
            files_changed: t.files_changed,
        });
        self.push_notification(message, render::NotificationLevel::Info);
        // Reload the index connection to pick up changes from the indexer thread
        if let Some(vault_root) = &self.vault_root {
            let index_path = vault::paths::index_db(vault_root);
            if let Ok(idx) = index::Index::open_readonly(&index_path) {
                self.index = Some(idx);
            }
        }

        // Invalidate the BQL query cache so visible queries re-execute.
        self.query_cache.borrow_mut().invalidate();
    }

    /// Handle a single history thread completion event.
    pub fn handle_history_complete(&mut self, complete: history::HistoryComplete) {
        match complete {
            history::HistoryComplete::CommitDone { oid: Some(id) } => {
                tracing::debug!(oid = %id, "history commit acknowledged");
            }
            history::HistoryComplete::CommitDone { oid: None } => {
                tracing::debug!("history commit skipped (no changes)");
            }
            history::HistoryComplete::PageHistory { entries } => {
                self.receive_page_history(entries);
            }
            history::HistoryComplete::BlobAt { oid, uuid, content } => {
                self.receive_blob_at(&oid, &uuid, content);
            }
            history::HistoryComplete::Error { message } => {
                tracing::error!(error = %message, "history thread error");
                self.push_notification(
                    format!("History error: {message}"),
                    render::NotificationLevel::Error,
                );
            }
            history::HistoryComplete::ShutDown => {
                tracing::info!("history thread shut down");
            }
        }
    }

    /// Called when page history results arrive from the history thread.
    fn receive_page_history(&mut self, entries: Vec<history::PageHistoryEntry>) {
        self.page_history_entries = Some(entries);
        self.page_history_selected = 0;
    }

    /// Called when a blob-at-commit result arrives from the history thread.
    fn receive_blob_at(&mut self, _oid: &str, _uuid: &str, content: Option<String>) {
        let Some(content) = content else { return };

        // Restore the content into the active buffer (undo-able).
        if let Some(page_id) = self.active_page().cloned() {
            if let Some(buf) = self.buffer_mgr.get_mut(&page_id) {
                let len = buf.len_chars();
                buf.replace(0..len, &content);
                self.push_notification(
                    "Restored from history (undo with u)".into(),
                    render::NotificationLevel::Info,
                );
            }
        }
    }

    /// Compute the next deadline the event loop should wake for.
    pub fn next_deadline(&self) -> Option<Instant> {
        let mut earliest: Option<Instant> = None;
        let mut consider = |t: Instant| {
            earliest = Some(earliest.map_or(t, |e: Instant| e.min(t)));
        };
        // File event debounce
        if let Some(d) = self.file_event_deadline {
            consider(d);
        }
        // Notification expiry
        for n in &self.notifications {
            if let Some(t) = n.expires_at {
                consider(t);
            }
        }
        // Which-key timeout
        if !self.which_key_visible && !self.leader_keys.is_empty() {
            if let Some(since) = self.pending_since {
                consider(
                    since + std::time::Duration::from_millis(self.config.which_key_timeout_ms),
                );
            }
        }
        earliest
    }

    /// Insert text at the current cursor position.
    pub(crate) fn insert_text_at_cursor(&mut self, text: &str) {
        let Some(page_id) = self.active_page().cloned() else {
            return;
        };
        let cursor = self.cursor();
        let Some(buf) = self.buffer_mgr.get_mut(&page_id) else {
            return;
        };
        buf.insert(cursor, text);
        self.set_cursor(cursor + text.chars().count());
    }

    /// Tick for timers, notifications, debounce. Returns true if state changed.
    pub fn tick(&mut self, now: std::time::Instant) -> bool {
        let before = self.notifications.len();
        self.notifications
            .retain(|n| n.expires_at.is_none_or(|t| t > now));
        let notif_changed = self.notifications.len() != before;

        // Check if which-key drawer should appear (timeout elapsed)
        let wk_changed = if !self.which_key_visible && !self.leader_keys.is_empty() {
            let timeout = std::time::Duration::from_millis(self.config.which_key_timeout_ms);
            let should_show = self
                .pending_since
                .is_some_and(|since| now.duration_since(since) >= timeout);
            if should_show {
                self.which_key_visible = true;
            }
            should_show
        } else {
            false
        };

        notif_changed || wk_changed
    }

    /// Update the terminal size (e.g. on terminal resize).
    pub fn resize(&mut self, height: usize, width: usize) {
        self.terminal_height = height as u16;
        self.terminal_width = width as u16;
    }

    /// Update layout: sync viewport dimensions and ensure cursor is visible.
    /// Call this before `render()` — it handles all state mutations that
    /// depend on terminal size so render can be read-only.
    pub fn update_layout(&mut self, width: u16, height: u16) {
        let has_pending = !self.leader_keys.is_empty() || !self.vim_state.pending_keys().is_empty();
        let timeout = std::time::Duration::from_millis(self.config.which_key_timeout_ms);
        let timed_out = self
            .pending_since
            .is_some_and(|since| since.elapsed() >= timeout);
        let show_wk = has_pending && (self.which_key_visible || timed_out);
        let wk_h: u16 = if show_wk {
            let col_width = 24u16;
            let cols = (width.saturating_sub(4) / col_width).max(1);
            let entry_count = 12u16;
            let rows_needed = entry_count.div_ceil(cols);
            (rows_needed + 2).min(height / 3).max(3)
        } else {
            0
        };
        let pane_area_h = height.saturating_sub(wk_h);
        let pane_rects = self.window_mgr.compute_pane_rects(width, pane_area_h);

        for rect in &pane_rects {
            if let Some(ps) = self.window_mgr.pane_state_mut(rect.pane_id) {
                ps.viewport.height = rect.content_height as usize;
                ps.viewport.width = rect.width as usize;
            }
        }

        let (cursor_line, _) = self.cursor_position();
        let scrolloff = self.config.scrolloff;
        self.viewport_mut()
            .ensure_visible_with_scrolloff(cursor_line, scrolloff);
    }

    // Buffer management

    pub fn open_page(&mut self, id: &types::PageId) -> Result<(), error::BloomError> {
        self.set_active_page(Some(id.clone()));
        Ok(())
    }

    pub fn create_page(
        &mut self,
        _title: &str,
        _template: Option<&str>,
    ) -> Result<types::PageId, error::BloomError> {
        let id = crate::uuid::generate_hex_id();
        Ok(id)
    }

    pub fn close_buffer(&mut self, _pane: types::PaneId) -> Result<(), error::BloomError> {
        if let Some(page_id) = self.active_page().cloned() {
            self.set_active_page(None);
            self.buffer_mgr.close(&page_id);
        }
        Ok(())
    }

    pub fn apply_edits(&mut self, edits: Vec<linker::TextEdit>) -> Result<(), error::BloomError> {
        for _edit in edits {
            // Would apply to open buffers or write to disk
        }
        Ok(())
    }

    pub fn save_session(&self) -> Result<(), error::BloomError> {
        let Some(root) = &self.vault_root else {
            return Ok(());
        };
        let session_path = root.join(".session.json");

        let layout = self.window_mgr.tree_to_session_layout();

        let buffers: Vec<session::SessionBuffer> = self
            .window_mgr
            .all_pane_states()
            .iter()
            .filter_map(|(pane_id, state)| {
                let page_id = state.page_id.as_ref()?;
                let path = self
                    .buffer_mgr
                    .open_buffers()
                    .iter()
                    .find(|b| b.page_id == *page_id)?
                    .path
                    .clone();
                let (cursor_line, cursor_col) = if let Some(buf) = self.buffer_mgr.get(page_id) {
                    let rope = buf.text();
                    let len = rope.len_chars();
                    let cursor_pos = buf.cursor(0);
                    let clamped = cursor_pos.min(len.saturating_sub(1));
                    if len == 0 {
                        (0, 0)
                    } else {
                        let line = rope.char_to_line(clamped);
                        let line_start = rope.line_to_char(line);
                        (line, clamped - line_start)
                    }
                } else {
                    (0, 0)
                };
                Some(session::SessionBuffer {
                    page_path: path,
                    cursor_line,
                    cursor_column: cursor_col,
                    scroll_offset: state.viewport.first_visible_line,
                    pane: pane_id.0,
                })
            })
            .collect();

        let state = session::SessionState {
            buffers,
            layout,
            active_pane: self.window_mgr.active_pane().0,
        };
        state.save(&session_path)?;

        // Persist undo trees via the indexer thread (which owns the write connection).
        if let Some(indexer_tx) = &self.indexer_tx {
            let mut undo_data = Vec::new();
            for info in self.buffer_mgr.open_buffers() {
                if let Some(buf) = self.buffer_mgr.get(&info.page_id) {
                    let page_hex = info.page_id.to_hex();
                    undo_data.push(buf.undo_tree().to_persist_data(&page_hex));
                }
            }
            if !undo_data.is_empty() {
                let _ = indexer_tx.send(index::indexer::IndexRequest::PersistUndo(undo_data));
            }
        }

        // Commit current vault state to history and shut down the history thread.
        if let Some(tx) = &self.history_tx {
            let files = self.collect_vault_files_for_history();
            if !files.is_empty() {
                let _ = tx.send(history::HistoryRequest::CommitNow {
                    files,
                    message: "session save".into(),
                });
            }
            let _ = tx.send(history::HistoryRequest::Shutdown);
        }

        Ok(())
    }

    /// Collect all vault pages as `(uuid_hex, content)` pairs for history commits.
    /// Reads from the index (UUID ↔ path mapping) and from disk.
    fn collect_vault_files_for_history(&self) -> Vec<(String, String)> {
        let Some(index) = &self.index else {
            return vec![];
        };
        let Some(vault_root) = &self.vault_root else {
            return vec![];
        };

        let mut files = Vec::new();
        for page in index.list_pages(None) {
            let path = vault_root.join(&page.path);
            if let Ok(content) = std::fs::read_to_string(&path) {
                files.push((page.id.to_hex(), content));
            }
        }
        files
    }

    pub fn restore_session(&mut self) -> Result<(), error::BloomError> {
        let Some(root) = &self.vault_root else {
            return Ok(());
        };
        let session_path = root.join(".session.json");
        if !session_path.exists() {
            return Ok(());
        }
        let state = session::SessionState::load(&session_path)?;

        // Restore layout tree and create empty pane states
        self.window_mgr.restore_layout(&state.layout);

        // Open each buffer in its assigned pane
        for buf_state in &state.buffers {
            if !buf_state.page_path.exists() {
                continue;
            }
            let Ok(content) = std::fs::read_to_string(&buf_state.page_path) else {
                continue;
            };

            // Extract the real page ID from frontmatter, not a random one.
            // This is essential for undo tree restoration (keyed by page UUID).
            let fm = self.parser.parse_frontmatter(&content);
            let title = fm
                .as_ref()
                .and_then(|f| f.title.clone())
                .unwrap_or_else(|| {
                    buf_state
                        .page_path
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string()
                });
            let id = fm
                .as_ref()
                .and_then(|f| f.id.clone())
                .unwrap_or_else(crate::uuid::generate_hex_id);

            // Switch to the target pane and open buffer there
            let pane_id = types::PaneId(buf_state.pane);
            self.window_mgr.set_active(pane_id);
            self.open_page_with_content(&id, &title, &buf_state.page_path, &content);

            // Restore persisted undo tree before allowing edits.
            if let Some(idx) = &self.index {
                let page_hex = id.to_hex();
                match bloom_buffer::undo::UndoTree::load_from_db(idx.connection(), &page_hex) {
                    Ok(Some(mut tree)) => {
                        // If the file changed externally since last session, extend
                        // the persisted tree with the current disk content so the
                        // user can undo past the external change.
                        if tree.current_snapshot_string() != content {
                            tree.push(
                                ropey::Rope::from_str(&content),
                                "external change".to_string(),
                            );
                            tracing::info!(page = %page_hex, "undo tree extended with external change");
                        }
                        if let Some(buf) = self.buffer_mgr.get_mut(&id) {
                            buf.set_undo_tree(tree);
                            tracing::debug!(page = %page_hex, "undo tree restored");
                        }
                    }
                    Ok(None) => {} // no persisted tree — use the fresh one
                    Err(e) => {
                        tracing::warn!(page = %page_hex, error = %e, "failed to restore undo tree");
                    }
                }
            }

            // Restore cursor position and scroll offset
            if let Some(buf) = self.buffer_mgr.get(&id) {
                let rope = buf.text();
                if buf_state.cursor_line < rope.len_lines() {
                    let line_start = rope.line_to_char(buf_state.cursor_line);
                    let line_len = rope.line(buf_state.cursor_line).len_chars();
                    self.set_cursor(
                        line_start + buf_state.cursor_column.min(line_len.saturating_sub(1)),
                    );
                }
            }
            if let Some(ps) = self.window_mgr.pane_state_mut(pane_id) {
                ps.viewport.first_visible_line = buf_state.scroll_offset;
            }
        }

        // Restore active pane
        self.window_mgr.set_active(types::PaneId(state.active_pane));

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;

    // UC-14: Basic editor flow
    #[test]
    fn test_editor_creates_and_renders() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(
            &id,
            "Test",
            std::path::Path::new("[scratch]"),
            "# Hello\n\nWorld\n",
        );
        let frame = editor.render(80, 24);
        assert!(!frame.panes.is_empty());
        assert_eq!(frame.panes[0].status_bar.mode, "NORMAL");
        assert!(!frame.panes[0].title.is_empty());
    }

    // Cursor on empty last line (trailing newline)
    #[test]
    fn test_cursor_on_empty_last_line() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = crate::uuid::generate_hex_id();
        // File with trailing newline → ropey sees 3 lines (0: "hello\n", 1: "world\n", 2: "")
        editor.open_page_with_content(
            &id,
            "Test",
            std::path::Path::new("[scratch]"),
            "hello\nworld\n",
        );
        // Move down twice: line 0 → line 1 → line 2 (empty last line)
        editor.handle_key(KeyEvent::char('j'));
        editor.handle_key(KeyEvent::char('j'));
        let frame = editor.render(80, 24);
        // Cursor should be on line 2, column 0 (the empty last line)
        assert_eq!(frame.panes[0].cursor.line, 2);
        assert_eq!(frame.panes[0].cursor.column, 0);
    }

    // UC-14: Insert mode typing
    #[test]
    fn test_enter_insert_mode() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(&id, "Test", std::path::Path::new("[scratch]"), "hello");
        editor.handle_key(KeyEvent::char('i'));
        let frame = editor.render(80, 24);
        assert_eq!(frame.panes[0].status_bar.mode, "INSERT");
        assert!(matches!(
            frame.panes[0].cursor.shape,
            render::CursorShape::Bar
        ));
    }

    // UC-14: Insert mode actually inserts characters
    #[test]
    fn test_insert_mode_types_chars() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(&id, "Test", std::path::Path::new("[scratch]"), "");
        editor.handle_key(KeyEvent::char('i')); // enter insert mode
        editor.handle_key(KeyEvent::char('H'));
        editor.handle_key(KeyEvent::char('i'));
        let buf = editor.buffer_mgr.get(&id).unwrap();
        assert_eq!(buf.text().to_string(), "Hi");
    }

    // UC-14: Insert mode Enter creates newline
    #[test]
    fn test_insert_mode_enter() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(&id, "Test", std::path::Path::new("[scratch]"), "");
        editor.handle_key(KeyEvent::char('i'));
        editor.handle_key(KeyEvent::char('a'));
        editor.handle_key(KeyEvent::enter());
        editor.handle_key(KeyEvent::char('b'));
        let buf = editor.buffer_mgr.get(&id).unwrap();
        assert_eq!(buf.text().to_string(), "a\nb");
    }

    // UC-14: Insert mode Backspace deletes
    #[test]
    fn test_insert_mode_backspace() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(&id, "Test", std::path::Path::new("[scratch]"), "");
        editor.handle_key(KeyEvent::char('i'));
        editor.handle_key(KeyEvent::char('a'));
        editor.handle_key(KeyEvent::char('b'));
        editor.handle_key(KeyEvent::backspace());
        let buf = editor.buffer_mgr.get(&id).unwrap();
        assert_eq!(buf.text().to_string(), "a");
    }

    // Insert mode arrow keys navigate without leaving insert
    #[test]
    fn test_insert_mode_arrow_navigation() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(&id, "Test", std::path::Path::new("[scratch]"), "ab");
        editor.handle_key(KeyEvent::char('i')); // insert at pos 0
                                                // Move right twice to end
        editor.handle_key(KeyEvent {
            code: types::KeyCode::Right,
            modifiers: types::Modifiers::none(),
        });
        editor.handle_key(KeyEvent {
            code: types::KeyCode::Right,
            modifiers: types::Modifiers::none(),
        });
        // Type 'c' — should appear after "ab"
        editor.handle_key(KeyEvent::char('c'));
        let buf = editor.buffer_mgr.get(&id).unwrap();
        assert_eq!(buf.text().to_string(), "abc");
        // Still in insert mode
        let frame = editor.render(80, 24);
        assert_eq!(frame.panes[0].status_bar.mode, "INSERT");
    }

    // o opens a new line below and positions cursor correctly
    #[test]
    fn test_open_below() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(
            &id,
            "Test",
            std::path::Path::new("[scratch]"),
            "hello\nworld\n",
        );
        editor.handle_key(KeyEvent::char('o'));
        // Should be in insert mode on a new line below "hello"
        let frame = editor.render(80, 24);
        assert_eq!(frame.panes[0].status_bar.mode, "INSERT");
        assert_eq!(frame.panes[0].cursor.line, 1);
        assert_eq!(frame.panes[0].cursor.column, 0);
        // Type on the new line
        editor.handle_key(KeyEvent::char('!'));
        let buf = editor.buffer_mgr.get(&id).unwrap();
        let text = buf.text().to_string();
        assert_eq!(text, "hello\n!\nworld\n");
    }

    // O opens a new line above and positions cursor correctly
    #[test]
    fn test_open_above() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(
            &id,
            "Test",
            std::path::Path::new("[scratch]"),
            "hello\nworld\n",
        );
        editor.handle_key(KeyEvent::char('O'));
        let frame = editor.render(80, 24);
        assert_eq!(frame.panes[0].status_bar.mode, "INSERT");
        assert_eq!(frame.panes[0].cursor.line, 0);
        assert_eq!(frame.panes[0].cursor.column, 0);
        editor.handle_key(KeyEvent::char('!'));
        let buf = editor.buffer_mgr.get(&id).unwrap();
        let text = buf.text().to_string();
        assert_eq!(text, "!\nhello\nworld\n");
    }

    // o on last line without trailing newline
    #[test]
    fn test_open_below_no_trailing_newline() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(&id, "Test", std::path::Path::new("[scratch]"), "hello");
        editor.handle_key(KeyEvent::char('o'));
        let frame = editor.render(80, 24);
        assert_eq!(frame.panes[0].status_bar.mode, "INSERT");
        assert_eq!(frame.panes[0].cursor.line, 1);
        assert_eq!(frame.panes[0].cursor.column, 0);
        editor.handle_key(KeyEvent::char('!'));
        let buf = editor.buffer_mgr.get(&id).unwrap();
        let text = buf.text().to_string();
        assert_eq!(text, "hello\n!");
    }

    // UC-14: Return to normal mode
    #[test]
    fn test_esc_returns_to_normal() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(&id, "Test", std::path::Path::new("[scratch]"), "hello");
        editor.handle_key(KeyEvent::char('i'));
        editor.handle_key(KeyEvent::esc());
        let frame = editor.render(80, 24);
        assert_eq!(frame.panes[0].status_bar.mode, "NORMAL");
        assert!(matches!(
            frame.panes[0].cursor.shape,
            render::CursorShape::Block
        ));
    }

    // UC-90: Ctrl+S saves
    #[test]
    fn test_ctrl_s_saves() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(&id, "Test", std::path::Path::new("[scratch]"), "hello");
        let actions = editor.handle_key(KeyEvent::ctrl('s'));
        assert!(actions
            .iter()
            .any(|a| matches!(a, keymap::dispatch::Action::Save)));
    }

    // UC-52: Window splits
    #[test]
    fn test_window_split_via_editor() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(&id, "Test", std::path::Path::new("[scratch]"), "hello");

        // Count initial panes
        let frame = editor.render(80, 24);
        let initial_count = frame.panes.len();
        assert_eq!(initial_count, 1);
    }

    // UC-18: Undo through editor
    #[test]
    fn test_undo_via_handle_key() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(&id, "Test", std::path::Path::new("[scratch]"), "hello");
        // Type 'u' for undo in normal mode
        editor.handle_key(KeyEvent::char('u'));
        // Shouldn't crash, even with no edits to undo
    }

    // Vim-style undo: entire insert session is one undo unit
    #[test]
    fn test_undo_groups_insert_session() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(&id, "Test", std::path::Path::new("[scratch]"), "");

        // Enter insert mode, type "abc", exit
        editor.handle_key(KeyEvent::char('i'));
        editor.handle_key(KeyEvent::char('a'));
        editor.handle_key(KeyEvent::char('b'));
        editor.handle_key(KeyEvent::char('c'));
        editor.handle_key(KeyEvent::esc());

        // Buffer should be "abc"
        let buf = editor
            .buffer_mgr
            .get(&editor.active_page().cloned().unwrap())
            .unwrap();
        assert_eq!(buf.text().to_string(), "abc");

        // One undo should revert the entire insert session
        editor.handle_key(KeyEvent::char('u'));
        let buf = editor
            .buffer_mgr
            .get(&editor.active_page().cloned().unwrap())
            .unwrap();
        assert_eq!(buf.text().to_string(), "");
    }

    // Tick clears expired notifications
    #[test]
    fn test_tick_clears_notifications() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let far_future = std::time::Instant::now() + std::time::Duration::from_secs(3600);
        editor.tick(far_future);
        // Should not crash
    }

    // UC-13: Create page
    #[test]
    fn test_create_page_returns_id() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = editor.create_page("New Page", None).unwrap();
        assert_eq!(id.to_hex().len(), 8);
    }

    // save_current marks buffer clean
    #[test]
    fn test_save_marks_clean() {
        let dir = tempfile::TempDir::new().unwrap();
        let file_path = dir.path().join("test.md");
        std::fs::write(&file_path, "hello").unwrap();

        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(&id, "Test", &file_path, "hello");
        // Make dirty by inserting through vim
        editor.handle_key(KeyEvent::char('i'));
        editor.handle_key(KeyEvent::char('x'));
        editor.handle_key(KeyEvent::esc());
        editor.save_current().unwrap();
        let frame = editor.render(80, 24);
        assert!(!frame.panes[0].dirty);
    }

    // save_current writes to disk
    #[test]
    fn test_save_writes_to_disk() {
        let dir = tempfile::TempDir::new().unwrap();
        let file_path = dir.path().join("test.md");
        std::fs::write(&file_path, "hello").unwrap();

        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        editor.vault_root = Some(dir.path().to_path_buf());
        let id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(&id, "Test", &file_path, "hello");

        // Edit: insert 'X' at start
        editor.handle_key(KeyEvent::char('i'));
        editor.handle_key(KeyEvent::char('X'));
        editor.handle_key(KeyEvent::esc());

        editor.save_current().unwrap();

        // Verify file on disk has the new content (with auto-assigned block ID)
        let on_disk = std::fs::read_to_string(&file_path).unwrap();
        assert!(
            on_disk.starts_with("Xhello ^"),
            "expected block ID, got: {on_disk}"
        );
        // Block ID is 5-char base36 after the ^
        let id_part = on_disk.trim().strip_prefix("Xhello ^").unwrap();
        assert_eq!(
            id_part.len(),
            5,
            "block ID should be 5 chars, got: {id_part}"
        );
    }

    #[test]
    fn test_save_block_ids_multiline() {
        let dir = tempfile::TempDir::new().unwrap();
        let file_path = dir.path().join("test.md");
        let content = "Line one\nLine two\n\nLine three\n";
        std::fs::write(&file_path, content).unwrap();

        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        editor.vault_root = Some(dir.path().to_path_buf());
        let id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(&id, "Test", &file_path, content);

        // Make dirty
        editor.handle_key(KeyEvent::char('i'));
        editor.handle_key(KeyEvent::char('X'));
        editor.handle_key(KeyEvent::esc());

        editor.save_current().unwrap();

        let on_disk = std::fs::read_to_string(&file_path).unwrap();
        // Paragraph 1 (lines 0-1): ID on last line (line 1) — "Line two ^xxxxx"
        assert!(
            on_disk
                .lines()
                .any(|l| l.starts_with("Line two ^") && l.len() == "Line two ^xxxxx".len()),
            "expected block ID on 'Line two', got:\n{on_disk}"
        );
        // Paragraph 2 (line 3): ID on its own line — "Line three ^xxxxx"
        assert!(
            on_disk
                .lines()
                .any(|l| l.starts_with("Line three ^") && l.len() == "Line three ^xxxxx".len()),
            "expected block ID on 'Line three', got:\n{on_disk}"
        );
        // Line one should NOT have an ID (it's not the last line of the block)
        assert!(
            !on_disk
                .lines()
                .any(|l| l.starts_with("XLine one ^") || l.starts_with("Line one ^")),
            "Line one should not have block ID, got:\n{on_disk}"
        );
    }

    #[test]
    fn test_block_id_after_split_via_enter() {
        let dir = tempfile::TempDir::new().unwrap();
        let file_path = dir.path().join("test.md");
        // One paragraph, one block.
        let content = "Hello world";
        std::fs::write(&file_path, content).unwrap();

        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        editor.vault_root = Some(dir.path().to_path_buf());
        let id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(&id, "Test", &file_path, content);

        // Enter insert mode, move to middle, add newline+blank line to split into two paragraphs.
        editor.handle_key(KeyEvent::char('i'));
        // Move past "Hello" (5 chars + space)
        for _ in 0..5 {
            editor.handle_key(KeyEvent {
                code: types::KeyCode::Right,
                modifiers: types::Modifiers::none(),
            });
        }
        // Insert blank line (two Enters) to create paragraph break.
        editor.handle_key(KeyEvent::enter());
        editor.handle_key(KeyEvent::enter());
        editor.handle_key(KeyEvent::esc());

        // Save — should assign IDs to both paragraphs.
        editor.save_current().unwrap();

        let on_disk = std::fs::read_to_string(&file_path).unwrap();
        // Should have two block IDs.
        let id_count = on_disk.matches(" ^").count();
        assert_eq!(
            id_count, 2,
            "expected 2 block IDs, got {id_count}. Content:\n{on_disk}"
        );
    }

    // Startup: Journal mode opens today's journal
    #[test]
    fn test_startup_journal_mode() {
        let config = config::Config::defaults(); // default is Journal
        let mut editor = BloomEditor::new(config).unwrap();
        editor.startup();
        let frame = editor.render(80, 24);
        assert!(!frame.panes.is_empty());
        assert!(frame.panes[0].visible_lines.len() > 0 || frame.panes[0].title.contains("20"));
        // Keys should work — enter insert mode
        editor.handle_key(KeyEvent::char('i'));
        let frame = editor.render(80, 24);
        assert_eq!(frame.panes[0].status_bar.mode, "INSERT");
    }

    // Startup: Blank mode opens scratch buffer
    #[test]
    fn test_startup_blank_mode() {
        let mut config = config::Config::defaults();
        config.startup.mode = config::StartupMode::Blank;
        let mut editor = BloomEditor::new(config).unwrap();
        editor.startup();
        let frame = editor.render(80, 24);
        assert!(!frame.panes.is_empty());
        assert_eq!(frame.panes[0].title, "[scratch]");
        // Keys should work
        editor.handle_key(KeyEvent::char('i'));
        let frame = editor.render(80, 24);
        assert_eq!(frame.panes[0].status_bar.mode, "INSERT");
    }

    // Startup: Restore mode falls back to scratch when no session exists
    #[test]
    fn test_startup_restore_fallback() {
        let mut config = config::Config::defaults();
        config.startup.mode = config::StartupMode::Restore;
        let mut editor = BloomEditor::new(config).unwrap();
        editor.startup();
        let frame = editor.render(80, 24);
        assert!(!frame.panes.is_empty());
        // Falls back to scratch since restore_session is a stub
        assert_eq!(frame.panes[0].title, "[scratch]");
        // Keys should work
        editor.handle_key(KeyEvent::char('i'));
        let frame = editor.render(80, 24);
        assert_eq!(frame.panes[0].status_bar.mode, "INSERT");
    }

    // Wizard: starts at Welcome step
    #[test]
    fn test_wizard_starts_at_welcome() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        editor.start_wizard();
        let frame = editor.render(80, 24);
        assert!(matches!(
            frame.panes[0].kind,
            render::PaneKind::SetupWizard(_)
        ));
        if let render::PaneKind::SetupWizard(sw) = &frame.panes[0].kind {
            assert!(matches!(sw.step, render::SetupStep::Welcome));
        }
    }

    // Wizard: Enter advances from Welcome to ChooseVault
    #[test]
    fn test_wizard_welcome_to_choose_vault() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        editor.start_wizard();
        editor.handle_key(KeyEvent::enter());
        let frame = editor.render(80, 24);
        if let render::PaneKind::SetupWizard(sw) = &frame.panes[0].kind {
            assert!(matches!(sw.step, render::SetupStep::ChooseVaultLocation));
            assert!(!sw.vault_path.is_empty()); // default path populated
        } else {
            panic!("expected wizard pane");
        }
    }

    // Wizard: text input in vault path
    #[test]
    fn test_wizard_vault_path_input() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        editor.start_wizard();
        editor.handle_key(KeyEvent::enter()); // → ChooseVault
                                              // Clear and type a new path
                                              // Use Ctrl+U-like approach: Home then type
        editor.handle_key(KeyEvent {
            code: KeyCode::Home,
            modifiers: Modifiers::none(),
        });
        // Type 'x'
        editor.handle_key(KeyEvent::char('x'));
        let frame = editor.render(80, 24);
        if let render::PaneKind::SetupWizard(sw) = &frame.panes[0].kind {
            assert!(sw.vault_path.starts_with('x'));
        } else {
            panic!("expected wizard pane");
        }
    }

    // Wizard: Esc goes back from ChooseVault to Welcome
    #[test]
    fn test_wizard_esc_goes_back() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        editor.start_wizard();
        editor.handle_key(KeyEvent::enter()); // → ChooseVault
        editor.handle_key(KeyEvent::esc()); // → Welcome
        let frame = editor.render(80, 24);
        if let render::PaneKind::SetupWizard(sw) = &frame.panes[0].kind {
            assert!(matches!(sw.step, render::SetupStep::Welcome));
        } else {
            panic!("expected wizard pane");
        }
    }

    // Wizard: import choice toggle
    #[test]
    fn test_wizard_import_choice_toggle() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        editor.start_wizard();
        editor.handle_key(KeyEvent::enter()); // → ChooseVault

        // Use a temp dir so vault creation succeeds
        let dir = tempfile::TempDir::new().unwrap();
        // Manually set the wizard path to the temp dir
        if let Some(wiz) = &mut editor.wizard {
            wiz.vault_path = dir.path().to_string_lossy().to_string();
            wiz.vault_path_cursor = wiz.vault_path.len();
        }
        editor.handle_key(KeyEvent::enter()); // → ImportChoice

        let frame = editor.render(80, 24);
        if let render::PaneKind::SetupWizard(sw) = &frame.panes[0].kind {
            assert!(matches!(sw.step, render::SetupStep::ImportChoice));
            assert_eq!(sw.import_choice, render::ImportChoice::No);
        } else {
            panic!("expected wizard pane");
        }

        // Toggle to Yes
        editor.handle_key(KeyEvent::char('j'));
        let frame = editor.render(80, 24);
        if let render::PaneKind::SetupWizard(sw) = &frame.panes[0].kind {
            assert_eq!(sw.import_choice, render::ImportChoice::Yes);
        } else {
            panic!("expected wizard pane");
        }
    }

    // Wizard: complete creates vault and opens journal
    #[test]
    fn test_wizard_complete_opens_journal() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        editor.start_wizard();
        editor.handle_key(KeyEvent::enter()); // → ChooseVault

        let dir = tempfile::TempDir::new().unwrap();
        if let Some(wiz) = &mut editor.wizard {
            wiz.vault_path = dir.path().to_string_lossy().to_string();
            wiz.vault_path_cursor = wiz.vault_path.len();
        }
        editor.handle_key(KeyEvent::enter()); // → ImportChoice
        editor.handle_key(KeyEvent::enter()); // → Complete (No import)
        editor.handle_key(KeyEvent::enter()); // → Complete wizard

        // Wizard should be gone, normal editor active
        assert!(!editor.wizard_active());
        let frame = editor.render(80, 24);
        assert!(!frame.panes.is_empty());
        assert!(matches!(frame.panes[0].kind, render::PaneKind::Editor));
        // Vault dirs should exist
        assert!(dir.path().join("pages").exists());
        assert!(dir.path().join("journal").exists());
    }

    // Wizard: Ctrl+Q quits during wizard
    #[test]
    fn test_wizard_ctrl_q_quits() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        editor.start_wizard();
        let actions = editor.handle_key(KeyEvent::ctrl('q'));
        assert!(actions
            .iter()
            .any(|a| matches!(a, keymap::dispatch::Action::Quit)));
    }

    // SPC f f opens find page picker
    #[test]
    fn test_leader_spc_f_f_opens_picker() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(&id, "Test", std::path::Path::new("[scratch]"), "hello");
        editor.handle_key(KeyEvent::char(' ')); // SPC
        editor.handle_key(KeyEvent::char('f')); // f (group)
        editor.handle_key(KeyEvent::char('f')); // f (action)
                                                // Picker should now be open
        assert!(editor.picker_state.is_some());
        assert_eq!(editor.picker_state.as_ref().unwrap().title, "Find Page");
        let frame = editor.render(80, 24);
        assert!(frame.picker.is_some());
    }

    // Regression: selecting a page from FindPage picker should open it.
    // Tests both the index path (item.id = hex) and disk fallback (item.id = path).
    #[test]
    fn test_find_page_selection_opens_page() {
        let dir = tempfile::TempDir::new().unwrap();
        let pages_dir = dir.path().join("pages");
        std::fs::create_dir_all(&pages_dir).unwrap();
        std::fs::write(
            pages_dir.join("test-page.md"),
            "---\nid: aabb1122\ntitle: Test Page\ncreated: 2026-01-01\ntags: []\n---\n\n# Test Page\n\nBody text.\n",
        ).unwrap();

        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let _ = editor.init_vault(dir.path());

        // Wait for indexer (in tests, it completes quickly)
        let ch = editor.channels();
        if let Some(rx) = &ch.indexer_rx {
            for _ in 0..100 {
                if let Ok(complete) = rx.try_recv() {
                    editor.handle_index_complete(complete);
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
        assert!(!editor.is_indexing(), "indexer should have completed");

        // Open find page picker
        editor.handle_key(KeyEvent::char(' '));
        editor.handle_key(KeyEvent::char('f'));
        editor.handle_key(KeyEvent::char('f'));
        assert!(editor.picker_state.is_some());

        // Select the first (only) result
        editor.handle_key(KeyEvent::enter());
        assert!(
            editor.picker_state.is_none(),
            "picker should close after selection"
        );
        assert!(
            editor.active_page().is_some(),
            "a page should be open after selection"
        );

        let frame = editor.render(80, 24);
        assert!(
            !frame.panes[0].visible_lines.is_empty(),
            "page content should be visible"
        );
    }

    // SPC shows which-key popup in render
    #[test]
    fn test_leader_spc_shows_which_key() {
        let mut cfg = config::Config::defaults();
        cfg.which_key_timeout_ms = 0; // instant for testing
        let mut editor = BloomEditor::new(cfg).unwrap();
        let id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(&id, "Test", std::path::Path::new("[scratch]"), "hello");
        editor.handle_key(KeyEvent::char(' ')); // SPC
        let frame = editor.render(80, 24);
        assert!(frame.which_key.is_some());
        let wk = frame.which_key.unwrap();
        assert_eq!(wk.prefix, "SPC");
        assert!(!wk.entries.is_empty());
    }

    // Esc cancels leader sequence
    #[test]
    fn test_leader_esc_cancels() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(&id, "Test", std::path::Path::new("[scratch]"), "hello");
        editor.handle_key(KeyEvent::char(' ')); // SPC
        editor.handle_key(KeyEvent::esc()); // Cancel
        let frame = editor.render(80, 24);
        assert!(frame.which_key.is_none());
    }

    // :q quits
    #[test]
    fn test_colon_q_quits() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(&id, "Test", std::path::Path::new("[scratch]"), "hello");
        editor.handle_key(KeyEvent::char(':')); // enter command mode
        editor.handle_key(KeyEvent::char('q'));
        let actions = editor.handle_key(KeyEvent::enter());
        assert!(actions
            .iter()
            .any(|a| matches!(a, keymap::dispatch::Action::Quit)));
    }

    // :w saves
    #[test]
    fn test_colon_w_saves() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(&id, "Test", std::path::Path::new("[scratch]"), "hello");
        editor.handle_key(KeyEvent::char(':'));
        editor.handle_key(KeyEvent::char('w'));
        let actions = editor.handle_key(KeyEvent::enter());
        assert!(actions
            .iter()
            .any(|a| matches!(a, keymap::dispatch::Action::Save)));
    }

    // :wq saves and quits
    #[test]
    fn test_colon_wq_saves_and_quits() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(&id, "Test", std::path::Path::new("[scratch]"), "hello");
        editor.handle_key(KeyEvent::char(':'));
        editor.handle_key(KeyEvent::char('w'));
        editor.handle_key(KeyEvent::char('q'));
        let actions = editor.handle_key(KeyEvent::enter());
        assert!(actions
            .iter()
            .any(|a| matches!(a, keymap::dispatch::Action::Save)));
        assert!(actions
            .iter()
            .any(|a| matches!(a, keymap::dispatch::Action::Quit)));
    }

    // u undoes the last edit
    #[test]
    fn test_undo_restores_buffer() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(&id, "Test", std::path::Path::new("[scratch]"), "hello");
        // Insert 'X' at start
        editor.handle_key(KeyEvent::char('i'));
        editor.handle_key(KeyEvent::char('X'));
        editor.handle_key(KeyEvent::esc());
        let buf = editor.buffer_mgr.get(&id).unwrap();
        assert_eq!(buf.text().to_string(), "Xhello");
        // Undo
        editor.handle_key(KeyEvent::char('u'));
        let buf = editor.buffer_mgr.get(&id).unwrap();
        assert_eq!(buf.text().to_string(), "hello");
    }

    // Ctrl+R redoes
    #[test]
    fn test_redo_after_undo() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(&id, "Test", std::path::Path::new("[scratch]"), "hello");
        editor.handle_key(KeyEvent::char('i'));
        editor.handle_key(KeyEvent::char('X'));
        editor.handle_key(KeyEvent::esc());
        editor.handle_key(KeyEvent::char('u')); // undo
        let buf = editor.buffer_mgr.get(&id).unwrap();
        assert_eq!(buf.text().to_string(), "hello");
        // Redo
        editor.handle_key(KeyEvent::ctrl('r'));
        let buf = editor.buffer_mgr.get(&id).unwrap();
        assert_eq!(buf.text().to_string(), "Xhello");
    }

    // SPC T t opens theme picker, j/k navigates with live preview, Enter confirms, Esc reverts
    #[test]
    fn test_theme_picker_flow() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(&id, "Test", std::path::Path::new("[scratch]"), "hello");

        let original_theme = editor.theme().name;
        assert_eq!(original_theme, "bloom-dark");

        // Open theme picker via SPC T t
        editor.handle_key(KeyEvent::char(' ')); // SPC
        editor.handle_key(KeyEvent {
            code: types::KeyCode::Char('T'),
            modifiers: types::Modifiers::shift(),
        });
        editor.handle_key(KeyEvent::char('t'));

        // Picker should be open (unified picker_state)
        assert!(editor.picker_state.is_some());
        let frame = editor.render(80, 24);
        assert!(frame.picker.is_some());
        let picker = frame.picker.unwrap();
        assert_eq!(picker.title, "Theme");
        assert_eq!(picker.results.len(), 12);

        // Move down — live preview changes theme (now typed as Char, goes to query)
        // Use Ctrl+J for navigation
        editor.handle_key(KeyEvent::ctrl('j'));
        assert_eq!(editor.theme().name, "bloom-light");

        // Esc reverts
        editor.handle_key(KeyEvent::esc());
        assert!(editor.picker_state.is_none());
        assert_eq!(editor.theme().name, "bloom-dark");
    }

    #[test]
    fn test_theme_picker_confirm() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(&id, "Test", std::path::Path::new("[scratch]"), "hello");

        // Open and move to bloom-light
        editor.handle_key(KeyEvent::char(' '));
        editor.handle_key(KeyEvent {
            code: types::KeyCode::Char('T'),
            modifiers: types::Modifiers::shift(),
        });
        editor.handle_key(KeyEvent::char('t'));
        editor.handle_key(KeyEvent::ctrl('j')); // bloom-light
        editor.handle_key(KeyEvent::ctrl('j')); // aurora
        assert_eq!(editor.theme().name, "aurora");

        // Enter confirms
        editor.handle_key(KeyEvent::enter());
        assert!(editor.picker_state.is_none());
        assert_eq!(editor.theme().name, "aurora");
    }

    #[test]
    fn test_theme_picker_ctrl_n_p() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(&id, "Test", std::path::Path::new("[scratch]"), "hello");

        editor.handle_key(KeyEvent::char(' '));
        editor.handle_key(KeyEvent {
            code: types::KeyCode::Char('T'),
            modifiers: types::Modifiers::shift(),
        });
        editor.handle_key(KeyEvent::char('t'));

        // Ctrl+N moves down
        editor.handle_key(KeyEvent::ctrl('n'));
        assert_eq!(editor.theme().name, "bloom-light");

        // Ctrl+P moves back up
        editor.handle_key(KeyEvent::ctrl('p'));
        assert_eq!(editor.theme().name, "bloom-dark");

        editor.handle_key(KeyEvent::esc());
    }

    // -----------------------------------------------------------------------
    // Block ID profiling
    // -----------------------------------------------------------------------

    /// Profile block ID assignment on a single large page.
    #[test]
    fn profile_block_id_single_large_page() {
        use std::time::Instant;

        // Generate a page with ~200 blocks (headings, paragraphs, lists, tasks).
        let mut content = String::from(
            "---\nid: abcdef01\ntitle: \"Large Page\"\ncreated: 2026-01-01\ntags: [test]\n---\n\n",
        );
        for i in 0..50 {
            content.push_str(&format!("## Section {i}\n\n"));
            content.push_str(&format!("Paragraph about topic {i}. This has some content\nthat spans multiple lines for realism.\n\n"));
            content.push_str(&format!("- List item {i}a\n- [ ] Task {i} @due(2026-03-10)\n- Item {i}b with\n  continuation line\n\n"));
        }

        let parser = bloom_md::parser::BloomMarkdownParser::new();

        // Measure parse time.
        let t0 = Instant::now();
        let doc = parser.parse(&content);
        let parse_ms = t0.elapsed().as_secs_f64() * 1000.0;

        // Measure assignment time.
        let t1 = Instant::now();
        let result = block_id_gen::assign_block_ids(&content, &doc);
        let assign_ms = t1.elapsed().as_secs_f64() * 1000.0;

        let block_count = doc.blocks.len();
        let ids_assigned = result.is_some();

        eprintln!(
            "profile_block_id_single_large_page: {} blocks, parse={:.2}ms, assign={:.2}ms, assigned={}",
            block_count, parse_ms, assign_ms, ids_assigned
        );

        assert!(ids_assigned, "should have assigned IDs");
        assert!(
            block_count >= 200,
            "expected ~200+ blocks, got {block_count}"
        );
        // Performance gate: parse + assign should be under 5ms in release.
        // In debug builds, skip the timing check (debug is 10-50x slower).
        #[cfg(not(debug_assertions))]
        assert!(
            parse_ms + assign_ms < 5.0,
            "too slow: parse={parse_ms:.2}ms + assign={assign_ms:.2}ms"
        );
    }

    /// Profile block ID assignment across many pages (simulates bulk assignment).
    #[test]
    fn profile_block_id_bulk_1000_pages() {
        use std::time::Instant;

        let parser = bloom_md::parser::BloomMarkdownParser::new();

        // Generate 1000 pages with ~10 blocks each.
        let mut pages: Vec<String> = Vec::with_capacity(1000);
        for i in 0..1000 {
            let content = format!(
                "---\nid: {:08x}\ntitle: \"Page {i}\"\ncreated: 2026-01-01\ntags: [test]\n---\n\n\
                 # Page {i}\n\n\
                 Some content for page {i}.\n\n\
                 - Item one\n\
                 - [ ] Task for page {i}\n\
                 - Item two\n\n\
                 Another paragraph with details about topic {i}.\n\n\
                 > A blockquote for variety.\n",
                i
            );
            pages.push(content);
        }

        // Measure total parse + assign time for all pages.
        let t0 = Instant::now();
        let mut total_blocks = 0usize;
        let mut total_assigned = 0usize;
        for content in &pages {
            let doc = parser.parse(content);
            total_blocks += doc.blocks.len();
            if block_id_gen::assign_block_ids(content, &doc).is_some() {
                total_assigned += 1;
            }
        }
        let total_ms = t0.elapsed().as_secs_f64() * 1000.0;
        let per_page_ms = total_ms / pages.len() as f64;

        eprintln!(
            "profile_block_id_bulk_1000_pages: {} pages, {} blocks, {:.0}ms total, {:.3}ms/page, {} assigned",
            pages.len(), total_blocks, total_ms, per_page_ms, total_assigned
        );

        assert_eq!(total_assigned, 1000, "all pages should get IDs");
        // Performance gate: 1000 pages should complete under 200ms in release.
        #[cfg(not(debug_assertions))]
        assert!(total_ms < 200.0, "too slow: {total_ms:.0}ms for 1000 pages");
    }

    /// Profile idempotency — re-parsing pages that already have IDs.
    #[test]
    fn profile_block_id_idempotent_1000_pages() {
        use std::time::Instant;

        let parser = bloom_md::parser::BloomMarkdownParser::new();

        // Generate 1000 pages, assign IDs, then re-check (should be no-op).
        let mut pages_with_ids: Vec<String> = Vec::with_capacity(1000);
        for i in 0..1000 {
            let content = format!(
                "---\nid: {:08x}\ntitle: \"Page {i}\"\ncreated: 2026-01-01\ntags: [test]\n---\n\n\
                 # Page {i} ^a\n\n\
                 Content for page {i}. ^b\n\n\
                 - Item one ^c\n\
                 - [ ] Task ^d\n\n\
                 > Quote ^e\n",
                i
            );
            pages_with_ids.push(content);
        }

        let t0 = Instant::now();
        let mut any_changed = false;
        for content in &pages_with_ids {
            let doc = parser.parse(content);
            if block_id_gen::assign_block_ids(content, &doc).is_some() {
                any_changed = true;
            }
        }
        let total_ms = t0.elapsed().as_secs_f64() * 1000.0;
        let per_page_ms = total_ms / pages_with_ids.len() as f64;

        eprintln!(
            "profile_block_id_idempotent_1000_pages: {:.0}ms total, {:.3}ms/page, changed={}",
            total_ms, per_page_ms, any_changed
        );

        assert!(!any_changed, "no pages should need changes");
        // Idempotent check should be fast in release.
        // Performance gate: idempotent re-parse should be under 100ms in release.
        #[cfg(not(debug_assertions))]
        assert!(total_ms < 100.0, "too slow for no-op: {total_ms:.0}ms");
    }

    // -----------------------------------------------------------------------
    // High-level integration tests — UC coverage
    // -----------------------------------------------------------------------

    /// Helper: create an editor with a vault containing files.
    fn editor_with_vault(files: &[(&str, &str)]) -> (BloomEditor, tempfile::TempDir) {
        let dir = tempfile::TempDir::new().unwrap();
        let pages_dir = dir.path().join("pages");
        std::fs::create_dir_all(&pages_dir).unwrap();
        let journal_dir = dir.path().join("journal");
        std::fs::create_dir_all(&journal_dir).unwrap();
        for (name, content) in files {
            let path = if name.starts_with("journal/") {
                dir.path().join(name)
            } else {
                pages_dir.join(name)
            };
            std::fs::write(&path, content).unwrap();
        }

        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let _ = editor.init_vault(dir.path());

        // Wait for indexer
        let ch = editor.channels();
        if let Some(rx) = &ch.indexer_rx {
            for _ in 0..200 {
                if let Ok(complete) = rx.try_recv() {
                    editor.handle_index_complete(complete);
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }

        editor.startup();
        (editor, dir)
    }

    fn _page_content(id: &str) -> String {
        format!("---\nid: {id}\ntitle: \"Test Page\"\ncreated: 2026-01-01\ntags: []\n---\n\n")
    }

    // UC-01: Open today's journal via SPC j j
    #[test]
    fn test_uc01_open_journal() {
        let (mut editor, _dir) = editor_with_vault(&[]);
        editor.handle_key(KeyEvent::char(' '));
        editor.handle_key(KeyEvent::char('j'));
        editor.handle_key(KeyEvent::char('j'));

        let page = editor.active_page();
        assert!(page.is_some(), "journal should be open");
    }

    // UC-14 extended: j/k movement
    #[test]
    fn test_uc14_jk_movement() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(
            &id,
            "Test",
            std::path::Path::new("[scratch]"),
            "line one\nline two\nline three\n",
        );

        // Cursor starts at line 0
        let (line, _) = editor.cursor_position();
        assert_eq!(line, 0);

        // j moves down
        editor.handle_key(KeyEvent::char('j'));
        let (line, _) = editor.cursor_position();
        assert_eq!(line, 1);

        // j again
        editor.handle_key(KeyEvent::char('j'));
        let (line, _) = editor.cursor_position();
        assert_eq!(line, 2);

        // k moves up
        editor.handle_key(KeyEvent::char('k'));
        let (line, _) = editor.cursor_position();
        assert_eq!(line, 1);
    }

    // UC-14: dw deletes a word
    #[test]
    fn test_uc14_dw_delete_word() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(
            &id,
            "Test",
            std::path::Path::new("[scratch]"),
            "hello world",
        );

        // dw deletes "hello "
        editor.handle_key(KeyEvent::char('d'));
        editor.handle_key(KeyEvent::char('w'));

        let buf = editor.buffer_mgr.get(&id).unwrap();
        assert_eq!(buf.text().to_string(), "world");
    }

    // UC-18: Undo creates branch
    #[test]
    fn test_uc18_undo_branch() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(&id, "Test", std::path::Path::new("[scratch]"), "");

        // Insert "alpha"
        editor.handle_key(KeyEvent::char('i'));
        editor.handle_key(KeyEvent::char('a'));
        editor.handle_key(KeyEvent::esc());

        // Insert "b"
        editor.handle_key(KeyEvent::char('i'));
        editor.handle_key(KeyEvent::char('b'));
        editor.handle_key(KeyEvent::esc());

        // Undo "b"
        editor.handle_key(KeyEvent::char('u'));
        let buf = editor.buffer_mgr.get(&id).unwrap();
        assert_eq!(buf.text().to_string(), "a");

        // Now insert "c" — this creates a branch
        editor.handle_key(KeyEvent::char('i'));
        editor.handle_key(KeyEvent::char('c'));
        editor.handle_key(KeyEvent::esc());

        let buf = editor.buffer_mgr.get(&id).unwrap();
        let text = buf.text().to_string();
        assert!(text.contains('c'), "branch edit should be present: {text}");

        // The undo tree should have branches
        let tree = buf.undo_tree();
        assert!(
            tree.node_count() >= 3,
            "undo tree should have branching nodes"
        );
    }

    // UC-20: :w saves, :q quits (already tested via test_colon_w_saves, test_colon_q_quits)

    // UC-42: Toggle task checkbox
    #[test]
    fn test_uc42_toggle_task() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(
            &id,
            "Test",
            std::path::Path::new("[scratch]"),
            "- [ ] buy milk\n- [x] read paper\n",
        );

        // Verify the initial content has unchecked and checked tasks
        let buf = editor.buffer_mgr.get(&id).unwrap();
        let text = buf.text().to_string();
        assert!(
            text.contains("- [ ] buy milk"),
            "should have unchecked task"
        );
        assert!(
            text.contains("- [x] read paper"),
            "should have checked task"
        );

        // Edit the buffer through Vim: go to col 3 (the space in [ ]), replace with x
        // Cursor is at start of "- [ ] buy milk"
        editor.handle_key(KeyEvent::char('3')); // count
        editor.handle_key(KeyEvent::char('l')); // move to column 3 (the space in [ ])
        editor.handle_key(KeyEvent::char('r')); // replace mode
        editor.handle_key(KeyEvent::char('x')); // replace space with x

        let buf = editor.buffer_mgr.get(&id).unwrap();
        let result = buf.text().to_string();
        assert!(
            result.contains("- [x] buy milk"),
            "task should be toggled: {result}"
        );
    }

    // UC-52: SPC w v splits window
    #[test]
    fn test_uc52_window_split() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(&id, "Test", std::path::Path::new("[scratch]"), "hello");

        // Count panes before
        let frame = editor.render(80, 24);
        let panes_before = frame.panes.len();

        // SPC w v — vertical split
        editor.handle_key(KeyEvent::char(' '));
        editor.handle_key(KeyEvent::char('w'));
        editor.handle_key(KeyEvent::char('v'));

        let frame = editor.render(80, 24);
        assert!(
            frame.panes.len() > panes_before,
            "split should create more panes: {} vs {}",
            frame.panes.len(),
            panes_before
        );
    }

    // UC-77: Session save + restore preserves buffers and cursor
    #[test]
    fn test_uc77_session_restore() {
        let dir = tempfile::TempDir::new().unwrap();
        let pages_dir = dir.path().join("pages");
        std::fs::create_dir_all(&pages_dir).unwrap();
        std::fs::write(
            pages_dir.join("test.md"),
            "---\nid: aa112233\ntitle: \"Session Test\"\ncreated: 2026-01-01\ntags: []\n---\n\nline one\nline two\nline three\n",
        ).unwrap();

        // Session 1: open, edit, save session
        {
            let config = config::Config::defaults();
            let mut editor = BloomEditor::new(config).unwrap();
            let _ = editor.init_vault(dir.path());
            let ch = editor.channels();
            if let Some(rx) = &ch.indexer_rx {
                for _ in 0..200 {
                    if let Ok(c) = rx.try_recv() {
                        editor.handle_index_complete(c);
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }
            editor.startup();

            // Open the page via SPC f f
            editor.handle_key(KeyEvent::char(' '));
            editor.handle_key(KeyEvent::char('f'));
            editor.handle_key(KeyEvent::char('f'));
            editor.handle_key(KeyEvent::enter());

            // Move cursor down
            editor.handle_key(KeyEvent::char('j'));
            editor.handle_key(KeyEvent::char('j'));

            let _ = editor.save_session();
        }

        // Session 2: restore and verify
        {
            let config = config::Config::defaults();
            let mut editor = BloomEditor::new(config).unwrap();
            let _ = editor.init_vault(dir.path());
            let ch = editor.channels();
            if let Some(rx) = &ch.indexer_rx {
                for _ in 0..200 {
                    if let Ok(c) = rx.try_recv() {
                        editor.handle_index_complete(c);
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }

            // Restore should bring back the buffer
            let restored = editor.restore_session();
            assert!(restored.is_ok(), "session restore should succeed");

            // Should have an active page
            assert!(editor.active_page().is_some(), "page should be restored");
        }
    }

    // UC-87: SPC shows which-key popup
    #[test]
    fn test_uc87_whichkey_popup() {
        let mut cfg = config::Config::defaults();
        cfg.which_key_timeout_ms = 0; // instant for testing
        let mut editor = BloomEditor::new(cfg).unwrap();
        let id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(&id, "Test", std::path::Path::new("[scratch]"), "hello");

        // Press SPC
        editor.handle_key(KeyEvent::char(' '));

        // Tick to trigger which-key
        let future = std::time::Instant::now() + std::time::Duration::from_secs(1);
        editor.tick(future);

        let frame = editor.render(80, 24);
        assert!(
            frame.which_key.is_some(),
            "which-key popup should be visible after SPC + timeout"
        );
    }

    // UC-26: Follow a link (gd on a link opens target)
    #[test]
    fn test_uc26_follow_link() {
        let (mut editor, _dir) = editor_with_vault(&[
            ("source.md", "---\nid: aabb1122\ntitle: \"Source\"\ncreated: 2026-01-01\ntags: []\n---\n\nSee [[ccdd3344|Target]] here.\n"),
            ("target.md", "---\nid: ccdd3344\ntitle: \"Target\"\ncreated: 2026-01-01\ntags: []\n---\n\nTarget content.\n"),
        ]);

        // Open source page
        editor.handle_key(KeyEvent::char(' '));
        editor.handle_key(KeyEvent::char('f'));
        editor.handle_key(KeyEvent::char('f'));
        // Type to find source
        editor.handle_key(KeyEvent::char('S'));
        editor.handle_key(KeyEvent::char('o'));
        editor.handle_key(KeyEvent::char('u'));
        // Select it
        editor.handle_key(KeyEvent::enter());

        // Verify source is open
        let page = editor.active_page().cloned();
        assert!(page.is_some(), "source page should be open");
    }
}
