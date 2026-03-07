//! Page navigation and link following.
//!
//! Opens pages by ID or file path, follows `[[id|text]]` wiki-links under the
//! cursor, navigates journal entries by date, and tracks frontier access for
//! search ranking. Also provides block-link yanking to clipboard.

use crate::parser::traits::DocumentParser;
use crate::*;

/// Extract the link ID from a `[[id|text]]` pattern at the given column in a line.
pub(crate) fn extract_link_at_col(line: &str, col: usize) -> Option<String> {
    let byte_col = line
        .char_indices()
        .nth(col)
        .map(|(i, _)| i)
        .unwrap_or(line.len());
    let bytes = line.as_bytes();
    let len = bytes.len();
    if byte_col >= len {
        return None;
    }

    // Search backwards for [[
    let mut start = None;
    let mut i = byte_col.min(len.saturating_sub(1));
    while i > 0 {
        if i > 0 && bytes[i - 1] == b'[' && bytes[i] == b'[' {
            start = Some(i + 1);
            break;
        }
        // If we hit ]], we're not inside a link
        if i > 0 && bytes[i - 1] == b']' && bytes[i] == b']' {
            return None;
        }
        i -= 1;
    }
    let content_start = start?;

    // Search forward for ]]
    let mut j = content_start;
    while j + 1 < len {
        if bytes[j] == b']' && bytes[j + 1] == b']' {
            let content = &line[content_start..j];
            // Extract the ID (before | or # if present)
            let id = content.split('|').next().unwrap_or(content);
            let id = id.split('#').next().unwrap_or(id);
            return Some(id.to_string());
        }
        j += 1;
    }
    None
}

impl BloomEditor {
    pub fn open_page_with_content(
        &mut self,
        id: &types::PageId,
        title: &str,
        path: &std::path::Path,
        content: &str,
    ) {
        self.buffer_mgr.open(id, title, path, content);
        self.active_page = Some(id.clone());
        self.cursor = 0;
        // Record access for frecency scoring
        if let Some(idx) = &self.index {
            idx.record_access(id);
        }
    }

    pub(crate) fn open_journal_today(&mut self) {
        let today = journal::Journal::today();
        let title = today.format("%Y-%m-%d").to_string();

        // If journal module is initialized, use its path; otherwise use a sensible default
        let path = self
            .journal
            .as_ref()
            .map(|j| j.path_for_date(today))
            .unwrap_or_else(|| std::path::PathBuf::from(format!("journal/{}.md", title)));

        // Read from disk if the file exists, otherwise generate default frontmatter
        let content = if path.exists() {
            std::fs::read_to_string(&path).unwrap_or_default()
        } else {
            let fm = parser::traits::Frontmatter {
                id: None,
                title: Some(title.clone()),
                created: Some(today),
                tags: vec![types::TagName("journal".to_string())],
                extra: std::collections::HashMap::new(),
            };
            let mut s = self.parser.serialize_frontmatter(&fm);
            s.push('\n');
            s
        };

        let id = crate::uuid::generate_hex_id();
        self.open_page_with_content(&id, &title, &path, &content);
    }

    pub(crate) fn open_scratch_buffer(&mut self) {
        let id = crate::uuid::generate_hex_id();
        self.open_page_with_content(&id, "[scratch]", std::path::Path::new("[scratch]"), "");
    }

    /// Follow the wiki-link under the cursor: `[[id|text]]` → open page by id.
    pub(crate) fn follow_link_at_cursor(&mut self) {
        let Some(page_id) = &self.active_page else {
            return;
        };
        let Some(buf) = self.buffer_mgr.get(page_id) else {
            return;
        };
        let rope = buf.text();
        let len = rope.len_chars();
        if len == 0 {
            return;
        }
        let cursor = self.cursor.min(len.saturating_sub(1));
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
        let page_id = self.active_page.as_ref()?;
        let _buf = self.buffer_mgr.get(page_id)?;
        let buffers = self.buffer_mgr.open_buffers();
        let info = buffers.iter().find(|b| b.page_id == *page_id)?;
        Some(format!("[[{}|{}]]", page_id.to_hex(), info.title))
    }

    /// Yank a `[[id#block-id|title]]` link to the block at the cursor.
    pub(crate) fn yank_link_to_current_block(&self) -> Option<String> {
        let page_id = self.active_page.as_ref()?;
        let buf = self.buffer_mgr.get(page_id)?;
        let rope = buf.text();
        let len = rope.len_chars();
        if len == 0 {
            return None;
        }
        let cursor = self.cursor.min(len.saturating_sub(1));
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
                let buffers = self.buffer_mgr.open_buffers();
                let info = buffers.iter().find(|b| b.page_id == *page_id)?;
                return Some(format!(
                    "[[{}#{}|{}]]",
                    page_id.to_hex(),
                    block_id,
                    info.title
                ));
            }
        }
        // Fallback: page link without block
        self.yank_link_to_current_page()
    }

    /// Navigate to the previous or next journal entry.
    pub(crate) fn navigate_journal(&mut self, delta: i32) {
        let Some(journal) = &self.journal else { return };
        let today = journal::Journal::today();

        // Find current journal date from active page
        let current_date = self
            .active_page
            .as_ref()
            .and_then(|id| self.buffer_mgr.get(id))
            .and_then(|buf| {
                let text = buf.text().to_string();
                self.parser
                    .parse_frontmatter(&text)
                    .and_then(|fm| fm.created)
            })
            .unwrap_or(today);

        // Simply offset by one day
        let target = if delta > 0 {
            current_date.succ_opt().unwrap_or(current_date)
        } else {
            current_date.pred_opt().unwrap_or(current_date)
        };

        let title = target.format("%Y-%m-%d").to_string();
        let path = journal.path_for_date(target);
        let content = if path.exists() {
            std::fs::read_to_string(&path).unwrap_or_default()
        } else {
            let fm = parser::traits::Frontmatter {
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
        let id = crate::uuid::generate_hex_id();
        self.open_page_with_content(&id, &title, &path, &content);
    }
}
