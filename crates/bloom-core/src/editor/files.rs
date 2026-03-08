//! File save, auto-save, and watcher integration.
//!
//! Handles explicit `:w` saves, debounced auto-save via the background
//! [`DiskWriter`](crate::store::DiskWriter), and file-watcher events with
//! conflict detection (external-change dialog when a buffer is dirty).

use crate::*;

impl BloomEditor {
    /// Handle a single DiskWriter completion ack.
    /// Returns true if the visual state changed (dirty indicator flipped).
    pub fn handle_write_complete(&mut self, wc: store::disk_writer::WriteComplete) -> bool {
        self.last_write_fingerprints
            .insert(wc.path.clone(), (wc.mtime, wc.size));
        if let Some(page_id) = self.buffer_mgr.find_by_path(&wc.path).cloned() {
            if let Some(buf) = self.buffer_mgr.get_mut(&page_id) {
                if buf.is_dirty() {
                    buf.mark_clean();
                    return true;
                }
            }
        }
        false
    }

    /// Handle a single file watcher event.
    /// Returns true if the visual state changed (dialog shown, buffer reloaded, or dirty flag flipped).
    pub fn handle_file_event(&mut self, event: store::traits::FileEvent) -> bool {
        let path = match &event {
            store::traits::FileEvent::Created(p)
            | store::traits::FileEvent::Modified(p)
            | store::traits::FileEvent::Deleted(p) => p.clone(),
            store::traits::FileEvent::Renamed { to, .. } => to.clone(),
        };
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            return false;
        }
        let Some(vault_root) = &self.vault_root else {
            return false;
        };
        let Ok(rel) = path.strip_prefix(vault_root) else {
            return false;
        };
        let first = rel.components().next().and_then(|c| c.as_os_str().to_str());
        if !matches!(first, Some("pages") | Some("journal")) {
            return false;
        }

        let mut visual_changed = false;

        if let Some(page_id) = self.buffer_mgr.find_by_path(&path).cloned() {
            let is_own_write = if let Some((recorded_mtime, recorded_size)) =
                self.last_write_fingerprints.remove(&path)
            {
                std::fs::metadata(&path)
                    .map(|meta| {
                        meta.len() == recorded_size && meta.modified().ok() == Some(recorded_mtime)
                    })
                    .unwrap_or(false)
            } else {
                false
            };

            if is_own_write {
                // Fingerprint matched — already marked clean in handle_write_complete
            } else if let Ok(disk_content) = std::fs::read_to_string(&path) {
                let buf_content = self.buffer_mgr.get(&page_id).map(|b| b.text().to_string());
                if buf_content.as_deref() == Some(disk_content.as_str()) {
                    if let Some(buf) = self.buffer_mgr.get_mut(&page_id) {
                        if buf.is_dirty() {
                            buf.mark_clean();
                            visual_changed = true;
                        }
                    }
                } else {
                    let is_dirty = self.buffer_mgr.get(&page_id).is_some_and(|b| b.is_dirty());
                    if is_dirty {
                        self.active_dialog = Some(ActiveDialog::FileChanged {
                            page_id,
                            path: path.clone(),
                            selected: 0,
                        });
                        visual_changed = true;
                    } else {
                        self.buffer_mgr.reload(&page_id, &disk_content);
                        self.set_cursor(0);
                        visual_changed = true;
                    }
                }
            }
        }

        self.pending_file_events.insert(rel.to_path_buf());
        self.file_event_deadline =
            Some(std::time::Instant::now() + std::time::Duration::from_millis(300));

        visual_changed
    }

    /// Flush debounced file events to the indexer if the deadline has passed.
    /// Returns true if a batch was sent.
    pub fn flush_file_event_debounce(&mut self) -> bool {
        if let Some(deadline) = self.file_event_deadline {
            if std::time::Instant::now() >= deadline && !self.pending_file_events.is_empty() {
                let paths: Vec<std::path::PathBuf> = self.pending_file_events.drain().collect();
                self.file_event_deadline = None;
                if let Some(tx) = &self.indexer_tx {
                    let _ = tx.send(index::indexer::IndexRequest::IncrementalBatch(paths));
                    self.indexing = true;
                    return true;
                }
            }
        }
        false
    }

    /// Schedule an auto-save for the given page via the disk writer thread.
    pub(crate) fn schedule_autosave(&self, page_id: &types::PageId) {
        let Some(tx) = &self.autosave_tx else { return };
        let Some(buf) = self.buffer_mgr.get(page_id) else {
            return;
        };
        let buffers = self.buffer_mgr.open_buffers();
        let Some(info) = buffers.iter().find(|b| b.page_id == *page_id) else {
            return;
        };
        // Assign block IDs before extracting content for write.
        // We can't mutate the buffer here (we only have &self), but block IDs
        // will be assigned on the next save_current() or when the buffer is
        // next obtained mutably. For autosave, we send the current text as-is
        // and rely on save_current() for ID assignment.
        let content = buf.text().to_string();
        let _ = tx.send(store::disk_writer::WriteRequest {
            path: info.path.clone(),
            content,
        });
    }

    /// Assign block IDs to the buffer if any blocks are missing them.
    /// Modifies the rope in-place. Returns true if any IDs were added.
    fn ensure_block_ids(&mut self, page_id: &types::PageId) -> bool {
        let Some(buf) = self.buffer_mgr.get(page_id) else {
            return false;
        };
        let text = buf.text().to_string();
        let doc = self.parser.parse(&text);
        let insertions = block_id_gen::compute_block_id_assignments(&doc);
        if insertions.is_empty() {
            return false;
        }

        let Some(buf) = self.buffer_mgr.get_mut(page_id) else {
            return false;
        };

        // Apply insertions in reverse order to preserve char offsets.
        buf.begin_edit_group();
        for ins in insertions.iter().rev() {
            let line_idx = ins.line;
            if line_idx >= buf.len_lines() {
                continue;
            }
            let line_slice = buf.line(line_idx);
            let line_str: String = line_slice.chars().collect();
            let trimmed_len = line_str.trim_end_matches(['\n', '\r']).trim_end().len();
            let line_start = buf.text().line_to_char(line_idx);
            let insert_at = line_start + trimmed_len;
            let insertion_text = format!(" ^{}", ins.id);
            buf.insert(insert_at, &insertion_text);
        }
        buf.end_edit_group();

        true
    }

    pub fn save_current(&mut self) -> Result<(), error::BloomError> {
        if let Some(page_id) = self.active_page().cloned() {
            // Skip pseudo-paths like [scratch]
            let is_pseudo = self
                .buffer_mgr
                .open_buffers()
                .iter()
                .find(|b| b.page_id == page_id)
                .map_or(true, |info| info.path.to_string_lossy().starts_with('['));
            if is_pseudo {
                return Ok(());
            }

            // Assign block IDs before saving.
            self.ensure_block_ids(&page_id);

            let (content, path) = {
                if let Some((buf, info)) = self.buffer_mgr.get_with_info(&page_id) {
                    if !buf.is_dirty() {
                        return Ok(());
                    }
                    (buf.text().to_string(), info.path.clone())
                } else {
                    return Ok(());
                }
            };

            if let Some(tx) = &self.autosave_tx {
                let _ = tx.send(store::disk_writer::WriteRequest { path, content });
            } else {
                store::disk_writer::atomic_write(&path, &content)?;
                if let Some(buf) = self.buffer_mgr.get_mut(&page_id) {
                    buf.mark_clean();
                }
            }
        }
        Ok(())
    }

    /// Assign block IDs to all vault files that are missing them.
    ///
    /// Called after first indexing completes. Reads each `.md` file, parses it,
    /// generates IDs for blocks without them, and writes back via DiskWriter.
    /// Files that already have IDs on all blocks are skipped (no write).
    pub fn assign_block_ids_bulk(&mut self) {
        let Some(vault_root) = self.vault_root.clone() else {
            return;
        };
        let Some(tx) = &self.autosave_tx else { return };

        let store = match store::local::LocalFileStore::new(vault_root.clone()) {
            Ok(s) => s,
            Err(_) => return,
        };
        use store::traits::NoteStore;
        let mut paths = store.list_pages().unwrap_or_default();
        paths.extend(store.list_journals().unwrap_or_default());

        let mut assigned_count = 0usize;
        for rel_path in &paths {
            let full = vault_root.join(rel_path);
            let Ok(content) = std::fs::read_to_string(&full) else {
                continue;
            };
            let doc = self.parser.parse(&content);
            if let Some(new_content) = block_id_gen::assign_block_ids(&content, &doc) {
                let _ = tx.send(store::disk_writer::WriteRequest {
                    path: full,
                    content: new_content,
                });
                assigned_count += 1;
            }
        }

        if assigned_count > 0 {
            self.push_notification(
                format!("Assigned block IDs to {assigned_count} files"),
                render::NotificationLevel::Info,
            );
        }
    }
}
