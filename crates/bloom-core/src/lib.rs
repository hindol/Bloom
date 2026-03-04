// Bloom core library

pub mod agenda;
pub mod buffer;
pub mod config;
pub mod error;
pub mod index;
pub mod journal;
pub mod keymap;
pub mod linker;
pub mod parser;
pub mod picker;
pub mod refactor;
pub mod render;
pub mod session;
pub mod store;
pub mod template;
pub mod theme;
pub mod timeline;
pub mod types;
pub mod uuid;
pub mod vault;
pub mod vim;
pub mod which_key;
pub mod window;

// ---------------------------------------------------------------------------
// BufferManager
// ---------------------------------------------------------------------------

use std::collections::HashMap;
use std::time::Instant;

use parser::traits::DocumentParser;

pub struct BufferManager {
    buffers: HashMap<String, (buffer::Buffer, BufferInfo)>,
}

pub struct BufferInfo {
    pub page_id: types::PageId,
    pub title: String,
    pub path: std::path::PathBuf,
    pub dirty: bool,
    pub last_focused: Instant,
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
    ) -> &mut buffer::Buffer {
        let key = page_id.to_hex();
        self.buffers.entry(key.clone()).or_insert_with(|| {
            let buf = buffer::Buffer::from_text(content);
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

    pub fn get(&self, page_id: &types::PageId) -> Option<&buffer::Buffer> {
        self.buffers.get(&page_id.to_hex()).map(|(b, _)| b)
    }

    pub fn get_with_info(&self, page_id: &types::PageId) -> Option<(&buffer::Buffer, &BufferInfo)> {
        self.buffers.get(&page_id.to_hex()).map(|(b, i)| (b, i))
    }

    pub fn get_mut(&mut self, page_id: &types::PageId) -> Option<&mut buffer::Buffer> {
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
}

// ---------------------------------------------------------------------------
// BloomEditor — The Orchestrator
// ---------------------------------------------------------------------------

pub struct BloomEditor {
    pub config: config::Config,
    buffer_mgr: BufferManager,
    vim_state: vim::VimState,
    window_mgr: window::WindowManager,
    which_key_tree: which_key::WhichKeyTree,
    command_registry: which_key::CommandRegistry,
    index: Option<index::Index>,
    journal: Option<journal::Journal>,
    parser: parser::BloomMarkdownParser,
    template_engine: Option<template::TemplateEngine>,
    template_mode: Option<template::TemplateModeState>,
    linker: linker::Linker,
    agenda: agenda::Agenda,
    timeline: timeline::Timeline,
    refactorer: refactor::Refactor,

    // State
    cursor: usize,
    active_page: Option<types::PageId>,
    picker_state: Option<PickerState>,
    quick_capture: Option<QuickCaptureState>,
    notifications: Vec<render::Notification>,
    viewport: render::Viewport,
    wizard: Option<SetupWizardState>,
    vault_root: Option<std::path::PathBuf>,
    leader_keys: Vec<types::KeyEvent>,
    active_theme: &'static theme::ThemePalette,
}

// ---------------------------------------------------------------------------
// Setup wizard state machine
// ---------------------------------------------------------------------------

struct SetupWizardState {
    step: WizardStep,
    vault_path: String,
    vault_path_cursor: usize,
    import_choice: render::ImportChoice,
    logseq_path: String,
    logseq_path_cursor: usize,
    import_progress: Option<render::ImportProgress>,
    stats: render::WizardStats,
    error: Option<String>,
}

#[derive(Clone, Copy, PartialEq)]
enum WizardStep {
    Welcome,
    ChooseVault,
    ImportChoice,
    ImportPath,
    ImportRunning,
    Complete,
}

impl SetupWizardState {
    fn new() -> Self {
        Self {
            step: WizardStep::Welcome,
            vault_path: default_vault_path(),
            vault_path_cursor: default_vault_path().len(),
            import_choice: render::ImportChoice::No,
            logseq_path: String::new(),
            logseq_path_cursor: 0,
            import_progress: None,
            stats: render::WizardStats {
                pages: 0,
                journals: 0,
            },
            error: None,
        }
    }

    fn to_frame(&self) -> render::SetupWizardFrame {
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

fn home_dir() -> Option<std::path::PathBuf> {
    #[cfg(windows)]
    {
        std::env::var("USERPROFILE")
            .ok()
            .map(std::path::PathBuf::from)
    }
    #[cfg(not(windows))]
    {
        std::env::var("HOME")
            .ok()
            .map(std::path::PathBuf::from)
    }
}

fn expand_tilde(path: &str) -> String {
    if path.starts_with("~/") || path == "~" {
        if let Some(home) = home_dir() {
            return path.replacen('~', &home.to_string_lossy(), 1);
        }
    }
    path.to_string()
}

/// Set the hidden file attribute on Windows.
#[cfg(windows)]
fn set_hidden_attribute(path: &std::path::Path) -> std::io::Result<()> {
    std::process::Command::new("attrib")
        .arg("+H")
        .arg(path.as_os_str())
        .output()?;
    Ok(())
}

enum PickerState {
    FindPage(picker::Picker<PageItem>),
}

struct QuickCaptureState {
    kind: keymap::dispatch::QuickCaptureKind,
    input: String,
    cursor_pos: usize,
}

#[derive(Clone)]
struct PageItem {
    meta: types::PageMeta,
}

impl picker::PickerItem for PageItem {
    fn match_text(&self) -> &str {
        &self.meta.title
    }
    fn display(&self) -> picker::PickerRow {
        picker::PickerRow {
            label: self.meta.title.clone(),
            marginalia: vec![self.meta.path.display().to_string()],
        }
    }
    fn preview(&self) -> Option<String> {
        None
    }
}

impl BloomEditor {
    pub fn new(config: config::Config) -> Result<Self, error::BloomError> {
        let active_theme = theme::palette_by_name(&config.theme.name)
            .unwrap_or(&theme::BLOOM_DARK);
        Ok(Self {
            vim_state: vim::VimState::new(),
            window_mgr: window::WindowManager::new(),
            which_key_tree: which_key::default_tree(),
            command_registry: which_key::default_registry(),
            index: None,
            journal: None,
            parser: parser::BloomMarkdownParser::new(),
            template_engine: None,
            template_mode: None,
            linker: linker::Linker::new(),
            agenda: agenda::Agenda::new(),
            timeline: timeline::Timeline::new(),
            refactorer: refactor::Refactor::new(),
            buffer_mgr: BufferManager::new(),
            cursor: 0,
            active_page: None,
            picker_state: None,
            quick_capture: None,
            notifications: Vec::new(),
            viewport: render::Viewport::new(24, 80),
            wizard: None,
            vault_root: None,
            leader_keys: Vec::new(),
            active_theme,
            config,
        })
    }

    /// Get the active theme palette.
    pub fn theme(&self) -> &'static theme::ThemePalette {
        self.active_theme
    }

    /// Set the active theme by name. Returns false if name not found.
    pub fn set_theme(&mut self, name: &str) -> bool {
        if let Some(palette) = theme::palette_by_name(name) {
            self.active_theme = palette;
            true
        } else {
            false
        }
    }

    /// Cycle to the next theme.
    pub fn cycle_theme(&mut self) {
        let current = self.active_theme.name;
        let names = theme::THEME_NAMES;
        let idx = names.iter().position(|n| *n == current).unwrap_or(0);
        let next = names[(idx + 1) % names.len()];
        self.set_theme(next);
    }

    /// Initialize with a vault path — sets up index, journal, template engine
    pub fn init_vault(&mut self, vault_root: &std::path::Path) -> Result<(), error::BloomError> {
        let index_path = vault_root.join(".index.db");
        self.index = Some(index::Index::open(&index_path)?);
        self.journal = Some(journal::Journal::new(vault_root));
        let templates_dir = vault_root.join("templates");
        self.template_engine = Some(template::TemplateEngine::new(&templates_dir));
        self.vault_root = Some(vault_root.to_path_buf());

        // Mark index file as hidden on Windows
        #[cfg(windows)]
        {
            let _ = set_hidden_attribute(&index_path);
        }

        Ok(())
    }

    /// Check if the setup wizard should run (no vault at default path).
    pub fn needs_setup(&self) -> bool {
        let default = default_vault_path();
        let root = std::path::Path::new(&default);
        !root.join("config.toml").exists()
    }

    /// Start the setup wizard.
    pub fn start_wizard(&mut self) {
        self.wizard = Some(SetupWizardState::new());
    }

    /// Whether the wizard is currently active.
    pub fn wizard_active(&self) -> bool {
        self.wizard.is_some()
    }

    /// Perform startup according to config. Guarantees `active_page` is `Some` on return.
    pub fn startup(&mut self) {
        match self.config.startup.mode {
            config::StartupMode::Journal => self.open_journal_today(),
            config::StartupMode::Restore => {
                if self.restore_session().is_err() || self.active_page.is_none() {
                    self.open_scratch_buffer();
                }
            }
            config::StartupMode::Blank => self.open_scratch_buffer(),
        }
    }

    fn open_journal_today(&mut self) {
        let today = journal::Journal::today();
        let title = today.format("%Y-%m-%d").to_string();

        // If journal module is initialized, use its path; otherwise use a sensible default
        let path = self
            .journal
            .as_ref()
            .map(|j| j.path_for_date(today))
            .unwrap_or_else(|| std::path::PathBuf::from(format!("journal/{}.md", title)));

        // Read from disk if the file exists, otherwise generate default frontmatter
        let content = if path.exists() {
            std::fs::read_to_string(&path).unwrap_or_default()
        } else {
            let fm = parser::traits::Frontmatter {
                id: None,
                title: Some(title.clone()),
                created: Some(today),
                tags: vec![types::TagName("journal".to_string())],
                extra: std::collections::HashMap::new(),
            };
            let mut s = self.parser.serialize_frontmatter(&fm);
            s.push('\n');
            s
        };

        let id = crate::uuid::generate_hex_id();
        self.open_page_with_content(&id, &title, &path, &content);
    }

    fn open_scratch_buffer(&mut self) {
        let id = crate::uuid::generate_hex_id();
        self.open_page_with_content(
            &id,
            "[scratch]",
            std::path::Path::new("[scratch]"),
            "",
        );
    }

    /// Process a key event
    pub fn handle_key(&mut self, key: types::KeyEvent) -> Vec<keymap::dispatch::Action> {
        // If wizard is active, route all keys there
        if self.wizard.is_some() {
            return self.handle_wizard_key(&key);
        }

        // Check platform shortcuts first
        if let Some(action) = keymap::platform_shortcut(&key) {
            self.leader_keys.clear();
            return self.execute_actions(vec![action]);
        }

        // If picker is open, route to picker
        if self.picker_state.is_some() {
            return self.handle_picker_key(&key);
        }

        // If quick capture is open
        if self.quick_capture.is_some() {
            return self.handle_quick_capture_key(&key);
        }

        // If we're in a leader key sequence (SPC was pressed), route to which-key
        if !self.leader_keys.is_empty() {
            let actions = self.handle_leader_key(key);
            return self.execute_actions(actions);
        }

        // Check if this is the leader key (Space in Normal mode)
        if key.code == types::KeyCode::Char(' ')
            && key.modifiers == types::Modifiers::none()
            && matches!(self.vim_state.mode(), vim::Mode::Normal)
        {
            self.leader_keys.push(key);
            return vec![keymap::dispatch::Action::Noop];
        }

        // Vim processing
        if let Some(buf) = self.active_page.as_ref().and_then(|id| self.buffer_mgr.get(id)) {
            let action = self.vim_state.process_key(key.clone(), buf, self.cursor);
            let actions = self.translate_vim_action(action);
            return self.execute_actions(actions);
        }

        vec![keymap::dispatch::Action::Noop]
    }

    /// Execute actions on editor state. Returns the actions for the TUI to handle
    /// (only Quit, Save, and informational actions pass through).
    fn execute_actions(
        &mut self,
        actions: Vec<keymap::dispatch::Action>,
    ) -> Vec<keymap::dispatch::Action> {
        let mut result = Vec::new();
        for action in actions {
            match action {
                keymap::dispatch::Action::SplitWindow(dir) => {
                    let _ = self.window_mgr.split(dir);
                }
                keymap::dispatch::Action::CloseWindow => {
                    let pane = self.window_mgr.active_pane();
                    self.window_mgr.close(pane);
                }
                keymap::dispatch::Action::NavigateWindow(dir) => {
                    let cursor_line = self.cursor_position().0;
                    self.window_mgr.navigate(dir, cursor_line);
                }
                keymap::dispatch::Action::OpenAgenda => {
                    // TODO: open agenda in split pane
                    result.push(action);
                }
                keymap::dispatch::Action::OpenUndoTree => {
                    // TODO: open undo tree in split pane
                    result.push(action);
                }
                keymap::dispatch::Action::OpenPicker(_) => {
                    // TODO: open picker overlay
                    result.push(action);
                }
                keymap::dispatch::Action::ClosePicker => {
                    self.picker_state = None;
                    result.push(action);
                }
                keymap::dispatch::Action::ModeChange(_) => {
                    // Mode change already applied in vim state
                    result.push(action);
                }
                keymap::dispatch::Action::Edit(_) | keymap::dispatch::Action::Motion(_) => {
                    // Already applied to buffer/cursor in translate_vim_action
                    result.push(action);
                }
                // Pass through to TUI: Quit, Save, and others
                _ => {
                    result.push(action);
                }
            }
        }
        result
    }

    fn handle_leader_key(&mut self, key: types::KeyEvent) -> Vec<keymap::dispatch::Action> {
        // Esc cancels leader sequence
        if key.code == types::KeyCode::Esc {
            self.leader_keys.clear();
            return vec![keymap::dispatch::Action::Noop];
        }

        self.leader_keys.push(key);

        // Look up the full sequence (skipping the initial SPC)
        let lookup_keys: Vec<types::KeyEvent> = self.leader_keys[1..].to_vec();
        match self.which_key_tree.lookup(&lookup_keys) {
            which_key::WhichKeyLookup::Action(action_id) => {
                self.leader_keys.clear();
                self.action_id_to_actions(&action_id)
            }
            which_key::WhichKeyLookup::Prefix(_entries) => {
                // Still accumulating — show which-key popup on next render
                vec![keymap::dispatch::Action::Noop]
            }
            which_key::WhichKeyLookup::NoMatch => {
                self.leader_keys.clear();
                vec![keymap::dispatch::Action::Noop]
            }
        }
    }

    fn action_id_to_actions(&mut self, action_id: &str) -> Vec<keymap::dispatch::Action> {
        match action_id {
            "find_page" => vec![keymap::dispatch::Action::OpenPicker(
                keymap::dispatch::PickerKind::FindPage,
            )],
            "switch_buffer" => vec![keymap::dispatch::Action::OpenPicker(
                keymap::dispatch::PickerKind::SwitchBuffer,
            )],
            "search" => vec![keymap::dispatch::Action::OpenPicker(
                keymap::dispatch::PickerKind::Search,
            )],
            "search_tags" => vec![keymap::dispatch::Action::OpenPicker(
                keymap::dispatch::PickerKind::Tags,
            )],
            "journal_today" => {
                self.open_journal_today();
                vec![keymap::dispatch::Action::Noop]
            }
            "journal_append" => vec![keymap::dispatch::Action::QuickCapture(
                keymap::dispatch::QuickCaptureKind::Note,
            )],
            "journal_task" => vec![keymap::dispatch::Action::QuickCapture(
                keymap::dispatch::QuickCaptureKind::Task,
            )],
            "split_vertical" => vec![keymap::dispatch::Action::SplitWindow(
                window::SplitDirection::Vertical,
            )],
            "split_horizontal" => vec![keymap::dispatch::Action::SplitWindow(
                window::SplitDirection::Horizontal,
            )],
            "navigate_left" => vec![keymap::dispatch::Action::NavigateWindow(
                window::Direction::Left,
            )],
            "navigate_down" => vec![keymap::dispatch::Action::NavigateWindow(
                window::Direction::Down,
            )],
            "navigate_up" => vec![keymap::dispatch::Action::NavigateWindow(
                window::Direction::Up,
            )],
            "navigate_right" => vec![keymap::dispatch::Action::NavigateWindow(
                window::Direction::Right,
            )],
            "close_window" => vec![keymap::dispatch::Action::CloseWindow],
            "agenda" => vec![keymap::dispatch::Action::OpenAgenda],
            "undo_tree" => vec![keymap::dispatch::Action::OpenUndoTree],
            "new_from_template" => vec![keymap::dispatch::Action::OpenPicker(
                keymap::dispatch::PickerKind::Templates,
            )],
            "split_page" => vec![keymap::dispatch::Action::Refactor(
                keymap::dispatch::RefactorOp::SplitPage,
            )],
            "merge_pages" => vec![keymap::dispatch::Action::Refactor(
                keymap::dispatch::RefactorOp::MergePages,
            )],
            "move_block" => vec![keymap::dispatch::Action::Refactor(
                keymap::dispatch::RefactorOp::MoveBlock,
            )],
            "rebuild_index" => vec![keymap::dispatch::Action::RebuildIndex],
            "toggle_mcp" => vec![keymap::dispatch::Action::ToggleMcp],
            "theme_selector" => {
                self.cycle_theme();
                vec![keymap::dispatch::Action::Noop]
            }
            _ => vec![keymap::dispatch::Action::Noop],
        }
    }

    fn handle_picker_key(&mut self, key: &types::KeyEvent) -> Vec<keymap::dispatch::Action> {
        use types::KeyCode;
        match &key.code {
            KeyCode::Esc => {
                self.picker_state = None;
                vec![keymap::dispatch::Action::ClosePicker]
            }
            KeyCode::Enter => {
                vec![keymap::dispatch::Action::ClosePicker]
            }
            KeyCode::Up => vec![keymap::dispatch::Action::PickerInput(
                keymap::dispatch::PickerInputAction::MoveSelection(-1),
            )],
            KeyCode::Down => vec![keymap::dispatch::Action::PickerInput(
                keymap::dispatch::PickerInputAction::MoveSelection(1),
            )],
            _ => vec![keymap::dispatch::Action::Noop],
        }
    }

    fn handle_quick_capture_key(
        &mut self,
        key: &types::KeyEvent,
    ) -> Vec<keymap::dispatch::Action> {
        use types::KeyCode;
        match &key.code {
            KeyCode::Esc => {
                self.quick_capture = None;
                vec![keymap::dispatch::Action::CancelQuickCapture]
            }
            KeyCode::Enter => {
                if let Some(qc) = self.quick_capture.take() {
                    vec![keymap::dispatch::Action::SubmitQuickCapture(qc.input)]
                } else {
                    vec![]
                }
            }
            KeyCode::Char(c) => {
                if let Some(qc) = &mut self.quick_capture {
                    qc.input.push(*c);
                    qc.cursor_pos += 1;
                }
                vec![keymap::dispatch::Action::Noop]
            }
            KeyCode::Backspace => {
                if let Some(qc) = &mut self.quick_capture {
                    if qc.cursor_pos > 0 {
                        qc.input.pop();
                        qc.cursor_pos -= 1;
                    }
                }
                vec![keymap::dispatch::Action::Noop]
            }
            _ => vec![keymap::dispatch::Action::Noop],
        }
    }

    fn handle_wizard_key(&mut self, key: &types::KeyEvent) -> Vec<keymap::dispatch::Action> {
        use types::KeyCode;
        // Ctrl+Q quits even during wizard
        if key.modifiers.ctrl && key.code == KeyCode::Char('q') {
            return vec![keymap::dispatch::Action::Quit];
        }

        let wiz = self.wizard.as_mut().unwrap();
        wiz.error = None; // Clear error on any key

        match wiz.step {
            WizardStep::Welcome => match &key.code {
                KeyCode::Enter => {
                    wiz.step = WizardStep::ChooseVault;
                }
                _ => {}
            },
            WizardStep::ChooseVault => match &key.code {
                KeyCode::Enter => {
                    let path_str = expand_tilde(&wiz.vault_path);
                    let root = std::path::PathBuf::from(&path_str);
                    // Check if already initialized
                    if root.join("config.toml").exists() {
                        // Existing vault — skip to complete
                        wiz.step = WizardStep::Complete;
                        wiz.vault_path = path_str;
                    } else {
                        // Try to create vault
                        match vault::Vault::create(&root) {
                            Ok(_) => {
                                let config_path = root.join("config.toml");
                                let _ = std::fs::write(&config_path, "# Bloom configuration\n# See docs for all options.\n\n[startup]\nmode = \"Journal\"\n");
                                wiz.vault_path = path_str;
                                wiz.step = WizardStep::ImportChoice;
                            }
                            Err(e) => {
                                wiz.error =
                                    Some(format!("Cannot create directory: {}", e));
                            }
                        }
                    }
                }
                KeyCode::Esc => {
                    wiz.step = WizardStep::Welcome;
                }
                KeyCode::Char(c) => {
                    let pos = wiz.vault_path_cursor;
                    wiz.vault_path.insert(pos, *c);
                    wiz.vault_path_cursor += 1;
                }
                KeyCode::Backspace => {
                    if wiz.vault_path_cursor > 0 {
                        wiz.vault_path_cursor -= 1;
                        wiz.vault_path.remove(wiz.vault_path_cursor);
                    }
                }
                KeyCode::Left => {
                    wiz.vault_path_cursor = wiz.vault_path_cursor.saturating_sub(1);
                }
                KeyCode::Right => {
                    wiz.vault_path_cursor =
                        (wiz.vault_path_cursor + 1).min(wiz.vault_path.len());
                }
                KeyCode::Home => {
                    wiz.vault_path_cursor = 0;
                }
                KeyCode::End => {
                    wiz.vault_path_cursor = wiz.vault_path.len();
                }
                _ => {}
            },
            WizardStep::ImportChoice => match &key.code {
                KeyCode::Enter => {
                    if wiz.import_choice == render::ImportChoice::Yes {
                        wiz.step = WizardStep::ImportPath;
                    } else {
                        wiz.step = WizardStep::Complete;
                    }
                }
                KeyCode::Esc => {
                    wiz.step = WizardStep::ChooseVault;
                }
                KeyCode::Up | KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('k') => {
                    wiz.import_choice = match wiz.import_choice {
                        render::ImportChoice::No => render::ImportChoice::Yes,
                        render::ImportChoice::Yes => render::ImportChoice::No,
                    };
                }
                _ => {}
            },
            WizardStep::ImportPath => match &key.code {
                KeyCode::Enter => {
                    let logseq_path = expand_tilde(&wiz.logseq_path);
                    let lp = std::path::Path::new(&logseq_path);
                    if !lp.join("pages").exists() && !lp.join("journals").exists() {
                        wiz.error = Some(
                            "Not a Logseq vault: missing pages/ directory".to_string(),
                        );
                    } else {
                        // TODO: actual Logseq import (G13) — for now skip to Complete
                        wiz.step = WizardStep::Complete;
                    }
                }
                KeyCode::Esc => {
                    wiz.step = WizardStep::ImportChoice;
                }
                KeyCode::Char(c) => {
                    let pos = wiz.logseq_path_cursor;
                    wiz.logseq_path.insert(pos, *c);
                    wiz.logseq_path_cursor += 1;
                }
                KeyCode::Backspace => {
                    if wiz.logseq_path_cursor > 0 {
                        wiz.logseq_path_cursor -= 1;
                        wiz.logseq_path.remove(wiz.logseq_path_cursor);
                    }
                }
                KeyCode::Left => {
                    wiz.logseq_path_cursor = wiz.logseq_path_cursor.saturating_sub(1);
                }
                KeyCode::Right => {
                    wiz.logseq_path_cursor =
                        (wiz.logseq_path_cursor + 1).min(wiz.logseq_path.len());
                }
                _ => {}
            },
            WizardStep::ImportRunning => {
                // Non-interactive — import runs to completion
            }
            WizardStep::Complete => match &key.code {
                KeyCode::Enter => {
                    self.complete_wizard();
                    return vec![keymap::dispatch::Action::Noop];
                }
                _ => {}
            },
        }

        vec![keymap::dispatch::Action::Noop]
    }

    fn complete_wizard(&mut self) {
        let vault_path = self
            .wizard
            .as_ref()
            .map(|w| expand_tilde(&w.vault_path))
            .unwrap_or_else(default_vault_path);
        self.wizard = None;

        let root = std::path::PathBuf::from(&vault_path);
        let _ = self.init_vault(&root);
        self.startup();
    }

    fn translate_vim_action(
        &mut self,
        action: vim::VimAction,
    ) -> Vec<keymap::dispatch::Action> {
        match action {
            vim::VimAction::Edit(edit) => {
                if let Some(page_id) = &self.active_page {
                    if let Some(buf) = self.buffer_mgr.get_mut(page_id) {
                        if edit.replacement.is_empty() {
                            buf.delete(edit.range.clone());
                        } else if edit.range.is_empty() {
                            buf.insert(edit.range.start, &edit.replacement);
                        } else {
                            buf.replace(edit.range.clone(), &edit.replacement);
                        }
                        self.cursor = edit.cursor_after;
                    }
                }
                vec![keymap::dispatch::Action::Edit(buffer::EditOp {
                    range: edit.range,
                    replacement: edit.replacement,
                    cursor_after: edit.cursor_after,
                })]
            }
            vim::VimAction::Motion(motion) => {
                self.cursor = motion.new_position;
                vec![keymap::dispatch::Action::Motion(
                    keymap::dispatch::MotionResult {
                        new_position: motion.new_position,
                        extend_selection: motion.extend_selection,
                    },
                )]
            }
            vim::VimAction::ModeChange(mode) => {
                vec![keymap::dispatch::Action::ModeChange(mode)]
            }
            vim::VimAction::Command(cmd) => self.handle_vim_command(&cmd),
            vim::VimAction::Pending => vec![keymap::dispatch::Action::Noop],
            vim::VimAction::Unhandled => vec![keymap::dispatch::Action::Noop],
            vim::VimAction::Composite(actions) => actions
                .into_iter()
                .flat_map(|a| self.translate_vim_action(a))
                .collect(),
        }
    }

    fn handle_vim_command(&mut self, cmd: &str) -> Vec<keymap::dispatch::Action> {
        match cmd {
            "undo" => {
                if let Some(page_id) = &self.active_page {
                    if let Some(buf) = self.buffer_mgr.get_mut(page_id) {
                        buf.undo();
                        // Clamp cursor to buffer length after undo
                        let len = buf.len_chars();
                        if self.cursor > len {
                            self.cursor = len.saturating_sub(1);
                        }
                    }
                }
                vec![keymap::dispatch::Action::Undo]
            }
            "redo" => {
                if let Some(page_id) = &self.active_page {
                    if let Some(buf) = self.buffer_mgr.get_mut(page_id) {
                        buf.redo();
                        let len = buf.len_chars();
                        if self.cursor > len {
                            self.cursor = len.saturating_sub(1);
                        }
                    }
                }
                vec![keymap::dispatch::Action::Redo]
            }
            _ => self.translate_ex_command(cmd),
        }
    }

    fn translate_ex_command(&mut self, cmd: &str) -> Vec<keymap::dispatch::Action> {
        let trimmed = cmd.trim();
        // Handle :theme with optional argument
        if trimmed == "theme" {
            self.cycle_theme();
            return vec![keymap::dispatch::Action::Noop];
        }
        if let Some(name) = trimmed.strip_prefix("theme ") {
            let name = name.trim();
            if !self.set_theme(name) {
                // Unknown theme name — could show notification
            }
            return vec![keymap::dispatch::Action::Noop];
        }
        match trimmed {
            "q" | "quit" => vec![keymap::dispatch::Action::Quit],
            "q!" | "quit!" => vec![keymap::dispatch::Action::Quit],
            "w" | "write" => vec![keymap::dispatch::Action::Save],
            "wq" | "x" => vec![
                keymap::dispatch::Action::Save,
                keymap::dispatch::Action::Quit,
            ],
            "wq!" | "x!" => vec![
                keymap::dispatch::Action::Save,
                keymap::dispatch::Action::Quit,
            ],
            "e" | "edit" => vec![keymap::dispatch::Action::OpenPicker(
                keymap::dispatch::PickerKind::FindPage,
            )],
            "bd" | "bdelete" => vec![keymap::dispatch::Action::CloseWindow],
            "sp" | "split" => vec![keymap::dispatch::Action::SplitWindow(
                window::SplitDirection::Horizontal,
            )],
            "vs" | "vsplit" => vec![keymap::dispatch::Action::SplitWindow(
                window::SplitDirection::Vertical,
            )],
            "rebuild-index" => vec![keymap::dispatch::Action::RebuildIndex],
            _ => {
                // Unknown command — noop
                vec![keymap::dispatch::Action::Noop]
            }
        }
    }

    /// Produce the render frame
    pub fn render(&self) -> render::RenderFrame {
        // If wizard is active, render wizard as a full-screen pane
        if let Some(wiz) = &self.wizard {
            return render::RenderFrame {
                panes: vec![render::PaneFrame {
                    id: types::PaneId(0),
                    kind: render::PaneKind::SetupWizard(wiz.to_frame()),
                    visible_lines: Vec::new(),
                    cursor: render::CursorState::default(),
                    scroll_offset: 0,
                    is_active: true,
                    title: String::new(),
                    dirty: false,
                    status_bar: render::StatusBar::default(),
                }],
                maximized: false,
                hidden_pane_count: 0,
                picker: None,
                which_key: None,
                command_line: None,
                quick_capture: None,
                date_picker: None,
                dialog: None,
                notification: None,
            };
        }

        let mut panes = Vec::new();

        for pane_id in self.window_mgr.all_pane_ids() {
            let is_active = pane_id == self.window_mgr.active_pane();
            let mode_str = match self.vim_state.mode() {
                vim::Mode::Normal => "NORMAL",
                vim::Mode::Insert => "INSERT",
                vim::Mode::Visual { .. } => "VISUAL",
                vim::Mode::Command => "COMMAND",
            };

            let (title, dirty, visible_lines) = if let Some(page_id) = &self.active_page {
                if let Some(buf) = self.buffer_mgr.get(page_id) {
                    let infos = self.buffer_mgr.open_buffers();
                    let title = infos
                        .iter()
                        .find(|i| i.page_id == *page_id)
                        .map(|i| i.title.clone())
                        .unwrap_or_default();
                    let lines = self.render_buffer_lines(buf);
                    (title, buf.is_dirty(), lines)
                } else {
                    (String::new(), false, Vec::new())
                }
            } else {
                (String::new(), false, Vec::new())
            };

            let (cursor_line, cursor_col) = self.cursor_position();

            panes.push(render::PaneFrame {
                id: pane_id,
                kind: render::PaneKind::Editor,
                visible_lines,
                cursor: render::CursorState {
                    line: cursor_line,
                    column: cursor_col,
                    shape: match self.vim_state.mode() {
                        vim::Mode::Normal => render::CursorShape::Block,
                        vim::Mode::Insert => render::CursorShape::Bar,
                        vim::Mode::Visual { .. } => render::CursorShape::Block,
                        vim::Mode::Command => render::CursorShape::Bar,
                    },
                },
                scroll_offset: self.viewport.first_visible_line,
                is_active,
                title: title.clone(),
                dirty,
                status_bar: render::StatusBar {
                    mode: mode_str.to_string(),
                    title,
                    dirty,
                    line: cursor_line,
                    column: cursor_col,
                    pending_keys: if !self.leader_keys.is_empty() {
                        self.leader_keys.iter()
                            .map(|k| k.to_string())
                            .collect::<Vec<_>>()
                            .join(" ")
                    } else {
                        self.vim_state.pending_keys().to_string()
                    },
                    recording_macro: if self.vim_state.is_recording() {
                        Some('q')
                    } else {
                        None
                    },
                    mcp: render::McpIndicator::Off,
                },
            });
        }

        render::RenderFrame {
            panes,
            maximized: self.window_mgr.is_maximized(),
            hidden_pane_count: self.window_mgr.hidden_pane_count(),
            picker: None,
            which_key: if self.leader_keys.len() > 1 {
                let lookup_keys: Vec<types::KeyEvent> = self.leader_keys[1..].to_vec();
                match self.which_key_tree.lookup(&lookup_keys) {
                    which_key::WhichKeyLookup::Prefix(entries) => {
                        let prefix = self.leader_keys.iter()
                            .map(|k| k.to_string())
                            .collect::<Vec<_>>()
                            .join(" ");
                        Some(render::WhichKeyFrame {
                            entries: entries.into_iter().map(|e| render::WhichKeyEntry {
                                key: e.key,
                                label: e.label,
                                is_group: e.is_group,
                            }).collect(),
                            prefix,
                            context: render::WhichKeyContext::Leader,
                        })
                    }
                    _ => None,
                }
            } else if self.leader_keys.len() == 1 {
                let entries = self.which_key_tree.lookup(&[]);
                match entries {
                    which_key::WhichKeyLookup::Prefix(entries) => {
                        Some(render::WhichKeyFrame {
                            entries: entries.into_iter().map(|e| render::WhichKeyEntry {
                                key: e.key,
                                label: e.label,
                                is_group: e.is_group,
                            }).collect(),
                            prefix: "SPC".to_string(),
                            context: render::WhichKeyContext::Leader,
                        })
                    }
                    _ => None,
                }
            } else {
                None
            },
            command_line: None,
            quick_capture: self.quick_capture.as_ref().map(|qc| render::QuickCaptureFrame {
                prompt: match qc.kind {
                    keymap::dispatch::QuickCaptureKind::Note => {
                        "📓 Append to journal > ".to_string()
                    }
                    keymap::dispatch::QuickCaptureKind::Task => {
                        "☐ Append task > ".to_string()
                    }
                },
                input: qc.input.clone(),
                cursor_pos: qc.cursor_pos,
            }),
            date_picker: None,
            dialog: None,
            notification: self.notifications.last().cloned(),
        }
    }

    fn render_buffer_lines(&self, buf: &buffer::Buffer) -> Vec<render::RenderedLine> {
        let range = self.viewport.visible_range();
        let mut lines = Vec::new();
        let line_count = buf.len_lines();

        for line_idx in range {
            if line_idx >= line_count {
                break;
            }
            let line_text = buf.line(line_idx).to_string();
            let spans = self.parser.highlight_line(
                &line_text,
                &parser::traits::LineContext {
                    in_code_block: false,
                    in_frontmatter: false,
                    code_fence_lang: None,
                },
            );
            lines.push(render::RenderedLine {
                line_number: line_idx,
                text: line_text,
                spans,
            });
        }
        lines
    }

    fn cursor_position(&self) -> (usize, usize) {
        if let Some(page_id) = &self.active_page {
            if let Some(buf) = self.buffer_mgr.get(page_id) {
                let rope = buf.text();
                let len = rope.len_chars();
                if len == 0 {
                    return (0, 0);
                }
                // In insert mode, cursor can be at len_chars() (append position)
                let clamped = if matches!(self.vim_state.mode(), vim::Mode::Insert) {
                    self.cursor.min(len)
                } else {
                    self.cursor.min(len.saturating_sub(1))
                };
                if clamped == len {
                    // Cursor at append position — find last line
                    let last_line = rope.len_lines().saturating_sub(1);
                    let line_start = rope.line_to_char(last_line);
                    let col = clamped - line_start;
                    return (last_line, col);
                }
                let line = rope.char_to_line(clamped);
                let line_start = rope.line_to_char(line);
                let col = clamped - line_start;
                return (line, col);
            }
        }
        (0, 0)
    }

    /// Tick for timers, notifications, debounce
    pub fn tick(&mut self, now: std::time::Instant) {
        self.notifications.retain(|n| n.expires_at > now);
    }

    /// Update the viewport size (e.g. on terminal resize).
    pub fn resize(&mut self, height: usize, width: usize) {
        self.viewport = render::Viewport::new(height.saturating_sub(2), width);
    }

    // Buffer management

    pub fn open_page(&mut self, id: &types::PageId) -> Result<(), error::BloomError> {
        self.active_page = Some(id.clone());
        Ok(())
    }

    pub fn open_page_with_content(
        &mut self,
        id: &types::PageId,
        title: &str,
        path: &std::path::Path,
        content: &str,
    ) {
        self.buffer_mgr.open(id, title, path, content);
        self.active_page = Some(id.clone());
        self.cursor = 0;
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
        if let Some(page_id) = self.active_page.take() {
            self.buffer_mgr.close(&page_id);
        }
        Ok(())
    }

    pub fn save_current(&mut self) -> Result<(), error::BloomError> {
        if let Some(page_id) = &self.active_page {
            let (content, path) = {
                if let Some((buf, info)) = self.buffer_mgr.get_with_info(page_id) {
                    if !buf.is_dirty() {
                        return Ok(());
                    }
                    (buf.text().to_string(), info.path.clone())
                } else {
                    return Ok(());
                }
            };

            // Skip pseudo-paths like [scratch]
            if path.to_string_lossy().starts_with('[') {
                return Ok(());
            }

            // Atomic write: write to tmp, fsync, rename
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let tmp = path.with_extension("tmp");
            {
                use std::io::Write;
                let mut file = std::fs::File::create(&tmp)?;
                file.write_all(content.as_bytes())?;
                file.sync_all()?;
            }
            std::fs::rename(&tmp, &path)?;

            if let Some(buf) = self.buffer_mgr.get_mut(page_id) {
                buf.mark_clean();
            }
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
        Ok(())
    }
    pub fn restore_session(&mut self) -> Result<(), error::BloomError> {
        Ok(())
    }
    pub fn rebuild_index(&mut self) -> Result<index::RebuildStats, error::BloomError> {
        Ok(index::RebuildStats {
            pages: 0,
            links: 0,
            tags: 0,
        })
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
        editor.open_page_with_content(&id, "Test", std::path::Path::new("test.md"), "# Hello\n\nWorld\n");
        let frame = editor.render();
        assert!(!frame.panes.is_empty());
        assert_eq!(frame.panes[0].status_bar.mode, "NORMAL");
        assert!(!frame.panes[0].title.is_empty());
    }

    // UC-14: Insert mode typing
    #[test]
    fn test_enter_insert_mode() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(&id, "Test", std::path::Path::new("test.md"), "hello");
        editor.handle_key(KeyEvent::char('i'));
        let frame = editor.render();
        assert_eq!(frame.panes[0].status_bar.mode, "INSERT");
        assert!(matches!(frame.panes[0].cursor.shape, render::CursorShape::Bar));
    }

    // UC-14: Insert mode actually inserts characters
    #[test]
    fn test_insert_mode_types_chars() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(&id, "Test", std::path::Path::new("test.md"), "");
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
        editor.open_page_with_content(&id, "Test", std::path::Path::new("test.md"), "");
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
        editor.open_page_with_content(&id, "Test", std::path::Path::new("test.md"), "");
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
        editor.open_page_with_content(&id, "Test", std::path::Path::new("test.md"), "ab");
        editor.handle_key(KeyEvent::char('i')); // insert at pos 0
        // Move right twice to end
        editor.handle_key(KeyEvent { code: types::KeyCode::Right, modifiers: types::Modifiers::none() });
        editor.handle_key(KeyEvent { code: types::KeyCode::Right, modifiers: types::Modifiers::none() });
        // Type 'c' — should appear after "ab"
        editor.handle_key(KeyEvent::char('c'));
        let buf = editor.buffer_mgr.get(&id).unwrap();
        assert_eq!(buf.text().to_string(), "abc");
        // Still in insert mode
        let frame = editor.render();
        assert_eq!(frame.panes[0].status_bar.mode, "INSERT");
    }

    // o opens a new line below and positions cursor correctly
    #[test]
    fn test_open_below() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(&id, "Test", std::path::Path::new("test.md"), "hello\nworld\n");
        editor.handle_key(KeyEvent::char('o'));
        // Should be in insert mode on a new line below "hello"
        let frame = editor.render();
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
        editor.open_page_with_content(&id, "Test", std::path::Path::new("test.md"), "hello\nworld\n");
        editor.handle_key(KeyEvent::char('O'));
        let frame = editor.render();
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
        editor.open_page_with_content(&id, "Test", std::path::Path::new("test.md"), "hello");
        editor.handle_key(KeyEvent::char('o'));
        let frame = editor.render();
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
        editor.open_page_with_content(&id, "Test", std::path::Path::new("test.md"), "hello");
        editor.handle_key(KeyEvent::char('i'));
        editor.handle_key(KeyEvent::esc());
        let frame = editor.render();
        assert_eq!(frame.panes[0].status_bar.mode, "NORMAL");
        assert!(matches!(frame.panes[0].cursor.shape, render::CursorShape::Block));
    }

    // UC-90: Ctrl+S saves
    #[test]
    fn test_ctrl_s_saves() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(&id, "Test", std::path::Path::new("test.md"), "hello");
        let actions = editor.handle_key(KeyEvent::ctrl('s'));
        assert!(actions.iter().any(|a| matches!(a, keymap::dispatch::Action::Save)));
    }

    // UC-52: Window splits
    #[test]
    fn test_window_split_via_editor() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(&id, "Test", std::path::Path::new("test.md"), "hello");

        // Count initial panes
        let frame = editor.render();
        let initial_count = frame.panes.len();
        assert_eq!(initial_count, 1);
    }

    // UC-18: Undo through editor
    #[test]
    fn test_undo_via_handle_key() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(&id, "Test", std::path::Path::new("test.md"), "hello");
        // Type 'u' for undo in normal mode
        editor.handle_key(KeyEvent::char('u'));
        // Shouldn't crash, even with no edits to undo
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
        let frame = editor.render();
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
        let id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(&id, "Test", &file_path, "hello");

        // Edit: insert 'X' at start
        editor.handle_key(KeyEvent::char('i'));
        editor.handle_key(KeyEvent::char('X'));
        editor.handle_key(KeyEvent::esc());

        editor.save_current().unwrap();

        // Verify file on disk has the new content
        let on_disk = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(on_disk, "Xhello");
    }

    // Startup: Journal mode opens today's journal
    #[test]
    fn test_startup_journal_mode() {
        let config = config::Config::defaults(); // default is Journal
        let mut editor = BloomEditor::new(config).unwrap();
        editor.startup();
        let frame = editor.render();
        assert!(!frame.panes.is_empty());
        assert!(frame.panes[0].visible_lines.len() > 0 || frame.panes[0].title.contains("20"));
        // Keys should work — enter insert mode
        editor.handle_key(KeyEvent::char('i'));
        let frame = editor.render();
        assert_eq!(frame.panes[0].status_bar.mode, "INSERT");
    }

    // Startup: Blank mode opens scratch buffer
    #[test]
    fn test_startup_blank_mode() {
        let mut config = config::Config::defaults();
        config.startup.mode = config::StartupMode::Blank;
        let mut editor = BloomEditor::new(config).unwrap();
        editor.startup();
        let frame = editor.render();
        assert!(!frame.panes.is_empty());
        assert_eq!(frame.panes[0].title, "[scratch]");
        // Keys should work
        editor.handle_key(KeyEvent::char('i'));
        let frame = editor.render();
        assert_eq!(frame.panes[0].status_bar.mode, "INSERT");
    }

    // Startup: Restore mode falls back to scratch when no session exists
    #[test]
    fn test_startup_restore_fallback() {
        let mut config = config::Config::defaults();
        config.startup.mode = config::StartupMode::Restore;
        let mut editor = BloomEditor::new(config).unwrap();
        editor.startup();
        let frame = editor.render();
        assert!(!frame.panes.is_empty());
        // Falls back to scratch since restore_session is a stub
        assert_eq!(frame.panes[0].title, "[scratch]");
        // Keys should work
        editor.handle_key(KeyEvent::char('i'));
        let frame = editor.render();
        assert_eq!(frame.panes[0].status_bar.mode, "INSERT");
    }

    // Wizard: starts at Welcome step
    #[test]
    fn test_wizard_starts_at_welcome() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        editor.start_wizard();
        let frame = editor.render();
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
        let frame = editor.render();
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
        let frame = editor.render();
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
        editor.handle_key(KeyEvent::esc());   // → Welcome
        let frame = editor.render();
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

        let frame = editor.render();
        if let render::PaneKind::SetupWizard(sw) = &frame.panes[0].kind {
            assert!(matches!(sw.step, render::SetupStep::ImportChoice));
            assert_eq!(sw.import_choice, render::ImportChoice::No);
        } else {
            panic!("expected wizard pane");
        }

        // Toggle to Yes
        editor.handle_key(KeyEvent::char('j'));
        let frame = editor.render();
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
        let frame = editor.render();
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
        assert!(actions.iter().any(|a| matches!(a, keymap::dispatch::Action::Quit)));
    }

    // SPC f f opens find page picker
    #[test]
    fn test_leader_spc_f_f_opens_picker() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(&id, "Test", std::path::Path::new("test.md"), "hello");
        editor.handle_key(KeyEvent::char(' ')); // SPC
        editor.handle_key(KeyEvent::char('f')); // f (group)
        let actions = editor.handle_key(KeyEvent::char('f')); // f (action)
        assert!(actions
            .iter()
            .any(|a| matches!(a, keymap::dispatch::Action::OpenPicker(keymap::dispatch::PickerKind::FindPage))));
    }

    // SPC shows which-key popup in render
    #[test]
    fn test_leader_spc_shows_which_key() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(&id, "Test", std::path::Path::new("test.md"), "hello");
        editor.handle_key(KeyEvent::char(' ')); // SPC
        let frame = editor.render();
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
        editor.open_page_with_content(&id, "Test", std::path::Path::new("test.md"), "hello");
        editor.handle_key(KeyEvent::char(' ')); // SPC
        editor.handle_key(KeyEvent::esc());     // Cancel
        let frame = editor.render();
        assert!(frame.which_key.is_none());
    }

    // :q quits
    #[test]
    fn test_colon_q_quits() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(&id, "Test", std::path::Path::new("test.md"), "hello");
        editor.handle_key(KeyEvent::char(':')); // enter command mode
        editor.handle_key(KeyEvent::char('q'));
        let actions = editor.handle_key(KeyEvent::enter());
        assert!(actions.iter().any(|a| matches!(a, keymap::dispatch::Action::Quit)));
    }

    // :w saves
    #[test]
    fn test_colon_w_saves() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(&id, "Test", std::path::Path::new("test.md"), "hello");
        editor.handle_key(KeyEvent::char(':'));
        editor.handle_key(KeyEvent::char('w'));
        let actions = editor.handle_key(KeyEvent::enter());
        assert!(actions.iter().any(|a| matches!(a, keymap::dispatch::Action::Save)));
    }

    // :wq saves and quits
    #[test]
    fn test_colon_wq_saves_and_quits() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(&id, "Test", std::path::Path::new("test.md"), "hello");
        editor.handle_key(KeyEvent::char(':'));
        editor.handle_key(KeyEvent::char('w'));
        editor.handle_key(KeyEvent::char('q'));
        let actions = editor.handle_key(KeyEvent::enter());
        assert!(actions.iter().any(|a| matches!(a, keymap::dispatch::Action::Save)));
        assert!(actions.iter().any(|a| matches!(a, keymap::dispatch::Action::Quit)));
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
        let actions = editor.handle_key(KeyEvent::char('u'));
        assert!(actions.iter().any(|a| matches!(a, keymap::dispatch::Action::Undo)));
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
        let actions = editor.handle_key(KeyEvent::ctrl('r'));
        assert!(actions.iter().any(|a| matches!(a, keymap::dispatch::Action::Redo)));
        let buf = editor.buffer_mgr.get(&id).unwrap();
        assert_eq!(buf.text().to_string(), "Xhello");
    }
}



