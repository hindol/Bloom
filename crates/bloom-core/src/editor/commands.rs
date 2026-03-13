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
            "search_tags" => vec![keymap::dispatch::Action::OpenPicker(
                keymap::dispatch::PickerKind::Tags,
            )],
            "journal_today" => {
                self.open_journal_today();
                self.in_journal_mode = true;
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
            "page_history" => vec![keymap::dispatch::Action::OpenPageHistory],
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
                    if let Some(buf) = self.buffer_mgr.get(&page_id) {
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
                    if let Some(buf) = self.buffer_mgr.get(&page_id) {
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
                // TODO: open rename input pre-filled with current title
                vec![keymap::dispatch::Action::Noop]
            }
            "delete_page" => {
                // TODO: show confirmation dialog, then delete
                vec![keymap::dispatch::Action::Noop]
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
            _ => vec![keymap::dispatch::Action::Noop],
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
}
