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

        // If picker is open (all picker types, including theme)
        if self.picker_state.is_some() {
            return self.handle_picker_key(&key);
        }

        // If date picker / calendar is open
        if self.date_picker_state.is_some() {
            return self.handle_date_picker_key(&key);
        }

        // If quick capture is open
        if self.quick_capture.is_some() {
            return self.handle_quick_capture_key(&key);
        }

        // If a view is active, handle q/Enter; other keys pass through to Vim
        if self.active_view.is_some() {
            let actions = self.handle_view_key(&key);
            if !actions.is_empty() {
                return actions;
            }
            // Fall through to normal key handling (Vim navigation works on read-only buffer)
        }

        // If inline completion is active, intercept navigation/accept keys
        if self.inline_completion.is_some() {
            match &key.code {
                types::KeyCode::Enter | types::KeyCode::Tab => {
                    self.accept_inline_completion();
                    return vec![keymap::dispatch::Action::Noop];
                }
                types::KeyCode::Esc => {
                    self.inline_completion = None;
                    // fall through to vim (Esc exits insert mode)
                }
                types::KeyCode::Up => {
                    if let Some(ic) = &mut self.inline_completion {
                        ic.selected = ic.selected.saturating_sub(1);
                    }
                    return vec![keymap::dispatch::Action::Noop];
                }
                types::KeyCode::Down => {
                    if let Some(ic) = &mut self.inline_completion {
                        ic.selected += 1; // clamped during render
                    }
                    return vec![keymap::dispatch::Action::Noop];
                }
                _ => {
                    // Let the key pass through to vim (typing continues)
                }
            }
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
            && matches!(self.vim_state.mode(), bloom_vim::Mode::Normal)
        {
            self.leader_keys.push(key);
            self.pending_since = Some(Instant::now());
            return vec![keymap::dispatch::Action::Noop];
        }

        // Command mode: intercept Tab for inline menu completion
        if matches!(self.vim_state.mode(), bloom_vim::Mode::Command)
            && key.code == types::KeyCode::Tab
        {
            // Accept the selected completion into the command line
            let input = self.vim_state.pending_keys().to_string();
            let completion = if let Some(arg_prefix) = input.strip_prefix("theme ") {
                // Argument completion
                bloom_md::theme::THEME_NAMES
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

        // In-buffer search prompt (/ or ?)
        if self.search_active {
            return self.handle_search_key(key);
        }

        // Mirror inline menu (SPC m m)
        if self.mirror_menu.is_some() {
            return self.handle_mirror_menu_key(key);
        }

        // Temporal strip (SPC H h — page history)
        if self.temporal_strip.is_some() {
            return self.handle_temporal_strip_key(&key);
        }

        // Vim processing — works for both mutable and frozen (read-only) buffers.
        let buf_for_vim = self
            .active_page()
            .and_then(|id| self.writer.buffers().get(id));
        let empty_buf = bloom_buffer::Buffer::from_text("");
        let buf = buf_for_vim.unwrap_or(&empty_buf);
        {
            let mode_before_key = self.vim_state.mode();
            let cursor = self.cursor();

            let action = self.vim_state.process_key(key.clone(), buf, cursor);

            // Esc in Normal mode with no overlays: dismiss persistent notifications
            if key.code == types::KeyCode::Esc && matches!(mode_before_key, bloom_vim::Mode::Normal)
            {
                self.dismiss_notifications();
            }

            let actions = self.translate_vim_action(action, mode_before_key);
            let result = self.execute_actions(actions);
            self.autosave_if_dirty();
            result
        }
    }

    /// Save if the active buffer is dirty.
    /// Skipped during Insert mode (mid-edit, partial state) and for read-only buffers.
    /// Autosave fires naturally when Insert→Normal transition completes
    /// (after edit group close, block IDs, and alignment).
    fn autosave_if_dirty(&mut self) {
        if matches!(self.vim_state.mode(), bloom_vim::Mode::Insert) {
            return; // Don't save mid-edit
        }
        if let Some(page_id) = self.active_page().cloned() {
            if !self.writer.buffers().is_read_only(&page_id) {
                self.save_page(&page_id);
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
                    self.open_agenda();
                }
                keymap::dispatch::Action::OpenUndoTree => {
                    // TODO: open undo tree in split pane
                    result.push(action);
                }
                keymap::dispatch::Action::OpenPageHistory => {
                    self.open_page_history();
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
                        self.writer.apply(crate::BufferMessage::Undo { page_id: page_id.clone() });
                        // Fix cursor bounds after undo
                        if let Some(buf) = self.writer.buffers().get(&page_id) {
                            let len = buf.len_chars();
                            if self.cursor() > len {
                                self.set_cursor(len.saturating_sub(1));
                            }
                        }
                    }
                }
                keymap::dispatch::Action::Redo => {
                    if let Some(page_id) = self.active_page().cloned() {
                        self.writer.apply(crate::BufferMessage::Redo { page_id: page_id.clone() });
                        // Fix cursor bounds after redo
                        if let Some(buf) = self.writer.buffers().get(&page_id) {
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
                keymap::dispatch::Action::QuickCapture(kind) => {
                    self.quick_capture = Some(QuickCaptureState {
                        kind,
                        input: String::new(),
                        cursor_pos: 0,
                    });
                }
                keymap::dispatch::Action::SubmitQuickCapture(kind, text) => {
                    if !text.is_empty() {
                        self.submit_quick_capture(&kind, &text);
                    }
                }
                keymap::dispatch::Action::CancelQuickCapture => {
                    // State already cleared in handle_quick_capture_key
                }
                keymap::dispatch::Action::OpenDatePicker(purpose) => {
                    let today = journal::Journal::today();
                    let original = self.active_page().cloned();
                    self.date_picker_state = Some(DatePickerState {
                        selected_date: self.last_viewed_journal_date.unwrap_or(today),
                        purpose,
                        pending_bracket: None,
                        original_page: original,
                        preview_buffers: Vec::new(),
                    });
                    // Load initial preview
                    self.update_calendar_preview();
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
                        self.handle_picker_selection(&ap.action, selected.clone());
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
                                    self.writer.apply(crate::BufferMessage::Reload {
                                        page_id: page_id.clone(),
                                        content,
                                    });
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
                    vec![keymap::dispatch::Action::SubmitQuickCapture(qc.kind, qc.input)]
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

    pub(crate) fn handle_date_picker_key(
        &mut self,
        key: &types::KeyEvent,
    ) -> Vec<keymap::dispatch::Action> {
        use chrono::{Datelike, Duration, NaiveDate};
        use types::KeyCode;

        let Some(dp) = &mut self.date_picker_state else {
            return vec![];
        };

        // Handle pending bracket: [d / ]d to skip to prev/next journal day
        if let Some(bracket) = dp.pending_bracket.take() {
            if let KeyCode::Char('d') = &key.code {
                let current = dp.selected_date;
                if let (Some(journal), Some(store)) = (&self.journal, &self.note_store) {
                    let target = if bracket == '[' {
                        journal.prev_date(current, store)
                    } else {
                        journal.next_date(current, store)
                    };
                    if let Some(date) = target {
                        if let Some(dp) = &mut self.date_picker_state {
                            dp.selected_date = date;
                        }
                        self.update_calendar_preview();
                    }
                }
                return vec![keymap::dispatch::Action::Noop];
            }
        }

        // Helper: update selected date and preview
        macro_rules! nav {
            ($new_date:expr) => {{
                let d = $new_date;
                if let Some(dp) = &mut self.date_picker_state {
                    dp.selected_date = d;
                }
                self.update_calendar_preview();
                vec![keymap::dispatch::Action::Noop]
            }};
        }

        match &key.code {
            // [ / ] start bracket prefix for skip navigation
            KeyCode::Char('[') | KeyCode::Char(']') => {
                if let KeyCode::Char(c) = &key.code {
                    dp.pending_bracket = Some(*c);
                }
                vec![keymap::dispatch::Action::Noop]
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                // Close all preview buffers and restore the original page
                let original = dp.original_page.clone();
                let previews: Vec<types::PageId> = dp.preview_buffers.clone();
                self.date_picker_state = None;
                for pid in &previews {
                    self.writer.apply(crate::BufferMessage::Close {
                        page_id: pid.clone(),
                    });
                }
                if let Some(page_id) = original {
                    self.set_active_page(Some(page_id));
                }
                vec![keymap::dispatch::Action::Noop]
            }
            // h/l: prev/next day
            KeyCode::Char('h') | KeyCode::Left => {
                let d = dp.selected_date - Duration::days(1);
                nav!(d)
            }
            KeyCode::Char('l') | KeyCode::Right => {
                let d = dp.selected_date + Duration::days(1);
                nav!(d)
            }
            // j/k: next/prev week
            KeyCode::Char('j') | KeyCode::Down => {
                let d = dp.selected_date + Duration::weeks(1);
                nav!(d)
            }
            KeyCode::Char('k') | KeyCode::Up => {
                let d = dp.selected_date - Duration::weeks(1);
                nav!(d)
            }
            // H/L: prev/next month
            KeyCode::Char('H') => {
                let d = dp.selected_date;
                let target = NaiveDate::from_ymd_opt(d.year(), d.month(), 1)
                    .and_then(|first| first.pred_opt())
                    .and_then(|prev_last| {
                        NaiveDate::from_ymd_opt(
                            prev_last.year(),
                            prev_last.month(),
                            d.day().min(prev_last.day()),
                        )
                    })
                    .unwrap_or(d);
                nav!(target)
            }
            KeyCode::Char('L') => {
                let d = dp.selected_date;
                let next_month = if d.month() == 12 {
                    NaiveDate::from_ymd_opt(d.year() + 1, 1, 1)
                } else {
                    NaiveDate::from_ymd_opt(d.year(), d.month() + 1, 1)
                };
                let target = next_month
                    .and_then(|first| {
                        let last_day = (first + Duration::days(31)).with_day(1)?.pred_opt()?;
                        NaiveDate::from_ymd_opt(first.year(), first.month(), d.day().min(last_day.day()))
                    })
                    .unwrap_or(d);
                nav!(target)
            }
            // Enter: confirm — keep current buffer, close other previews, enter JRNL mode
            KeyCode::Enter => {
                let date = dp.selected_date;
                let previews: Vec<types::PageId> = dp.preview_buffers.clone();
                // Drop the mutable borrow before accessing self
                let current_page = {
                    let _ = &self.date_picker_state; // reborrow check
                    self.active_page().cloned()
                };
                self.date_picker_state = None;
                for pid in &previews {
                    if current_page.as_ref() != Some(pid) {
                        self.writer.apply(crate::BufferMessage::Close {
                            page_id: pid.clone(),
                        });
                    }
                }
                self.last_viewed_journal_date = Some(date);
                self.in_journal_mode = true;
                self.journal_nav_at = Some(Instant::now());
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
                                let _ = std::fs::write(&config_path, "# Bloom configuration\n# See docs for all options.\n\n[startup]\nmode = \"journal\"\n");
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
        let Some(buf) = self.writer.buffers().get(&page_id) else {
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
            self.writer.apply(crate::BufferMessage::Edit {
                page_id,
                range: bracket_start..bracket_start + 3,
                replacement: "[x]".to_string(),
                cursor_after: cursor,
                cursor_idx: self.active_cursor_idx(),
            });
        } else if trimmed.starts_with("- [x] ") || trimmed.starts_with("- [X] ") {
            // Checked → unchecked
            let bracket_start = line_start + indent + 2;
            self.writer.apply(crate::BufferMessage::Edit {
                page_id,
                range: bracket_start..bracket_start + 3,
                replacement: "[ ]".to_string(),
                cursor_after: cursor,
                cursor_idx: self.active_cursor_idx(),
            });
        }
    }

    // -----------------------------------------------------------------------
    // Agenda overlay
    // -----------------------------------------------------------------------

    pub(crate) fn open_agenda(&mut self) {
        // Look for an existing page titled "Agenda"
        if let Some(idx) = &self.index {
            if let Some(meta) = idx.find_page_by_title("Agenda") {
                // Already open in a buffer?
                if self.writer.buffers().get(&meta.id).is_some() {
                    self.set_active_page(Some(meta.id.clone()));
                    return;
                }
                // Load from disk
                if let Some(vault_root) = &self.vault_root {
                    let path = vault_root.join(&meta.path);
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        self.open_page_with_content(&meta.id, "Agenda", &path, &content);
                        return;
                    }
                }
            }
        }

        // No Agenda page exists — create one from the built-in template
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let id = crate::uuid::generate_hex_id();
        let content = format!(
            "---\nid: {}\ntitle: \"Agenda\"\ncreated: {}\ntags: []\n---\n\n\
             ## Overdue\n\n\
             ## This Week\n\n\
             ## No Due Date\n",
            id.to_hex(),
            today,
        );

        // Write to disk if vault is initialized
        if let Some(vault_root) = &self.vault_root {
            let path = vault_root.join("pages").join("Agenda.md");
            if let Some(tx) = &self.autosave_tx {
                let _ = tx.send(bloom_store::disk_writer::WriteRequest {
                    path: path.clone(),
                    content: content.clone(),
                    write_id: 0,
                    buffer_version: 0,
                });
            }
            self.open_page_with_content(&id, "Agenda", &path, &content);
        } else {
            self.open_page_with_content(&id, "Agenda", std::path::Path::new("[agenda]"), &content);
        }
    }

    // Will be used by named views task toggle
    #[allow(dead_code)]
    pub(crate) fn toggle_task_in_page(&mut self, page_id: &types::PageId, line: usize) {
        // Ensure the page is loaded in a buffer
        let needs_load = self.writer.buffers().get(page_id).is_none();
        if needs_load {
            if let Some(idx) = &self.index {
                if let Some(meta) = idx.find_page_by_id(page_id) {
                    if let Ok(content) = std::fs::read_to_string(&meta.path) {
                        self.writer.apply(crate::BufferMessage::Open {
                            page_id: page_id.clone(),
                            title: meta.title.clone(),
                            path: meta.path.clone(),
                            content,
                        });
                    }
                }
            }
        }
        let Some(buf) = self.writer.buffers().get(page_id) else {
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
            self.writer.apply(crate::BufferMessage::Edit {
                page_id: page_id.clone(),
                range: bracket_start..bracket_start + 3,
                replacement: "[x]".to_string(),
                cursor_after: bracket_start + 3,
                cursor_idx: self.active_cursor_idx(),
            });
        } else if trimmed.starts_with("- [x] ") || trimmed.starts_with("- [X] ") {
            let bracket_start = line_start + indent + 2;
            self.writer.apply(crate::BufferMessage::Edit {
                page_id: page_id.clone(),
                range: bracket_start..bracket_start + 3,
                replacement: "[ ]".to_string(),
                cursor_after: bracket_start + 3,
                cursor_idx: self.active_cursor_idx(),
            });
        }
    }

    /// Close the active buffer. Opens journal or scratch if it was the last buffer.
    pub(crate) fn close_active_buffer(&mut self) {
        if let Some(page_id) = self.active_page().cloned() {
            // If this is a view buffer, clean up the view state
            if let Some(vs) = &self.active_view {
                if vs.buffer_id.as_ref() == Some(&page_id) {
                    let prev = vs.previous_page.clone();
                    self.active_view = None;
                    // After closing, restore the previous page if available
                    self.writer.apply(crate::BufferMessage::Close {
                        page_id: page_id.clone(),
                    });
                    if let Some(prev_id) = prev {
                        if self.writer.buffers().is_open(&prev_id) {
                            self.set_active_page(Some(prev_id));
                            return;
                        }
                    }
                    // Fall through to normal next-buffer logic
                    if let Some(next) = self.writer.buffers().open_buffers().first() {
                        self.set_active_page(Some(next.page_id.clone()));
                        self.set_cursor(0);
                    } else {
                        self.open_journal_today();
                    }
                    return;
                }
            }
            // Normal buffer close
            // Prune this page's persisted undo tree.
            if let Some(tx) = &self.indexer_tx {
                let _ = tx.send(index::indexer::IndexRequest::PruneUndoPages(vec![
                    page_id.to_hex()
                ]));
            }
            self.set_active_page(None);
            self.writer.apply(crate::BufferMessage::Close {
                page_id: page_id.clone(),
            });
            if let Some(next) = self.writer.buffers().open_buffers().first() {
                self.set_active_page(Some(next.page_id.clone()));
                self.set_cursor(0);
            } else {
                self.open_journal_today();
            }
        }
    }

    pub(crate) fn translate_vim_action(
        &mut self,
        action: bloom_vim::VimAction,
        prev_mode: bloom_vim::Mode,
    ) -> Vec<keymap::dispatch::Action> {
        // Read-only buffers: silently drop mutations, prevent Insert mode entry.
        // Navigation, search, Visual mode, Command mode all pass through.
        let is_ro = self
            .active_page()
            .map(|id| self.writer.buffers().is_read_only(id))
            .unwrap_or(false);
        if is_ro {
            match &action {
                bloom_vim::VimAction::Edit(_) => {
                    return vec![keymap::dispatch::Action::Noop];
                }
                bloom_vim::VimAction::ModeChange(mode)
                    if matches!(mode, bloom_vim::Mode::Insert) =>
                {
                    // Revert Vim's mode back to Normal
                    self.vim_state.force_normal_mode();
                    return vec![keymap::dispatch::Action::Noop];
                }
                _ => {} // motions, other mode changes — pass through
            }
        }

        match action {
            bloom_vim::VimAction::Edit(edit) => {
                self.pending_since = None;
                self.which_key_visible = false;
                if let Some(page_id) = self.active_page().cloned() {
                    self.writer.apply(crate::BufferMessage::Edit {
                        page_id: page_id.clone(),
                        range: edit.range.clone(),
                        replacement: edit.replacement.clone(),
                        cursor_after: edit.cursor_after,
                        cursor_idx: self.active_cursor_idx(),
                    });
                }
                // Check for inline completion triggers after an edit in Insert mode
                if matches!(self.vim_state.mode(), bloom_vim::Mode::Insert) {
                    self.check_inline_triggers();
                }
                vec![keymap::dispatch::Action::Edit(bloom_buffer::EditOp {
                    range: edit.range,
                    replacement: edit.replacement,
                    cursor_after: edit.cursor_after,
                })]
            }
            bloom_vim::VimAction::Motion(motion) => {
                self.pending_since = None;
                self.which_key_visible = false;
                self.inline_completion = None;

                // Debug: log cursor movement for diagnosing past-EOF bug
                if let Some(page_id) = self.active_page() {
                    if let Some(buf) = self.writer.buffers().get(page_id) {
                        let len = buf.text().len_chars();
                        let lines = buf.text().len_lines();
                        let old_cur = buf.cursor(0);
                        if motion.new_position > len || old_cur > len {
                            tracing::error!(
                                old_cursor = old_cur,
                                new_position = motion.new_position,
                                len_chars = len,
                                len_lines = lines,
                                "cursor past EOF detected!"
                            );
                        }
                    }
                }

                self.set_cursor(motion.new_position);
                vec![keymap::dispatch::Action::Motion(
                    keymap::dispatch::MotionResult {
                        new_position: motion.new_position,
                        extend_selection: motion.extend_selection,
                    },
                )]
            }
            bloom_vim::VimAction::ModeChange(ref mode) => {
                if !matches!(mode, bloom_vim::Mode::Insert) {
                    self.inline_completion = None;
                }
                let was_insert = matches!(prev_mode, bloom_vim::Mode::Insert);
                if matches!(mode, bloom_vim::Mode::Command) {
                    self.pending_since = Some(Instant::now());
                } else {
                    self.pending_since = None;
                    self.which_key_visible = false;
                }
                // Edit group lifecycle: begin on Insert entry, end on Insert exit
                if matches!(mode, bloom_vim::Mode::Insert) {
                    if let Some(page_id) = self.active_page().cloned() {
                        self.writer.apply(crate::BufferMessage::BeginEditGroup { page_id });
                    }
                } else if matches!(mode, bloom_vim::Mode::Normal) {
                    // Leaving Insert (or Visual, Command) → close the edit group first,
                    // then run system ops (block IDs, alignment, mirrors).
                    // System ops push their own undo nodes outside the group so that
                    // undo first reverts the system change, then the user edit.
                    // TODO: merge system ops into the edit group for single-undo.
                    if let Some(page_id) = self.active_page().cloned() {
                        let is_ro = self.writer.buffers().is_read_only(&page_id);
                        self.writer.apply(crate::BufferMessage::EndEditGroup { page_id: page_id.clone() });
                        if !is_ro {
                            self.ensure_block_ids(&page_id);
                            if was_insert {
                                self.propagate_mirror_edit(&page_id);
                            }
                        }
                    }
                    // Auto-align only on Insert→Normal transition, skip for read-only
                    if was_insert {
                        let is_ro = self
                            .active_page()
                            .map(|id| self.writer.buffers().is_read_only(id))
                            .unwrap_or(true);
                        if !is_ro {
                            match self.config.auto_align {
                                config::AutoAlignMode::Page => {
                                    if let Some(page_id) = self.active_page().cloned() {
                                        self.writer.apply(crate::BufferMessage::AlignPage {
                                            page_id,
                                        });
                                    }
                                }
                                config::AutoAlignMode::Block => {
                                    let cursor_line = self.cursor_position().0;
                                    if let Some(page_id) = self.active_page().cloned() {
                                        self.writer.apply(crate::BufferMessage::AlignBlock {
                                            page_id,
                                            cursor_line,
                                        });
                                    }
                                }
                                config::AutoAlignMode::None => {}
                            }
                        }
                    }
                }
                vec![keymap::dispatch::Action::ModeChange(mode.clone())]
            }
            bloom_vim::VimAction::Command(cmd) => self.handle_vim_command(&cmd),
            bloom_vim::VimAction::Pending => {
                if self.pending_since.is_none() {
                    self.pending_since = Some(Instant::now());
                }
                vec![keymap::dispatch::Action::Noop]
            }
            bloom_vim::VimAction::Unhandled => vec![keymap::dispatch::Action::Noop],
            bloom_vim::VimAction::RestoreCheckpoint => {
                if let Some(page_id) = self.active_page().cloned() {
                    if let Some(buf) = self.writer.buffers_mut().get_mut(&page_id) {
                        buf.restore_edit_group_checkpoint();
                        self.set_cursor(0);
                    }
                }
                vec![keymap::dispatch::Action::Noop]
            }
            bloom_vim::VimAction::Composite(actions) => actions
                .into_iter()
                .flat_map(|a| self.translate_vim_action(a, prev_mode.clone()))
                .collect(),
        }
    }

    pub(crate) fn handle_vim_command(&mut self, cmd: &str) -> Vec<keymap::dispatch::Action> {
        // Resolve partial commands: if the typed text is a prefix of exactly
        // one command (or the first matching command), use the full command.
        let resolved = resolve_command(cmd);
        match resolved.as_str() {
            "undo" => vec![keymap::dispatch::Action::Undo],
            "redo" => vec![keymap::dispatch::Action::Redo],
            "search-forward" => {
                self.begin_search(true);
                vec![keymap::dispatch::Action::Noop]
            }
            "search-backward" => {
                self.begin_search(false);
                vec![keymap::dispatch::Action::Noop]
            }
            "next-match" => {
                self.jump_to_match(true);
                vec![keymap::dispatch::Action::Noop]
            }
            "prev-match" => {
                self.jump_to_match(false);
                vec![keymap::dispatch::Action::Noop]
            }
            "repeat" => {
                // Dot repeat: replay the last repeatable command's keys.
                if let Some(recorded) = self.vim_state.last_command().cloned() {
                    for key in recorded.keys {
                        self.handle_key(key);
                    }
                }
                vec![keymap::dispatch::Action::Noop]
            }
            _ if resolved.starts_with("play-macro:") => {
                // Macro replay: get stored keys from register, replay them.
                let register = resolved.chars().last().unwrap_or('a');
                let keys = self.vim_state.play_macro(register);
                for key in keys {
                    let actions = self.handle_key(key);
                    // Actions are applied inline (handle_key dispatches)
                    let _ = actions;
                }
                vec![keymap::dispatch::Action::Noop]
            }
            "bracket:[d" => {
                self.navigate_journal(-1);
                vec![keymap::dispatch::Action::Noop]
            }
            "bracket:]d" => {
                self.navigate_journal(1);
                vec![keymap::dispatch::Action::Noop]
            }
            "bracket:[l" => {
                // Jump to previous broken link (existing G20 feature)
                vec![keymap::dispatch::Action::Noop] // TODO
            }
            "bracket:]l" => {
                // Jump to next broken link (existing G20 feature)
                vec![keymap::dispatch::Action::Noop] // TODO
            }
            _ if resolved.starts_with("bracket:") => {
                // Unrecognized bracket command — ignore
                vec![keymap::dispatch::Action::Noop]
            }
            _ => self.translate_ex_command(&resolved),
        }
    }
}

/// Resolve a partial command to its full form. If the input matches
/// a known ex-command prefix uniquely (or is the first match), return
/// the full command. Also handles "theme <partial>" argument completion.
fn resolve_command(cmd: &str) -> String {
    let trimmed = cmd.trim();

    // Handle :theme <partial_name>
    if let Some(arg) = trimmed.strip_prefix("theme ") {
        let arg = arg.trim();
        if let Some(name) = bloom_md::theme::THEME_NAMES
            .iter()
            .find(|n| n.starts_with(arg))
        {
            return format!("theme {name}");
        }
        return trimmed.to_string();
    }

    // Exact match — no resolution needed
    if EX_COMMANDS.iter().any(|(c, _)| *c == trimmed)
        || matches!(
            trimmed,
            "q" | "q!"
                | "qa" | "qa!"
                | "quit"
                | "quit!"
                | "quitall"
                | "w"
                | "write"
                | "wq"
                | "wq!"
                | "x"
                | "x!"
                | "e"
                | "edit"
                | "bd"
                | "bdelete"
                | "sp"
                | "split"
                | "vs"
                | "vsplit"
                | "undo"
                | "redo"
        )
    {
        return trimmed.to_string();
    }

    // Prefix match against EX_COMMANDS
    let matches: Vec<&str> = EX_COMMANDS
        .iter()
        .filter(|(c, _)| c.starts_with(trimmed))
        .map(|(c, _)| *c)
        .collect();
    if matches.len() == 1 {
        return matches[0].to_string();
    }

    trimmed.to_string()
}

impl BloomEditor {
    fn check_inline_triggers(&mut self) {
        let cursor = self.cursor();
        let Some(page_id) = self.active_page().cloned() else {
            return;
        };
        let Some(buf) = self.writer.buffers().get(&page_id) else {
            return;
        };
        let rope = buf.text();

        // Already in a completion session — validate the cursor is still past
        // the trigger position; if not, cancel.
        if let Some(ref ic) = self.inline_completion {
            if cursor < ic.trigger_pos {
                self.inline_completion = None;
            }
            return;
        }

        // Check for [[ trigger: cursor >= 2, two preceding chars are '[['
        if cursor >= 2 {
            let c1 = rope.char(cursor - 2);
            let c2 = rope.char(cursor - 1);
            if c1 == '[' && c2 == '[' {
                self.inline_completion = Some(InlineCompletion {
                    kind: InlineCompletionKind::Link,
                    trigger_pos: cursor, // query starts after [[
                    selected: 0,
                });
                return;
            }
        }

        // Check for # trigger: char before cursor is #, preceded by whitespace
        // or start-of-line.
        if cursor >= 1 {
            let prev = rope.char(cursor - 1);
            if prev == '#' {
                let is_valid_start = cursor < 2 || {
                    let before_hash = rope.char(cursor - 2);
                    before_hash.is_whitespace() || before_hash == '\n'
                };
                if is_valid_start {
                    self.inline_completion = Some(InlineCompletion {
                        kind: InlineCompletionKind::Tag,
                        trigger_pos: cursor, // query starts after #
                        selected: 0,
                    });
                }
            }
        }
    }

    fn accept_inline_completion(&mut self) {
        let Some(ic) = self.inline_completion.take() else {
            return;
        };
        let items = self.collect_inline_items(&ic);
        let selected = ic.selected.min(items.len().saturating_sub(1));
        let Some(item) = items.get(selected) else {
            return;
        };

        let Some(page_id) = self.active_page().cloned() else {
            return;
        };

        let cursor = self.cursor();

        match ic.kind {
            InlineCompletionKind::Link => {
                // Replace from [[ (trigger_pos - 2) to cursor with [[id|label]]
                let start = ic.trigger_pos.saturating_sub(2);
                let replacement = format!(
                    "[[{}|{}]]",
                    item.id.as_deref().unwrap_or(&item.label),
                    item.label
                );
                let new_cursor = start + replacement.len();
                self.writer.apply(crate::BufferMessage::Edit {
                    page_id,
                    range: start..cursor,
                    replacement,
                    cursor_after: new_cursor,
                    cursor_idx: self.active_cursor_idx(),
                });
            }
            InlineCompletionKind::Tag => {
                // Replace from # (trigger_pos - 1) to cursor with #tagname
                let start = ic.trigger_pos.saturating_sub(1);
                let replacement = format!("#{}", item.label);
                let new_cursor = start + replacement.len();
                self.writer.apply(crate::BufferMessage::Edit {
                    page_id,
                    range: start..cursor,
                    replacement,
                    cursor_after: new_cursor,
                    cursor_idx: self.active_cursor_idx(),
                });
            }
        }
    }

    pub(crate) fn collect_inline_items(
        &self,
        ic: &InlineCompletion,
    ) -> Vec<render::InlineMenuItem> {
        // Extract query text from the buffer (text between trigger_pos and cursor).
        let query = if let Some(page_id) = self.active_page() {
            if let Some(buf) = self.writer.buffers().get(page_id) {
                let rope = buf.text();
                let end = self.cursor().min(rope.len_chars());
                let start = ic.trigger_pos.min(end);
                rope.slice(start..end).to_string()
            } else {
                String::new()
            }
        } else {
            String::new()
        };
        let query_lower = query.to_lowercase();

        match ic.kind {
            InlineCompletionKind::Link => self
                .collect_page_items()
                .into_iter()
                .filter(|item| query.is_empty() || item.label.to_lowercase().contains(&query_lower))
                .take(10)
                .map(|item| render::InlineMenuItem {
                    id: Some(item.id),
                    label: item.label,
                    right: item.right,
                })
                .collect(),
            InlineCompletionKind::Tag => {
                if let Some(idx) = &self.index {
                    idx.all_tags()
                        .into_iter()
                        .filter(|(tag, _)| {
                            query.is_empty() || tag.0.to_lowercase().contains(&query_lower)
                        })
                        .take(10)
                        .map(|(tag, count)| render::InlineMenuItem {
                            id: None,
                            label: tag.0,
                            right: Some(format!("{count}")),
                        })
                        .collect()
                } else {
                    Vec::new()
                }
            }
        }
    }

    pub(crate) fn handle_view_key(
        &mut self,
        key: &types::KeyEvent,
    ) -> Vec<keymap::dispatch::Action> {
        let Some(view_state) = &self.active_view else {
            return vec![];
        };

        // Prompt mode: text input for query editing
        if view_state.is_prompt {
            match &key.code {
                types::KeyCode::Esc => {
                    self.close_active_view();
                    return vec![keymap::dispatch::Action::Noop];
                }
                _ => return self.handle_view_prompt_key(key),
            }
        }

        // Named view: only intercept Enter in Normal mode (jump to source).
        // Everything else passes through to Vim — the buffer is read-only
        // so mutations are silently dropped. Close via :q or SPC b d.
        if matches!(self.vim_state.mode(), bloom_vim::Mode::Normal) {
            match &key.code {
                types::KeyCode::Enter => {
                    let cursor_line = self.cursor_position().0;
                    let source = view_state.row_map.get(cursor_line).cloned();
                    if let Some(RowSource::Source {
                        page_id,
                        page_title,
                        line,
                        ..
                    }) = source
                    {
                        self.close_active_view();
                        if let Some(pid) = types::PageId::from_hex(&page_id) {
                            if let Some(idx) = &self.index {
                                if let Some(meta) = idx.find_page_by_id(&pid) {
                                    let full = self
                                        .vault_root
                                        .as_ref()
                                        .map(|r| r.join(&meta.path))
                                        .unwrap_or_else(|| meta.path.clone());
                                    if let Ok(content) = std::fs::read_to_string(&full) {
                                        self.open_page_with_content(
                                            &pid, &page_title, &full, &content,
                                        );
                                        self.set_cursor(line);
                                    }
                                }
                            }
                        }
                    }
                    return vec![keymap::dispatch::Action::Noop];
                }
                types::KeyCode::Char('x') => {
                    // Toggle task at cursor line via RowSource
                    let cursor_line = self.cursor_position().0;
                    if let Some(source) = view_state.row_map.get(cursor_line).cloned() {
                        self.handle_view_toggle_task(&source);
                    }
                    return vec![keymap::dispatch::Action::Noop];
                }
                _ => {}
            }
        }
        vec![] // Pass through to Vim
    }

    /// Close the active view and restore the previous page.
    fn close_active_view(&mut self) {
        if let Some(vs) = self.active_view.take() {
            if let Some(buf_id) = &vs.buffer_id {
                self.writer.apply(crate::BufferMessage::Close {
                    page_id: buf_id.clone(),
                });
            }
            if let Some(prev) = vs.previous_page {
                self.set_active_page(Some(prev));
            }
        }
    }

    /// After Insert→Normal, if the cursor line has a `^=` mirror marker,
    /// propagate the full line to all mirror peers via MirrorEdit.
    fn propagate_mirror_edit(&mut self, page_id: &types::PageId) {
        let cursor_line = self.cursor_position().0;
        let (line_text, block_id) = {
            let Some(buf) = self.writer.buffers().get(page_id) else {
                return;
            };
            if cursor_line >= buf.len_lines() {
                return;
            }
            let line_text = buf.line(cursor_line).to_string();
            let bid = bloom_md::parser::extensions::parse_block_id(&line_text, cursor_line);
            match bid {
                Some(b) if b.is_mirror => (line_text, b.id.0),
                _ => return, // not a mirror line — nothing to propagate
            }
        };

        let Some(idx) = &self.index else { return };
        let mirrors = idx.find_all_pages_by_block_id(&types::BlockId(block_id));
        let new_trimmed = line_text.trim_end_matches('\n');
        let mut mirror_count = 0usize;

        for (meta, mirror_line) in &mirrors {
            if meta.id == *page_id {
                continue; // skip the source page
            }
            // Load mirror page into buffer if needed
            let need_load = self.writer.buffers().get(&meta.id).is_none();
            if need_load {
                let full = self
                    .vault_root
                    .as_ref()
                    .map(|r| r.join(&meta.path))
                    .unwrap_or_else(|| meta.path.clone());
                if let Ok(content) = std::fs::read_to_string(&full) {
                    self.writer.apply(crate::BufferMessage::Open {
                        page_id: meta.id.clone(),
                        title: meta.title.clone(),
                        path: full,
                        content,
                    });
                }
            }
            // Replace the mirror line
            if let Some(buf) = self.writer.buffers_mut().get_mut(&meta.id) {
                if *mirror_line < buf.len_lines() {
                    let old_line = buf.line(*mirror_line).to_string();
                    let old_trimmed = old_line.trim_end_matches('\n');
                    let ls = buf.text().line_to_char(*mirror_line);
                    buf.replace(ls..ls + old_trimmed.len(), new_trimmed);
                }
            }
            self.save_page(&meta.id);
            mirror_count += 1;
        }
        if mirror_count > 0 {
            self.push_notification(
                format!("🪞 Updated {} mirror{}", mirror_count, if mirror_count == 1 { "" } else { "s" }),
                crate::render::NotificationLevel::Info,
            );
        }
    }

    /// Toggle a task from a view by finding the source line and flipping the checkbox.
    /// Then re-render the view buffer with fresh BQL results.
    fn handle_view_toggle_task(&mut self, source: &RowSource) {
        let RowSource::Source { page_id, line, .. } = source else {
            return;
        };
        let Some(pid) = types::PageId::from_hex(page_id) else {
            return;
        };

        // Load the source page into a buffer if not already open
        let need_load = self.writer.buffers().get(&pid).is_none();
        if need_load {
            if let Some(idx) = &self.index {
                if let Some(meta) = idx.find_page_by_id(&pid) {
                    let full = self
                        .vault_root
                        .as_ref()
                        .map(|r| r.join(&meta.path))
                        .unwrap_or_else(|| meta.path.clone());
                    if let Ok(content) = std::fs::read_to_string(&full) {
                        self.writer.apply(crate::BufferMessage::Open {
                            page_id: pid.clone(),
                            title: meta.title.clone(),
                            path: full.clone(),
                            content,
                        });
                    }
                }
            }
        }

        // Find the line and flip the checkbox
        let mut toggled_new_text: Option<String> = None;
        let mut block_id_on_line: Option<String> = None;
        if let Some(buf) = self.writer.buffers_mut().get_mut(&pid) {
            let line_count = buf.len_lines();
            if *line < line_count {
                let line_text = buf.line(*line).to_string();
                let new_text = if line_text.contains("- [ ] ") {
                    line_text.replacen("- [ ] ", "- [x] ", 1)
                } else if line_text.contains("- [x] ") {
                    line_text.replacen("- [x] ", "- [ ] ", 1)
                } else {
                    return; // not a task line
                };
                let line_start = buf.text().line_to_char(*line);
                let old_trimmed = line_text.trim_end_matches('\n');
                let new_trimmed = new_text.trim_end_matches('\n');
                buf.replace(line_start..line_start + old_trimmed.len(), new_trimmed);
                toggled_new_text = Some(new_trimmed.to_string());
                // Extract block ID via parser
                if let Some(bid) = bloom_md::parser::extensions::parse_block_id(old_trimmed, *line) {
                    block_id_on_line = Some(bid.id.0);
                }
            }
        }

        // Save the source page
        self.save_page(&pid);

        // Mirror propagation: if the toggled line has a block ID, find all other
        // pages containing that block and update the line there too.
        if let (Some(new_text), Some(bid)) = (&toggled_new_text, &block_id_on_line) {
            if let Some(idx) = &self.index {
                let block = types::BlockId(bid.clone());
                let mirrors = idx.find_all_pages_by_block_id(&block);
                for (meta, mirror_line) in &mirrors {
                    if meta.id == pid {
                        continue; // skip the source page itself
                    }
                    // Load mirror page, replace the line
                    let full = self
                        .vault_root
                        .as_ref()
                        .map(|r| r.join(&meta.path))
                        .unwrap_or_else(|| meta.path.clone());
                    let need_load = self.writer.buffers().get(&meta.id).is_none();
                    if need_load {
                        if let Ok(content) = std::fs::read_to_string(&full) {
                            self.writer.apply(crate::BufferMessage::Open {
                                page_id: meta.id.clone(),
                                title: meta.title.clone(),
                                path: full.clone(),
                                content,
                            });
                        }
                    }
                    if let Some(buf) = self.writer.buffers_mut().get_mut(&meta.id) {
                        if *mirror_line < buf.len_lines() {
                            let old_line = buf.line(*mirror_line).to_string();
                            let old_trimmed = old_line.trim_end_matches('\n');
                            let ls = buf.text().line_to_char(*mirror_line);
                            buf.replace(ls..ls + old_trimmed.len(), new_text);
                        }
                    }
                    self.save_page(&meta.id);
                }
            }
        }

        // Re-render the view with fresh results
        if let Some(vs) = self.active_view.as_mut() {
            if let Some(buf_id) = vs.buffer_id.take() {
                self.writer.apply(crate::BufferMessage::Close {
                    page_id: buf_id.clone(),
                });
            }
        }
        let mut vs = self.active_view.take().unwrap();
        self.render_view_to_buffer(&mut vs);
        self.active_view = Some(vs);
    }

    fn handle_view_prompt_key(
        &mut self,
        key: &types::KeyEvent,
    ) -> Vec<keymap::dispatch::Action> {
        match &key.code {
            types::KeyCode::Enter => {
                // Execute query and render to buffer
                if let Some(view_state) = &mut self.active_view {
                    if view_state.is_prompt && !view_state.query_input.is_empty() {
                        view_state.query = view_state.query_input.clone();
                    }
                }
                if let Some(vs) = self.active_view.as_mut() {
                    // Close old preview buffer if any
                    if let Some(old_id) = vs.buffer_id.take() {
                        self.writer.apply(crate::BufferMessage::Close {
                            page_id: old_id.clone(),
                        });
                    }
                }
                let mut vs = self.active_view.take().unwrap();
                self.render_view_to_buffer(&mut vs);
                self.active_view = Some(vs);
                vec![keymap::dispatch::Action::Noop]
            }
            types::KeyCode::Backspace => {
                if let Some(view_state) = &mut self.active_view {
                    if view_state.query_cursor > 0 {
                        view_state.query_input.remove(view_state.query_cursor - 1);
                        view_state.query_cursor -= 1;
                    }
                }
                vec![keymap::dispatch::Action::Noop]
            }
            types::KeyCode::Left => {
                if let Some(view_state) = &mut self.active_view {
                    view_state.query_cursor = view_state.query_cursor.saturating_sub(1);
                }
                vec![keymap::dispatch::Action::Noop]
            }
            types::KeyCode::Right => {
                if let Some(view_state) = &mut self.active_view {
                    view_state.query_cursor = (view_state.query_cursor + 1).min(view_state.query_input.len());
                }
                vec![keymap::dispatch::Action::Noop]
            }
            types::KeyCode::Char(c) => {
                if let Some(view_state) = &mut self.active_view {
                    view_state.query_input.insert(view_state.query_cursor, *c);
                    view_state.query_cursor += 1;
                }
                vec![keymap::dispatch::Action::Noop]
            }
            _ => vec![keymap::dispatch::Action::Noop],
        }
    }

    // ── In-buffer search (/  ?  n  N) ────────────────────────────────

    /// Open the search prompt. `forward`: true for `/`, false for `?`.
    fn begin_search(&mut self, forward: bool) {
        self.search_active = true;
        self.search_forward = forward;
        self.search_origin = self.cursor();
        self.vim_state.set_command_line("");
        self.vim_state.force_mode(bloom_vim::Mode::Command);
    }

    /// Handle keystrokes while the search prompt is active.
    fn handle_search_key(
        &mut self,
        key: types::KeyEvent,
    ) -> Vec<keymap::dispatch::Action> {
        match key.code {
            types::KeyCode::Esc => {
                self.search_active = false;
                self.set_cursor(self.search_origin);
                self.vim_state.force_mode(bloom_vim::Mode::Normal);
                self.vim_state.set_command_line("");
            }
            types::KeyCode::Enter => {
                let pattern = self.vim_state.pending_keys().to_string();
                self.search_active = false;
                self.vim_state.force_mode(bloom_vim::Mode::Normal);
                self.vim_state.set_command_line("");
                if !pattern.is_empty() {
                    self.search_pattern = Some(pattern);
                    // Cursor is already at the match from live search — don't re-jump
                }
            }
            types::KeyCode::Backspace => {
                let mut text = self.vim_state.pending_keys().to_string();
                text.pop();
                if text.is_empty() {
                    self.search_active = false;
                    self.set_cursor(self.search_origin);
                    self.vim_state.force_mode(bloom_vim::Mode::Normal);
                    self.vim_state.set_command_line("");
                } else {
                    self.vim_state.set_command_line(&text);
                    self.search_pattern = Some(text);
                    self.jump_to_match_from(self.search_origin, self.search_forward);
                }
            }
            types::KeyCode::Char(c) => {
                let mut text = self.vim_state.pending_keys().to_string();
                text.push(c);
                self.vim_state.set_command_line(&text);
                self.search_pattern = Some(text);
                self.jump_to_match_from(self.search_origin, self.search_forward);
            }
            _ => {}
        }
        vec![keymap::dispatch::Action::Noop]
    }

    /// Jump to next (or prev) match of the active search pattern.
    fn jump_to_match(&mut self, forward: bool) {
        let cursor = self.cursor();
        let origin = if forward {
            cursor + 1
        } else {
            cursor.saturating_sub(1)
        };
        self.jump_to_match_from(origin, forward);
    }

    /// Jump to the first match from `origin` in direction, wrapping at EOF/BOF.
    fn jump_to_match_from(&mut self, origin: usize, forward: bool) {
        let pattern = match &self.search_pattern {
            Some(p) if !p.is_empty() => p.clone(),
            _ => return,
        };
        let Some(page_id) = self.active_page().cloned() else { return };
        let Some(buf) = self.writer.buffers().get(&page_id) else { return };
        let text = buf.text().to_string();
        let text_lower = text.to_lowercase();
        let pat_lower = pattern.to_lowercase();

        let mut positions: Vec<usize> = Vec::new();
        let mut start = 0;
        while let Some(pos) = text_lower[start..].find(&pat_lower) {
            positions.push(start + pos);
            start += pos + pat_lower.len();
        }

        if positions.is_empty() {
            return;
        }

        let target = if forward {
            positions
                .iter()
                .find(|&&p| p >= origin)
                .or(positions.first())
        } else {
            positions
                .iter()
                .rev()
                .find(|&&p| p < origin)
                .or(positions.last())
        };

        if let Some(&pos) = target {
            self.set_cursor(pos);
        }
    }

    // ── Mirror inline menu ───────────────────────────────────────────

    fn handle_mirror_menu_key(
        &mut self,
        key: types::KeyEvent,
    ) -> Vec<keymap::dispatch::Action> {
        match key.code {
            types::KeyCode::Esc | types::KeyCode::Char('q') => {
                self.mirror_menu = None;
            }
            types::KeyCode::Up | types::KeyCode::Char('k') => {
                if let Some(menu) = &mut self.mirror_menu {
                    if menu.selected > 0 {
                        menu.selected -= 1;
                    }
                }
            }
            types::KeyCode::Down | types::KeyCode::Char('j') => {
                if let Some(menu) = &mut self.mirror_menu {
                    if menu.selected + 1 < menu.items.len() {
                        menu.selected += 1;
                    }
                }
            }
            types::KeyCode::Enter => {
                if let Some(menu) = self.mirror_menu.take() {
                    if let Some(item) = menu.items.get(menu.selected) {
                        let pid = item.page_id.clone();
                        let line = item.line;
                        let title = item.title.clone();
                        if let Some(idx) = &self.index {
                            if let Some(meta) = idx.find_page_by_id(&pid) {
                                let full = self
                                    .vault_root
                                    .as_ref()
                                    .map(|r| r.join(&meta.path))
                                    .unwrap_or_else(|| meta.path.clone());
                                if let Ok(content) = std::fs::read_to_string(&full) {
                                    self.open_page_with_content(&pid, &title, &full, &content);
                                    if let Some(buf) = self.writer.buffers().get(&pid) {
                                        let target = buf
                                            .text()
                                            .line_to_char(line.min(buf.len_lines().saturating_sub(1)));
                                        self.set_cursor(target);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        vec![keymap::dispatch::Action::Noop]
    }
}
