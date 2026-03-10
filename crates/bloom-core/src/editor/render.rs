//! Render frame construction.
//!
//! Assembles a complete [`RenderFrame`](crate::render::RenderFrame) from current
//! editor state — pane layout, visible lines, cursor, status bar, picker overlay,
//! which-key drawer, and notifications. The frame is UI-agnostic; frontends
//! (TUI / GUI) consume it without any core logic.

use crate::editor::commands::EX_COMMANDS;
use crate::*;

impl BloomEditor {
    /// Produce the render frame. `width` and `height` are the actual terminal
    /// dimensions — used directly for layout computation so pane rects always
    /// tile the exact screen area.
    pub fn render(&mut self, width: u16, height: u16) -> render::RenderFrame {
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
                inline_menu: None,
                which_key: None,
                date_picker: None,
                dialog: None,
                notifications: Vec::new(),
                scrolloff: self.config.scrolloff,
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
        let has_pending = !self.leader_keys.is_empty() || !self.vim_state.pending_keys().is_empty();
        let timeout = std::time::Duration::from_millis(self.config.which_key_timeout_ms);
        let timed_out = self
            .pending_since
            .is_some_and(|since| since.elapsed() >= timeout);
        let show_wk = has_pending && (self.which_key_visible || timed_out);

        if show_wk && !self.which_key_visible {
            self.which_key_visible = true;
        }

        let wk_h = if show_wk {
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

        // Update each pane's viewport dimensions from its cell rect
        for rect in &pane_rects {
            if let Some(ps) = self.window_mgr.pane_state_mut(rect.pane_id) {
                ps.viewport.height = rect.content_height as usize;
                ps.viewport.width = rect.width as usize;
            }
        }

        // Ensure cursor is visible in the active pane (scrolls the viewport if needed)
        let (cursor_line, _cursor_col) = self.cursor_position();
        let scrolloff = self.config.scrolloff;
        self.viewport_mut()
            .ensure_visible_with_scrolloff(cursor_line, scrolloff);

        for rect in &pane_rects {
            let is_active = rect.pane_id == self.window_mgr.active_pane();
            let pane_state = self.window_mgr.pane_state(rect.pane_id);

            let (title, dirty, visible_lines, pane_cursor_line, pane_cursor_col, scroll_offset) =
                if let Some(ps) = pane_state {
                    if let Some(page_id) = &ps.page_id {
                        if let Some(buf) = self.buffer_mgr.get(page_id) {
                            let infos = self.buffer_mgr.open_buffers();
                            let title = infos
                                .iter()
                                .find(|i| i.page_id == *page_id)
                                .map(|i| i.title.clone())
                                .unwrap_or_default();
                            let lines = self.render_buffer_lines_with_viewport(buf, &ps.viewport);
                            let (cl, cc) =
                                Self::cursor_position_for(ps.cursor, buf, &self.vim_state);
                            (
                                title,
                                buf.is_dirty(),
                                lines,
                                cl,
                                cc,
                                ps.viewport.first_visible_line,
                            )
                        } else {
                            (String::new(), false, Vec::new(), 0, 0, 0)
                        }
                    } else {
                        (String::new(), false, Vec::new(), 0, 0, 0)
                    }
                } else {
                    (String::new(), false, Vec::new(), 0, 0, 0)
                };

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
                            "- [ ] Append task > ".to_string()
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
                        line: pane_cursor_line,
                        column: pane_cursor_col,
                        pending_keys: if !self.leader_keys.is_empty() {
                            self.leader_keys
                                .iter()
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
                        indexing: self.indexing,
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
                        line: pane_cursor_line,
                        column: pane_cursor_col,
                        pending_keys: String::new(),
                        recording_macro: None,
                        mcp: render::McpIndicator::Off,
                        indexing: self.indexing,
                    }),
                    mode: mode_str.to_string(),
                }
            };

            panes.push(render::PaneFrame {
                id: rect.pane_id,
                kind: render::PaneKind::Editor,
                visible_lines,
                cursor: render::CursorState {
                    line: pane_cursor_line,
                    column: pane_cursor_col,
                    shape: match self.vim_state.mode() {
                        vim::Mode::Normal => render::CursorShape::Block,
                        vim::Mode::Insert => render::CursorShape::Bar,
                        vim::Mode::Visual { .. } => render::CursorShape::Block,
                        vim::Mode::Command => render::CursorShape::Bar,
                    },
                },
                scroll_offset,
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
                    ap.picker
                        .results()
                        .into_iter()
                        .map(|item| render::PickerRow {
                            label: item.label.clone(),
                            middle: item.middle.clone(),
                            right: item.right.clone(),
                        })
                        .collect()
                };
                let preview = if below_min {
                    None
                } else {
                    ap.picker.selected().and_then(|item| {
                        // 1. Pre-set preview (e.g., search context, theme sample)
                        if item.preview_text.is_some() {
                            return item.preview_text.clone();
                        }
                        // 2. Try in-memory buffer (already open pages — free)
                        if let Some(page_id) = types::PageId::from_hex(&item.id) {
                            if let Some(buf) = self.buffer_mgr.get(&page_id) {
                                let text = buf.text();
                                let lines: Vec<_> =
                                    text.lines().take(20).map(|l| l.to_string()).collect();
                                if !lines.is_empty() {
                                    return Some(lines.join("\n"));
                                }
                            }
                            // 3. Read from disk via vault path + index metadata
                            if let Some(idx) = &self.index {
                                if let Some(meta) = idx.find_page_by_id(&page_id) {
                                    let full = self.vault_root.as_ref().map(|r| r.join(&meta.path));
                                    if let Some(path) = full {
                                        if let Ok(content) = std::fs::read_to_string(&path) {
                                            let preview: String = content
                                                .lines()
                                                .take(20)
                                                .collect::<Vec<_>>()
                                                .join("\n");
                                            return Some(preview);
                                        }
                                    }
                                }
                            }
                        }
                        None
                    })
                };
                Some(render::PickerFrame {
                    title: ap.title.clone(),
                    query: ap.query.clone(),
                    results,
                    selected_index: if below_min {
                        0
                    } else {
                        ap.picker.selected_index()
                    },
                    filters: Vec::new(),
                    preview,
                    total_count: ap.picker.total_count(),
                    filtered_count: if below_min {
                        0
                    } else {
                        ap.picker.filtered_count()
                    },
                    status_noun: ap.status_noun.clone(),
                    min_query_len: ap.min_query_len,
                    query_selected: ap.query_selected,
                    wide: matches!(
                        ap.kind,
                        keymap::dispatch::PickerKind::Search
                            | keymap::dispatch::PickerKind::Backlinks(_)
                            | keymap::dispatch::PickerKind::UnlinkedMentions(_)
                    ),
                })
            } else {
                None
            },
            inline_menu: if let Some(ic) = &self.inline_completion {
                let items = self.collect_inline_items(ic);
                let (cursor_line, cursor_col) = self.cursor_position();
                if !items.is_empty() {
                    let selected = ic.selected.min(items.len().saturating_sub(1));
                    Some(render::InlineMenuFrame {
                        items,
                        selected,
                        anchor: render::InlineMenuAnchor::Cursor {
                            line: cursor_line.saturating_sub(self.viewport().first_visible_line),
                            col: cursor_col + 5, // 5 = gutter width
                        },
                        hint: None,
                    })
                } else {
                    None
                }
            } else if matches!(self.vim_state.mode(), vim::Mode::Command) {
                let input = self.vim_state.pending_keys();

                // Detect argument completion: "theme <partial>"
                let (items, selected) = if let Some(arg_prefix) = input.strip_prefix("theme ") {
                    let theme_names = theme::THEME_NAMES;
                    let items: Vec<render::InlineMenuItem> = theme_names
                        .iter()
                        .filter(|name| arg_prefix.is_empty() || name.starts_with(arg_prefix))
                        .map(|name| render::InlineMenuItem {
                            id: None,
                            label: name.to_string(),
                            right: None,
                        })
                        .collect();
                    (items, 0)
                } else {
                    let items: Vec<render::InlineMenuItem> = EX_COMMANDS
                        .iter()
                        .filter(|(cmd, _)| input.is_empty() || cmd.starts_with(input))
                        .map(|(cmd, desc)| render::InlineMenuItem {
                            id: None,
                            label: cmd.to_string(),
                            right: Some(desc.to_string()),
                        })
                        .collect();
                    (items, 0)
                };

                if !items.is_empty() {
                    Some(render::InlineMenuFrame {
                        items,
                        selected,
                        anchor: render::InlineMenuAnchor::CommandLine,
                        hint: None,
                    })
                } else {
                    None
                }
            } else {
                None
            },
            which_key: {
                if !show_wk {
                    None
                } else if matches!(self.vim_state.mode(), vim::Mode::Command) {
                    // Command mode: use inline_menu instead (see inline_menu field below)
                    None
                } else if self.leader_keys.len() > 1 {
                    let lookup_keys: Vec<types::KeyEvent> = self.leader_keys[1..].to_vec();
                    match self.which_key_tree.lookup(&lookup_keys) {
                        which_key::WhichKeyLookup::Prefix(entries) => {
                            let prefix = self
                                .leader_keys
                                .iter()
                                .map(|k| k.to_string())
                                .collect::<Vec<_>>()
                                .join(" ");
                            Some(render::WhichKeyFrame {
                                entries: entries
                                    .into_iter()
                                    .map(|e| render::WhichKeyEntry {
                                        key: e.key,
                                        label: e.label,
                                        is_group: e.is_group,
                                    })
                                    .collect(),
                                prefix,
                                context: render::WhichKeyContext::Leader,
                            })
                        }
                        _ => None,
                    }
                } else if self.leader_keys.len() == 1 {
                    let entries = self.which_key_tree.lookup(&[]);
                    match entries {
                        which_key::WhichKeyLookup::Prefix(entries) => Some(render::WhichKeyFrame {
                            entries: entries
                                .into_iter()
                                .map(|e| render::WhichKeyEntry {
                                    key: e.key,
                                    label: e.label,
                                    is_group: e.is_group,
                                })
                                .collect(),
                            prefix: "SPC".to_string(),
                            context: render::WhichKeyContext::Leader,
                        }),
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
                            render::WhichKeyEntry {
                                key: "w".into(),
                                label: "word".into(),
                                is_group: false,
                            },
                            render::WhichKeyEntry {
                                key: "b".into(),
                                label: "back word".into(),
                                is_group: false,
                            },
                            render::WhichKeyEntry {
                                key: "e".into(),
                                label: "end of word".into(),
                                is_group: false,
                            },
                            render::WhichKeyEntry {
                                key: "$".into(),
                                label: "end of line".into(),
                                is_group: false,
                            },
                            render::WhichKeyEntry {
                                key: "0".into(),
                                label: "start of line".into(),
                                is_group: false,
                            },
                            render::WhichKeyEntry {
                                key: "j".into(),
                                label: "line down".into(),
                                is_group: false,
                            },
                            render::WhichKeyEntry {
                                key: "k".into(),
                                label: "line up".into(),
                                is_group: false,
                            },
                            render::WhichKeyEntry {
                                key: "gg".into(),
                                label: "top of file".into(),
                                is_group: false,
                            },
                            render::WhichKeyEntry {
                                key: "G".into(),
                                label: "end of file".into(),
                                is_group: false,
                            },
                            render::WhichKeyEntry {
                                key: "%".into(),
                                label: "matching bracket".into(),
                                is_group: false,
                            },
                            render::WhichKeyEntry {
                                key: "f…".into(),
                                label: "find char".into(),
                                is_group: false,
                            },
                            render::WhichKeyEntry {
                                key: "t…".into(),
                                label: "till char".into(),
                                is_group: false,
                            },
                            // Text objects
                            render::WhichKeyEntry {
                                key: "iw".into(),
                                label: "inner word".into(),
                                is_group: false,
                            },
                            render::WhichKeyEntry {
                                key: "aw".into(),
                                label: "a word".into(),
                                is_group: false,
                            },
                            render::WhichKeyEntry {
                                key: "ip".into(),
                                label: "inner paragraph".into(),
                                is_group: false,
                            },
                            render::WhichKeyEntry {
                                key: "ap".into(),
                                label: "a paragraph".into(),
                                is_group: false,
                            },
                            render::WhichKeyEntry {
                                key: "il".into(),
                                label: "inner link".into(),
                                is_group: false,
                            },
                            render::WhichKeyEntry {
                                key: "al".into(),
                                label: "a link".into(),
                                is_group: false,
                            },
                            render::WhichKeyEntry {
                                key: "i#".into(),
                                label: "inner tag".into(),
                                is_group: false,
                            },
                            render::WhichKeyEntry {
                                key: "a#".into(),
                                label: "a tag".into(),
                                is_group: false,
                            },
                            render::WhichKeyEntry {
                                key: "i@".into(),
                                label: "inner timestamp".into(),
                                is_group: false,
                            },
                            render::WhichKeyEntry {
                                key: "ih".into(),
                                label: "inner heading".into(),
                                is_group: false,
                            },
                            render::WhichKeyEntry {
                                key: "ah".into(),
                                label: "a heading".into(),
                                is_group: false,
                            },
                        ];
                        entries.push(render::WhichKeyEntry {
                            key: op.to_string(),
                            label: format!("{op_name} line ({op}{op})"),
                            is_group: false,
                        });
                        Some(render::WhichKeyFrame {
                            entries,
                            prefix: op.to_string(),
                            context: render::WhichKeyContext::VimOperator {
                                operator: op.to_string(),
                            },
                        })
                    } else {
                        None
                    }
                }
            }, // which_key
            date_picker: None,
            dialog: match &self.active_dialog {
                Some(ActiveDialog::FileChanged { path, selected, .. }) => {
                    let filename = path
                        .file_name()
                        .map(|f| f.to_string_lossy().to_string())
                        .unwrap_or_else(|| "file".to_string());
                    Some(render::DialogFrame {
                        message: format!("{} changed on disk. Reload?", filename),
                        choices: vec!["Reload".to_string(), "Keep buffer".to_string()],
                        selected: *selected,
                    })
                }
                None => None,
            },
            notifications: self
                .notifications
                .iter()
                .rev()
                .take(3)
                .rev()
                .cloned()
                .collect(),
            scrolloff: self.config.scrolloff,
        }
    }

    pub(crate) fn render_buffer_lines_with_viewport(
        &self,
        buf: &buffer::Buffer,
        viewport: &render::Viewport,
    ) -> Vec<render::RenderedLine> {
        let range = viewport.visible_range();
        let screen_height = viewport.height;
        let mut lines = Vec::new();
        let line_count = buf.len_lines();

        let mut in_frontmatter = false;
        let mut in_code_block = false;
        let mut code_fence_lang: Option<String> = None;
        let mut seen_first_delimiter = false;

        // Scan from line 0 for state tracking (frontmatter/code-block), but only
        // emit lines from range.start onward. Stop when the screen is full or we
        // run out of buffer lines.
        let mut line_idx = 0;
        while line_idx < line_count {
            // Stop once we've filled the screen (only count emitted lines).
            if line_idx >= range.start && lines.len() >= screen_height {
                break;
            }
            let line_text = buf.line(line_idx).to_string();
            let trimmed = line_text.trim().to_string();

            if line_idx == 0 && trimmed == "---" {
                in_frontmatter = true;
                seen_first_delimiter = true;
            } else if in_frontmatter && seen_first_delimiter && trimmed == "---" {
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
                        source: render::LineSource::Buffer(line_idx),
                        text: line_text,
                        spans,
                    });
                }
                in_frontmatter = false;
                line_idx += 1;
                continue;
            }

            if !in_frontmatter && (trimmed.starts_with("```") || trimmed.starts_with("~~~")) {
                if in_code_block {
                    in_code_block = false;
                    code_fence_lang = None;
                } else {
                    in_code_block = true;
                    let lang = trimmed
                        .trim_start_matches('`')
                        .trim_start_matches('~')
                        .trim();
                    code_fence_lang = if lang.is_empty() {
                        None
                    } else {
                        Some(lang.to_string())
                    };
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
                    source: render::LineSource::Buffer(line_idx),
                    text: line_text,
                    spans,
                });
            }
            line_idx += 1;
        }
        lines
    }

    /// Compute cursor (line, col) for a given char offset in a buffer.
    fn cursor_position_for(
        cursor: usize,
        buf: &buffer::Buffer,
        vim_state: &vim::VimState,
    ) -> (usize, usize) {
        let rope = buf.text();
        let len = rope.len_chars();
        if len == 0 {
            return (0, 0);
        }
        let clamped = if matches!(vim_state.mode(), vim::Mode::Insert) {
            cursor.min(len)
        } else {
            cursor.min(len.saturating_sub(1))
        };
        if clamped == len {
            let last_line = rope.len_lines().saturating_sub(1);
            let line_start = rope.line_to_char(last_line);
            let col = clamped - line_start;
            return (last_line, col);
        }
        let line = rope.char_to_line(clamped);
        let line_start = rope.line_to_char(line);
        let col = clamped - line_start;
        if rope.char(clamped) == '\n' && line + 1 < rope.len_lines() {
            let next_line_start = rope.line_to_char(line + 1);
            let next_line_len = rope.line(line + 1).len_chars();
            if next_line_len == 0 && next_line_start == len {
                return (line + 1, 0);
            }
        }
        (line, col)
    }

    pub(crate) fn cursor_position(&self) -> (usize, usize) {
        if let Some(page_id) = self.active_page() {
            if let Some(buf) = self.buffer_mgr.get(page_id) {
                return Self::cursor_position_for(self.cursor(), buf, &self.vim_state);
            }
        }
        (0, 0)
    }
}
