//! Page history — `SPC H h`.
//!
//! Opens a temporal strip showing the unified history: undo tree (recent,
//! branching) followed by git commits (older, linear). Preview shows diff.

use crate::history::HistoryRequest;
use crate::*;

impl BloomEditor {
    /// Open page history as a temporal strip.
    pub(crate) fn open_page_history(&mut self) {
        let page_id = match self.active_page() {
            Some(id) => id.clone(),
            None => return,
        };

        let current_content = match self.writer.buffers().get(&page_id) {
            Some(buf) => buf.text().to_string(),
            None => return,
        };

        // Collect undo tree nodes (recent history)
        let mut items: Vec<TemporalItem> = Vec::new();
        if let Some(buf) = self.writer.buffers().get(&page_id) {
            let tree = buf.undo_tree();
            let mut node_id = tree.current();
            let mut visited = std::collections::HashSet::new();
            while visited.insert(node_id) {
                let info = tree.node_info(node_id);
                let elapsed = info.timestamp.elapsed();
                let label = if elapsed.as_secs() < 60 {
                    format!("{}s", elapsed.as_secs())
                } else if elapsed.as_secs() < 3600 {
                    format!("{}m", elapsed.as_secs() / 60)
                } else {
                    format!("{}h", elapsed.as_secs() / 3600)
                };
                let branch_count = tree.children(node_id).len();
                items.push(TemporalItem {
                    label,
                    detail: Some(info.description.clone()),
                    kind: render::StripNodeKind::UndoNode,
                    branch_count,
                    content: Some(tree.node_snapshot_string(node_id)),
                    undo_node_id: Some(node_id),
                    git_oid: None,
                });
                match tree.parent(node_id) {
                    Some(parent) => node_id = parent,
                    None => break,
                }
            }
        }
        // Undo items are newest-first (from current to root). Reverse for left=older.
        items.reverse();

        // Request git history (async — will arrive via handle_history_complete)
        let uuid_hex = page_id.to_hex();
        if let Some(tx) = &self.history_tx {
            let _ = tx.send(HistoryRequest::PageHistory {
                uuid: uuid_hex,
                limit: 50,
            });
        }

        // Select the most recent item (rightmost)
        let selected = items.len().saturating_sub(1);

        self.temporal_strip = Some(TemporalStripState {
            mode: render::TemporalMode::PageHistory,
            items,
            selected,
            compact: true,
            page_id,
            current_content,
        });
    }

    /// Append git history entries to the temporal strip when they arrive.
    pub(crate) fn append_git_history(&mut self, entries: &[history::PageHistoryEntry]) {
        let Some(ts) = &mut self.temporal_strip else { return };
        if !matches!(ts.mode, render::TemporalMode::PageHistory) {
            return;
        }

        // Git entries are newest-first. Insert at the BEGINNING (older = left).
        let git_items: Vec<TemporalItem> = entries
            .iter()
            .rev()
            .map(|entry| {
                let date = chrono::DateTime::from_timestamp(entry.timestamp, 0)
                    .map(|dt| dt.format("%b %-d").to_string())
                    .unwrap_or_else(|| "?".to_string());
                TemporalItem {
                    label: date,
                    detail: Some(entry.message.clone()),
                    kind: render::StripNodeKind::GitCommit,
                    branch_count: 0,
                    content: None, // Loaded on-demand via BlobAt
                    undo_node_id: None,
                    git_oid: Some(entry.oid.clone()),
                }
            })
            .collect();

        let git_count = git_items.len();
        // Insert git items before undo items (older on the left)
        let mut new_items = git_items;
        new_items.append(&mut ts.items);
        ts.items = new_items;
        // Adjust selected to keep pointing at the same item
        ts.selected += git_count;
    }

    /// Handle keys when temporal strip is active.
    pub(crate) fn handle_temporal_strip_key(
        &mut self,
        key: &types::KeyEvent,
    ) -> Vec<keymap::dispatch::Action> {
        match &key.code {
            types::KeyCode::Char('h') | types::KeyCode::Left => {
                if let Some(ts) = &mut self.temporal_strip {
                    if ts.selected > 0 {
                        ts.selected -= 1;
                        // Load git content on-demand
                        self.load_temporal_content_if_needed();
                    }
                }
            }
            types::KeyCode::Char('l') | types::KeyCode::Right => {
                if let Some(ts) = &mut self.temporal_strip {
                    if ts.selected + 1 < ts.items.len() {
                        ts.selected += 1;
                    }
                }
            }
            types::KeyCode::Char('e') => {
                if let Some(ts) = &mut self.temporal_strip {
                    ts.compact = !ts.compact;
                }
            }
            types::KeyCode::Char('r') => {
                self.temporal_strip_restore();
            }
            types::KeyCode::Char('q') | types::KeyCode::Esc => {
                self.temporal_strip = None;
            }
            _ => {}
        }
        vec![keymap::dispatch::Action::Noop]
    }

    /// Restore the selected temporal item to the buffer.
    fn temporal_strip_restore(&mut self) {
        let (content, undo_node_id, page_id) = {
            let Some(ts) = &self.temporal_strip else { return };
            let Some(item) = ts.items.get(ts.selected) else { return };
            (item.content.clone(), item.undo_node_id, ts.page_id.clone())
        };

        if let Some(node_id) = undo_node_id {
            // Restore from undo tree (preserves branching)
            if let Some(buf) = self.writer.buffers_mut().get_mut(&page_id) {
                buf.restore_state(node_id);
            }
        } else if let Some(content) = content {
            // Restore from git (replaces buffer content)
            self.writer.apply(crate::BufferMessage::Reload {
                page_id: page_id.clone(),
                content,
            });
        } else {
            return;
        }

        self.temporal_strip = None;
        self.save_page(&page_id);
        self.push_notification(
            "Restored from history".into(),
            render::NotificationLevel::Info,
        );
    }

    /// Load content for the selected temporal item (git commits are lazy-loaded).
    fn load_temporal_content_if_needed(&self) {
        let Some(ts) = &self.temporal_strip else { return };
        let Some(item) = ts.items.get(ts.selected) else { return };
        if item.content.is_some() {
            return; // Already loaded
        }
        if let Some(oid) = &item.git_oid {
            if let Some(tx) = &self.history_tx {
                let _ = tx.send(HistoryRequest::BlobAt {
                    oid: oid.clone(),
                    uuid: ts.page_id.to_hex(),
                });
            }
        }
    }
}
