//! Key event dispatch and input routing.
//!
//! Processes each [`KeyEvent`](crate::types::KeyEvent) through the modal dispatch
//! pipeline: wizard → dialog → picker → quick-capture → leader sequences →
//! Vim state machine → ex-command line. Handles all mode-specific input logic.

use std::time::Instant;

use crate::editor::commands::EX_COMMANDS;
use crate::editor::pickers::picker_kind_key;
use crate::*;

impl BloomEditor {
    /// Process a key event.
    pub fn handle_key(&mut self, key: types::KeyEvent) -> Vec<keymap::dispatch::Action> {
        // If wizard is active, route all keys there
        if self.wizard.is_some() {
            return self.handle_wizard_key(&key);
        }

        // If dialog is active, handle dialog input
        if self.active_dialog.is_some() {
            return self.handle_dialog_key(&key);
        }

        // Check platform shortcuts first
        if let Some(action) = keymap::platform_shortcut(&key) {
            self.leader_keys.clear();
            self.pending_since = None;
            self.which_key_visible = false;
            let result = self.execute_actions(vec![action]);
            self.autosave_if_dirty();
            return result;
        }

        // If agenda overlay is open
        if self.agenda_state.is_some() {
            return self.handle_agenda_key(&key);
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
            let result = self.execute_actions(actions);
            self.autosave_if_dirty();
            return result;
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

        // Command mode: intercept Tab for inline menu completion
        if matches!(self.vim_state.mode(), vim::Mode::Command) && key.code == types::KeyCode::Tab {
            // Accept the selected completion into the command line
            let input = self.vim_state.pending_keys().to_string();
            let completion = if let Some(arg_prefix) = input.strip_prefix("theme ") {
                // Argument completion
                theme::THEME_NAMES
                    .iter()
                    .find(|name| arg_prefix.is_empty() || name.starts_with(arg_prefix))
                    .map(|name| format!("theme {name}"))
            } else {
                // Command completion
                EX_COMMANDS
                    .iter()
                    .find(|(cmd, _)| input.is_empty() || cmd.starts_with(&input))
                    .map(|(cmd, _)| cmd.to_string())
            };

            if let Some(text) = completion {
                self.vim_state.set_command_line(&text);
            }
            return vec![keymap::dispatch::Action::Noop];
        }

        // Vim processing
        if let Some(buf) = self.active_page().and_then(|id| self.buffer_mgr.get(id)) {
            let mode_before_key = self.vim_state.mode();
            let action = self.vim_state.process_key(key.clone(), buf, self.cursor());

            // Esc in Normal mode with no overlays: dismiss persistent notifications
            if key.code == types::KeyCode::Esc && matches!(mode_before_key, vim::Mode::Normal) {
                self.dismiss_notifications();
            }

            let actions = self.translate_vim_action(action, mode_before_key);
            let result = self.execute_actions(actions);
            self.autosave_if_dirty();
            return result;
        }

        vec![keymap::dispatch::Action::Noop]
    }

    /// Schedule autosave if the active buffer is dirty.
    /// Called once at the end of handle_key() — covers edits, undo, redo,
    /// task toggle, and any other mutation without each handler needing to
    /// remember to call schedule_autosave().
    fn autosave_if_dirty(&self) {
        if let Some(page_id) = self.active_page() {
            if self.buffer_mgr.get(page_id).is_some_and(|b| b.is_dirty()) {
                self.schedule_autosave(page_id);
            }
        }
    }

    /// Execute actions on editor state. Returns the actions for the TUI to handle
    /// (only Quit, Save, and informational actions pass through).
    pub(crate) fn execute_actions(
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
                keymap::dispatch::Action::CloseOtherWindows => {
                    self.window_mgr.close_others();
                }
                keymap::dispatch::Action::ResizeWindow(ref op) => {
                    let pane = self.window_mgr.active_pane();
                    match op {
                        keymap::dispatch::ResizeOp::IncreaseWidth => {
                            self.window_mgr
                                .resize(pane, 1, window::SplitDirection::Vertical);
                        }
                        keymap::dispatch::ResizeOp::DecreaseWidth => {
                            self.window_mgr
                                .resize(pane, -1, window::SplitDirection::Vertical);
                        }
                        keymap::dispatch::ResizeOp::IncreaseHeight => {
                            self.window_mgr
                                .resize(pane, 1, window::SplitDirection::Horizontal);
                        }
                        keymap::dispatch::ResizeOp::DecreaseHeight => {
                            self.window_mgr
                                .resize(pane, -1, window::SplitDirection::Horizontal);
                        }
                    }
                }
                keymap::dispatch::Action::SwapWindow => {
                    self.window_mgr.swap_with_next();
                }
                keymap::dispatch::Action::RotateLayout => {
                    self.window_mgr.rotate_layout();
                }
                keymap::dispatch::Action::MoveBuffer(dir) => {
                    self.window_mgr.move_buffer(dir);
                }
                keymap::dispatch::Action::OpenAgenda => {
                    if self.agenda_state.is_some() {
                        self.agenda_state = None;
                    } else {
                        self.open_agenda();
                    }
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
                keymap::dispatch::Action::Undo => {
                    if let Some(page_id) = self.active_page().cloned() {
                        if let Some(buf) = self.buffer_mgr.get_mut(&page_id) {
                            buf.undo();
                            let len = buf.len_chars();
                            if self.cursor() > len {
                                self.set_cursor(len.saturating_sub(1));
                            }
                        }
                    }
                }
                keymap::dispatch::Action::Redo => {
                    if let Some(page_id) = self.active_page().cloned() {
                        if let Some(buf) = self.buffer_mgr.get_mut(&page_id) {
                            buf.redo();
                            let len = buf.len_chars();
                            if self.cursor() > len {
                                self.set_cursor(len.saturating_sub(1));
                            }
                        }
                    }
                }
                keymap::dispatch::Action::FollowLink => {
                    self.follow_link_at_cursor();
                }
                keymap::dispatch::Action::CopyToClipboard(ref text) => {
                    // Pass through — TUI handles actual clipboard
                    result.push(keymap::dispatch::Action::CopyToClipboard(text.clone()));
                }
                keymap::dispatch::Action::RebuildIndex => {
                    if let Some(tx) = &self.indexer_tx {
                        let _ = tx.send(index::indexer::IndexRequest::FullRebuild);
                        self.indexing = true;
                    }
                }
                // Pass through to TUI: Quit, Save, and others
                _ => {
                    result.push(action);
                }
            }
        }
        result
    }

    pub(crate) fn handle_leader_key(
        &mut self,
        key: types::KeyEvent,
    ) -> Vec<keymap::dispatch::Action> {
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

    pub(crate) fn handle_picker_key(
        &mut self,
        key: &types::KeyEvent,
    ) -> Vec<keymap::dispatch::Action> {
        use types::KeyCode;
        let is_theme = self
            .picker_state
            .as_ref()
            .is_some_and(|ap| matches!(ap.kind, keymap::dispatch::PickerKind::Theme));

        // Ctrl+key shortcuts
        if key.modifiers.ctrl {
            match &key.code {
                // Ctrl+N / Ctrl+J → next result
                KeyCode::Char('n') | KeyCode::Char('j') => {
                    if let Some(ap) = &mut self.picker_state {
                        ap.picker.move_selection(1);
                    }
                    if is_theme {
                        self.theme_picker_preview_current();
                    }
                    return vec![keymap::dispatch::Action::Noop];
                }
                // Ctrl+P / Ctrl+K → previous result
                KeyCode::Char('p') | KeyCode::Char('k') => {
                    if let Some(ap) = &mut self.picker_state {
                        ap.picker.move_selection(-1);
                    }
                    if is_theme {
                        self.theme_picker_preview_current();
                    }
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
                    self.save_picker_query();
                    self.picker_state = None;
                }
                vec![keymap::dispatch::Action::Noop]
            }
            KeyCode::Enter => {
                if is_theme {
                    self.theme_picker_confirm();
                } else if let Some(ap) = self.picker_state.take() {
                    if !ap.query.is_empty() {
                        self.last_picker_queries
                            .insert(picker_kind_key(&ap.kind), ap.query.clone());
                    }
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
                if is_theme {
                    self.theme_picker_preview_current();
                }
                vec![keymap::dispatch::Action::Noop]
            }
            KeyCode::Down => {
                if let Some(ap) = &mut self.picker_state {
                    ap.picker.move_selection(1);
                }
                if is_theme {
                    self.theme_picker_preview_current();
                }
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
                    if ap.query_selected {
                        ap.query.clear();
                        ap.query_selected = false;
                    } else if !ap.query.is_empty() {
                        ap.query.pop();
                    }
                    if matches!(ap.kind, keymap::dispatch::PickerKind::Search) {
                        self.refresh_search_picker();
                    } else if let Some(ap) = &mut self.picker_state {
                        ap.picker.set_query(&ap.query);
                    }
                }
                vec![keymap::dispatch::Action::Noop]
            }
            KeyCode::Char(c) => {
                if let Some(ap) = &mut self.picker_state {
                    if ap.query_selected {
                        ap.query.clear();
                        ap.query_selected = false;
                    }
                    ap.query.push(*c);
                    if matches!(ap.kind, keymap::dispatch::PickerKind::Search) {
                        self.refresh_search_picker();
                    } else {
                        ap.picker.set_query(&ap.query);
                    }
                }
                vec![keymap::dispatch::Action::Noop]
            }
            _ => vec![keymap::dispatch::Action::Noop],
        }
    }

    fn save_picker_query(&mut self) {
        if let Some(ap) = &self.picker_state {
            if !ap.query.is_empty() {
                self.last_picker_queries
                    .insert(picker_kind_key(&ap.kind), ap.query.clone());
            }
        }
    }

    pub(crate) fn handle_dialog_key(
        &mut self,
        key: &types::KeyEvent,
    ) -> Vec<keymap::dispatch::Action> {
        use types::KeyCode;
        match &key.code {
            KeyCode::Esc => {
                // Dismiss dialog (keep buffer version)
                self.active_dialog = None;
            }
            KeyCode::Enter => {
                if let Some(dialog) = self.active_dialog.take() {
                    match dialog {
                        ActiveDialog::FileChanged {
                            page_id,
                            path,
                            selected,
                        } => {
                            if selected == 0 {
                                // Reload from disk
                                if let Ok(content) = std::fs::read_to_string(&path) {
                                    self.buffer_mgr.reload(&page_id, &content);
                                    self.set_cursor(0);
                                }
                            }
                            // selected == 1: keep buffer (do nothing)
                        }
                    }
                }
            }
            KeyCode::Left | KeyCode::Char('h') => {
                if let Some(ActiveDialog::FileChanged {
                    ref mut selected, ..
                }) = self.active_dialog
                {
                    *selected = 0;
                }
            }
            KeyCode::Right | KeyCode::Char('l') => {
                if let Some(ActiveDialog::FileChanged {
                    ref mut selected, ..
                }) = self.active_dialog
                {
                    *selected = 1;
                }
            }
            _ => {}
        }
        vec![keymap::dispatch::Action::Noop]
    }

    pub(crate) fn handle_quick_capture_key(
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

    pub(crate) fn handle_wizard_key(
        &mut self,
        key: &types::KeyEvent,
    ) -> Vec<keymap::dispatch::Action> {
        use types::KeyCode;
        // Ctrl+Q quits even during wizard
        if key.modifiers.ctrl && key.code == KeyCode::Char('q') {
            return vec![keymap::dispatch::Action::Quit];
        }

        let wiz = self.wizard.as_mut().unwrap();
        wiz.error = None; // Clear error on any key

        match wiz.step {
            WizardStep::Welcome => {
                if key.code == KeyCode::Enter {
                    wiz.step = WizardStep::ChooseVault;
                }
            }
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
                                wiz.error = Some(format!("Cannot create directory: {}", e));
                            }
                        }
                    }
                }
                KeyCode::Esc => {
                    wiz.step = WizardStep::Welcome;
                }
                KeyCode::Char(c) => {
                    let byte_pos = wiz
                        .vault_path
                        .char_indices()
                        .nth(wiz.vault_path_cursor)
                        .map(|(i, _)| i)
                        .unwrap_or(wiz.vault_path.len());
                    wiz.vault_path.insert(byte_pos, *c);
                    wiz.vault_path_cursor += 1;
                }
                KeyCode::Backspace => {
                    if wiz.vault_path_cursor > 0 {
                        wiz.vault_path_cursor -= 1;
                        let byte_pos = wiz
                            .vault_path
                            .char_indices()
                            .nth(wiz.vault_path_cursor)
                            .map(|(i, _)| i)
                            .unwrap_or(wiz.vault_path.len());
                        wiz.vault_path.remove(byte_pos);
                    }
                }
                KeyCode::Left => {
                    wiz.vault_path_cursor = wiz.vault_path_cursor.saturating_sub(1);
                }
                KeyCode::Right => {
                    wiz.vault_path_cursor = (wiz.vault_path_cursor + 1).min(wiz.vault_path.len());
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
                        wiz.error =
                            Some("Not a Logseq vault: missing pages/ directory".to_string());
                    } else {
                        // TODO: actual Logseq import (G13) — for now skip to Complete
                        wiz.step = WizardStep::Complete;
                    }
                }
                KeyCode::Esc => {
                    wiz.step = WizardStep::ImportChoice;
                }
                KeyCode::Char(c) => {
                    let byte_pos = wiz
                        .logseq_path
                        .char_indices()
                        .nth(wiz.logseq_path_cursor)
                        .map(|(i, _)| i)
                        .unwrap_or(wiz.logseq_path.len());
                    wiz.logseq_path.insert(byte_pos, *c);
                    wiz.logseq_path_cursor += 1;
                }
                KeyCode::Backspace => {
                    if wiz.logseq_path_cursor > 0 {
                        wiz.logseq_path_cursor -= 1;
                        let byte_pos = wiz
                            .logseq_path
                            .char_indices()
                            .nth(wiz.logseq_path_cursor)
                            .map(|(i, _)| i)
                            .unwrap_or(wiz.logseq_path.len());
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
            WizardStep::Complete => {
                if key.code == KeyCode::Enter {
                    self.complete_wizard();
                    return vec![keymap::dispatch::Action::Noop];
                }
            }
        }

        vec![keymap::dispatch::Action::Noop]
    }

    /// Toggle task checkbox on the line at the cursor: `[ ]` ↔ `[x]`.
    pub(crate) fn toggle_task_at_cursor(&mut self) {
        let Some(page_id) = self.active_page().cloned() else {
            return;
        };
        let cursor = self.cursor();
        let Some(buf) = self.buffer_mgr.get_mut(&page_id) else {
            return;
        };
        let rope = buf.text();
        let len = rope.len_chars();
        if len == 0 {
            return;
        }
        let cursor = cursor.min(len.saturating_sub(1));
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

    // -----------------------------------------------------------------------
    // Agenda overlay
    // -----------------------------------------------------------------------

    pub(crate) fn open_agenda(&mut self) {
        let today = chrono::Local::now().date_naive();
        let filters = index::AgendaFilters {
            tags: vec![],
            page: None,
            date_range: None,
        };
        let Some(idx) = &self.index else {
            self.agenda_state = Some(AgendaState {
                selected_index: 0,
                items: Vec::new(),
            });
            return;
        };
        let view = self.agenda.build(today, idx, &filters);
        let mut items = Vec::new();
        for task in &view.overdue {
            let source_title = idx
                .find_page_by_id(&task.source_page)
                .map(|m| m.title)
                .unwrap_or_default();
            items.push(AgendaFlatItem {
                task: task.clone(),
                source_title,
                bucket: AgendaBucket::Overdue,
            });
        }
        for task in &view.today {
            let source_title = idx
                .find_page_by_id(&task.source_page)
                .map(|m| m.title)
                .unwrap_or_default();
            items.push(AgendaFlatItem {
                task: task.clone(),
                source_title,
                bucket: AgendaBucket::Today,
            });
        }
        for task in &view.upcoming {
            let source_title = idx
                .find_page_by_id(&task.source_page)
                .map(|m| m.title)
                .unwrap_or_default();
            items.push(AgendaFlatItem {
                task: task.clone(),
                source_title,
                bucket: AgendaBucket::Upcoming,
            });
        }
        self.agenda_state = Some(AgendaState {
            selected_index: 0,
            items,
        });
    }

    pub(crate) fn handle_agenda_key(
        &mut self,
        key: &types::KeyEvent,
    ) -> Vec<keymap::dispatch::Action> {
        use types::KeyCode;
        let noop = vec![keymap::dispatch::Action::Noop];

        if key.modifiers.ctrl {
            match &key.code {
                KeyCode::Char('n') => {
                    if let Some(st) = &mut self.agenda_state {
                        if !st.items.is_empty() {
                            st.selected_index = (st.selected_index + 1).min(st.items.len() - 1);
                        }
                    }
                    return noop;
                }
                KeyCode::Char('p') => {
                    if let Some(st) = &mut self.agenda_state {
                        st.selected_index = st.selected_index.saturating_sub(1);
                    }
                    return noop;
                }
                _ => return noop,
            }
        }

        match &key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if let Some(st) = &mut self.agenda_state {
                    if !st.items.is_empty() {
                        st.selected_index = (st.selected_index + 1).min(st.items.len() - 1);
                    }
                }
                noop
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if let Some(st) = &mut self.agenda_state {
                    st.selected_index = st.selected_index.saturating_sub(1);
                }
                noop
            }
            KeyCode::Char('q') | KeyCode::Esc => {
                self.agenda_state = None;
                noop
            }
            KeyCode::Enter => {
                let page_id = self
                    .agenda_state
                    .as_ref()
                    .and_then(|st| st.items.get(st.selected_index))
                    .map(|item| item.task.source_page.clone());
                self.agenda_state = None;
                if let Some(id) = page_id {
                    self.navigate_to_page_by_id(&id);
                }
                noop
            }
            KeyCode::Char('x') => {
                if let Some(st) = &self.agenda_state {
                    if let Some(item) = st.items.get(st.selected_index) {
                        let page_id = item.task.source_page.clone();
                        let line = item.task.line;
                        self.toggle_task_in_page(&page_id, line);
                    }
                }
                // Refresh the agenda after toggling
                self.open_agenda();
                noop
            }
            _ => noop,
        }
    }

    pub(crate) fn toggle_task_in_page(&mut self, page_id: &types::PageId, line: usize) {
        // Ensure the page is loaded in a buffer
        let needs_load = self.buffer_mgr.get(page_id).is_none();
        if needs_load {
            if let Some(idx) = &self.index {
                if let Some(meta) = idx.find_page_by_id(page_id) {
                    if let Ok(content) = std::fs::read_to_string(&meta.path) {
                        self.buffer_mgr
                            .open(page_id, &meta.title, &meta.path, &content);
                    }
                }
            }
        }
        let Some(buf) = self.buffer_mgr.get_mut(page_id) else {
            return;
        };
        let rope = buf.text();
        if rope.len_lines() == 0 {
            return;
        }
        let line_idx = line.min(rope.len_lines().saturating_sub(1));
        let line_text = rope.line(line_idx).to_string();
        let trimmed = line_text.trim_start();
        let indent = line_text.len() - trimmed.len();
        let line_start = rope.line_to_char(line_idx);

        if trimmed.starts_with("- [ ] ") {
            let bracket_start = line_start + indent + 2;
            buf.replace(bracket_start..bracket_start + 3, "[x]");
        } else if trimmed.starts_with("- [x] ") || trimmed.starts_with("- [X] ") {
            let bracket_start = line_start + indent + 2;
            buf.replace(bracket_start..bracket_start + 3, "[ ]");
        }
    }

    /// Close the active buffer. Opens journal or scratch if it was the last buffer.
    pub(crate) fn close_active_buffer(&mut self) {
        if let Some(page_id) = self.active_page().cloned() {
            self.set_active_page(None);
            self.buffer_mgr.close(&page_id);
            if let Some(next) = self.buffer_mgr.open_buffers().first() {
                self.set_active_page(Some(next.page_id.clone()));
                self.set_cursor(0);
            } else {
                self.open_journal_today();
            }
        }
    }

    pub(crate) fn translate_vim_action(
        &mut self,
        action: vim::VimAction,
        prev_mode: vim::Mode,
    ) -> Vec<keymap::dispatch::Action> {
        match action {
            vim::VimAction::Edit(edit) => {
                self.pending_since = None;
                self.which_key_visible = false;
                if let Some(page_id) = self.active_page().cloned() {
                    if let Some(buf) = self.buffer_mgr.get_mut(&page_id) {
                        if edit.replacement.is_empty() {
                            buf.delete(edit.range.clone());
                        } else if edit.range.is_empty() {
                            buf.insert(edit.range.start, &edit.replacement);
                        } else {
                            buf.replace(edit.range.clone(), &edit.replacement);
                        }
                        self.set_cursor(edit.cursor_after);
                    }
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
                self.set_cursor(motion.new_position);
                vec![keymap::dispatch::Action::Motion(
                    keymap::dispatch::MotionResult {
                        new_position: motion.new_position,
                        extend_selection: motion.extend_selection,
                    },
                )]
            }
            vim::VimAction::ModeChange(ref mode) => {
                let was_insert = matches!(prev_mode, vim::Mode::Insert);
                if matches!(mode, vim::Mode::Command) {
                    self.pending_since = Some(Instant::now());
                } else {
                    self.pending_since = None;
                    self.which_key_visible = false;
                }
                // Edit group lifecycle: begin on Insert entry, end on Insert exit
                if matches!(mode, vim::Mode::Insert) {
                    if let Some(page_id) = self.active_page().cloned() {
                        if let Some(buf) = self.buffer_mgr.get_mut(&page_id) {
                            buf.begin_edit_group();
                        }
                    }
                } else if matches!(mode, vim::Mode::Normal) {
                    // Leaving Insert (or Visual, Command) → close any open group
                    if let Some(page_id) = self.active_page().cloned() {
                        if let Some(buf) = self.buffer_mgr.get_mut(&page_id) {
                            buf.end_edit_group();
                        }
                    }
                    // Auto-align only on Insert→Normal transition
                    if was_insert {
                        match self.config.auto_align {
                            config::AutoAlignMode::Page => {
                                if let Some(page_id) = self.active_page().cloned() {
                                    if let Some(buf) = self.buffer_mgr.get_mut(&page_id) {
                                        align::auto_align_page(buf);
                                    }
                                }
                            }
                            config::AutoAlignMode::Block => {
                                let cursor_line = self.cursor_position().0;
                                if let Some(page_id) = self.active_page().cloned() {
                                    if let Some(buf) = self.buffer_mgr.get_mut(&page_id) {
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
                if let Some(page_id) = self.active_page().cloned() {
                    if let Some(buf) = self.buffer_mgr.get_mut(&page_id) {
                        buf.restore_edit_group_checkpoint();
                        self.set_cursor(0);
                    }
                }
                vec![keymap::dispatch::Action::Noop]
            }
            vim::VimAction::Composite(actions) => actions
                .into_iter()
                .flat_map(|a| self.translate_vim_action(a, prev_mode.clone()))
                .collect(),
        }
    }

    pub(crate) fn handle_vim_command(&mut self, cmd: &str) -> Vec<keymap::dispatch::Action> {
        match cmd {
            "undo" => vec![keymap::dispatch::Action::Undo],
            "redo" => vec![keymap::dispatch::Action::Redo],
            _ => self.translate_ex_command(cmd),
        }
    }
}
