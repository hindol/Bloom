//! Render frame construction.
//!
//! Assembles a complete [`RenderFrame`](crate::render::RenderFrame) from current
//! editor state — pane layout, visible lines, cursor, status bar, picker overlay,
//! which-key drawer, and notifications. The frame is UI-agnostic; frontends
//! (TUI / GUI) consume it without any core logic.

use crate::editor::commands::EX_COMMANDS;
use crate::*;

/// Format a millisecond-epoch timestamp as a human-readable "time ago" string.
fn format_time_ago(accessed_ms: i64) -> String {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    let delta_s = ((now_ms - accessed_ms) / 1000).max(0);
    if delta_s < 60 {
        "just now".to_string()
    } else if delta_s < 3600 {
        format!("{}m ago", delta_s / 60)
    } else if delta_s < 86400 {
        format!("{}h ago", delta_s / 3600)
    } else if delta_s < 86400 * 7 {
        let days = delta_s / 86400;
        if days == 1 {
            "yesterday".to_string()
        } else {
            format!("{}d ago", days)
        }
    } else {
        format!("{}w ago", delta_s / (86400 * 7))
    }
}

const DASHBOARD_TIPS: &[&str] = &[
    "SPC l t opens a timeline of every note that links to the current page.",
    "[[ in Insert mode triggers the link picker \u{2014} type to search pages.",
    "SPC s t lets you browse all tags and filter by one.",
    "SPC w v splits the window \u{2014} edit two pages side by side.",
    "SPC u u opens page history \u{2014} recent undo and older checkpoints live together.",
    "#tag anywhere in text creates an inline tag \u{2014} searchable immediately.",
    "@due(2026-03-25) on a task makes it appear in the agenda (SPC a a).",
    "SPC H h shows the full history of the current page \u{2014} every version.",
    "SPC r s extracts a section into its own page \u{2014} with links preserved.",
    "[d and ]d hop between journal days \u{2014} the scrubber shows context.",
    "SPC T t opens the theme picker with live preview.",
    "SPC ? shows all commands \u{2014} fuzzy-searchable.",
    "SPC l y copies a link to the current page \u{2014} paste it anywhere.",
    "ah selects the entire heading section \u{2014} great for moving or deleting.",
    "Group related journal notes under a ## Heading, then SPC r s to extract them into their own page.",
    "SPC i y opens the kill ring \u{2014} browse and paste from your clipboard history.",
];

/// Convert the window manager's binary split tree into the render module's
/// serializable multi-child tree for GUI consumption.
fn wm_tree_to_render(tree: &window::LayoutTree) -> render::LayoutTree {
    match tree {
        window::LayoutTree::Leaf(id) => render::LayoutTree::Leaf(*id),
        window::LayoutTree::Split {
            direction,
            ratio,
            left,
            right,
        } => {
            let dir = match direction {
                window::SplitDirection::Vertical => render::SplitDirection::Vertical,
                window::SplitDirection::Horizontal => render::SplitDirection::Horizontal,
            };
            render::LayoutTree::Split {
                direction: dir,
                children: vec![
                    (*ratio, wm_tree_to_render(left)),
                    (1.0 - ratio, wm_tree_to_render(right)),
                ],
            }
        }
    }
}

/// Compute ghost text (the untyped suffix) for command-line completion.
fn command_ghost_text(input: &str) -> Option<String> {
    if input.is_empty() {
        return None;
    }
    // :theme <partial> → ghost is the rest of the theme name
    if let Some(arg) = input.strip_prefix("theme ") {
        let match_name = bloom_md::theme::THEME_NAMES
            .iter()
            .find(|n| n.starts_with(arg))?;
        let suffix = &match_name[arg.len()..];
        if suffix.is_empty() {
            return None;
        }
        return Some(suffix.to_string());
    }
    // Command prefix → ghost is the rest of the command name
    let (cmd, _) = EX_COMMANDS.iter().find(|(c, _)| c.starts_with(input))?;
    let suffix = &cmd[input.len()..];
    if suffix.is_empty() {
        return None;
    }
    Some(suffix.to_string())
}

impl BloomEditor {
    /// Build the dashboard frame shown when no buffers are open.
    fn build_dashboard_frame(&self) -> render::DashboardFrame {
        let recent_pages: Vec<render::DashboardRecentPage> = self
            .index
            .as_ref()
            .map(|idx| idx.frecency_top_with_time(5))
            .unwrap_or_default()
            .into_iter()
            .map(|(meta, accessed_ms)| render::DashboardRecentPage {
                title: meta.title,
                time_ago: format_time_ago(accessed_ms),
            })
            .collect();

        let open_tasks = self
            .index
            .as_ref()
            .map(|idx| idx.all_open_tasks().len())
            .unwrap_or(0);

        let tip_idx = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as usize % DASHBOARD_TIPS.len())
            .unwrap_or(0);

        render::DashboardFrame {
            recent_pages,
            open_tasks,
            pages_edited_today: 0,
            journal_entries_today: 0,
            tip: DASHBOARD_TIPS[tip_idx].to_string(),
        }
    }

    /// Build the which-key popup frame if applicable.
    fn build_which_key_frame(&self, show_wk: bool) -> Option<render::WhichKeyFrame> {
        if !show_wk {
            return None;
        }
        if matches!(self.vim_state.mode(), bloom_vim::Mode::Command) {
            return None;
        }
        if self.leader_keys.len() > 1 {
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
            match self.which_key_tree.lookup(&[]) {
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
            None
        }
    }

    fn build_active_status_bar(
        &self,
        mode_str: &str,
        title: &str,
        dirty: bool,
        pane_cursor_line: usize,
        pane_cursor_col: usize,
    ) -> render::StatusBarFrame {
        let content = if matches!(self.vim_state.mode(), bloom_vim::Mode::Command) {
            let raw_input = self.vim_state.pending_keys().to_string();
            let (input, ghost_text) = if self.search_active {
                let prefix = if self.search_forward { "/" } else { "?" };
                (format!("{}{}", prefix, raw_input), None)
            } else {
                let ghost = command_ghost_text(&raw_input);
                (raw_input.clone(), ghost)
            };
            render::StatusBarContent::CommandLine(render::CommandLineSlot {
                input,
                cursor_pos: self.vim_state.pending_keys().len()
                    + if self.search_active { 1 } else { 0 },
                ghost_text,
                error: None,
            })
        } else if let Some(qc) = &self.quick_capture {
            let prompt = match qc.kind {
                keymap::dispatch::QuickCaptureKind::Note => "📓 Append to journal > ".to_string(),
                keymap::dispatch::QuickCaptureKind::Task => "- [ ] Append task > ".to_string(),
                keymap::dispatch::QuickCaptureKind::Rename => "✏️  Rename page > ".to_string(),
            };
            render::StatusBarContent::QuickCapture(render::QuickCaptureSlot {
                prompt,
                input: qc.input.clone(),
                cursor_pos: qc.cursor_pos,
            })
        } else {
            render::StatusBarContent::Normal(render::NormalStatus {
                title: title.to_string(),
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

        let has_active_page = self.active_page().is_some();
        let show_jrnl = has_active_page
            && self.in_journal_mode
            && self.temporal_strip.is_none()
            && matches!(content, render::StatusBarContent::Normal(_))
            && matches!(self.vim_state.mode(), bloom_vim::Mode::Normal);

        let show_hist = has_active_page && self.temporal_strip.is_some();

        let mirror_hint = if !show_jrnl && !show_hist {
            self.mirror_hint_for_cursor()
        } else {
            None
        };

        render::StatusBarFrame {
            content,
            mode: if show_hist {
                "HIST".to_string()
            } else if show_jrnl {
                "JRNL".to_string()
            } else {
                mode_str.to_string()
            },
            right_hints: if show_hist {
                Some(format!(
                    "h/l:scrub  e:detail  r:restore  q:close  ·  {}",
                    self.history_durable_health()
                ))
            } else if show_jrnl {
                Some("↵:calendar  [d/]d".to_string())
            } else {
                mirror_hint
            },
        }
    }

    fn build_picker_frame(&self, height: u16) -> Option<render::PickerFrame> {
        let ap = self.picker_state.as_ref()?;
        let below_min = ap.query.len() < ap.min_query_len;
        let picker_height = (height as usize * 70 / 100).max(5);
        let max_visible = picker_height.saturating_sub(4);
        let all_results = if below_min {
            Vec::new()
        } else {
            ap.picker.results()
        };
        let total = all_results.len();
        let selected = if total == 0 {
            0
        } else {
            ap.picker.selected_index().min(total - 1)
        };
        let half = max_visible / 2;
        let window_start = if total <= max_visible || selected <= half {
            0
        } else if selected + half >= total {
            total.saturating_sub(max_visible)
        } else {
            selected - half
        };
        let window_end = (window_start + max_visible).min(total);

        let results: Vec<render::PickerRow> = all_results[window_start..window_end]
            .iter()
            .map(|item| render::PickerRow {
                label: item.label.clone(),
                middle: item.middle.clone(),
                right: item.right.clone(),
            })
            .collect();

        let preview = if below_min {
            None
        } else {
            ap.picker.selected().and_then(|item| {
                if item.preview_text.is_some() {
                    return item.preview_text.clone();
                }
                if let Some(page_id) = types::PageId::from_hex(&item.id) {
                    if let Some(buf) = self.writer.buffers().get(&page_id) {
                        let text = buf.text();
                        let lines: Vec<_> = text.lines().take(20).map(|l| l.to_string()).collect();
                        if !lines.is_empty() {
                            return Some(lines.join("\n"));
                        }
                    }
                    if let Some(idx) = &self.index {
                        if let Some(meta) = idx.find_page_by_id(&page_id) {
                            let full = self.vault_root.as_ref().map(|r| r.join(&meta.path));
                            if let Some(path) = full {
                                if let Ok(content) = std::fs::read_to_string(&path) {
                                    return Some(
                                        content.lines().take(20).collect::<Vec<_>>().join("\n"),
                                    );
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
            selected_index: selected.saturating_sub(window_start),
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
    }

    fn build_inline_menu_frame(&self) -> Option<render::InlineMenuFrame> {
        if let Some(ic) = &self.inline_completion {
            let items = self.collect_inline_items(ic);
            let (cursor_line, cursor_col) = self.cursor_position();
            if !items.is_empty() {
                let selected = ic.selected.min(items.len().saturating_sub(1));
                Some(render::InlineMenuFrame {
                    items,
                    selected,
                    anchor: render::InlineMenuAnchor::Cursor {
                        line: cursor_line.saturating_sub(self.viewport().first_visible_line),
                        col: cursor_col + 5,
                    },
                    hint: None,
                })
            } else {
                None
            }
        } else if matches!(self.vim_state.mode(), bloom_vim::Mode::Command) {
            let input = self.vim_state.pending_keys();
            let (items, selected) = if let Some(arg_prefix) = input.strip_prefix("theme ") {
                let items: Vec<render::InlineMenuItem> = bloom_md::theme::THEME_NAMES
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
        } else if let Some(mm) = &self.mirror_menu {
            let items: Vec<render::InlineMenuItem> = mm
                .items
                .iter()
                .map(|item| render::InlineMenuItem {
                    id: Some(item.page_id.to_hex()),
                    label: item.title.clone(),
                    right: Some(format!("L{}", item.line + 1)),
                })
                .collect();
            Some(render::InlineMenuFrame {
                items,
                selected: mm.selected,
                anchor: render::InlineMenuAnchor::Cursor {
                    line: mm
                        .cursor_line
                        .saturating_sub(self.viewport().first_visible_line),
                    col: mm.cursor_col + 5,
                },
                hint: Some("🪞 mirrors".to_string()),
            })
        } else {
            None
        }
    }

    fn build_dialog_frame(&self) -> Option<render::DialogFrame> {
        match &self.active_dialog {
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
            Some(ActiveDialog::DeletePage { page_id, selected }) => {
                let title = self
                    .writer
                    .buffers()
                    .info(page_id)
                    .map(|i| i.title.clone())
                    .unwrap_or_else(|| "this page".to_string());
                Some(render::DialogFrame {
                    message: format!("Delete \"{}\"? This cannot be undone.", title),
                    choices: vec!["Cancel".to_string(), "Delete".to_string()],
                    selected: *selected,
                })
            }
            None => None,
        }
    }

    fn visible_notifications(&self) -> Vec<render::Notification> {
        self.notifications
            .iter()
            .rev()
            .take(3)
            .rev()
            .cloned()
            .collect()
    }

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
                    total_lines: 0,
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
                context_strip: None,
                temporal_strip: None,
                dialog: None,
                view: None,
                notifications: Vec::new(),
                scrolloff: self.config.scrolloff,
                word_wrap: self.config.word_wrap,
                wrap_indicator: self.config.wrap_indicator.clone(),
                theme_name: self.active_theme.name.to_string(),
                layout_tree: render::LayoutTree::Leaf(types::PaneId(0)),
                clipboard_text: None,
            };
        }

        let mut panes = Vec::new();

        let mode_str = match self.vim_state.mode() {
            bloom_vim::Mode::Normal => "NORMAL",
            bloom_vim::Mode::Insert => "INSERT",
            bloom_vim::Mode::Visual { .. } => "VISUAL",
            bloom_vim::Mode::Command => "COMMAND",
        };

        // Compute pane rects from the core layout engine.
        // Reserve space for the which-key drawer only for leader key sequences.
        // Vim operator pending (d, c, y, [, ]) uses an overlay popup — no space needed.
        // Command mode uses the inline menu — no space needed.
        let has_leader_pending = !self.leader_keys.is_empty();
        let timeout = std::time::Duration::from_millis(self.config.which_key_timeout_ms);
        let timed_out = self
            .pending_since
            .is_some_and(|since| since.elapsed() >= timeout);
        let show_wk = has_leader_pending && (self.which_key_visible || timed_out);

        let wk_h = if show_wk {
            let col_width = 24u16;
            let cols = (width.saturating_sub(4) / col_width).max(1);
            let entry_count = 12u16;
            let rows_needed = entry_count.div_ceil(cols);
            (rows_needed + 2).min(height / 3).max(3)
        } else {
            0
        };
        let ts_h = self
            .temporal_strip
            .as_ref()
            .map(|ts| ts.drawer_height())
            .unwrap_or(0);
        let drawer_h = wk_h.max(ts_h);
        let pane_area_h = height.saturating_sub(drawer_h);
        let pane_rects = self.window_mgr.compute_pane_rects(width, pane_area_h);

        // Dashboard: when no buffers are open, show the dashboard pane.
        if self.writer.buffers().open_buffers().is_empty() {
            let dashboard = self.build_dashboard_frame();
            let which_key = self.build_which_key_frame(show_wk);
            let first_rect = pane_rects.first();
            return render::RenderFrame {
                panes: vec![render::PaneFrame {
                    id: first_rect.map(|r| r.pane_id).unwrap_or(types::PaneId(0)),
                    kind: render::PaneKind::Dashboard(dashboard),
                    visible_lines: Vec::new(),
                    cursor: render::CursorState::default(),
                    scroll_offset: 0,
                    total_lines: 0,
                    is_active: true,
                    title: String::from("Dashboard"),
                    dirty: false,
                    status_bar: self.build_active_status_bar(mode_str, "Dashboard", false, 0, 0),
                    rect: first_rect
                        .map(|r| render::PaneRectFrame {
                            x: r.x,
                            y: r.y,
                            width: r.width,
                            content_height: r.content_height,
                            total_height: r.height,
                        })
                        .unwrap_or_default(),
                }],
                maximized: false,
                hidden_pane_count: 0,
                picker: self.build_picker_frame(height),
                inline_menu: self.build_inline_menu_frame(),
                which_key,
                date_picker: self.build_date_picker_frame(),
                context_strip: self.build_context_strip(),
                temporal_strip: self.build_temporal_strip_frame(),
                dialog: self.build_dialog_frame(),
                view: None,
                notifications: self.visible_notifications(),
                scrolloff: self.config.scrolloff,
                word_wrap: self.config.word_wrap,
                wrap_indicator: self.config.wrap_indicator.clone(),
                theme_name: self.active_theme.name.to_string(),
                layout_tree: render::LayoutTree::Leaf(
                    first_rect.map(|r| r.pane_id).unwrap_or(types::PaneId(0)),
                ),
                clipboard_text: self.pending_clipboard.take(),
            };
        }

        // Layout is computed above; now rendering is read-only below.

        for rect in &pane_rects {
            let is_active = rect.pane_id == self.window_mgr.active_pane();
            let pane_state = self.window_mgr.pane_state(rect.pane_id);

            // Special pane kinds get their own render frames.
            let win_kind = self.window_mgr.pane_kind(rect.pane_id);
            if matches!(win_kind, Some(window::PaneKind::PageHistory)) {
                panes.push(self.build_page_history_pane_frame(rect, is_active, mode_str));
                continue;
            }

            let (
                title,
                dirty,
                visible_lines,
                pane_cursor_line,
                pane_cursor_col,
                scroll_offset,
                buf_total_lines,
            ): (String, bool, Vec<_>, usize, usize, usize, usize) = if let Some(ps) = pane_state {
                if let Some(page_id) = &ps.page_id {
                    if let Some(buf) = self.writer.buffers().get(page_id) {
                        let title = self
                            .writer
                            .buffers()
                            .info(page_id)
                            .map(|i| i.title.clone())
                            .unwrap_or_default();
                        let dirty = !self.writer.buffers().is_read_only(page_id) && buf.is_dirty();
                        let is_md = self
                            .writer
                            .buffers()
                            .info(page_id)
                            .map(|i| i.path.extension().and_then(|e| e.to_str()) == Some("md"))
                            .unwrap_or(true);
                        let lines = self.render_buffer_lines_with_viewport(
                            buf,
                            &ps.viewport,
                            is_md,
                            page_id,
                        );
                        let (cl, cc) = Self::cursor_position_for(
                            buf.cursor(ps.cursor_idx),
                            buf,
                            &self.vim_state,
                        );
                        (
                            title,
                            dirty,
                            lines,
                            cl,
                            cc,
                            ps.viewport.first_visible_line,
                            buf.len_lines(),
                        )
                    } else {
                        (String::new(), false, Vec::new(), 0, 0, 0, 0)
                    }
                } else {
                    (String::new(), false, Vec::new(), 0, 0, 0, 0)
                }
            } else {
                (String::new(), false, Vec::new(), 0, 0, 0, 0)
            };

            // Build per-pane status bar
            let status_bar = if is_active {
                self.build_active_status_bar(
                    mode_str,
                    &title,
                    dirty,
                    pane_cursor_line,
                    pane_cursor_col,
                )
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
                    right_hints: None,
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
                        bloom_vim::Mode::Normal => render::CursorShape::Block,
                        bloom_vim::Mode::Insert => render::CursorShape::Bar,
                        bloom_vim::Mode::Visual { .. } => render::CursorShape::Block,
                        bloom_vim::Mode::Command => render::CursorShape::Bar,
                    },
                },
                scroll_offset,
                total_lines: buf_total_lines,
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
            picker: self.build_picker_frame(height),
            inline_menu: self.build_inline_menu_frame(),
            which_key: {
                if !show_wk {
                    None
                } else if matches!(self.vim_state.mode(), bloom_vim::Mode::Command) {
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
            date_picker: self.build_date_picker_frame(),
            context_strip: self.build_context_strip(),
            temporal_strip: self.build_temporal_strip_frame(),
            dialog: self.build_dialog_frame(),
            view: None,
            notifications: self.visible_notifications(),
            scrolloff: self.config.scrolloff,
            word_wrap: self.config.word_wrap,
            wrap_indicator: self.config.wrap_indicator.clone(),
            theme_name: self.active_theme.name.to_string(),
            layout_tree: wm_tree_to_render(self.window_mgr.layout()),
            clipboard_text: self.pending_clipboard.take(),
        }
    }

    /// Build context strip for journal day-hopping (single horizontal line).
    /// Auto-hides 3 seconds after the last journal navigation.
    fn build_context_strip(&self) -> Option<render::ContextStripFrame> {
        if !self.in_journal_mode {
            return None;
        }
        // Auto-hide after 3 seconds of no journal navigation
        if let Some(nav_at) = self.journal_nav_at {
            if nav_at.elapsed() > std::time::Duration::from_secs(3) {
                return None;
            }
        } else {
            return None;
        }
        let journal = self.journal.as_ref()?;
        let store = self.note_store.as_ref()?;
        let current = self.last_viewed_journal_date?;

        let build_day = |d: chrono::NaiveDate| -> render::ContextStripDay {
            let label = format!("{} {}", d.format("%b %-d"), d.format("%a"));
            let path = journal.path_for_date(d);
            let (stats, first_line) = if path.exists() {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    let mut in_fm = false;
                    let body_lines: Vec<&str> = content
                        .lines()
                        .filter(|l| {
                            if l.trim() == "---" {
                                in_fm = !in_fm;
                                return false;
                            }
                            !in_fm
                        })
                        .filter(|l| !l.is_empty())
                        .collect();

                    let item_count = body_lines.len();
                    let tags: Vec<&str> = body_lines
                        .iter()
                        .flat_map(|l| l.split_whitespace())
                        .filter(|w| w.starts_with('#') && w.len() > 1)
                        .collect::<std::collections::BTreeSet<_>>()
                        .into_iter()
                        .take(3)
                        .collect();
                    let tag_str = if tags.is_empty() {
                        String::new()
                    } else {
                        format!(" · {}", tags.join(" "))
                    };
                    let stats = format!("{item_count} items{tag_str}");

                    // First unchecked task, or first body line
                    let first = body_lines
                        .iter()
                        .find(|l| l.contains("- [ ] "))
                        .or(body_lines.first())
                        .map(|l| l.trim().to_string())
                        .unwrap_or_default();

                    (stats, first)
                } else {
                    ("—".to_string(), String::new())
                }
            } else {
                ("no entries".to_string(), String::new())
            };
            render::ContextStripDay {
                label,
                stats,
                first_line,
            }
        };

        let prev = journal.prev_date(current, store);
        let next = journal.next_date(current, store);

        Some(render::ContextStripFrame {
            prev: prev.map(&build_day),
            current: build_day(current),
            next: next.map(&build_day),
        })
    }

    /// Build the date picker / calendar frame from current state.
    fn build_date_picker_frame(&self) -> Option<render::DatePickerFrame> {
        use chrono::{Datelike, NaiveDate};
        let dp = self.date_picker_state.as_ref()?;
        let selected = dp.selected_date;
        let today = journal::Journal::today();
        let year = selected.year();
        let month = selected.month();

        // Build month grid (weeks × 7 days, Monday-start ISO)
        let first = NaiveDate::from_ymd_opt(year, month, 1)?;
        let first_weekday = first.weekday().num_days_from_monday(); // Mon=0..Sun=6

        let days_in_month = if month == 12 {
            NaiveDate::from_ymd_opt(year + 1, 1, 1)
        } else {
            NaiveDate::from_ymd_opt(year, month + 1, 1)
        }
        .map(|d| d.pred_opt().map(|p| p.day()).unwrap_or(28))
        .unwrap_or(28);

        let mut month_view: Vec<Vec<Option<u32>>> = Vec::new();
        let mut week: Vec<Option<u32>> = vec![None; first_weekday as usize];
        for day in 1..=days_in_month {
            week.push(Some(day));
            if week.len() == 7 {
                month_view.push(week);
                week = Vec::new();
            }
        }
        if !week.is_empty() {
            while week.len() < 7 {
                week.push(None);
            }
            month_view.push(week);
        }

        // Find journal days in this month
        let journal_days = if let (Some(journal), Some(store)) = (&self.journal, &self.note_store) {
            journal
                .all_dates(store)
                .unwrap_or_default()
                .into_iter()
                .filter(|d| d.year() == year && d.month() == month)
                .map(|d| d.day())
                .collect()
        } else {
            Vec::new()
        };

        Some(render::DatePickerFrame {
            selected_date: selected,
            month_view,
            prompt: "Journal Calendar".to_string(),
            journal_days,
            today,
            year,
            month,
        })
    }

    fn build_page_history_pane_frame(
        &self,
        rect: &window::CellRect,
        is_active: bool,
        mode_str: &str,
    ) -> render::PaneFrame {
        let title = "History".to_string();

        // Build entries from stored history data.
        let entries: Vec<render::PageHistoryEntryFrame> = self
            .page_history_entries
            .as_ref()
            .map(|entries| {
                entries
                    .iter()
                    .map(|e| {
                        let dt =
                            chrono::DateTime::from_timestamp(e.timestamp, 0).unwrap_or_default();
                        let date = dt.format("%b %d, %H:%M").to_string();
                        let description = e.message.lines().next().unwrap_or("").to_string();
                        render::PageHistoryEntryFrame {
                            oid: e.oid.clone(),
                            date,
                            diff_stat: format!("{} file(s)", e.changed_files.len()),
                            description,
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        let total_versions = entries.len();
        let page_title = self
            .active_page()
            .and_then(|id| {
                self.writer
                    .buffers()
                    .open_buffers()
                    .iter()
                    .find(|b| b.page_id == *id)
                    .map(|b| b.title.clone())
            })
            .unwrap_or_else(|| "Unknown".into());

        let history_frame = render::PageHistoryFrame {
            page_title,
            entries,
            selected_index: self.page_history_selected,
            total_versions,
        };

        let status_bar = render::StatusBarFrame {
            content: render::StatusBarContent::Normal(render::NormalStatus {
                title: title.clone(),
                dirty: false,
                line: self.page_history_selected,
                column: 0,
                pending_keys: String::new(),
                recording_macro: None,
                mcp: render::McpIndicator::Off,
                indexing: false,
            }),
            mode: if is_active {
                "HISTORY".to_string()
            } else {
                mode_str.to_string()
            },
            right_hints: if is_active {
                Some("d:diff  r:restore  ↵:list".to_string())
            } else {
                None
            },
        };

        render::PaneFrame {
            id: rect.pane_id,
            kind: render::PaneKind::PageHistory(history_frame),
            visible_lines: vec![],
            cursor: render::CursorState::default(),
            scroll_offset: 0,
            total_lines: 0,
            is_active,
            title,
            dirty: false,
            status_bar,
            rect: render::PaneRectFrame {
                x: rect.x,
                y: rect.y,
                width: rect.width,
                content_height: rect.content_height,
                total_height: rect.height,
            },
        }
    }

    pub(crate) fn render_buffer_lines_with_viewport(
        &self,
        buf: &bloom_buffer::Buffer,
        viewport: &render::Viewport,
        is_markdown: bool,
        page_id: &types::PageId,
    ) -> Vec<render::RenderedLine> {
        let range = viewport.visible_range();
        let screen_height = viewport.height;
        let mut lines = Vec::new();
        let line_count = buf.len_lines();

        // Use ParseTree for O(1) context lookup when available.
        let parse_tree = self.writer.buffers().parse_tree(page_id);

        let mut line_idx = range.start;
        while line_idx < line_count && lines.len() < screen_height {
            let line_text = buf.line(line_idx).to_string();

            if line_idx >= range.start {
                // Get context from ParseTree (O(1)) or fall back to default.
                let ctx = if is_markdown {
                    parse_tree
                        .map(|pt| pt.context_before(line_idx))
                        .unwrap_or_default()
                } else {
                    bloom_md::parser::traits::LineContext::default()
                };

                let mut spans = if is_markdown {
                    self.span_cache.borrow_mut().get_or_highlight(
                        page_id,
                        line_idx,
                        &line_text,
                        &ctx,
                        &self.parser,
                    )
                } else {
                    vec![bloom_md::parser::traits::StyledSpan {
                        byte_range: 0..line_text.len(),
                        style: bloom_md::parser::traits::Style::Normal,
                    }]
                };
                // Overlay search highlights if a search pattern is active
                if let Some(ref pattern) = self.search_pattern {
                    let search_spans =
                        render::search_highlight::highlight_matches(&line_text, pattern);
                    if !search_spans.is_empty() {
                        spans = render::search_highlight::overlay_search_spans(
                            &spans,
                            &search_spans,
                            line_text.len(),
                        );
                    }
                }
                // Block history inline diff: replace spans on the block line
                if let Some(ts) = &self.temporal_strip {
                    if matches!(ts.mode, render::TemporalMode::BlockHistory)
                        && ts.block_line == Some(line_idx)
                    {
                        if let Some(item) = ts.items.get(ts.selected) {
                            if let Some(hist_line) = &item.content {
                                let diff_segs = word_diff(hist_line, &ts.current_content);
                                let mut diff_spans = Vec::new();
                                let mut pos = 0usize;
                                for seg in &diff_segs {
                                    let style = match seg.kind {
                                        render::DiffLineKind::Context => {
                                            bloom_md::parser::traits::Style::Normal
                                        }
                                        render::DiffLineKind::Added => {
                                            bloom_md::parser::traits::Style::DiffAdded
                                        }
                                        render::DiffLineKind::Removed => {
                                            bloom_md::parser::traits::Style::DiffRemoved
                                        }
                                        render::DiffLineKind::Modified => {
                                            bloom_md::parser::traits::Style::Normal
                                        }
                                    };
                                    let end = pos + seg.text.len();
                                    diff_spans.push(bloom_md::parser::traits::StyledSpan {
                                        byte_range: pos..end,
                                        style,
                                    });
                                    pos = end;
                                }
                                let diff_text: String =
                                    diff_segs.iter().map(|s| s.text.as_str()).collect();
                                lines.push(render::RenderedLine {
                                    source: render::LineSource::Buffer(line_idx),
                                    is_mirror: diff_text.contains(" ^="),
                                    text: diff_text,
                                    spans: diff_spans,
                                });
                                line_idx += 1;
                                continue;
                            }
                        }
                    }
                }

                lines.push(render::RenderedLine {
                    source: render::LineSource::Buffer(line_idx),
                    is_mirror: line_text.contains(" ^="),
                    text: line_text,
                    spans,
                });
            }
            line_idx += 1;
        }

        // Pre-warm span cache for ±20 lines beyond the viewport so that
        // scrolling hits cached spans instead of re-highlighting.
        if is_markdown {
            let overscan: usize = 20;
            let pre_start = range.start.saturating_sub(overscan);
            let pre_end = (range.start + screen_height + overscan).min(line_count);
            let mut sc = self.span_cache.borrow_mut();
            for pre_idx in pre_start..pre_end {
                // Skip lines we already highlighted above.
                if pre_idx >= range.start && pre_idx < range.start + lines.len() {
                    continue;
                }
                let pre_text = buf.line(pre_idx).to_string();
                let pre_ctx = parse_tree
                    .map(|pt| pt.context_before(pre_idx))
                    .unwrap_or_default();
                sc.get_or_highlight(page_id, pre_idx, &pre_text, &pre_ctx, &self.parser);
            }
        }

        lines
    }

    /// Compute cursor (line, col) for a given char offset in a buffer.
    fn cursor_position_for(
        cursor: usize,
        buf: &bloom_buffer::Buffer,
        vim_state: &bloom_vim::VimState,
    ) -> (usize, usize) {
        let rope = buf.text();
        let len = rope.len_chars();
        if len == 0 {
            return (0, 0);
        }
        let has_trailing_empty_line = len > 0 && rope.char(len - 1) == '\n';
        let max_pos =
            if matches!(vim_state.mode(), bloom_vim::Mode::Insert) || has_trailing_empty_line {
                len
            } else {
                len.saturating_sub(1)
            };
        let clamped = cursor.min(max_pos);
        if clamped == len {
            let last_line = rope.len_lines().saturating_sub(1);
            let line_start = rope.line_to_char(last_line);
            let col = clamped - line_start;
            return (last_line, col);
        }
        let line = rope.char_to_line(clamped);
        let line_start = rope.line_to_char(line);
        let col = clamped - line_start;

        // Debug: detect line past actual content
        let total_lines = rope.len_lines();
        if line >= total_lines {
            tracing::error!(
                cursor,
                clamped,
                len,
                line,
                total_lines,
                "cursor_position_for: line >= total_lines!"
            );
        }

        (line, col)
    }

    pub(crate) fn cursor_position(&self) -> (usize, usize) {
        if let Some(page_id) = self.active_page() {
            if let Some(buf) = self.writer.buffers().get(page_id) {
                return Self::cursor_position_for(self.cursor(), buf, &self.vim_state);
            }
        }
        (0, 0)
    }

    /// If the cursor is on a `^=` line, return a status bar hint with mirror count.
    fn mirror_hint_for_cursor(&self) -> Option<String> {
        let page_id = self.active_page()?;
        let doc = self.writer.buffers().document(page_id)?;
        let (cursor_line, _) =
            Self::cursor_position_for(self.cursor(), doc.buffer(), &self.vim_state);
        let bid = doc.block_id_at_line(cursor_line)?;
        if !bid.is_mirror {
            return None;
        }
        let count = self
            .index
            .as_ref()
            .map(|idx| idx.find_all_pages_by_block_id(&bid.id).len())
            .unwrap_or(0);
        if count < 2 {
            return None;
        }
        Some(format!("🪞 {} pages · SPC m: mirror", count))
    }

    fn build_temporal_strip_frame(&self) -> Option<render::TemporalStripFrame> {
        let ts = self.temporal_strip.as_ref()?;
        let selected_item = ts.items.get(ts.selected);
        let items: Vec<render::StripNode> = ts
            .items
            .iter()
            .map(|item| render::StripNode {
                label: item.label.clone(),
                detail: item.detail.clone(),
                kind: item.kind,
                branch_count: item.branch_count,
                skip: item.skip,
            })
            .collect();

        // For block history: compute word diff for inline preview on the block line.
        // For page history: compute full page diff for the overlay.
        let (preview_lines, block_line, block_diff_segments) = match ts.mode {
            render::TemporalMode::BlockHistory => {
                let segments = if let Some(item) = ts.items.get(ts.selected) {
                    if let Some(hist_line) = &item.content {
                        word_diff(hist_line, &ts.current_content)
                    } else {
                        Vec::new()
                    }
                } else {
                    Vec::new()
                };
                (Vec::new(), ts.block_line, segments)
            }
            _ => {
                let lines = if let Some(item) = ts.items.get(ts.selected) {
                    if let Some(hist_content) = &item.content {
                        compute_diff_lines(hist_content, &ts.current_content)
                    } else {
                        Vec::new()
                    }
                } else {
                    Vec::new()
                };
                (lines, None, Vec::new())
            }
        };

        let title = match ts.mode {
            render::TemporalMode::PageHistory => {
                // Use page title
                self.active_page()
                    .and_then(|id| {
                        self.writer
                            .buffers()
                            .open_buffers()
                            .iter()
                            .find(|b| b.page_id == *id)
                            .map(|b| b.title.clone())
                    })
                    .unwrap_or_else(|| "History".to_string())
            }
            render::TemporalMode::BlockHistory => "Block History".to_string(),
            render::TemporalMode::DayActivity => "Day Activity".to_string(),
        };

        let selected_summary = selected_item
            .map(|item| format!("{} · {}", temporal_kind_label(item.kind), item.summary))
            .unwrap_or_default();
        let selected_scope = selected_item
            .map(|item| {
                [
                    temporal_time_label(&item.time),
                    temporal_scope_label(&item.scope_summary),
                ]
                .join(" · ")
            })
            .unwrap_or_default();
        let selected_restore = selected_item
            .map(|item| format!("Restore: {}", temporal_restore_label(&item.restore_effect)))
            .unwrap_or_default();
        let selected_context = selected_item
            .and_then(temporal_context_label)
            .unwrap_or_default();
        let durable_health = self.history_durable_health();

        Some(render::TemporalStripFrame {
            items,
            selected: ts.selected,
            mode: ts.mode,
            compact: ts.compact,
            preview_lines,
            title,
            selected_summary,
            selected_scope,
            selected_restore,
            selected_context,
            durable_health,
            block_line,
            block_diff_segments,
        })
    }

    fn history_durable_health(&self) -> String {
        match self.durable_capture_status() {
            DurableCaptureStatus::Unsaved => "Durability: unsaved changes".to_string(),
            DurableCaptureStatus::SavedPendingDurable => {
                format!("Durability: saved, checkpoint pending{}", {
                    let count = self.durable_capture.pending_pages.len();
                    format!(" ({count} page{})", if count == 1 { "" } else { "s" })
                })
            }
            DurableCaptureStatus::Committing => "Durability: writing checkpoint".to_string(),
            DurableCaptureStatus::DurableCurrent => {
                let commit = self
                    .durable_capture
                    .last_successful_commit_oid
                    .as_deref()
                    .map(|oid| oid.chars().take(7).collect::<String>());
                let ago = self
                    .durable_capture
                    .last_successful_commit_at
                    .map(|timestamp_s| format_time_ago(timestamp_s * 1000));
                match (commit, ago) {
                    (Some(commit), Some(ago)) => {
                        format!("Durability: current · {commit} · {ago}")
                    }
                    (Some(commit), None) => format!("Durability: current · {commit}"),
                    _ => "Durability: current".to_string(),
                }
            }
            DurableCaptureStatus::DurableError => self
                .durable_capture
                .last_error
                .as_ref()
                .map(|err| format!("Durability: error · {err}"))
                .unwrap_or_else(|| "Durability: error".to_string()),
        }
    }
}

fn temporal_kind_label(kind: render::StripNodeKind) -> &'static str {
    match kind {
        render::StripNodeKind::UndoNode => "Undo node",
        render::StripNodeKind::GitCommit => "Checkpoint",
        render::StripNodeKind::LineageEvent => "Lineage event",
    }
}

fn temporal_time_label(time: &TemporalStopTime) -> String {
    match &time.absolute_label {
        Some(absolute) => format!("When: {} ({})", time.relative_label, absolute),
        None => format!("When: {}", time.relative_label),
    }
}

fn temporal_scope_label(scope: &TemporalScopeSummary) -> String {
    match scope {
        TemporalScopeSummary::CurrentPage => "Scope: current page".to_string(),
        TemporalScopeSummary::CurrentBlock => "Scope: current block".to_string(),
        TemporalScopeSummary::PageSet {
            count,
            includes_mirrors,
        } => format!(
            "Scope: {} page{}{}",
            count,
            if *count == 1 { "" } else { "s" },
            if *includes_mirrors {
                " incl. mirrors"
            } else {
                ""
            }
        ),
    }
}

fn temporal_restore_label(effect: &TemporalRestoreEffect) -> &'static str {
    match effect {
        TemporalRestoreEffect::RestoreUndoNode => "jump to this undo node",
        TemporalRestoreEffect::ReplaceBufferCreatesUndoNode => {
            "replace the current buffer and record an undo node"
        }
        TemporalRestoreEffect::ReplaceBlockLineCreatesUndoNode => {
            "replace this block line and record an undo node"
        }
    }
}

fn temporal_context_label(item: &TemporalItem) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(branch) = &item.branch {
        let status = match branch.status {
            TemporalBranchStatus::CurrentPath => "current path",
            TemporalBranchStatus::AlternatePath => "alternate path",
            TemporalBranchStatus::ForkNode => "fork point",
        };
        parts.push(format!("Branch: {} ({})", branch.summary, status));
    }
    if let Some(checkpoint) = &item.checkpoint {
        let reason = match checkpoint.reason {
            TemporalCheckpointReason::IdleTimeout => "idle timeout",
            TemporalCheckpointReason::MaxInterval => "max interval",
            TemporalCheckpointReason::SessionSave => "session save",
            TemporalCheckpointReason::Unknown => "checkpoint",
        };
        parts.push(format!(
            "Checkpoint: {} · {} page{}",
            reason,
            checkpoint.changed_pages,
            if checkpoint.changed_pages == 1 {
                ""
            } else {
                "s"
            }
        ));
    }
    if let Some(lineage) = &item.lineage {
        let event = match lineage.event {
            TemporalLineageEventKind::Moved => "moved",
            TemporalLineageEventKind::SplitSpawnedChild => "split; spawned child",
            TemporalLineageEventKind::SplitFromParent => "split from parent",
            TemporalLineageEventKind::MergedInto => "merged into survivor",
            TemporalLineageEventKind::MergedFrom => "merged from retired block",
        };
        let related = if lineage.related_ids.is_empty() {
            String::new()
        } else {
            format!(
                " · {}",
                lineage
                    .related_ids
                    .iter()
                    .map(|id| format!("^{id}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        parts.push(format!("Lineage: {event}{related}"));
        if let Some(page_context) = &lineage.page_context {
            let from = page_context.from_page.as_deref().unwrap_or("?");
            let to = page_context.to_page.as_deref().unwrap_or("?");
            parts.push(format!("Pages: {from} → {to}"));
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" · "))
    }
}

/// Compute diff between historical and current content using `similar`'s
/// `DiffOp` API. `Replace` ops get word-level diff; `Insert`/`Delete` are
/// pure additions/removals. Line numbers track both old and new sides.
fn compute_diff_lines(historical: &str, current: &str) -> Vec<render::DiffLine> {
    use similar::{DiffOp, TextDiff};

    let diff = TextDiff::from_lines(historical, current);
    let old_lines: Vec<&str> = historical.lines().collect();
    let new_lines: Vec<&str> = current.lines().collect();
    let mut result = Vec::new();

    for op in diff.ops() {
        match *op {
            DiffOp::Equal {
                old_index,
                new_index,
                len,
            } => {
                for i in 0..len {
                    let text = old_lines.get(old_index + i).unwrap_or(&"").to_string();
                    result.push(render::DiffLine {
                        segments: vec![render::DiffSegment {
                            text,
                            kind: render::DiffLineKind::Context,
                        }],
                        kind: render::DiffLineKind::Context,
                        old_line: Some(old_index + i + 1),
                        new_line: Some(new_index + i + 1),
                    });
                }
            }
            DiffOp::Delete {
                old_index, old_len, ..
            } => {
                for i in 0..old_len {
                    let text = old_lines.get(old_index + i).unwrap_or(&"").to_string();
                    result.push(render::DiffLine {
                        segments: vec![render::DiffSegment {
                            text,
                            kind: render::DiffLineKind::Removed,
                        }],
                        kind: render::DiffLineKind::Removed,
                        old_line: Some(old_index + i + 1),
                        new_line: None,
                    });
                }
            }
            DiffOp::Insert {
                new_index, new_len, ..
            } => {
                for i in 0..new_len {
                    let text = new_lines.get(new_index + i).unwrap_or(&"").to_string();
                    result.push(render::DiffLine {
                        segments: vec![render::DiffSegment {
                            text,
                            kind: render::DiffLineKind::Added,
                        }],
                        kind: render::DiffLineKind::Added,
                        old_line: None,
                        new_line: Some(new_index + i + 1),
                    });
                }
            }
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => {
                let paired = old_len.min(new_len);
                for i in 0..paired {
                    let old_text = old_lines.get(old_index + i).unwrap_or(&"");
                    let new_text = new_lines.get(new_index + i).unwrap_or(&"");

                    // If lines are <40% similar, show as separate remove+add
                    // (cleaner than noisy word-level diff on mostly-changed lines).
                    let similarity = similar::TextDiff::from_chars(*old_text, *new_text).ratio();
                    if similarity < 0.4 {
                        result.push(render::DiffLine {
                            segments: vec![render::DiffSegment {
                                text: old_text.to_string(),
                                kind: render::DiffLineKind::Removed,
                            }],
                            kind: render::DiffLineKind::Removed,
                            old_line: Some(old_index + i + 1),
                            new_line: None,
                        });
                        result.push(render::DiffLine {
                            segments: vec![render::DiffSegment {
                                text: new_text.to_string(),
                                kind: render::DiffLineKind::Added,
                            }],
                            kind: render::DiffLineKind::Added,
                            old_line: None,
                            new_line: Some(new_index + i + 1),
                        });
                    } else {
                        let segments = word_diff(old_text, new_text);
                        result.push(render::DiffLine {
                            segments,
                            kind: render::DiffLineKind::Modified,
                            old_line: Some(old_index + i + 1),
                            new_line: Some(new_index + i + 1),
                        });
                    }
                }
                for i in paired..old_len {
                    let text = old_lines.get(old_index + i).unwrap_or(&"").to_string();
                    result.push(render::DiffLine {
                        segments: vec![render::DiffSegment {
                            text,
                            kind: render::DiffLineKind::Removed,
                        }],
                        kind: render::DiffLineKind::Removed,
                        old_line: Some(old_index + i + 1),
                        new_line: None,
                    });
                }
                for i in paired..new_len {
                    let text = new_lines.get(new_index + i).unwrap_or(&"").to_string();
                    result.push(render::DiffLine {
                        segments: vec![render::DiffSegment {
                            text,
                            kind: render::DiffLineKind::Added,
                        }],
                        kind: render::DiffLineKind::Added,
                        old_line: None,
                        new_line: Some(new_index + i + 1),
                    });
                }
            }
        }
    }
    result
}
/// Word-level diff using similar crate.
/// Returns segments: Context (shared), Added (in current), Removed (in historical).
fn word_diff(historical: &str, current: &str) -> Vec<render::DiffSegment> {
    use similar::{ChangeTag, TextDiff};

    let diff = TextDiff::from_words(historical, current);
    diff.iter_all_changes()
        .map(|change| {
            let kind = match change.tag() {
                ChangeTag::Equal => render::DiffLineKind::Context,
                ChangeTag::Insert => render::DiffLineKind::Added,
                ChangeTag::Delete => render::DiffLineKind::Removed,
            };
            render::DiffSegment {
                text: change.value().to_string(),
                kind,
            }
        })
        .collect()
}
