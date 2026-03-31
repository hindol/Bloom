//! Ex-command dispatch and action registration.
//!
//! Defines all `:` commands (`:w`, `:q`, `:split`, `:theme`, …) and maps
//! action IDs from the which-key tree to editor [`Action`](crate::keymap::dispatch::Action)
//! sequences. Also handles inline quick-capture and agenda toggle commands.

use crate::*;

/// All registered `:` commands with their descriptions.
pub(crate) const EX_COMMANDS: &[(&str, &str)] = &[
    ("w", "write (save)"),
    ("write", "write (save)"),
    ("q", "quit"),
    ("quit", "quit"),
    ("qa", "quit all"),
    ("wq", "write and quit"),
    ("x", "write and quit"),
    ("q!", "quit without saving"),
    ("e", "edit (find page)"),
    ("edit", "edit (find page)"),
    ("sp", "split horizontal"),
    ("split", "split horizontal"),
    ("vs", "vsplit vertical"),
    ("vsplit", "vsplit vertical"),
    ("bd", "close buffer"),
    ("bdelete", "close buffer"),
    ("theme", "switch theme"),
    ("checkpoint", "create explicit checkpoint"),
    ("cp", "create explicit checkpoint"),
    ("rebuild-index", "rebuild search index"),
    ("stats", "show vault and index stats"),
    ("messages", "show notification history"),
    ("log", "open log file"),
    ("config", "open config file"),
];

impl BloomEditor {
    pub(crate) fn action_id_to_actions(
        &mut self,
        action_id: &str,
    ) -> Vec<keymap::dispatch::Action> {
        match action_id {
            "find_page" => vec![keymap::dispatch::Action::OpenPicker(
                keymap::dispatch::PickerKind::FindPage,
            )],
            "find_pages_only" => vec![keymap::dispatch::Action::OpenPicker(
                keymap::dispatch::PickerKind::PagesOnly,
            )],
            "switch_buffer" => vec![keymap::dispatch::Action::OpenPicker(
                keymap::dispatch::PickerKind::SwitchBuffer,
            )],
            "search" => vec![keymap::dispatch::Action::OpenPicker(
                keymap::dispatch::PickerKind::Search,
            )],
            "search_word_under_cursor" => {
                self.search_word_under_cursor();
                vec![keymap::dispatch::Action::Noop]
            }
            "search_tags" => vec![keymap::dispatch::Action::OpenPicker(
                keymap::dispatch::PickerKind::Tags,
            )],
            "journal_today" => {
                self.open_journal_today();
                self.in_journal_mode = true;
                self.journal_nav_at = Some(Instant::now());
                vec![keymap::dispatch::Action::Noop]
            }
            "journal_picker" => vec![keymap::dispatch::Action::OpenPicker(
                keymap::dispatch::PickerKind::Journal,
            )],
            "journal_calendar" => vec![keymap::dispatch::Action::OpenDatePicker(
                keymap::dispatch::DatePickerPurpose::JumpToJournal,
            )],
            "journal_append" => vec![keymap::dispatch::Action::QuickCapture(
                keymap::dispatch::QuickCaptureKind::Note,
            )],
            "journal_task" => vec![keymap::dispatch::Action::QuickCapture(
                keymap::dispatch::QuickCaptureKind::Task,
            )],
            "mirror_sever" => {
                self.mirror_sever();
                vec![keymap::dispatch::Action::Noop]
            }
            "mirror_goto" => {
                self.mirror_goto();
                vec![keymap::dispatch::Action::Noop]
            }
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
            "agenda" => {
                // Use the configured Agenda view if available, otherwise open legacy agenda
                if let Some(agenda_view) = self.config.views.iter().find(|v| v.name == "Agenda") {
                    self.open_named_view(agenda_view.clone());
                    vec![keymap::dispatch::Action::Noop]
                } else {
                    vec![keymap::dispatch::Action::OpenAgenda]
                }
            }
            "undo_tree" => vec![keymap::dispatch::Action::OpenUndoTree],
            "page_history" => vec![keymap::dispatch::Action::OpenPageHistory],
            "checkpoint" => vec![keymap::dispatch::Action::ExplicitCheckpoint],
            "block_history" => {
                self.open_block_history();
                vec![keymap::dispatch::Action::Noop]
            }
            "day_activity" => {
                self.push_notification(
                    "Day activity not yet implemented".into(),
                    render::NotificationLevel::Info,
                );
                vec![keymap::dispatch::Action::Noop]
            }
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
                if let Some(page_id) = self.active_page().cloned() {
                    if let Some(buf) = self.writer.buffers().get(&page_id) {
                        let text = buf.text().to_string();
                        if let Some(_fm) = self.parser.parse_frontmatter(&text) {
                            // Prompt would be ideal, but for now insert a placeholder tag
                            // The user types the tag name after #
                            self.insert_text_at_cursor("#");
                        }
                    }
                }
                vec![keymap::dispatch::Action::Noop]
            }
            "remove_tag" => {
                if let Some(page_id) = self.active_page().cloned() {
                    if let Some(buf) = self.writer.buffers().get(&page_id) {
                        let text = buf.text().to_string();
                        if let Some(fm) = self.parser.parse_frontmatter(&text) {
                            if !fm.tags.is_empty() {
                                let items: Vec<GenericPickerItem> = fm
                                    .tags
                                    .iter()
                                    .map(|t| GenericPickerItem {
                                        id: t.0.clone(),
                                        label: format!("#{}", t.0),
                                        middle: None,
                                        right: Some("remove".to_string()),
                                        preview_text: None,
                                        score_boost: 0,
                                    })
                                    .collect();
                                self.picker_state = Some(ActivePicker {
                                    kind: keymap::dispatch::PickerKind::Tags,
                                    action: PickerAction::Noop,
                                    picker: picker::Picker::new(items),
                                    title: "Remove Tag".to_string(),
                                    query: String::new(),
                                    status_noun: "tags".to_string(),
                                    min_query_len: 0,
                                    previous_theme: None,
                                    query_selected: false,
                                });
                            }
                        }
                    }
                }
                vec![keymap::dispatch::Action::Noop]
            }
            "insert_due" => {
                self.insert_text_at_cursor("@due()");
                self.set_cursor(self.cursor().saturating_sub(1));
                vec![keymap::dispatch::Action::Noop]
            }
            "insert_start" => {
                self.insert_text_at_cursor("@start()");
                self.set_cursor(self.cursor().saturating_sub(1));
                vec![keymap::dispatch::Action::Noop]
            }
            "insert_at" => {
                self.insert_text_at_cursor("@at()");
                self.set_cursor(self.cursor().saturating_sub(1));
                vec![keymap::dispatch::Action::Noop]
            }
            "kill_ring" => vec![keymap::dispatch::Action::OpenPicker(
                keymap::dispatch::PickerKind::KillRing,
            )],
            "search_backlinks" => {
                if let Some(id) = self.active_page().cloned() {
                    vec![keymap::dispatch::Action::OpenPicker(
                        keymap::dispatch::PickerKind::Backlinks(id),
                    )]
                } else {
                    vec![keymap::dispatch::Action::Noop]
                }
            }
            "search_unlinked" => {
                if let Some(id) = self.active_page().cloned() {
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
                if let Some(id) = self.active_page().cloned() {
                    vec![keymap::dispatch::Action::OpenTimeline(id)]
                } else {
                    vec![keymap::dispatch::Action::Noop]
                }
            }
            "backlinks" => {
                if let Some(id) = self.active_page().cloned() {
                    vec![keymap::dispatch::Action::OpenPicker(
                        keymap::dispatch::PickerKind::Backlinks(id),
                    )]
                } else {
                    vec![keymap::dispatch::Action::Noop]
                }
            }
            "rename_page" => {
                if let Some(page_id) = self.active_page().cloned() {
                    if self.writer.buffers().get(&page_id).is_some() {
                        let current_title = self
                            .writer
                            .buffers()
                            .info(&page_id)
                            .map(|i| i.title.clone())
                            .unwrap_or_default();
                        let len = current_title.len();
                        self.quick_capture = Some(QuickCaptureState {
                            kind: keymap::dispatch::QuickCaptureKind::Rename,
                            input: current_title,
                            cursor_pos: len,
                        });
                    }
                }
                vec![]
            }
            "delete_page" => {
                if let Some(page_id) = self.active_page().cloned() {
                    let title = self
                        .writer
                        .buffers()
                        .info(&page_id)
                        .map(|i| i.title.clone())
                        .unwrap_or_default();
                    self.active_dialog = Some(ActiveDialog::DeletePage {
                        page_id,
                        selected: 0,
                    });
                    let _ = title; // title used for dialog rendering
                }
                vec![]
            }
            "close_other_windows" => vec![keymap::dispatch::Action::CloseOtherWindows],
            "widen_window" => vec![keymap::dispatch::Action::ResizeWindow(
                keymap::dispatch::ResizeOp::IncreaseWidth,
            )],
            "narrow_window" => vec![keymap::dispatch::Action::ResizeWindow(
                keymap::dispatch::ResizeOp::DecreaseWidth,
            )],
            "taller_window" => vec![keymap::dispatch::Action::ResizeWindow(
                keymap::dispatch::ResizeOp::IncreaseHeight,
            )],
            "shorter_window" => vec![keymap::dispatch::Action::ResizeWindow(
                keymap::dispatch::ResizeOp::DecreaseHeight,
            )],
            "swap_window" => vec![keymap::dispatch::Action::SwapWindow],
            "rotate_layout" => vec![keymap::dispatch::Action::RotateLayout],
            "move_buffer_left" => vec![keymap::dispatch::Action::MoveBuffer(
                window::Direction::Left,
            )],
            "move_buffer_down" => vec![keymap::dispatch::Action::MoveBuffer(
                window::Direction::Down,
            )],
            "move_buffer_up" => vec![keymap::dispatch::Action::MoveBuffer(window::Direction::Up)],
            "move_buffer_right" => vec![keymap::dispatch::Action::MoveBuffer(
                window::Direction::Right,
            )],
            "balance" => {
                self.window_mgr.balance();
                vec![keymap::dispatch::Action::Noop]
            }
            "maximize" => {
                self.window_mgr.maximize_toggle();
                vec![keymap::dispatch::Action::Noop]
            }
            "all_commands" => vec![keymap::dispatch::Action::OpenPicker(
                keymap::dispatch::PickerKind::AllCommands,
            )],
            "view_prompt" => {
                self.open_view_prompt();
                vec![keymap::dispatch::Action::Noop]
            }
            "view_list" => {
                self.open_view_list();
                vec![keymap::dispatch::Action::Noop]
            }
            "view_edit" => {
                self.edit_current_view();
                vec![keymap::dispatch::Action::Noop]
            }
            "view_delete" => {
                self.delete_current_view();
                vec![keymap::dispatch::Action::Noop]
            }
            _ => {
                // Check for dynamic view commands from config
                if let Some(view_name) = action_id.strip_prefix("view_") {
                    if let Some(view) = self.config.views.iter().find(|v| v.name == view_name) {
                        self.open_named_view(view.clone());
                        return vec![keymap::dispatch::Action::Noop];
                    }
                }
                vec![keymap::dispatch::Action::Noop]
            }
        }
    }

    pub(crate) fn translate_ex_command(&mut self, cmd: &str) -> Vec<keymap::dispatch::Action> {
        let trimmed = cmd.trim();
        // Handle :theme with optional argument
        if trimmed == "theme" {
            self.cycle_theme();
            self.persist_theme_to_config();
            return vec![keymap::dispatch::Action::Noop];
        }
        if let Some(name) = trimmed.strip_prefix("theme ") {
            let name = name.trim();
            if self.set_theme(name) {
                self.persist_theme_to_config();
            }
            return vec![keymap::dispatch::Action::Noop];
        }
        match trimmed {
            "q" | "quit" | "q!" | "quit!" => {
                // Vim semantics: close current pane. If last pane, quit app.
                if self.window_mgr.pane_count() <= 1 {
                    vec![keymap::dispatch::Action::Quit]
                } else {
                    let pane = self.window_mgr.active_pane();
                    self.window_mgr.close(pane);
                    vec![keymap::dispatch::Action::Noop]
                }
            }
            "qa" | "qa!" | "quitall" => vec![keymap::dispatch::Action::Quit],
            "w" | "write" => vec![keymap::dispatch::Action::Save],
            "checkpoint" | "cp" => vec![keymap::dispatch::Action::ExplicitCheckpoint],
            "wq" | "x" | "wq!" | "x!" => {
                let _ = self.save_current();
                if self.window_mgr.pane_count() <= 1 {
                    vec![
                        keymap::dispatch::Action::Save,
                        keymap::dispatch::Action::Quit,
                    ]
                } else {
                    let pane = self.window_mgr.active_pane();
                    self.window_mgr.close(pane);
                    vec![keymap::dispatch::Action::Noop]
                }
            }
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
            "stats" => {
                self.show_stats();
                vec![keymap::dispatch::Action::Noop]
            }
            "messages" => {
                self.open_messages_buffer();
                vec![keymap::dispatch::Action::Noop]
            }
            "log" => {
                self.open_log_buffer();
                vec![keymap::dispatch::Action::Noop]
            }
            "config" => {
                self.open_config_buffer();
                vec![keymap::dispatch::Action::Noop]
            }
            _ => {
                // Unknown command — noop
                vec![keymap::dispatch::Action::Noop]
            }
        }
    }

    // View management methods

    fn open_view_prompt(&mut self) {
        let previous_page = self.active_page().cloned();
        self.active_view = Some(ViewState {
            name: "Query Prompt".to_string(),
            query: String::new(),
            error: None,
            is_prompt: true,
            query_input: String::new(),
            query_cursor: 0,
            buffer_id: None,
            row_map: Vec::new(),
            previous_page,
        });
    }

    fn open_view_list(&mut self) {
        let items: Vec<crate::GenericPickerItem> = self
            .config
            .views
            .iter()
            .map(|v| crate::GenericPickerItem {
                id: v.name.clone(),
                label: v.name.clone(),
                middle: v.key.as_ref().map(|k| format!("SPC {k}")),
                right: Some(v.query.clone()),
                preview_text: None,
                score_boost: 0,
            })
            .collect();
        let picker = crate::picker::Picker::new(items);
        self.picker_state = Some(crate::ActivePicker {
            kind: keymap::dispatch::PickerKind::AllCommands,
            action: crate::PickerAction::ExecuteCommand,
            picker,
            title: "Views".to_string(),
            query: String::new(),
            status_noun: "views".to_string(),
            min_query_len: 0,
            previous_theme: None,
            query_selected: false,
        });
    }

    fn edit_current_view(&mut self) {
        // TODO: Implement view editing
        self.notifications.push(render::Notification {
            message: "View editing not implemented yet".to_string(),
            level: render::NotificationLevel::Info,
            expires_at: Some(std::time::Instant::now() + std::time::Duration::from_secs(3)),
            created_at: std::time::Instant::now(),
            wall_time: chrono::Local::now(),
        });
    }

    fn delete_current_view(&mut self) {
        // TODO: Implement view deletion
        self.notifications.push(render::Notification {
            message: "View deletion not implemented yet".to_string(),
            level: render::NotificationLevel::Info,
            expires_at: Some(std::time::Instant::now() + std::time::Duration::from_secs(3)),
            created_at: std::time::Instant::now(),
            wall_time: chrono::Local::now(),
        });
    }

    pub(crate) fn open_named_view(&mut self, view_config: config::ViewConfig) {
        let previous_page = self.active_page().cloned();
        let mut view_state = ViewState {
            name: view_config.name.clone(),
            query: view_config.query.clone(),
            error: None,
            is_prompt: false,
            query_input: String::new(),
            query_cursor: 0,
            buffer_id: None,
            row_map: Vec::new(),
            previous_page,
        };

        self.render_view_to_buffer(&mut view_state);
        self.active_view = Some(view_state);
    }

    /// Execute the view query and render results into a read-only buffer.
    pub(crate) fn render_view_to_buffer(&mut self, view_state: &mut ViewState) {
        let query = if view_state.is_prompt {
            &view_state.query_input
        } else {
            &view_state.query
        };
        if query.is_empty() {
            return;
        }

        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let today_date = journal::Journal::today();

        let result = if let Some(index) = &self.index {
            query::run_query_with_limit(
                query,
                index.connection(),
                &today,
                None,
                self.config.max_results,
            )
        } else {
            Err("Index not available".to_string())
        };

        match result {
            Ok(result) => {
                view_state.error = None;
                let (content, row_map) = format_view_result(&result, today_date);

                let id = crate::uuid::generate_hex_id();
                self.writer.apply(crate::BufferMessage::OpenReadOnly {
                    page_id: id.clone(),
                    title: view_state.name.clone(),
                    content,
                });
                self.set_active_page(Some(id.clone()));
                self.set_cursor(0);
                view_state.buffer_id = Some(id);
                view_state.row_map = row_map;
            }
            Err(err) => {
                view_state.error = Some(err.clone());
                let id = crate::uuid::generate_hex_id();
                self.writer.apply(crate::BufferMessage::OpenReadOnly {
                    page_id: id.clone(),
                    title: view_state.name.clone(),
                    content: format!("Error: {err}"),
                });
                self.set_active_page(Some(id.clone()));
                view_state.buffer_id = Some(id);
                view_state.row_map = Vec::new();
            }
        }
    }

    /// SPC * — grab word under cursor, set as search pattern, open vault search.
    fn search_word_under_cursor(&mut self) {
        let Some(page_id) = self.active_page().cloned() else {
            return;
        };
        let Some(buf) = self.writer.buffers().get(&page_id) else {
            return;
        };
        let cursor = self.cursor();
        let text = buf.text().to_string();

        // Extract word at cursor position
        if cursor >= text.len() {
            return;
        }
        let bytes = text.as_bytes();
        let is_word_char = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b == b'-';
        if !is_word_char(bytes[cursor]) {
            return;
        }
        let mut start = cursor;
        while start > 0 && is_word_char(bytes[start - 1]) {
            start -= 1;
        }
        let mut end = cursor;
        while end < bytes.len() && is_word_char(bytes[end]) {
            end += 1;
        }
        let word = &text[start..end];
        if word.is_empty() {
            return;
        }

        // Set as in-buffer search pattern (for n/N)
        self.search_pattern = Some(word.to_string());

        // Open vault search picker pre-filled with the word
        self.open_picker(keymap::dispatch::PickerKind::Search);
        if let Some(ap) = &mut self.picker_state {
            ap.query = word.to_string();
            ap.picker.set_query(&ap.query);
        }
        self.refresh_search_picker();
    }

    /// Sever mirror: replace ^=xxxxx with a new ^yyyyy on the cursor line.
    fn mirror_sever(&mut self) {
        let page_id = match self.active_page().cloned() {
            Some(id) => id,
            None => return,
        };
        let cursor_line = self.cursor_position().0;
        let is_mirror = self
            .writer
            .buffers()
            .document(&page_id)
            .and_then(|doc| doc.block_id_at_line(cursor_line).cloned());
        let Some(entry) = is_mirror else {
            self.push_notification(
                "Not on a mirrored block".into(),
                crate::render::NotificationLevel::Warning,
            );
            return;
        };
        if !entry.is_mirror {
            self.push_notification(
                "Not on a mirrored block".into(),
                crate::render::NotificationLevel::Warning,
            );
            return;
        };

        // Generate a new unique ID
        let existing = crate::block_id_gen::load_all_known_ids(
            self.index.as_ref().map(|i| i.connection()).unwrap(),
        );
        let new_id = bloom_buffer::block_id::next_block_id(&existing);
        if let Some(mut doc) = self.writer.buffers_mut().document_mut(&page_id) {
            let _ =
                doc.set_block_id_at_line(cursor_line, crate::types::BlockId(new_id.clone()), false);
        }

        self.save_page(&page_id);
        self.push_notification(
            format!("Mirror severed — new ID ^{}", new_id),
            crate::render::NotificationLevel::Info,
        );
    }

    /// Go to mirror: open a picker of all pages sharing the cursor line's block ID.
    fn mirror_goto(&mut self) {
        let page_id = match self.active_page().cloned() {
            Some(id) => id,
            None => return,
        };
        let (cursor_line, cursor_col) = self.cursor_position();
        let bid = {
            let Some(doc) = self.writer.buffers().document(&page_id) else {
                return;
            };
            match doc.block_id_at_line(cursor_line) {
                Some(entry) if entry.is_mirror => entry.id.clone(),
                _ => {
                    self.push_notification(
                        "Not on a mirrored block".into(),
                        crate::render::NotificationLevel::Warning,
                    );
                    return;
                }
            }
        };

        let Some(idx) = &self.index else { return };
        let mirrors = idx.find_all_pages_by_block_id(&bid);
        let items: Vec<MirrorMenuItem> = mirrors
            .iter()
            .filter(|(meta, _)| meta.id != page_id)
            .map(|(meta, line)| MirrorMenuItem {
                page_id: meta.id.clone(),
                title: meta.title.clone(),
                line: *line,
            })
            .collect();

        if items.is_empty() {
            self.push_notification(
                "No other mirrors found".into(),
                crate::render::NotificationLevel::Warning,
            );
            return;
        }

        self.mirror_menu = Some(MirrorMenu {
            items,
            selected: 0,
            cursor_line,
            cursor_col,
        });
    }
}
fn format_view_result(
    result: &query::QueryResult,
    today: chrono::NaiveDate,
) -> (String, Vec<RowSource>) {
    let mut lines = Vec::new();
    let mut row_map = Vec::new();

    match &result.kind {
        query::QueryResultKind::Rows(rr) => {
            let is_tasks = matches!(result.source, query::Source::Tasks);
            let done_col = rr.columns.iter().position(|c| c == "done");
            let due_col = rr.columns.iter().position(|c| c == "due");
            let text_col = rr.columns.iter().position(|c| c == "text");
            let page_col = rr.columns.iter().position(|c| c == "page");
            let line_col = rr.columns.iter().position(|c| c == "line");

            let mut last_section: Option<String> = Option::None;

            for row in &rr.rows {
                // Insert section headers for tasks sorted by due date
                if is_tasks {
                    if let Some(idx) = due_col {
                        let section = match &row.values.get(idx) {
                            Some(query::CellValue::Text(d)) if !d.is_empty() => {
                                match chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d") {
                                    Ok(date) if date < today => "Overdue".to_string(),
                                    Ok(date) if date == today => {
                                        format!("Today · {}", today.format("%b %-d"))
                                    }
                                    Ok(_) => "Upcoming".to_string(),
                                    Err(_) => "No date".to_string(),
                                }
                            }
                            _ => "No date".to_string(),
                        };
                        if last_section.as_ref() != Some(&section) {
                            if !lines.is_empty() {
                                lines.push(String::new());
                                row_map.push(RowSource::Header);
                            }
                            lines.push(section.clone());
                            row_map.push(RowSource::Header);
                            last_section = Some(section);
                        }
                    }
                }

                // Format the data row
                let page_id = page_col
                    .and_then(|i| row.values.get(i))
                    .map(|v| v.to_string())
                    .unwrap_or_default();
                let page_title = page_id.clone(); // BQL returns title in page column
                let line_num = line_col
                    .and_then(|i| row.values.get(i))
                    .and_then(|v| match v {
                        query::CellValue::Int(n) => Some(*n as usize),
                        _ => None,
                    })
                    .unwrap_or(0);

                let text = if is_tasks {
                    let done = done_col
                        .and_then(|i| row.values.get(i))
                        .map(|v| {
                            matches!(v, query::CellValue::Bool(true) | query::CellValue::Int(1))
                        })
                        .unwrap_or(false);
                    let checkbox = if done { "[x]" } else { "[ ]" };
                    let task_text = text_col
                        .and_then(|i| row.values.get(i))
                        .map(|v| v.to_string())
                        .unwrap_or_default();
                    let due = due_col
                        .and_then(|i| row.values.get(i))
                        .map(|v| v.to_string())
                        .unwrap_or_default();
                    let page_hint = if page_id.is_empty() {
                        String::new()
                    } else {
                        format!("  ({})", page_id)
                    };
                    if due.is_empty() {
                        format!("{checkbox} {task_text}{page_hint}")
                    } else {
                        format!("{checkbox} {task_text}  @due({due}){page_hint}")
                    }
                } else {
                    // Generic: join all columns
                    row.values
                        .iter()
                        .map(|v| v.to_string())
                        .collect::<Vec<_>>()
                        .join("  ")
                };

                lines.push(text);
                row_map.push(RowSource::Source {
                    page_id: page_id.clone(),
                    page_title,
                    line: line_num,
                });
            }
        }
        query::QueryResultKind::Count(count) => {
            lines.push(format!("Count: {count}"));
            row_map.push(RowSource::None);
        }
        query::QueryResultKind::GroupCounts(groups) => {
            for (group, count) in groups {
                lines.push(format!("{group}  ({count})"));
                row_map.push(RowSource::None);
            }
        }
    }

    (lines.join("\n"), row_map)
}
