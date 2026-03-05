// Bloom core library

pub mod agenda;
pub mod align;
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
    picker_state: Option<ActivePicker>,
    quick_capture: Option<QuickCaptureState>,
    notifications: Vec<render::Notification>,
    viewport: render::Viewport,
    wizard: Option<SetupWizardState>,
    vault_root: Option<std::path::PathBuf>,
    leader_keys: Vec<types::KeyEvent>,
    pending_since: Option<Instant>,
    which_key_visible: bool,
    active_theme: &'static theme::ThemePalette,
    // Auto-save
    autosave_tx: Option<crossbeam::channel::Sender<store::disk_writer::WriteRequest>>,
    terminal_height: u16,
    terminal_width: u16,
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

/// Extract the link ID from a `[[id|text]]` pattern at the given column in a line.
fn extract_link_at_col(line: &str, col: usize) -> Option<String> {
    let byte_col = line.char_indices().nth(col).map(|(i, _)| i).unwrap_or(line.len());
    let bytes = line.as_bytes();
    let len = bytes.len();
    if byte_col >= len { return None; }

    // Search backwards for [[
    let mut start = None;
    let mut i = byte_col.min(len.saturating_sub(1));
    while i > 0 {
        if i > 0 && bytes[i - 1] == b'[' && bytes[i] == b'[' {
            start = Some(i + 1);
            break;
        }
        // If we hit ]], we're not inside a link
        if i > 0 && bytes[i - 1] == b']' && bytes[i] == b']' {
            return None;
        }
        i -= 1;
    }
    let content_start = start?;

    // Search forward for ]]
    let mut j = content_start;
    while j + 1 < len {
        if bytes[j] == b']' && bytes[j + 1] == b']' {
            let content = &line[content_start..j];
            // Extract the ID (before | or # if present)
            let id = content.split('|').next().unwrap_or(content);
            let id = id.split('#').next().unwrap_or(id);
            return Some(id.to_string());
        }
        j += 1;
    }
    None
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

struct ActivePicker {
    kind: keymap::dispatch::PickerKind,
    picker: picker::Picker<GenericPickerItem>,
    title: String,
    query: String,
    status_noun: String,
    min_query_len: usize,
    /// For theme picker: the theme to revert to on cancel.
    previous_theme: Option<&'static theme::ThemePalette>,
}

#[derive(Clone)]
struct GenericPickerItem {
    id: String,
    label: String,
    middle: Option<String>,
    right: Option<String>,
    preview_text: Option<String>,
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
}

struct QuickCaptureState {
    kind: keymap::dispatch::QuickCaptureKind,
    input: String,
    cursor_pos: usize,
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
            pending_since: None,
            which_key_visible: false,
            active_theme,
            autosave_tx: None,
            terminal_height: 24,
            terminal_width: 80,
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

    fn open_picker(&mut self, kind: keymap::dispatch::PickerKind) {
        use keymap::dispatch::PickerKind;
        let (title, status_noun, items) = match &kind {
            PickerKind::FindPage => {
                ("Find Page".to_string(), "pages".to_string(), self.collect_page_items())
            }
            PickerKind::SwitchBuffer => {
                let items: Vec<GenericPickerItem> = self.buffer_mgr.open_buffers().iter().map(|info| {
                    GenericPickerItem {
                        id: info.page_id.to_hex(),
                        label: info.title.clone(),
                        middle: Some("[+]".to_string()),
                        right: Some(info.path.display().to_string()),
                        preview_text: None,
                    }
                }).collect();
                ("Switch Buffer".to_string(), "open buffers".to_string(), items)
            }
            PickerKind::Search => {
                let (noun, items) = self.collect_search_items();
                ("Search".to_string(), noun, items)
            }
            PickerKind::Journal => {
                ("Journal".to_string(), "journal entries".to_string(), self.collect_journal_items())
            }
            PickerKind::Tags => {
                let items = if let Some(idx) = &self.index {
                    idx.all_tags().into_iter().map(|(tag, count)| {
                        GenericPickerItem {
                            id: tag.0.clone(),
                            label: format!("#{}", tag.0),
                            middle: None,
                            right: Some(format!("{count} pages")),
                            preview_text: None,
                        }
                    }).collect()
                } else {
                    Vec::new()
                };
                ("Tags".to_string(), "tags".to_string(), items)
            }
            PickerKind::AllCommands => {
                let items: Vec<GenericPickerItem> = vec![
                    ("find_page", "Find page", "SPC f f"),
                    ("switch_buffer", "Switch buffer", "SPC b b"),
                    ("journal_today", "Journal today", "SPC j j"),
                    ("search", "Search", "SPC s s"),
                    ("search_tags", "Search tags", "SPC s t"),
                    ("split_vertical", "Split vertical", "SPC w v"),
                    ("split_horizontal", "Split horizontal", "SPC w s"),
                    ("agenda", "Agenda", "SPC a a"),
                    ("undo_tree", "Undo tree", "SPC u u"),
                    ("theme_selector", "Theme selector", "SPC T t"),
                    ("new_from_template", "New from template", "SPC n"),
                    ("rebuild_index", "Rebuild index", "SPC h r"),
                ].into_iter().map(|(id, label, keys)| {
                    GenericPickerItem {
                        id: id.to_string(),
                        label: label.to_string(),
                        middle: Some(keys.to_string()),
                        right: None,
                        preview_text: None,
                    }
                }).collect();
                ("All Commands".to_string(), "commands".to_string(), items)
            }
            PickerKind::Templates => {
                ("Templates".to_string(), "templates".to_string(), Vec::new())
            }
            _ => {
                ("Picker".to_string(), "results".to_string(), Vec::new())
            }
        };
        let min_query_len = match &kind {
            PickerKind::Search => 2,
            _ => 0,
        };
        let match_mode = match &kind {
            PickerKind::Search => picker::MatchMode::AllWords,
            _ => picker::MatchMode::Fuzzy,
        };
        self.picker_state = Some(ActivePicker {
            kind,
            picker: picker::Picker::with_match_mode(items, match_mode),
            title,
            query: String::new(),
            status_noun,
            min_query_len,
            previous_theme: None,
        });
    }

    fn collect_page_items(&self) -> Vec<GenericPickerItem> {
        // Scan vault directory for .md files in pages/
        if let Some(root) = &self.vault_root {
            let pages_dir = root.join("pages");
            if pages_dir.exists() {
                if let Ok(entries) = std::fs::read_dir(&pages_dir) {
                    return entries.filter_map(|e| {
                        let e = e.ok()?;
                        let path = e.path();
                        if path.extension()?.to_str()? != "md" { return None; }
                        let content = std::fs::read_to_string(&path).ok()?;
                        let fm = self.parser.parse_frontmatter(&content)?;
                        let title = fm.title.unwrap_or_else(|| {
                            path.file_stem().unwrap_or_default().to_string_lossy().to_string()
                        });
                        let tags = fm.tags.iter().map(|t| format!("#{}", t.0)).collect::<Vec<_>>().join(" ");
                        let preview = content.lines().take(10).collect::<Vec<_>>().join("\n");
                        let date = fm.created.map(|d| d.format("%b %d").to_string());
                        Some(GenericPickerItem {
                            id: path.to_string_lossy().to_string(),
                            label: title,
                            middle: if tags.is_empty() { None } else { Some(tags) },
                            right: date,
                            preview_text: Some(preview),
                        })
                    }).collect();
                }
            }
        }
        Vec::new()
    }

    fn collect_journal_items(&self) -> Vec<GenericPickerItem> {
        if let Some(root) = &self.vault_root {
            let journal_dir = root.join("journal");
            if journal_dir.exists() {
                if let Ok(entries) = std::fs::read_dir(&journal_dir) {
                    let mut items: Vec<GenericPickerItem> = entries.filter_map(|e| {
                        let e = e.ok()?;
                        let path = e.path();
                        if path.extension()?.to_str()? != "md" { return None; }
                        let stem = path.file_stem()?.to_string_lossy().to_string();
                        let preview = std::fs::read_to_string(&path).ok()
                            .map(|c| c.lines().take(8).collect::<Vec<_>>().join("\n"));
                        Some(GenericPickerItem {
                            id: path.to_string_lossy().to_string(),
                            label: stem,
                            middle: None,
                            right: Some("journal".to_string()),
                            preview_text: preview,
                        })
                    }).collect();
                    items.sort_by(|a, b| b.label.cmp(&a.label)); // most recent first
                    return items;
                }
            }
        }
        Vec::new()
    }

    fn collect_search_items(&self) -> (String, Vec<GenericPickerItem>) {
        // Collect content lines from pages and journals for full-text search.
        // Each item is a single line with source page title as right column.
        let mut items = Vec::new();
        let mut page_titles = std::collections::HashSet::new();
        if let Some(root) = &self.vault_root {
            for subdir in &["pages", "journal"] {
                let is_journal = *subdir == "journal";
                let dir = root.join(subdir);
                if !dir.exists() {
                    continue;
                }
                let entries = match std::fs::read_dir(&dir) {
                    Ok(e) => e,
                    Err(_) => continue,
                };
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) != Some("md") {
                        continue;
                    }
                    let content = match std::fs::read_to_string(&path) {
                        Ok(c) => c,
                        Err(_) => continue,
                    };
                    let fm = self.parser.parse_frontmatter(&content);
                    let title = fm
                        .and_then(|f| f.title)
                        .unwrap_or_else(|| {
                            path.file_stem()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_string()
                        });
                    let display_title = if is_journal {
                        format!("{} (journal)", title)
                    } else {
                        title.clone()
                    };

                    let lines: Vec<&str> = content.lines().collect();

                    // Skip frontmatter region
                    let body_start = if lines.first().map_or(false, |l| l.trim() == "---") {
                        lines.iter()
                            .skip(1)
                            .position(|l| l.trim() == "---")
                            .map(|i| i + 2)   // +1 for skip(1), +1 to get past closing ---
                            .unwrap_or(0)
                    } else {
                        0
                    };

                    let mut page_has_items = false;
                    for (line_num, line_text) in lines.iter().enumerate() {
                        if line_num < body_start {
                            continue;
                        }
                        let trimmed = line_text.trim();
                        if trimmed.is_empty() {
                            continue;
                        }

                        // Build ±5 lines of context with ❯ marker on matching line
                        let ctx_start = line_num.saturating_sub(5);
                        let ctx_end = (line_num + 6).min(lines.len());
                        let preview: String = lines[ctx_start..ctx_end]
                            .iter()
                            .enumerate()
                            .map(|(i, l)| {
                                if ctx_start + i == line_num {
                                    format!("❯ {l}")
                                } else {
                                    format!("  {l}")
                                }
                            })
                            .collect::<Vec<_>>()
                            .join("\n");

                        items.push(GenericPickerItem {
                            id: format!("{}:{}", path.display(), line_num),
                            label: trimmed.to_string(),
                            middle: None,
                            right: Some(display_title.clone()),
                            preview_text: Some(preview),
                        });
                        page_has_items = true;
                    }
                    if page_has_items {
                        page_titles.insert(title);
                    }
                }
            }
        }
        let page_count = page_titles.len();
        let noun = format!("matches across {} {}", page_count, if page_count == 1 { "page" } else { "pages" });
        (noun, items)
    }

    fn open_theme_picker(&mut self) {
        let current_name = self.active_theme.name;
        let previous_theme = self.active_theme;
        let sample = "## Preview\n\n- [ ] Sample task @due(2026-03-05)\n- [x] Completed task\nSee [[abc123|Text Editor Theory]].\n#rust #editors";
        let items: Vec<GenericPickerItem> = theme::THEME_NAMES
            .iter()
            .map(|name| {
                let current_marker = if *name == current_name { "(current)" } else { "" };
                let desc = theme::theme_description(name);
                let right = if current_marker.is_empty() {
                    if desc.is_empty() { None } else { Some(desc.to_string()) }
                } else {
                    Some(format!(
                        "{}{}{}",
                        desc,
                        if desc.is_empty() { "" } else { "  " },
                        current_marker,
                    ))
                };
                GenericPickerItem {
                    id: name.to_string(),
                    label: name.to_string(),
                    middle: None,
                    right,
                    preview_text: Some(sample.to_string()),
                }
            })
            .collect();
        let current_idx = theme::THEME_NAMES
            .iter()
            .position(|n| *n == current_name)
            .unwrap_or(0);
        let mut picker = picker::Picker::new(items);
        // Pre-select the current theme
        for _ in 0..current_idx {
            picker.move_selection(1);
        }
        self.picker_state = Some(ActivePicker {
            kind: keymap::dispatch::PickerKind::Theme,
            picker,
            title: "Theme".to_string(),
            query: String::new(),
            status_noun: "themes".to_string(),
            min_query_len: 0,
            previous_theme: Some(previous_theme),
        });
    }

    /// Live-preview theme on picker selection change.
    fn theme_picker_preview_current(&mut self) {
        if let Some(ap) = &self.picker_state {
            if matches!(ap.kind, keymap::dispatch::PickerKind::Theme) {
                if let Some(item) = ap.picker.selected() {
                    if let Some(palette) = theme::palette_by_name(&item.id) {
                        self.active_theme = palette;
                    }
                }
            }
        }
    }

    fn theme_picker_confirm(&mut self) {
        if let Some(ap) = self.picker_state.take() {
            if !matches!(ap.kind, keymap::dispatch::PickerKind::Theme) { return; }
            if let Some(selected) = ap.picker.selected() {
                let name = selected.id.clone();
                self.set_theme(&name);
                // Persist to config.toml
                if let Some(root) = &self.vault_root {
                    let config_path = root.join("config.toml");
                    if config_path.exists() {
                        if let Ok(content) = std::fs::read_to_string(&config_path) {
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
                    }
                }
            }
        }
    }

    fn theme_picker_cancel(&mut self) {
        if let Some(ap) = self.picker_state.take() {
            if let Some(prev) = ap.previous_theme {
                self.active_theme = prev;
            }
        }
    }

    /// Initialize with a vault path — sets up index, journal, template engine
    pub fn init_vault(&mut self, vault_root: &std::path::Path) -> Result<(), error::BloomError> {
        let index_path = vault_root.join(".index.db");
        self.index = Some(index::Index::open(&index_path)?);
        self.journal = Some(journal::Journal::new(vault_root));
        let templates_dir = vault_root.join("templates");
        self.template_engine = Some(template::TemplateEngine::new(&templates_dir));
        self.vault_root = Some(vault_root.to_path_buf());

        // Start auto-save disk writer thread
        let (writer, tx) = store::disk_writer::DiskWriter::new(self.config.autosave_debounce_ms);
        self.autosave_tx = Some(tx);
        std::thread::Builder::new()
            .name("bloom-disk-writer".into())
            .spawn(move || writer.start())
            .ok();

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
            self.pending_since = None;
                self.which_key_visible = false;
            return self.execute_actions(vec![action]);
        }

        // If picker is open (all picker types, including theme)
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
            self.pending_since = Some(Instant::now());
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
                keymap::dispatch::Action::OpenPicker(ref kind) => {
                    if matches!(kind, keymap::dispatch::PickerKind::Theme) {
                        self.open_theme_picker();
                    } else {
                        self.open_picker(kind.clone());
                    }
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
                keymap::dispatch::Action::ToggleTask => {
                    self.toggle_task_at_cursor();
                }
                keymap::dispatch::Action::FollowLink => {
                    self.follow_link_at_cursor();
                }
                keymap::dispatch::Action::CopyToClipboard(ref text) => {
                    // Pass through — TUI handles actual clipboard
                    result.push(keymap::dispatch::Action::CopyToClipboard(text.clone()));
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
            self.pending_since = None;
                self.which_key_visible = false;
            return vec![keymap::dispatch::Action::Noop];
        }

        self.leader_keys.push(key);
        self.pending_since = Some(Instant::now());

        // Look up the full sequence (skipping the initial SPC)
        let lookup_keys: Vec<types::KeyEvent> = self.leader_keys[1..].to_vec();
        match self.which_key_tree.lookup(&lookup_keys) {
            which_key::WhichKeyLookup::Action(action_id) => {
                self.leader_keys.clear();
                self.pending_since = None;
                self.which_key_visible = false;
                self.action_id_to_actions(&action_id)
            }
            which_key::WhichKeyLookup::Prefix(_entries) => {
                // Still accumulating — show which-key popup after timeout
                vec![keymap::dispatch::Action::Noop]
            }
            which_key::WhichKeyLookup::NoMatch => {
                self.leader_keys.clear();
                self.pending_since = None;
                self.which_key_visible = false;
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
            "theme_selector" => vec![keymap::dispatch::Action::OpenPicker(
                keymap::dispatch::PickerKind::Theme,
            )],
            "close_buffer" => {
                self.close_active_buffer();
                vec![keymap::dispatch::Action::Noop]
            }
            "toggle_task" => vec![keymap::dispatch::Action::ToggleTask],
            "follow_link" => vec![keymap::dispatch::Action::FollowLink],
            "yank_link" => {
                if let Some(link) = self.yank_link_to_current_page() {
                    vec![keymap::dispatch::Action::CopyToClipboard(link)]
                } else {
                    vec![keymap::dispatch::Action::Noop]
                }
            }
            "yank_block_link" => {
                if let Some(link) = self.yank_link_to_current_block() {
                    vec![keymap::dispatch::Action::CopyToClipboard(link)]
                } else {
                    vec![keymap::dispatch::Action::Noop]
                }
            }
            "insert_link" => vec![keymap::dispatch::Action::OpenPicker(
                keymap::dispatch::PickerKind::InlineLink,
            )],
            "add_tag" => {
                // TODO: open tag input picker
                vec![keymap::dispatch::Action::Noop]
            }
            "remove_tag" => {
                // TODO: open picker with current page's tags for selection
                vec![keymap::dispatch::Action::Noop]
            }
            "insert_due" => {
                self.insert_text_at_cursor("@due()");
                // Move cursor inside the parens
                self.cursor = self.cursor.saturating_sub(1);
                vec![keymap::dispatch::Action::Noop]
            }
            "insert_start" => {
                self.insert_text_at_cursor("@start()");
                self.cursor = self.cursor.saturating_sub(1);
                vec![keymap::dispatch::Action::Noop]
            }
            "insert_at" => {
                self.insert_text_at_cursor("@at()");
                self.cursor = self.cursor.saturating_sub(1);
                vec![keymap::dispatch::Action::Noop]
            }
            "search_backlinks" => {
                if let Some(id) = self.active_page.clone() {
                    vec![keymap::dispatch::Action::OpenPicker(
                        keymap::dispatch::PickerKind::Backlinks(id),
                    )]
                } else {
                    vec![keymap::dispatch::Action::Noop]
                }
            }
            "search_unlinked" => {
                if let Some(id) = self.active_page.clone() {
                    vec![keymap::dispatch::Action::OpenPicker(
                        keymap::dispatch::PickerKind::UnlinkedMentions(id),
                    )]
                } else {
                    vec![keymap::dispatch::Action::Noop]
                }
            }
            "search_journal" => vec![keymap::dispatch::Action::OpenPicker(
                keymap::dispatch::PickerKind::Journal,
            )],
            "timeline" => {
                if let Some(id) = self.active_page.clone() {
                    vec![keymap::dispatch::Action::OpenTimeline(id)]
                } else {
                    vec![keymap::dispatch::Action::Noop]
                }
            }
            "backlinks" => {
                if let Some(id) = self.active_page.clone() {
                    vec![keymap::dispatch::Action::OpenPicker(
                        keymap::dispatch::PickerKind::Backlinks(id),
                    )]
                } else {
                    vec![keymap::dispatch::Action::Noop]
                }
            }
            "journal_prev" => {
                self.navigate_journal(-1);
                vec![keymap::dispatch::Action::Noop]
            }
            "journal_next" => {
                self.navigate_journal(1);
                vec![keymap::dispatch::Action::Noop]
            }
            "rename_page" => {
                // TODO: open rename input pre-filled with current title
                vec![keymap::dispatch::Action::Noop]
            }
            "delete_page" => {
                // TODO: show confirmation dialog, then delete
                vec![keymap::dispatch::Action::Noop]
            }
            "balance" => {
                self.window_mgr.balance();
                vec![keymap::dispatch::Action::Noop]
            }
            "maximize" => {
                self.window_mgr.maximize_toggle();
                vec![keymap::dispatch::Action::Noop]
            }
            _ => vec![keymap::dispatch::Action::Noop],
        }
    }

    fn handle_picker_key(&mut self, key: &types::KeyEvent) -> Vec<keymap::dispatch::Action> {
        use types::KeyCode;
        let is_theme = self.picker_state.as_ref()
            .map_or(false, |ap| matches!(ap.kind, keymap::dispatch::PickerKind::Theme));

        // Ctrl+key shortcuts
        if key.modifiers.ctrl {
            match &key.code {
                // Ctrl+N / Ctrl+J → next result
                KeyCode::Char('n') | KeyCode::Char('j') => {
                    if let Some(ap) = &mut self.picker_state {
                        ap.picker.move_selection(1);
                    }
                    if is_theme { self.theme_picker_preview_current(); }
                    return vec![keymap::dispatch::Action::Noop];
                }
                // Ctrl+P / Ctrl+K → previous result
                KeyCode::Char('p') | KeyCode::Char('k') => {
                    if let Some(ap) = &mut self.picker_state {
                        ap.picker.move_selection(-1);
                    }
                    if is_theme { self.theme_picker_preview_current(); }
                    return vec![keymap::dispatch::Action::Noop];
                }
                // Ctrl+G → close picker
                KeyCode::Char('g') => {
                    if is_theme {
                        self.theme_picker_cancel();
                    } else {
                        self.picker_state = None;
                    }
                    return vec![keymap::dispatch::Action::Noop];
                }
                // Ctrl+U → clear search input
                KeyCode::Char('u') => {
                    if let Some(ap) = &mut self.picker_state {
                        ap.query.clear();
                        ap.picker.set_query("");
                    }
                    return vec![keymap::dispatch::Action::Noop];
                }
                _ => return vec![keymap::dispatch::Action::Noop],
            }
        }

        match &key.code {
            KeyCode::Esc => {
                if is_theme {
                    self.theme_picker_cancel();
                } else {
                    self.picker_state = None;
                }
                vec![keymap::dispatch::Action::Noop]
            }
            KeyCode::Enter => {
                if is_theme {
                    self.theme_picker_confirm();
                } else if let Some(ap) = self.picker_state.take() {
                    if let Some(selected) = ap.picker.selected() {
                        self.handle_picker_selection(&ap.kind, selected.clone());
                    }
                }
                vec![keymap::dispatch::Action::Noop]
            }
            KeyCode::Up => {
                if let Some(ap) = &mut self.picker_state {
                    ap.picker.move_selection(-1);
                }
                if is_theme { self.theme_picker_preview_current(); }
                vec![keymap::dispatch::Action::Noop]
            }
            KeyCode::Down => {
                if let Some(ap) = &mut self.picker_state {
                    ap.picker.move_selection(1);
                }
                if is_theme { self.theme_picker_preview_current(); }
                vec![keymap::dispatch::Action::Noop]
            }
            KeyCode::Tab => {
                if let Some(ap) = &mut self.picker_state {
                    ap.picker.toggle_mark();
                }
                vec![keymap::dispatch::Action::Noop]
            }
            KeyCode::Backspace => {
                if let Some(ap) = &mut self.picker_state {
                    if !ap.query.is_empty() {
                        ap.query.pop();
                        ap.picker.set_query(&ap.query);
                    }
                }
                vec![keymap::dispatch::Action::Noop]
            }
            KeyCode::Char(c) => {
                if let Some(ap) = &mut self.picker_state {
                    ap.query.push(*c);
                    ap.picker.set_query(&ap.query);
                }
                vec![keymap::dispatch::Action::Noop]
            }
            _ => vec![keymap::dispatch::Action::Noop],
        }
    }

    fn handle_picker_selection(
        &mut self,
        kind: &keymap::dispatch::PickerKind,
        item: GenericPickerItem,
    ) {
        use keymap::dispatch::PickerKind;
        match kind {
            PickerKind::FindPage | PickerKind::Journal => {
                // Open the selected page
                let path = std::path::PathBuf::from(&item.id);
                if let Ok(content) = std::fs::read_to_string(&path) {
                    let fm = self.parser.parse_frontmatter(&content);
                    let title = fm.and_then(|f| f.title).unwrap_or_else(|| item.label.clone());
                    let id = crate::uuid::generate_hex_id();
                    self.open_page_with_content(&id, &title, &path, &content);
                }
            }
            PickerKind::Search => {
                // item.id is "path:line_number" — open page and jump to line
                if let Some(colon) = item.id.rfind(':') {
                    let path_str = &item.id[..colon];
                    let line_num: usize = item.id[colon + 1..].parse().unwrap_or(0);
                    let path = std::path::PathBuf::from(path_str);
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        let fm = self.parser.parse_frontmatter(&content);
                        let title = fm.and_then(|f| f.title).unwrap_or_else(|| item.label.clone());
                        let id = crate::uuid::generate_hex_id();
                        self.open_page_with_content(&id, &title, &path, &content);
                        // Jump cursor to the matching line
                        if let Some(page_id) = &self.active_page {
                            if let Some(buf) = self.buffer_mgr.get(page_id) {
                                let target_char = buf.text().line_to_char(
                                    line_num.min(buf.len_lines().saturating_sub(1)),
                                );
                                self.cursor = target_char;
                            }
                        }
                    }
                }
            }
            PickerKind::SwitchBuffer => {
                // Switch to the selected buffer
                if let Some(page_id) = types::PageId::from_hex(&item.id) {
                    self.active_page = Some(page_id);
                    self.cursor = 0;
                }
            }
            PickerKind::Tags => {
                // Open a search filtered by this tag (for now, just log)
                // TODO: transition to search picker filtered by tag
            }
            PickerKind::AllCommands => {
                // Execute the command
                let actions = self.action_id_to_actions(&item.id);
                // Need to execute these actions
                let _ = self.execute_actions(actions);
            }
            _ => {}
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
                    let byte_pos = wiz.vault_path.char_indices().nth(wiz.vault_path_cursor).map(|(i, _)| i).unwrap_or(wiz.vault_path.len());
                    wiz.vault_path.insert(byte_pos, *c);
                    wiz.vault_path_cursor += 1;
                }
                KeyCode::Backspace => {
                    if wiz.vault_path_cursor > 0 {
                        wiz.vault_path_cursor -= 1;
                        let byte_pos = wiz.vault_path.char_indices().nth(wiz.vault_path_cursor).map(|(i, _)| i).unwrap_or(wiz.vault_path.len());
                        wiz.vault_path.remove(byte_pos);
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
                    let byte_pos = wiz.logseq_path.char_indices().nth(wiz.logseq_path_cursor).map(|(i, _)| i).unwrap_or(wiz.logseq_path.len());
                    wiz.logseq_path.insert(byte_pos, *c);
                    wiz.logseq_path_cursor += 1;
                }
                KeyCode::Backspace => {
                    if wiz.logseq_path_cursor > 0 {
                        wiz.logseq_path_cursor -= 1;
                        let byte_pos = wiz.logseq_path.char_indices().nth(wiz.logseq_path_cursor).map(|(i, _)| i).unwrap_or(wiz.logseq_path.len());
                        wiz.logseq_path.remove(byte_pos);
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

    /// Toggle task checkbox on the line at the cursor: `[ ]` ↔ `[x]`.
    fn toggle_task_at_cursor(&mut self) {
        let Some(page_id) = &self.active_page else { return };
        let Some(buf) = self.buffer_mgr.get_mut(page_id) else { return };
        let rope = buf.text();
        let len = rope.len_chars();
        if len == 0 { return; }
        let cursor = self.cursor.min(len.saturating_sub(1));
        let line_idx = rope.char_to_line(cursor);
        let line_text = rope.line(line_idx).to_string();
        let trimmed = line_text.trim_start();
        let indent = line_text.len() - trimmed.len();

        let line_start = rope.line_to_char(line_idx);

        if trimmed.starts_with("- [ ] ") {
            // Unchecked → checked
            let bracket_start = line_start + indent + 2; // position of '['
            buf.replace(bracket_start..bracket_start + 3, "[x]");
        } else if trimmed.starts_with("- [x] ") || trimmed.starts_with("- [X] ") {
            // Checked → unchecked
            let bracket_start = line_start + indent + 2;
            buf.replace(bracket_start..bracket_start + 3, "[ ]");
        }
    }

    /// Close the active buffer. Opens journal or scratch if it was the last buffer.
    fn close_active_buffer(&mut self) {
        if let Some(page_id) = self.active_page.take() {
            self.buffer_mgr.close(&page_id);
            // Switch to another open buffer, or open journal
            if let Some(next) = self.buffer_mgr.open_buffers().first() {
                self.active_page = Some(next.page_id.clone());
                self.cursor = 0;
            } else {
                self.open_journal_today();
            }
        }
    }

    /// Follow the wiki-link under the cursor: `[[id|text]]` → open page by id.
    fn follow_link_at_cursor(&mut self) {
        let Some(page_id) = &self.active_page else { return };
        let Some(buf) = self.buffer_mgr.get(page_id) else { return };
        let rope = buf.text();
        let len = rope.len_chars();
        if len == 0 { return; }
        let cursor = self.cursor.min(len.saturating_sub(1));
        let line_idx = rope.char_to_line(cursor);
        let line_text = rope.line(line_idx).to_string();
        let col = cursor - rope.line_to_char(line_idx);

        // Find [[...]] surrounding the cursor column
        if let Some(link_id) = extract_link_at_col(&line_text, col) {
            // Try to open the page from the vault
            if let Some(target_id) = types::PageId::from_hex(&link_id) {
                if let Some(root) = &self.vault_root {
                    let pages_dir = root.join("pages");
                    // Scan for the file containing this id
                    if let Ok(entries) = std::fs::read_dir(&pages_dir) {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            let name = path.file_name().unwrap_or_default().to_string_lossy();
                            if name.contains(&link_id) && name.ends_with(".md") {
                                if let Ok(content) = std::fs::read_to_string(&path) {
                                    let title = self.parser.parse_frontmatter(&content)
                                        .and_then(|f| f.title)
                                        .unwrap_or_else(|| link_id.clone());
                                    self.open_page_with_content(&target_id, &title, &path, &content);
                                    return;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Yank a `[[id|title]]` link to the current page.
    fn yank_link_to_current_page(&self) -> Option<String> {
        let page_id = self.active_page.as_ref()?;
        let _buf = self.buffer_mgr.get(page_id)?;
        let buffers = self.buffer_mgr.open_buffers();
        let info = buffers.iter().find(|b| b.page_id == *page_id)?;
        Some(format!("[[{}|{}]]", page_id.to_hex(), info.title))
    }

    /// Yank a `[[id#block-id|title]]` link to the block at the cursor.
    fn yank_link_to_current_block(&self) -> Option<String> {
        let page_id = self.active_page.as_ref()?;
        let buf = self.buffer_mgr.get(page_id)?;
        let rope = buf.text();
        let len = rope.len_chars();
        if len == 0 { return None; }
        let cursor = self.cursor.min(len.saturating_sub(1));
        let line_idx = rope.char_to_line(cursor);
        let line_text = rope.line(line_idx).to_string();
        // Look for ^block-id on this line
        if let Some(caret_pos) = line_text.rfind('^') {
            let block_id = line_text[caret_pos + 1..].trim();
            if !block_id.is_empty() && block_id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
                let buffers = self.buffer_mgr.open_buffers();
                let info = buffers.iter().find(|b| b.page_id == *page_id)?;
                return Some(format!("[[{}#{}|{}]]", page_id.to_hex(), block_id, info.title));
            }
        }
        // Fallback: page link without block
        self.yank_link_to_current_page()
    }

    /// Insert text at the current cursor position.
    fn insert_text_at_cursor(&mut self, text: &str) {
        let Some(page_id) = &self.active_page else { return };
        let Some(buf) = self.buffer_mgr.get_mut(page_id) else { return };
        buf.insert(self.cursor, text);
        self.cursor += text.chars().count();
    }

    /// Schedule an auto-save for the given page via the disk writer thread.
    fn schedule_autosave(&self, page_id: &types::PageId) {
        let Some(tx) = &self.autosave_tx else { return };
        let Some(buf) = self.buffer_mgr.get(page_id) else { return };
        let buffers = self.buffer_mgr.open_buffers();
        let Some(info) = buffers.iter().find(|b| b.page_id == *page_id) else { return };
        let content = buf.text().to_string();
        let _ = tx.send(store::disk_writer::WriteRequest {
            path: info.path.clone(),
            content,
        });
    }

    /// Navigate to the previous or next journal entry.
    fn navigate_journal(&mut self, delta: i32) {
        let Some(journal) = &self.journal else { return };
        let today = journal::Journal::today();

        // Find current journal date from active page
        let current_date = self.active_page.as_ref()
            .and_then(|id| self.buffer_mgr.get(id))
            .and_then(|buf| {
                let text = buf.text().to_string();
                self.parser.parse_frontmatter(&text)
                    .and_then(|fm| fm.created)
            })
            .unwrap_or(today);

        // Simply offset by one day
        let target = if delta > 0 {
            current_date.succ_opt().unwrap_or(current_date)
        } else {
            current_date.pred_opt().unwrap_or(current_date)
        };

        let title = target.format("%Y-%m-%d").to_string();
        let path = journal.path_for_date(target);
        let content = if path.exists() {
            std::fs::read_to_string(&path).unwrap_or_default()
        } else {
            let fm = parser::traits::Frontmatter {
                id: None,
                title: Some(title.clone()),
                created: Some(target),
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

    fn translate_vim_action(
        &mut self,
        action: vim::VimAction,
    ) -> Vec<keymap::dispatch::Action> {
        match action {
            vim::VimAction::Edit(edit) => {
                self.pending_since = None;
                self.which_key_visible = false;
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
                    self.schedule_autosave(page_id);
                }
                vec![keymap::dispatch::Action::Edit(buffer::EditOp {
                    range: edit.range,
                    replacement: edit.replacement,
                    cursor_after: edit.cursor_after,
                })]
            }
            vim::VimAction::Motion(motion) => {
                self.pending_since = None;
                self.which_key_visible = false;
                self.cursor = motion.new_position;
                vec![keymap::dispatch::Action::Motion(
                    keymap::dispatch::MotionResult {
                        new_position: motion.new_position,
                        extend_selection: motion.extend_selection,
                    },
                )]
            }
            vim::VimAction::ModeChange(ref mode) => {
                let was_insert = matches!(self.vim_state.mode(), vim::Mode::Insert);
                if matches!(mode, vim::Mode::Command) {
                    self.pending_since = Some(Instant::now());
                } else {
                    self.pending_since = None;
                self.which_key_visible = false;
                }
                // Edit group lifecycle: begin on Insert entry, end on Insert exit
                if matches!(mode, vim::Mode::Insert) {
                    if let Some(page_id) = &self.active_page {
                        if let Some(buf) = self.buffer_mgr.get_mut(page_id) {
                            buf.begin_edit_group();
                        }
                    }
                } else if matches!(mode, vim::Mode::Normal) {
                    // Leaving Insert (or Visual, Command) → close any open group
                    if let Some(page_id) = &self.active_page {
                        if let Some(buf) = self.buffer_mgr.get_mut(page_id) {
                            buf.end_edit_group();
                        }
                    }
                    // Auto-align only on Insert→Normal transition
                    if was_insert {
                        match self.config.auto_align {
                            config::AutoAlignMode::Page => {
                                if let Some(page_id) = &self.active_page {
                                    if let Some(buf) = self.buffer_mgr.get_mut(page_id) {
                                        align::auto_align_page(buf);
                                    }
                                }
                            }
                            config::AutoAlignMode::Block => {
                                let cursor_line = self.cursor_position().0;
                                if let Some(page_id) = &self.active_page {
                                    if let Some(buf) = self.buffer_mgr.get_mut(page_id) {
                                        align::auto_align_block(buf, cursor_line);
                                    }
                                }
                            }
                            config::AutoAlignMode::None => {}
                        }
                    }
                }
                vec![keymap::dispatch::Action::ModeChange(mode.clone())]
            }
            vim::VimAction::Command(cmd) => self.handle_vim_command(&cmd),
            vim::VimAction::Pending => {
                if self.pending_since.is_none() {
                    self.pending_since = Some(Instant::now());
                }
                vec![keymap::dispatch::Action::Noop]
            }
            vim::VimAction::Unhandled => vec![keymap::dispatch::Action::Noop],
            vim::VimAction::RestoreCheckpoint => {
                // Ctrl+U in Insert mode: revert to checkpoint
                if let Some(page_id) = &self.active_page {
                    if let Some(buf) = self.buffer_mgr.get_mut(page_id) {
                        buf.restore_edit_group_checkpoint();
                        self.cursor = 0; // reset cursor to start (safe default)
                    }
                }
                vec![keymap::dispatch::Action::Noop]
            }
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
    pub fn render(&mut self) -> render::RenderFrame {
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
                    status_bar: render::StatusBarFrame::default(),
                    rect: render::PaneRectFrame::default(),
                }],
                maximized: false,
                hidden_pane_count: 0,
                picker: None,
                which_key: None,
                date_picker: None,
                dialog: None,
                notification: None,
            };
        }

        let mut panes = Vec::new();

        let mode_str = match self.vim_state.mode() {
            vim::Mode::Normal => "NORMAL",
            vim::Mode::Insert => "INSERT",
            vim::Mode::Visual { .. } => "VISUAL",
            vim::Mode::Command => "COMMAND",
        };

        // Compute pane rects from the core layout engine.
        // Reserve space for the which-key drawer only after timeout fires
        // (or if it's already visible from a previous render).
        let has_pending = !self.leader_keys.is_empty()
            || self.vim_state.pending_keys().len() > 0
            || matches!(self.vim_state.mode(), vim::Mode::Command);
        let timeout = std::time::Duration::from_millis(self.config.which_key_timeout_ms);
        let timed_out = self.pending_since
            .map_or(false, |since| since.elapsed() >= timeout);
        let show_wk = has_pending && (self.which_key_visible || timed_out);

        if show_wk && !self.which_key_visible {
            self.which_key_visible = true;
        }

        let wk_h = if show_wk {
            let col_width = 24u16;
            let cols = (self.terminal_width.saturating_sub(4) / col_width).max(1);
            let entry_count = 12u16;
            let rows_needed = (entry_count + cols - 1) / cols;
            (rows_needed + 2).min(self.terminal_height / 3).max(3)
        } else {
            0
        };
        let pane_area_h = self.terminal_height.saturating_sub(wk_h);
        let pane_rects = self.window_mgr.compute_pane_rects(self.terminal_width, pane_area_h);

        // Update viewport from the active pane's content height
        if let Some(active_rect) = pane_rects.iter().find(|r| r.pane_id == self.window_mgr.active_pane()) {
            self.viewport.height = active_rect.content_height as usize;
            self.viewport.width = active_rect.width as usize;
        }

        // Ensure cursor is visible (scrolls the viewport if needed)
        let (cursor_line, cursor_col) = self.cursor_position();
        self.viewport.ensure_visible(cursor_line);

        for rect in &pane_rects {
            let is_active = rect.pane_id == self.window_mgr.active_pane();

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

            // Build per-pane status bar
            let status_bar = if is_active {
                // Active pane: priority CommandLine > QuickCapture > Normal
                let content = if matches!(self.vim_state.mode(), vim::Mode::Command) {
                    render::StatusBarContent::CommandLine(render::CommandLineSlot {
                        input: self.vim_state.pending_keys().to_string(),
                        cursor_pos: self.vim_state.pending_keys().len(),
                        error: None,
                    })
                } else if let Some(qc) = &self.quick_capture {
                    let prompt = match qc.kind {
                        keymap::dispatch::QuickCaptureKind::Note => {
                            "📓 Append to journal > ".to_string()
                        }
                        keymap::dispatch::QuickCaptureKind::Task => {
                            "☐ Append task > ".to_string()
                        }
                    };
                    render::StatusBarContent::QuickCapture(render::QuickCaptureSlot {
                        prompt,
                        input: qc.input.clone(),
                        cursor_pos: qc.cursor_pos,
                    })
                } else {
                    render::StatusBarContent::Normal(render::NormalStatus {
                        title: title.clone(),
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
                    })
                };
                render::StatusBarFrame {
                    content,
                    mode: mode_str.to_string(),
                }
            } else {
                // Inactive pane: just title
                render::StatusBarFrame {
                    content: render::StatusBarContent::Normal(render::NormalStatus {
                        title: title.clone(),
                        dirty,
                        line: cursor_line,
                        column: cursor_col,
                        pending_keys: String::new(),
                        recording_macro: None,
                        mcp: render::McpIndicator::Off,
                    }),
                    mode: mode_str.to_string(),
                }
            };

            panes.push(render::PaneFrame {
                id: rect.pane_id,
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
                status_bar,
                rect: render::PaneRectFrame {
                    x: rect.x,
                    y: rect.y,
                    width: rect.width,
                    content_height: rect.content_height,
                    total_height: rect.height,
                },
            });
        }

        render::RenderFrame {
            panes,
            maximized: self.window_mgr.is_maximized(),
            hidden_pane_count: self.window_mgr.hidden_pane_count(),
            picker: if let Some(ap) = &self.picker_state {
                let below_min = ap.query.len() < ap.min_query_len;
                let results: Vec<render::PickerRow> = if below_min {
                    Vec::new()
                } else {
                    ap.picker.results().into_iter().map(|item| {
                        render::PickerRow {
                            label: item.label.clone(),
                            middle: item.middle.clone(),
                            right: item.right.clone(),
                        }
                    }).collect()
                };
                let preview = if below_min {
                    None
                } else {
                    ap.picker.selected().and_then(|item| item.preview_text.clone())
                };
                Some(render::PickerFrame {
                    title: ap.title.clone(),
                    query: ap.query.clone(),
                    results,
                    selected_index: if below_min { 0 } else { ap.picker.selected_index() },
                    filters: Vec::new(),
                    preview,
                    total_count: ap.picker.total_count(),
                    filtered_count: if below_min { 0 } else { ap.picker.filtered_count() },
                    status_noun: ap.status_noun.clone(),
                    min_query_len: ap.min_query_len,
                })
            } else {
                None
            },
            which_key: {
                if !show_wk {
                    None
                } else if matches!(self.vim_state.mode(), vim::Mode::Command) {
                // Show available commands as which-key hints
                let input = self.vim_state.pending_keys();
                let commands: Vec<(&str, &str)> = vec![
                    ("w", "write (save)"),
                    ("q", "quit"),
                    ("wq", "write and quit"),
                    ("e", "edit (find page)"),
                    ("sp", "split horizontal"),
                    ("vs", "vsplit vertical"),
                    ("bd", "close buffer"),
                    ("theme", "switch theme"),
                    ("rebuild-index", "rebuild search index"),
                ];
                let filtered: Vec<render::WhichKeyEntry> = commands.iter()
                    .filter(|(cmd, _)| input.is_empty() || cmd.starts_with(input))
                    .map(|(cmd, label)| render::WhichKeyEntry {
                        key: cmd.to_string(),
                        label: label.to_string(),
                        is_group: false,
                    })
                    .collect();
                if !filtered.is_empty() {
                    Some(render::WhichKeyFrame {
                        entries: filtered,
                        prefix: format!(":{input}"),
                        context: render::WhichKeyContext::CommandLine,
                    })
                } else {
                    None
                }
            } else if self.leader_keys.len() > 1 {
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
                // Vim grammar which-key: show motions/text objects when an operator is pending
                let pending = self.vim_state.pending_keys();
                let op_char = match pending {
                    "d" => Some("d"),
                    "c" => Some("c"),
                    "y" => Some("y"),
                    ">" => Some(">"),
                    "<" => Some("<"),
                    _ => None,
                };
                if let Some(op) = op_char {
                    let op_name = match op {
                        "d" => "delete",
                        "c" => "change",
                        "y" => "yank",
                        ">" => "indent",
                        "<" => "dedent",
                        _ => op,
                    };
                    let mut entries = vec![
                        // Motions
                        render::WhichKeyEntry { key: "w".into(), label: "word".into(), is_group: false },
                        render::WhichKeyEntry { key: "b".into(), label: "back word".into(), is_group: false },
                        render::WhichKeyEntry { key: "e".into(), label: "end of word".into(), is_group: false },
                        render::WhichKeyEntry { key: "$".into(), label: "end of line".into(), is_group: false },
                        render::WhichKeyEntry { key: "0".into(), label: "start of line".into(), is_group: false },
                        render::WhichKeyEntry { key: "j".into(), label: "line down".into(), is_group: false },
                        render::WhichKeyEntry { key: "k".into(), label: "line up".into(), is_group: false },
                        render::WhichKeyEntry { key: "gg".into(), label: "top of file".into(), is_group: false },
                        render::WhichKeyEntry { key: "G".into(), label: "end of file".into(), is_group: false },
                        render::WhichKeyEntry { key: "%".into(), label: "matching bracket".into(), is_group: false },
                        render::WhichKeyEntry { key: "f…".into(), label: "find char".into(), is_group: false },
                        render::WhichKeyEntry { key: "t…".into(), label: "till char".into(), is_group: false },
                        // Text objects
                        render::WhichKeyEntry { key: "iw".into(), label: "inner word".into(), is_group: false },
                        render::WhichKeyEntry { key: "aw".into(), label: "a word".into(), is_group: false },
                        render::WhichKeyEntry { key: "ip".into(), label: "inner paragraph".into(), is_group: false },
                        render::WhichKeyEntry { key: "ap".into(), label: "a paragraph".into(), is_group: false },
                        render::WhichKeyEntry { key: "il".into(), label: "inner link".into(), is_group: false },
                        render::WhichKeyEntry { key: "al".into(), label: "a link".into(), is_group: false },
                        render::WhichKeyEntry { key: "i#".into(), label: "inner tag".into(), is_group: false },
                        render::WhichKeyEntry { key: "a#".into(), label: "a tag".into(), is_group: false },
                        render::WhichKeyEntry { key: "i@".into(), label: "inner timestamp".into(), is_group: false },
                        render::WhichKeyEntry { key: "ih".into(), label: "inner heading".into(), is_group: false },
                        render::WhichKeyEntry { key: "ah".into(), label: "a heading".into(), is_group: false },
                    ];
                    entries.push(render::WhichKeyEntry {
                        key: format!("{op}"),
                        label: format!("{op_name} line ({op}{op})"),
                        is_group: false,
                    });
                    Some(render::WhichKeyFrame {
                        entries,
                        prefix: op.to_string(),
                        context: render::WhichKeyContext::VimOperator { operator: op.to_string() },
                    })
                } else {
                    None
                }
            }}, // which_key
            date_picker: None,
            dialog: None,
            notification: self.notifications.last().cloned(),
        }
    }

    fn render_buffer_lines(&self, buf: &buffer::Buffer) -> Vec<render::RenderedLine> {
        let range = self.viewport.visible_range();
        let mut lines = Vec::new();
        let line_count = buf.len_lines();

        // Compute line context (frontmatter/code block state) from the start
        // of the document up to and through the visible range.
        let scan_end = if range.end < line_count { range.end } else { line_count };
        let mut in_frontmatter = false;
        let mut in_code_block = false;
        let mut code_fence_lang: Option<String> = None;
        let mut seen_first_delimiter = false;

        for line_idx in 0..scan_end {
            let line_text = buf.line(line_idx).to_string();
            let trimmed = line_text.trim();

            // Track frontmatter: first line must be "---", closed by next "---"
            if line_idx == 0 && trimmed == "---" {
                in_frontmatter = true;
                seen_first_delimiter = true;
            } else if in_frontmatter && seen_first_delimiter && trimmed == "---" {
                // This line is still inside frontmatter (closing delimiter),
                // but after it we're out.
                if line_idx >= range.start {
                    let spans = self.parser.highlight_line(
                        &line_text,
                        &parser::traits::LineContext {
                            in_code_block: false,
                            in_frontmatter: true,
                            code_fence_lang: None,
                        },
                    );
                    lines.push(render::RenderedLine {
                        line_number: line_idx,
                        text: line_text,
                        spans,
                    });
                }
                in_frontmatter = false;
                continue;
            }

            // Track code fences
            if !in_frontmatter && trimmed.starts_with("```") {
                if in_code_block {
                    in_code_block = false;
                    code_fence_lang = None;
                } else {
                    in_code_block = true;
                    let lang = trimmed[3..].trim();
                    code_fence_lang = if lang.is_empty() { None } else { Some(lang.to_string()) };
                }
            }

            if line_idx >= range.start {
                let spans = self.parser.highlight_line(
                    &line_text,
                    &parser::traits::LineContext {
                        in_code_block,
                        in_frontmatter,
                        code_fence_lang: code_fence_lang.clone(),
                    },
                );
                lines.push(render::RenderedLine {
                    line_number: line_idx,
                    text: line_text,
                    spans,
                });
            }
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
                // If clamped landed on a trailing newline and there's an empty
                // last line after it, place the cursor there instead. This
                // matches Vim's behavior where the block cursor can sit on an
                // empty final line (e.g. after `G` or `j`).
                if rope.char(clamped) == '\n' && line + 1 < rope.len_lines() {
                    let next_line_start = rope.line_to_char(line + 1);
                    let next_line_len = rope.line(line + 1).len_chars();
                    if next_line_len == 0 && next_line_start == len {
                        return (line + 1, 0);
                    }
                }
                return (line, col);
            }
        }
        (0, 0)
    }

    /// Tick for timers, notifications, debounce
    pub fn tick(&mut self, now: std::time::Instant) {
        self.notifications.retain(|n| n.expires_at > now);
    }

    /// Update the terminal size (e.g. on terminal resize).
    pub fn resize(&mut self, height: usize, width: usize) {
        self.terminal_height = height as u16;
        self.terminal_width = width as u16;
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
        let Some(root) = &self.vault_root else { return Ok(()) };
        let session_path = root.join(".session.json");
        let buffers: Vec<session::SessionBuffer> = self.buffer_mgr.open_buffers().iter().map(|info| {
            let (cursor_line, cursor_col) = if Some(&info.page_id) == self.active_page.as_ref() {
                self.cursor_position()
            } else {
                (0, 0)
            };
            session::SessionBuffer {
                page_path: info.path.clone(),
                cursor_line,
                cursor_column: cursor_col,
                scroll_offset: self.viewport.first_visible_line,
                pane: 0,
            }
        }).collect();
        let state = session::SessionState {
            buffers,
            layout: session::SessionLayout::Leaf(0),
            active_pane: 0,
        };
        state.save(&session_path)
    }

    pub fn restore_session(&mut self) -> Result<(), error::BloomError> {
        let Some(root) = &self.vault_root else { return Ok(()) };
        let session_path = root.join(".session.json");
        if !session_path.exists() { return Ok(()) }
        let state = session::SessionState::load(&session_path)?;
        for buf_state in &state.buffers {
            if buf_state.page_path.exists() {
                if let Ok(content) = std::fs::read_to_string(&buf_state.page_path) {
                    let title = self.parser.parse_frontmatter(&content)
                        .and_then(|fm| fm.title)
                        .unwrap_or_else(|| buf_state.page_path.file_stem()
                            .unwrap_or_default().to_string_lossy().to_string());
                    let id = crate::uuid::generate_hex_id();
                    self.open_page_with_content(&id, &title, &buf_state.page_path, &content);
                    self.cursor = 0; // Restore cursor position via line/col
                    if let Some(buf) = self.buffer_mgr.get(&id) {
                        let rope = buf.text();
                        if buf_state.cursor_line < rope.len_lines() {
                            let line_start = rope.line_to_char(buf_state.cursor_line);
                            self.cursor = line_start + buf_state.cursor_column.min(
                                rope.line(buf_state.cursor_line).len_chars().saturating_sub(1)
                            );
                        }
                    }
                }
            }
        }
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

    // Cursor on empty last line (trailing newline)
    #[test]
    fn test_cursor_on_empty_last_line() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = crate::uuid::generate_hex_id();
        // File with trailing newline → ropey sees 3 lines (0: "hello\n", 1: "world\n", 2: "")
        editor.open_page_with_content(&id, "Test", std::path::Path::new("test.md"), "hello\nworld\n");
        // Move down twice: line 0 → line 1 → line 2 (empty last line)
        editor.handle_key(KeyEvent::char('j'));
        editor.handle_key(KeyEvent::char('j'));
        let frame = editor.render();
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

    // Vim-style undo: entire insert session is one undo unit
    #[test]
    fn test_undo_groups_insert_session() {
        let config = config::Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(&id, "Test", std::path::Path::new("test.md"), "");

        // Enter insert mode, type "abc", exit
        editor.handle_key(KeyEvent::char('i'));
        editor.handle_key(KeyEvent::char('a'));
        editor.handle_key(KeyEvent::char('b'));
        editor.handle_key(KeyEvent::char('c'));
        editor.handle_key(KeyEvent::esc());

        // Buffer should be "abc"
        let buf = editor.buffer_mgr.get(&editor.active_page.clone().unwrap()).unwrap();
        assert_eq!(buf.text().to_string(), "abc");

        // One undo should revert the entire insert session
        editor.handle_key(KeyEvent::char('u'));
        let buf = editor.buffer_mgr.get(&editor.active_page.clone().unwrap()).unwrap();
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
        editor.handle_key(KeyEvent::char('f')); // f (action)
        // Picker should now be open
        assert!(editor.picker_state.is_some());
        assert_eq!(editor.picker_state.as_ref().unwrap().title, "Find Page");
        let frame = editor.render();
        assert!(frame.picker.is_some());
    }

    // SPC shows which-key popup in render
    #[test]
    fn test_leader_spc_shows_which_key() {
        let mut cfg = config::Config::defaults();
        cfg.which_key_timeout_ms = 0; // instant for testing
        let mut editor = BloomEditor::new(cfg).unwrap();
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
        let frame = editor.render();
        assert!(frame.picker.is_some());
        let picker = frame.picker.unwrap();
        assert_eq!(picker.title, "Theme");
        assert_eq!(picker.results.len(), 15);

        // Move down — live preview changes theme (now typed as Char, goes to query)
        // Use Ctrl+J for navigation
        editor.handle_key(KeyEvent::ctrl('j'));
        assert_eq!(editor.theme().name, "bloom-dark-faded");

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
        editor.handle_key(KeyEvent::ctrl('j')); // bloom-dark-faded
        editor.handle_key(KeyEvent::ctrl('j')); // bloom-light
        assert_eq!(editor.theme().name, "bloom-light");

        // Enter confirms
        editor.handle_key(KeyEvent::enter());
        assert!(editor.picker_state.is_none());
        assert_eq!(editor.theme().name, "bloom-light");
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
        assert_eq!(editor.theme().name, "bloom-dark-faded");

        // Ctrl+P moves back up
        editor.handle_key(KeyEvent::ctrl('p'));
        assert_eq!(editor.theme().name, "bloom-dark");

        editor.handle_key(KeyEvent::esc());
    }
}
