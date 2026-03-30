//! Page navigation and link following.
//!
//! Opens pages by ID or file path, follows `[[id|text]]` wiki-links under the
//! cursor, navigates journal entries by date, and tracks frontier access for
//! search ranking. Also provides block-link yanking to clipboard.

use crate::*;
use bloom_md::parser::traits::DocumentParser;

/// Re-export from parser — single source of truth for link-at-cursor extraction.
pub(crate) use bloom_md::parser::extract_link_at_col;

impl BloomEditor {
    pub fn open_page_with_content(
        &mut self,
        id: &types::PageId,
        title: &str,
        path: &std::path::Path,
        content: &str,
    ) {
        self.writer.apply(crate::BufferMessage::Open {
            page_id: id.clone(),
            title: title.to_string(),
            path: path.to_path_buf(),
            content: content.to_string(),
        });
        self.set_active_page(Some(id.clone()));
        self.set_cursor(0);
        // Record access for frecency scoring
        if let Some(idx) = &self.index {
            idx.record_access(id);
        }
        // Clear journal mode — journal-specific callers re-set it after this.
        self.in_journal_mode = false;
    }

    pub(crate) fn open_journal_today(&mut self) {
        let today = journal::Journal::today();
        let title = today.format("%Y-%m-%d").to_string();

        let path = self
            .journal
            .as_ref()
            .map(|j| j.path_for_date(today))
            .unwrap_or_else(|| std::path::PathBuf::from(format!("journal/{}.md", title)));

        // Read from disk if the file exists, otherwise generate default frontmatter
        let (content, id) = if path.exists() {
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            let fm = self.parser.parse_frontmatter(&content);
            let id = fm.and_then(|f| f.id);
            (content, id)
        } else {
            (String::new(), None)
        };

        // Use existing ID or generate a stable one, caching per date
        let id = id.unwrap_or_else(|| {
            // Check the per-date cache first
            if let Some(cached) = self.journal_id_cache.get(&title) {
                return cached.clone();
            }
            // Check if we already have this file open by path
            if let Some(existing) = self.writer.buffers().find_by_path(&path) {
                return existing.clone();
            }
            // Generate new ID and embed it in frontmatter
            crate::uuid::generate_hex_id()
        });
        self.journal_id_cache
            .entry(title.clone())
            .or_insert_with(|| id.clone());

        if self.writer.buffers().is_open(&id) {
            self.set_active_page(Some(id));
        } else {
            // If content has no frontmatter ID, create content with one
            let content =
                if content.is_empty() || !content.contains(&format!("id: {}", id.to_hex())) {
                    let fm = bloom_md::parser::traits::Frontmatter {
                        id: Some(id.clone()),
                        title: Some(title.clone()),
                        created: Some(today),
                        tags: vec![types::TagName("journal".to_string())],
                        extra: std::collections::HashMap::new(),
                    };
                    let mut s = self.parser.serialize_frontmatter(&fm);
                    s.push('\n');
                    // If file had content beyond frontmatter, append it
                    if !content.is_empty() {
                        if let Some(body_start) = content.find("\n---\n") {
                            let body = &content[body_start + 5..];
                            s.push_str(body);
                        }
                    }
                    s
                } else {
                    content
                };
            self.open_page_with_content(&id, &title, &path, &content);
        }
        self.last_viewed_journal_date = Some(today);
        self.in_journal_mode = true;
    }

    pub(crate) fn open_scratch_buffer(&mut self) {
        let id = crate::uuid::generate_hex_id();
        self.open_page_with_content(&id, "[scratch]", std::path::Path::new("[scratch]"), "");
    }

    /// Open a journal date as a preview buffer during calendar navigation.
    /// The buffer is tracked so it can be silently closed on Esc.
    pub(crate) fn update_calendar_preview(&mut self) {
        let date = match &self.date_picker_state {
            Some(dp) => dp.selected_date,
            None => return,
        };
        let Some(journal) = &self.journal else { return };
        let path = journal.path_for_date(date);
        if !path.exists() {
            return;
        }
        let Ok(content) = std::fs::read_to_string(&path) else {
            return;
        };
        // Read the page ID from frontmatter instead of generating a new one.
        // This way re-visiting the same day reuses the buffer.
        let id = self
            .parser
            .parse_frontmatter(&content)
            .and_then(|fm| fm.id)
            .or_else(|| self.writer.buffers().find_by_path(&path).cloned())
            .unwrap_or_else(crate::uuid::generate_hex_id);
        let title = date.format("%Y-%m-%d").to_string();
        if !self.writer.buffers().is_open(&id) {
            self.open_page_with_content(&id, &title, &path, &content);
        } else {
            self.set_active_page(Some(id.clone()));
        }
        if let Some(dp) = &mut self.date_picker_state {
            if !dp.preview_buffers.contains(&id) {
                dp.preview_buffers.push(id);
            }
        }
    }

    /// Follow the wiki-link under the cursor: `[[id|text]]` → open page by id.
    pub(crate) fn follow_link_at_cursor(&mut self) {
        let Some(page_id) = self.active_page().cloned() else {
            return;
        };
        let Some(buf) = self.writer.buffers().get(&page_id) else {
            return;
        };
        let rope = buf.text();
        let len = rope.len_chars();
        if len == 0 {
            return;
        }
        let cursor = self.cursor().min(len.saturating_sub(1));
        let line_idx = rope.char_to_line(cursor);
        let line_text = rope.line(line_idx).to_string();
        let col = cursor - rope.line_to_char(line_idx);

        // Find [[...]] surrounding the cursor column
        if let Some(link_id) = extract_link_at_col(&line_text, col) {
            // Try to open the page from the vault
            if let Some(target_id) = types::PageId::from_hex(&link_id) {
                if let Some(root) = &self.vault_root {
                    let pages_dir = root.join("pages");
                    // Scan for the file containing this id
                    if let Ok(entries) = std::fs::read_dir(&pages_dir) {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            let name = path.file_name().unwrap_or_default().to_string_lossy();
                            if name.contains(&link_id) && name.ends_with(".md") {
                                if let Ok(content) = std::fs::read_to_string(&path) {
                                    let title = self
                                        .parser
                                        .parse_frontmatter(&content)
                                        .and_then(|f| f.title)
                                        .unwrap_or_else(|| link_id.clone());
                                    self.open_page_with_content(
                                        &target_id, &title, &path, &content,
                                    );
                                    return;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Will be used by named views jump-to-source
    #[allow(dead_code)]
    pub(crate) fn navigate_to_page_by_id(&mut self, page_id: &types::PageId) {
        if let Some(idx) = &self.index {
            if let Some(meta) = idx.find_page_by_id(page_id) {
                if let Ok(content) = std::fs::read_to_string(&meta.path) {
                    self.open_page_with_content(page_id, &meta.title, &meta.path, &content);
                }
            }
        }
    }

    /// Yank a `[[id|title]]` link to the current page.
    pub(crate) fn yank_link_to_current_page(&self) -> Option<String> {
        let page_id = self.active_page()?;
        let _buf = self.writer.buffers().get(page_id)?;
        let buffers = self.writer.buffers().open_buffers();
        let info = buffers.iter().find(|b| b.page_id == *page_id)?;
        Some(format!("[[{}|{}]]", page_id.to_hex(), info.title))
    }

    /// Yank a `[[^block_id|]]` block link for the block at the cursor.
    pub(crate) fn yank_link_to_current_block(&self) -> Option<String> {
        let page_id = self.active_page()?;
        let buf = self.writer.buffers().get(page_id)?;
        let rope = buf.text();
        let len = rope.len_chars();
        if len == 0 {
            return None;
        }
        let cursor = self.cursor().min(len.saturating_sub(1));
        let line_idx = rope.char_to_line(cursor);
        let line_text = rope.line(line_idx).to_string();
        // Look for ^block-id on this line
        if let Some(caret_pos) = line_text.rfind('^') {
            let block_id = line_text[caret_pos + 1..].trim();
            if !block_id.is_empty()
                && block_id
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
            {
                return Some(format!("[[^{}|]]", block_id));
            }
        }
        // Fallback: page link without block
        self.yank_link_to_current_page()
    }

    /// Navigate to the previous or next journal entry that has a file.
    /// Empty days are skipped — only days with actual journal files are visited.
    pub(crate) fn navigate_journal(&mut self, delta: i32) {
        let Some(journal) = &self.journal else { return };
        let Some(store) = &self.note_store else {
            return;
        };
        let today = journal::Journal::today();

        // Use last_viewed_journal_date if available, else try active page, else today
        let current_date = self
            .last_viewed_journal_date
            .or_else(|| {
                self.active_page()
                    .and_then(|id| self.writer.buffers().get(id))
                    .and_then(|buf| {
                        let text = buf.text().to_string();
                        self.parser
                            .parse_frontmatter(&text)
                            .and_then(|fm| fm.created)
                    })
            })
            .unwrap_or(today);

        // Skip to next/prev day that has a journal file
        let target = if delta > 0 {
            journal.next_date(current_date, store)
        } else {
            journal.prev_date(current_date, store)
        };

        let Some(target) = target else {
            // No journal in that direction — do nothing
            return;
        };

        let title = target.format("%Y-%m-%d").to_string();
        let path = journal.path_for_date(target);
        let content = if path.exists() {
            std::fs::read_to_string(&path).unwrap_or_default()
        } else {
            // Should not happen since next_date/prev_date only return existing dates,
            // but handle gracefully.
            let fm = bloom_md::parser::traits::Frontmatter {
                id: None,
                title: Some(title.clone()),
                created: Some(target),
                tags: vec![types::TagName("journal".to_string())],
                extra: std::collections::HashMap::new(),
            };
            let mut s = self.parser.serialize_frontmatter(&fm);
            s.push('\n');
            s
        };
        let fm_parsed = self.parser.parse_frontmatter(&content);
        let id = fm_parsed
            .and_then(|f| f.id)
            .or_else(|| self.writer.buffers().find_by_path(&path).cloned())
            .unwrap_or_else(crate::uuid::generate_hex_id);
        if self.writer.buffers().is_open(&id) {
            self.set_active_page(Some(id));
        } else {
            self.open_page_with_content(&id, &title, &path, &content);
        }
        self.last_viewed_journal_date = Some(target);
        self.in_journal_mode = true;
        self.journal_nav_at = Some(Instant::now());
    }

    /// Persist a quick-capture submission to today's journal.
    pub(crate) fn submit_quick_capture(
        &mut self,
        kind: &keymap::dispatch::QuickCaptureKind,
        text: &str,
    ) {
        if let keymap::dispatch::QuickCaptureKind::Rename = kind {
            self.rename_current_page(text.to_string());
            return;
        }

        let Some(journal) = &self.journal else { return };
        let Some(store) = &self.note_store else {
            return;
        };
        let today = journal::Journal::today();

        let result = match kind {
            keymap::dispatch::QuickCaptureKind::Note => {
                let line = format!("- {text}");
                journal.append(today, &line, store, &self.parser)
            }
            keymap::dispatch::QuickCaptureKind::Task => {
                journal.append_task(today, text, store, &self.parser)
            }
            keymap::dispatch::QuickCaptureKind::Rename => unreachable!(),
        };

        match result {
            Ok(()) => {
                let label = today.format("%b %-d").to_string();
                self.push_notification(
                    format!("Added to {label} journal"),
                    render::NotificationLevel::Info,
                );
            }
            Err(e) => {
                self.push_notification(
                    format!("Journal write failed: {e}"),
                    render::NotificationLevel::Error,
                );
            }
        }
    }

    /// Rename the current page: update frontmatter title, rename file on disk,
    /// and update buffer metadata.
    fn rename_current_page(&mut self, new_title: String) {
        let Some(page_id) = self.active_page().cloned() else {
            return;
        };

        // 1. Update the title in the buffer's frontmatter text.
        if let Some(buf) = self.writer.buffers().get(&page_id) {
            let text = buf.text().to_string();
            if let Some(updated) = Self::replace_frontmatter_title(&text, &new_title) {
                let cursor_policy = crate::document::CursorPolicy::reanchor_to_cursor(
                    buf,
                    self.active_cursor_idx(),
                );
                self.writer.apply(crate::BufferMessage::Reload {
                    page_id: page_id.clone(),
                    content: updated,
                    cursor_policy,
                });
            }
        }

        // 2. Rename file on disk.
        let old_path = self.writer.buffers().info(&page_id).map(|i| i.path.clone());
        if let Some(old) = &old_path {
            if old.to_string_lossy().starts_with('[') {
                // Pseudo-path like [scratch] — skip disk rename
            } else if let Some(parent) = old.parent() {
                let new_filename = sanitize_filename(&new_title);
                let new_path = parent.join(format!("{new_filename}.md"));
                if old != &new_path {
                    let _ = std::fs::rename(old, &new_path);
                    // Update stored path in buffer info
                    if let Some(info) = self.writer.buffers_mut().info_mut(&page_id) {
                        info.path = new_path.clone();
                    }
                }
            }
        }

        // 3. Update buffer title in metadata.
        if let Some(info) = self.writer.buffers_mut().info_mut(&page_id) {
            info.title = new_title.clone();
        }

        self.push_notification(
            format!("Renamed to: {new_title}"),
            render::NotificationLevel::Info,
        );
    }

    /// Replace the `title:` value in YAML frontmatter.
    /// Returns `Some(new_content)` if frontmatter was found and updated.
    fn replace_frontmatter_title(content: &str, new_title: &str) -> Option<String> {
        // Frontmatter: starts with "---\n", ends with "\n---\n"
        if !content.starts_with("---\n") {
            return None;
        }
        let end = content[4..].find("\n---\n").map(|i| i + 4)?;
        let fm_block = &content[4..end];
        let after = &content[end + 5..];

        let mut new_fm_lines = Vec::new();
        for line in fm_block.lines() {
            if line.starts_with("title:") {
                new_fm_lines.push(format!("title: \"{}\"", new_title));
            } else {
                new_fm_lines.push(line.to_string());
            }
        }
        let mut result = String::from("---\n");
        result.push_str(&new_fm_lines.join("\n"));
        result.push_str("\n---\n");
        result.push_str(after);
        Some(result)
    }
}

/// Convert a page title to a safe filename (lowercase, spaces → hyphens,
/// strip non-alphanumeric except hyphens/underscores).
fn sanitize_filename(title: &str) -> String {
    let s: String = title
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c.to_ascii_lowercase()
            } else if c == ' ' {
                '-'
            } else {
                '_'
            }
        })
        .collect();
    // Collapse consecutive hyphens/underscores
    let mut result = String::with_capacity(s.len());
    let mut prev_sep = false;
    for c in s.chars() {
        if c == '-' || c == '_' {
            if !prev_sep {
                result.push(c);
            }
            prev_sep = true;
        } else {
            result.push(c);
            prev_sep = false;
        }
    }
    if result.is_empty() {
        result.push_str("untitled");
    }
    result
}
