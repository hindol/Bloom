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
            config,
        })
    }

    /// Initialize with a vault path — sets up index, journal, template engine
    pub fn init_vault(&mut self, vault_root: &std::path::Path) -> Result<(), error::BloomError> {
        let index_path = vault_root.join(".bloom").join("index.db");
        self.index = Some(index::Index::open(&index_path)?);
        self.journal = Some(journal::Journal::new(vault_root));
        let templates_dir = vault_root.join(".bloom").join("templates");
        self.template_engine = Some(template::TemplateEngine::new(&templates_dir));
        Ok(())
    }

    /// Process a key event
    pub fn handle_key(&mut self, key: types::KeyEvent) -> Vec<keymap::dispatch::Action> {
        // Check platform shortcuts first
        if let Some(action) = keymap::platform_shortcut(&key) {
            return vec![action];
        }

        // If picker is open, route to picker
        if self.picker_state.is_some() {
            return self.handle_picker_key(&key);
        }

        // If quick capture is open
        if self.quick_capture.is_some() {
            return self.handle_quick_capture_key(&key);
        }

        // Vim processing
        if let Some(buf) = self.active_page.as_ref().and_then(|id| self.buffer_mgr.get(id)) {
            let action = self.vim_state.process_key(key.clone(), buf, self.cursor);
            return self.translate_vim_action(action);
        }

        vec![keymap::dispatch::Action::Noop]
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
            vim::VimAction::Command(_cmd) => {
                vec![keymap::dispatch::Action::Noop]
            }
            vim::VimAction::Pending => vec![keymap::dispatch::Action::Noop],
            vim::VimAction::Unhandled => vec![keymap::dispatch::Action::Noop],
            vim::VimAction::Composite(actions) => actions
                .into_iter()
                .flat_map(|a| self.translate_vim_action(a))
                .collect(),
        }
    }

    /// Produce the render frame
    pub fn render(&self) -> render::RenderFrame {
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
                title,
                dirty,
                status_bar: render::StatusBar {
                    mode: mode_str.to_string(),
                    filename: self
                        .active_page
                        .as_ref()
                        .map(|id| id.to_hex())
                        .unwrap_or_default(),
                    dirty,
                    line: cursor_line,
                    column: cursor_col,
                    pending_keys: self.vim_state.pending_keys().to_string(),
                    recording_macro: if self.vim_state.is_recording() {
                        Some('q')
                    } else {
                        None
                    },
                    mcp_status: None,
                },
            });
        }

        render::RenderFrame {
            panes,
            maximized: self.window_mgr.is_maximized(),
            hidden_pane_count: self.window_mgr.hidden_pane_count(),
            picker: None,
            which_key: None,
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
            let render_spans = spans
                .into_iter()
                .map(|s| render::StyledSpan {
                    range: s.range,
                    style: convert_style(s.style),
                })
                .collect();
            lines.push(render::RenderedLine {
                line_number: line_idx,
                spans: render_spans,
            });
        }
        lines
    }

    fn cursor_position(&self) -> (usize, usize) {
        if let Some(page_id) = &self.active_page {
            if let Some(buf) = self.buffer_mgr.get(page_id) {
                let rope = buf.text();
                let line =
                    rope.char_to_line(self.cursor.min(rope.len_chars().saturating_sub(1)));
                let line_start = rope.line_to_char(line);
                let col = self.cursor.saturating_sub(line_start);
                return (line, col);
            }
        }
        (0, 0)
    }

    /// Tick for timers, notifications, debounce
    pub fn tick(&mut self, now: std::time::Instant) {
        self.notifications.retain(|n| n.expires_at > now);
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

fn convert_style(s: parser::traits::Style) -> render::Style {
    match s {
        parser::traits::Style::Normal => render::Style::Normal,
        parser::traits::Style::Heading { level } => render::Style::Heading { level },
        parser::traits::Style::Bold => render::Style::Bold,
        parser::traits::Style::Italic => render::Style::Italic,
        parser::traits::Style::Code => render::Style::Code,
        parser::traits::Style::CodeBlock => render::Style::CodeBlock,
        parser::traits::Style::Link => render::Style::Link,
        parser::traits::Style::Tag => render::Style::Tag,
        parser::traits::Style::Timestamp => render::Style::Timestamp,
        parser::traits::Style::BlockId => render::Style::BlockId,
        parser::traits::Style::ListMarker => render::Style::ListMarker,
        parser::traits::Style::CheckboxUnchecked => render::Style::CheckboxUnchecked,
        parser::traits::Style::CheckboxChecked => render::Style::CheckboxChecked,
        parser::traits::Style::Frontmatter => render::Style::Frontmatter,
        parser::traits::Style::BrokenLink => render::Style::BrokenLink,
        parser::traits::Style::SyntaxNoise => render::Style::SyntaxNoise,
    }
}
