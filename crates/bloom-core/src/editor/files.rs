//! File save, auto-save, and watcher integration.
//!
//! All saves go through [`BloomEditor::save_page`] — the single save path.
//! Write IDs track each request so that stale completions don't clear dirty flags.
//! FileEvent handling uses content comparison (no fingerprints).

use crate::*;

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
                // Mark buffer clean only if this is the LATEST write AND buffer
                // hasn't been edited since the save was initiated.
                if let Some(page_id) = self.writer.buffers().find_by_path(&path).cloned() {
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
                                format!("✓ Saved {filename}"),
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
                let buf_content = self
                    .writer
                    .buffers()
                    .get(&page_id)
                    .map(|b| b.text().to_string());
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
                        self.writer.apply(crate::BufferMessage::Reload {
                            page_id: page_id.clone(),
                            content: disk_content,
                        });
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

        // Extract content, path, and version.
        let (content, path, buffer_version) = {
            let Some((buf, info)) = self.writer.buffers().get_with_info(page_id) else {
                return;
            };
            if !buf.is_dirty() {
                return;
            }
            (buf.text().to_string(), info.path.clone(), buf.version())
        };

        // Write with monotonic write ID.
        if let Some(tx) = &self.autosave_tx {
            self.write_counter += 1;
            let write_id = self.write_counter;
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

        let Some(buf) = self.writer.buffers().get(page_id) else {
            return false;
        };
        let text = buf.text().to_string();
        let doc = self.parser.parse(&text);
        let insertions = block_id_gen::compute_block_id_assignments(&doc);
        if insertions.is_empty() {
            return false;
        }

        let Some(buf) = self.writer.buffers_mut().get_mut(page_id) else {
            return false;
        };

        // Buffer owns cursors — insert() auto-adjusts them.
        buf.begin_edit_group();
        for ins in insertions.iter().rev() {
            let line_idx = ins.line;
            if line_idx >= buf.len_lines() {
                continue;
            }
            let line_start = buf.text().line_to_char(line_idx);
            let line_slice = buf.line(line_idx);
            let mut content_chars = line_slice.len_chars();
            let chars: Vec<char> = line_slice.chars().collect();
            while content_chars > 0 && matches!(chars[content_chars - 1], '\n' | '\r' | ' ' | '\t')
            {
                content_chars -= 1;
            }
            let insert_at = line_start + content_chars;
            let insertion_text = format!(" ^{}", ins.id);
            buf.insert(insert_at, &insertion_text);
        }
        buf.end_edit_group();

        true
    }
}
