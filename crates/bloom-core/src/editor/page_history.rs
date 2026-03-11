//! Page history overlay — `SPC H h`.
//!
//! Opens a split pane showing the git commit history for the current page.
//! Navigation: j/k to move, Enter to view read-only, d for diff, r to restore, q to close.

use crate::history::HistoryRequest;
use crate::window::{PaneKind, SplitDirection};
use crate::*;

impl BloomEditor {
    /// Open the page history split pane for the current page.
    pub(crate) fn open_page_history(&mut self) {
        let page_id = match self.active_page() {
            Some(id) => id.clone(),
            None => return,
        };

        let uuid_hex = page_id.to_hex();

        // Request history from the history thread.
        if let Some(tx) = &self.history_tx {
            let _ = tx.send(HistoryRequest::PageHistory {
                uuid: uuid_hex,
                limit: 100,
            });
        }

        // Open (or toggle) the PageHistory split pane.
        self.window_mgr
            .open_special_view(PaneKind::PageHistory, SplitDirection::Vertical);
    }

    /// Whether the active pane is a PageHistory pane.
    pub(crate) fn is_page_history_active(&self) -> bool {
        let active = self.window_mgr.active_pane();
        self.window_mgr.pane_kind(active) == Some(&PaneKind::PageHistory)
    }

    /// Handle keys when the page history pane is active.
    pub(crate) fn handle_page_history_key(
        &mut self,
        key: &types::KeyEvent,
    ) -> Vec<keymap::dispatch::Action> {
        match &key.code {
            types::KeyCode::Char('j') | types::KeyCode::Down => {
                self.page_history_move(1);
            }
            types::KeyCode::Char('k') | types::KeyCode::Up => {
                self.page_history_move(-1);
            }
            types::KeyCode::Char('r') => {
                self.page_history_restore();
            }
            types::KeyCode::Char('q') | types::KeyCode::Esc => {
                self.close_page_history();
            }
            _ => {}
        }
        vec![keymap::dispatch::Action::Noop]
    }

    /// Navigate page history selection (j/k in the history pane).
    pub(crate) fn page_history_move(&mut self, delta: i32) {
        if let Some(entries) = &self.page_history_entries {
            if entries.is_empty() {
                return;
            }
            let len = entries.len() as i32;
            let new_idx = (self.page_history_selected as i32 + delta).clamp(0, len - 1);
            self.page_history_selected = new_idx as usize;
        }
    }

    /// Restore the selected history version into the current buffer (undo-able).
    pub(crate) fn page_history_restore(&mut self) {
        let (oid, uuid) = match self.page_history_selected_entry() {
            Some((oid, _)) => {
                let uuid = self.active_page().map(|p| p.to_hex()).unwrap_or_default();
                (oid.to_string(), uuid)
            }
            None => return,
        };

        // Request blob content from the history thread.
        if let Some(tx) = &self.history_tx {
            let _ = tx.send(HistoryRequest::BlobAt { oid, uuid });
        }
    }

    /// Close the page history pane.
    pub(crate) fn close_page_history(&mut self) {
        if let Some(pane) = self.window_mgr.find_pane_by_kind(&PaneKind::PageHistory) {
            self.window_mgr.close(pane);
        }
        self.page_history_entries = None;
        self.page_history_selected = 0;
    }

    /// Get the currently selected history entry (oid, message).
    fn page_history_selected_entry(&self) -> Option<(&str, &str)> {
        let entries = self.page_history_entries.as_ref()?;
        let entry = entries.get(self.page_history_selected)?;
        Some((&entry.oid, &entry.message))
    }
}
