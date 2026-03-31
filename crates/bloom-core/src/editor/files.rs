//! File save, auto-save, and watcher integration.
//!
//! All saves go through [`BloomEditor::save_page`] — the single save path.
//! Write IDs track each request so that stale completions don't clear dirty flags.
//! FileEvent handling uses content-hash self-write detection: we hash every
//! file we save and recognise watcher echoes of our own writes by matching
//! the disk content hash against the stored set.

use crate::*;

/// Hash content bytes with DefaultHasher (SipHash-2-4). Fast, in stdlib.
fn hash_content(content: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    content.hash(&mut hasher);
    hasher.finish()
}

/// Max hashes to retain per path (bounded buffer).
const MAX_SELF_WRITE_HASHES: usize = 8;

impl BloomEditor {
    /// Handle a DiskWriter result (success or failure).
    /// Returns true if the visual state changed (dirty indicator flipped).
    pub fn handle_write_result(&mut self, result: bloom_store::disk_writer::WriteResult) -> bool {
        match result {
            bloom_store::disk_writer::WriteResult::Complete {
                path,
                write_id,
                buffer_version,
            } => {
                tracing::debug!(path = %path.display(), write_id, "write complete");
                // Signal history thread
                if let Some(tx) = &self.history_tx {
                    let _ = tx.send(history::HistoryRequest::FileDirty);
                }
                if let Some(page_id) = self.writer.buffers().find_by_path(&path).cloned() {
                    self.durable_capture.mark_page_saved(&page_id);
                    // Mark buffer clean only if this is the LATEST write AND buffer
                    // hasn't been edited since the save was initiated.
                    if let Some(buf) = self.writer.buffers().get(&page_id) {
                        let is_latest = buf.pending_write_id() == Some(write_id);
                        let unchanged = buf.version() == buffer_version;
                        if is_latest && unchanged {
                            self.writer.apply(crate::BufferMessage::MarkClean {
                                page_id: page_id.clone(),
                            });
                            let filename = path
                                .file_name()
                                .map(|f| f.to_string_lossy().to_string())
                                .unwrap_or_else(|| "file".to_string());
                            self.push_notification(
                                format!("Saved {filename}"),
                                render::NotificationLevel::Info,
                            );
                            return true;
                        }
                    }
                    // Clear pending regardless (this write is done)
                    if let Some(buf) = self.writer.buffers_mut().get_mut(&page_id) {
                        if buf.pending_write_id() == Some(write_id) {
                            buf.clear_pending_write_id();
                        }
                    }
                }
                false
            }
            bloom_store::disk_writer::WriteResult::Failed {
                path,
                write_id,
                error,
            } => {
                tracing::error!(path = %path.display(), write_id, error = %error, "write failed");
                // Clear pending so future saves aren't blocked
                if let Some(page_id) = self.writer.buffers().find_by_path(&path).cloned() {
                    if let Some(buf) = self.writer.buffers_mut().get_mut(&page_id) {
                        if buf.pending_write_id() == Some(write_id) {
                            buf.clear_pending_write_id();
                        }
                    }
                }
                self.push_notification(
                    format!("Write failed: {error}"),
                    render::NotificationLevel::Error,
                );
                true
            }
        }
    }

    /// Handle a single file watcher event.
    /// Returns true if the visual state changed (dialog shown, buffer reloaded, or dirty flag flipped).
    pub fn handle_file_event(&mut self, event: bloom_store::traits::FileEvent) -> bool {
        let path = match &event {
            bloom_store::traits::FileEvent::Created(p)
            | bloom_store::traits::FileEvent::Modified(p)
            | bloom_store::traits::FileEvent::Deleted(p) => p.clone(),
            bloom_store::traits::FileEvent::Renamed { to, .. } => to.clone(),
        };
        tracing::debug!(path = %path.display(), event = ?std::mem::discriminant(&event), "file event received");
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

        if let Some(page_id) = self.writer.buffers().find_by_path(&path).cloned() {
            // Content comparison: read disk, compare to buffer. No fingerprints.
            if let Ok(disk_content) = std::fs::read_to_string(&path) {
                // ── Self-write detection ──
                // If the disk content hash matches one we recently wrote,
                // this is an echo of our own save — suppress entirely.
                let disk_hash = hash_content(&disk_content);
                if let Some(hashes) = self.self_write_hashes.get_mut(&path) {
                    if let Some(pos) = hashes.iter().position(|&h| h == disk_hash) {
                        hashes.remove(pos);
                        tracing::debug!(path = %path.display(), "file event: self-write detected, suppressing");
                        // Still queue for indexer (file changed on disk)
                        self.pending_file_events.insert(rel.to_path_buf());
                        self.file_event_deadline =
                            Some(std::time::Instant::now() + std::time::Duration::from_millis(300));
                        return false;
                    }
                }

                let buf_content = self
                    .writer
                    .buffers()
                    .document(&page_id)
                    .map(|doc| doc.canonical_text());
                if buf_content.as_deref() == Some(disk_content.as_str()) {
                    // Content matches — our write or identical external. No action.
                    tracing::debug!(path = %path.display(), "file event: content matches buffer");
                } else {
                    let is_dirty = self
                        .writer
                        .buffers()
                        .get(&page_id)
                        .is_some_and(|b| b.is_dirty());
                    if is_dirty {
                        // Conflict: buffer dirty + disk differs → ask user
                        self.active_dialog = Some(ActiveDialog::FileChanged {
                            page_id,
                            path: path.clone(),
                            selected: 0,
                        });
                        visual_changed = true;
                    } else {
                        // Clean buffer + disk differs → auto-reload
                        let cursor_policy = self
                            .writer
                            .buffers()
                            .get(&page_id)
                            .map(|b| crate::document::CursorPolicy::reanchor_to_cursor(b, 0))
                            .unwrap_or(crate::document::CursorPolicy::Explicit { idx: 0, pos: 0 });
                        self.writer.apply(crate::BufferMessage::Reload {
                            page_id: page_id.clone(),
                            content: disk_content,
                            cursor_policy,
                        });
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
                    tracing::info!(file_count = paths.len(), files = ?paths, "flushing file events to indexer");
                    let _ = tx.send(index::indexer::IndexRequest::IncrementalBatch(paths));
                    self.indexing = true;
                    return true;
                }
            }
        }
        false
    }

    // ---------------------------------------------------------------------
    // Unified save path
    // ---------------------------------------------------------------------

    /// The single save path. All saves — autosave, `:w`, session save — go here.
    ///
    /// 1. Skips pseudo-paths (`[scratch]`) and uninitialized vaults
    /// 2. Assigns block IDs to blocks that don't have them
    /// 3. Extracts content and writes via DiskWriter (or inline in tests)
    pub(crate) fn save_page(&mut self, page_id: &types::PageId) {
        tracing::debug!(page = %page_id.to_hex(), "save_page called");
        // Clear any pending autosave deadline (we're saving now).
        self.autosave_deadline = None;
        // Skip pseudo-paths like [scratch].
        let is_pseudo = self
            .writer
            .buffers_mut()
            .open_buffers()
            .iter()
            .find(|b| b.page_id == *page_id)
            .is_none_or(|info| info.path.to_string_lossy().starts_with('['));
        if is_pseudo {
            return;
        }

        // Saving is the file-truth boundary: make sure missing block IDs are
        // materialized into the document before we snapshot content for disk.
        self.ensure_block_ids(page_id);

        // Extract canonical content, path, and version.
        let (content, path, buffer_version) = {
            let Some(doc) = self.writer.buffers().document(page_id) else {
                return;
            };
            let Some(info) = self.writer.buffers().info(page_id) else {
                return;
            };
            let content = doc.canonical_text();
            let should_write = doc.buffer().is_dirty()
                || std::fs::read_to_string(&info.path)
                    .map(|disk| disk != content)
                    .unwrap_or(true);
            if !should_write {
                return;
            }
            (content, info.path.clone(), doc.buffer().version())
        };

        // Write with monotonic write ID.
        if let Some(tx) = &self.autosave_tx {
            self.write_counter += 1;
            let write_id = self.write_counter;
            // Record content hash for self-write detection
            let content_hash = hash_content(&content);
            let hashes = self.self_write_hashes.entry(path.clone()).or_default();
            hashes.push_back(content_hash);
            if hashes.len() > MAX_SELF_WRITE_HASHES {
                hashes.pop_front();
            }
            // Set pending write ID on buffer so WriteComplete can match
            if let Some(buf) = self.writer.buffers_mut().get_mut(page_id) {
                buf.set_pending_write_id(write_id);
            }
            let _ = tx.send(bloom_store::disk_writer::WriteRequest {
                path,
                content,
                write_id,
                buffer_version,
            });
        } else {
            // No DiskWriter (tests, pre-init). Inline atomic write.
            if bloom_store::disk_writer::atomic_write(&path, &content).is_ok() {
                self.durable_capture.mark_page_saved(page_id);
                if let Some(tx) = &self.history_tx {
                    let _ = tx.send(history::HistoryRequest::FileDirty);
                }
                self.writer.apply(crate::BufferMessage::MarkClean {
                    page_id: page_id.clone(),
                });
            }
        }
    }

    /// Public save for `:w` and TUI `Action::Save`.
    pub fn save_current(&mut self) -> Result<(), error::BloomError> {
        if let Some(page_id) = self.active_page().cloned() {
            self.save_page(&page_id);
        }
        Ok(())
    }

    /// Assign block IDs to the buffer if any blocks are missing them.
    /// Modifies the rope in-place. Returns true if any IDs were added.
    pub(crate) fn ensure_block_ids(&mut self, page_id: &types::PageId) -> bool {
        if self.vault_root.is_none() {
            return false;
        }

        // Only assign block IDs to Markdown files.
        let is_md = self
            .writer
            .buffers()
            .open_buffers()
            .iter()
            .find(|b| b.page_id == *page_id)
            .is_some_and(|info| {
                info.path
                    .extension()
                    .is_some_and(|ext: &std::ffi::OsStr| ext.eq_ignore_ascii_case("md"))
            });
        if !is_md {
            return false;
        }

        let parser = &self.parser;
        let known_ids = if self.known_block_ids.is_empty() {
            None
        } else {
            Some(&mut self.known_block_ids)
        };
        let Some(mut doc) = self.writer.buffers_mut().document_mut(page_id) else {
            return false;
        };
        doc.ensure_block_ids(parser, known_ids)
    }
}
