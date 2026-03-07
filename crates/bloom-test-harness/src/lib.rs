// Bloom test harness utilities
//
//! Test utilities for Bloom: `TestVault`, `SimInput`, `SnapshotHelpers`,
//! `PageBuilder`, and `AssertFrame`.

use bloom_core::render::RenderFrame;
use bloom_core::types::{KeyEvent, PageId};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// TestVault
// ---------------------------------------------------------------------------

/// Creates a temporary vault with pre-populated pages. Auto-cleanup on drop.
pub struct TestVault {
    dir: TempDir,
    pages: Vec<TestPage>,
}

struct TestPage {
    _id: PageId,
    #[allow(dead_code)]
    title: String,
    content: String,
    tags: Vec<String>,
}

impl TestVault {
    pub fn new() -> Self {
        Self {
            dir: TempDir::new().expect("failed to create temp dir"),
            pages: Vec::new(),
        }
    }

    pub fn page(mut self, title: &str) -> Self {
        let id = bloom_core::uuid::generate_hex_id();
        let content = format!(
            "---\nid: {}\ntitle: \"{}\"\ncreated: 2026-01-01\ntags: []\n---\n\n",
            id.to_hex(),
            title
        );
        self.pages.push(TestPage {
            _id: id,
            title: title.to_string(),
            content,
            tags: Vec::new(),
        });
        self
    }

    pub fn with_content(mut self, content: &str) -> Self {
        if let Some(last) = self.pages.last_mut() {
            last.content.push_str(content);
        }
        self
    }

    pub fn tags(mut self, tags: &[&str]) -> Self {
        if let Some(last) = self.pages.last_mut() {
            last.tags = tags.iter().map(|s| s.to_string()).collect();
            let tag_str = last
                .tags
                .iter()
                .map(|t| t.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            last.content = last
                .content
                .replace("tags: []", &format!("tags: [{}]", tag_str));
        }
        self
    }

    /// Build the vault — writes all pages to disk and returns the root path.
    pub fn build(&self) -> PathBuf {
        let root = self.dir.path().to_path_buf();
        let pages_dir = root.join("pages");
        let journal_dir = root.join("journal");
        let bloom_dir = root.join(".bloom");
        std::fs::create_dir_all(&pages_dir).unwrap();
        std::fs::create_dir_all(&journal_dir).unwrap();
        std::fs::create_dir_all(&bloom_dir).unwrap();

        for page in &self.pages {
            let filename = format!("{}.md", page.title.to_lowercase().replace(' ', "-"));
            let path = pages_dir.join(&filename);
            std::fs::write(&path, &page.content).unwrap();
        }

        root
    }

    pub fn root(&self) -> &Path {
        self.dir.path()
    }
}

impl Default for TestVault {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// SimInput
// ---------------------------------------------------------------------------

/// Simulates keystrokes, returns `RenderFrame`.
///
/// NOTE: Full functionality requires `BloomEditor` which is not yet
/// implemented in `bloom-core`. The struct is provided so that downstream
/// test code can be written now; the body will be filled in once
/// `BloomEditor` lands.
pub struct SimInput {
    _private: (),
}

impl SimInput {
    /// Send a key sequence like `"diw"`, `"SPC f f"`, `":wq <CR>"`.
    pub fn keys(&mut self, sequence: &str) -> &mut Self {
        let _keys = parse_key_sequence(sequence);
        // Will forward to BloomEditor::handle_key once available.
        self
    }

    /// Type literal text (each char sent as a key event).
    pub fn type_text(&mut self, text: &str) -> &mut Self {
        let _keys: Vec<KeyEvent> = text.chars().map(KeyEvent::char).collect();
        // Will forward to BloomEditor::handle_key once available.
        self
    }
}

// ---------------------------------------------------------------------------
// parse_key_sequence
// ---------------------------------------------------------------------------

/// Parse a key sequence string into `KeyEvent`s.
///
/// Supports: `"dw"`, `"SPC f f"`, `"<Esc>"`, `"<CR>"`, `"C-s"`, etc.
pub fn parse_key_sequence(sequence: &str) -> Vec<KeyEvent> {
    let mut keys = Vec::new();
    let parts: Vec<&str> = sequence.split_whitespace().collect();

    for part in parts {
        match part {
            "SPC" => keys.push(KeyEvent::char(' ')),
            "<Esc>" | "Esc" => keys.push(KeyEvent::esc()),
            "<CR>" | "Enter" => keys.push(KeyEvent::enter()),
            "<Tab>" | "Tab" => keys.push(KeyEvent::tab()),
            "<BS>" => keys.push(KeyEvent::backspace()),
            s if s.starts_with("C-") => {
                if let Some(c) = s.chars().nth(2) {
                    keys.push(KeyEvent::ctrl(c));
                }
            }
            s => {
                for c in s.chars() {
                    keys.push(KeyEvent::char(c));
                }
            }
        }
    }
    keys
}

// ---------------------------------------------------------------------------
// SnapshotHelpers
// ---------------------------------------------------------------------------

/// Formats `RenderFrame` into deterministic strings for `insta` snapshots.
pub struct SnapshotHelpers;

impl SnapshotHelpers {
    /// Format visible lines as a string.
    pub fn format_lines(frame: &RenderFrame) -> String {
        let mut output = String::new();
        for pane in &frame.panes {
            for line in &pane.visible_lines {
                output.push_str(&format!("{:>3}| ", line.line_number + 1));
                output.push('\n');
            }
        }
        output
    }

    /// Format buffer content (cursor + mode).
    pub fn format_buffer(frame: &RenderFrame) -> String {
        let mut output = String::new();
        if let Some(pane) = frame.panes.first() {
            output.push_str(&format!(
                "cursor: {}:{}\n",
                pane.cursor.line, pane.cursor.column
            ));
            output.push_str(&format!("mode: {}\n", pane.status_bar.mode));
        }
        output
    }

    /// Format picker state.
    pub fn format_picker(frame: &RenderFrame) -> String {
        let mut output = String::new();
        if let Some(picker) = &frame.picker {
            output.push_str(&format!("query: {}\n", picker.query));
            output.push_str(&format!(
                "results ({}/{}):\n",
                picker.filtered_count, picker.total_count
            ));
            for (i, row) in picker.results.iter().enumerate() {
                let marker = if i == picker.selected_index {
                    "▸"
                } else {
                    " "
                };
                output.push_str(&format!("{} {}\n", marker, row.label));
            }
        }
        output
    }
}

// ---------------------------------------------------------------------------
// PageBuilder
// ---------------------------------------------------------------------------

/// Builder pattern for creating test pages with links, tags, and content.
pub struct PageBuilder {
    title: String,
    content: String,
    tags: Vec<String>,
    links: Vec<String>,
}

impl PageBuilder {
    pub fn new(title: &str) -> Self {
        Self {
            title: title.to_string(),
            content: String::new(),
            tags: Vec::new(),
            links: Vec::new(),
        }
    }

    pub fn content(mut self, content: &str) -> Self {
        self.content = content.to_string();
        self
    }

    pub fn tag(mut self, tag: &str) -> Self {
        self.tags.push(tag.to_string());
        self
    }

    pub fn link(mut self, link: &str) -> Self {
        self.links.push(link.to_string());
        self
    }

    pub fn build(&self) -> String {
        let id = bloom_core::uuid::generate_hex_id();
        let tags = if self.tags.is_empty() {
            "[]".to_string()
        } else {
            format!("[{}]", self.tags.join(", "))
        };
        format!(
            "---\nid: {}\ntitle: \"{}\"\ncreated: 2026-01-01\ntags: {}\n---\n\n{}",
            id.to_hex(),
            self.title,
            tags,
            self.content
        )
    }
}

// ---------------------------------------------------------------------------
// AssertFrame
// ---------------------------------------------------------------------------

/// Fluent assertions on `RenderFrame` fields.
pub struct AssertFrame<'a> {
    frame: &'a RenderFrame,
}

impl<'a> AssertFrame<'a> {
    pub fn new(frame: &'a RenderFrame) -> Self {
        Self { frame }
    }

    pub fn cursor_at(self, line: usize, column: usize) -> Self {
        if let Some(pane) = self.frame.panes.first() {
            assert_eq!(pane.cursor.line, line, "cursor line mismatch");
            assert_eq!(pane.cursor.column, column, "cursor column mismatch");
        }
        self
    }

    pub fn mode(self, expected: &str) -> Self {
        if let Some(pane) = self.frame.panes.first() {
            assert_eq!(pane.status_bar.mode, expected, "mode mismatch");
        }
        self
    }

    pub fn dirty(self, expected: bool) -> Self {
        if let Some(pane) = self.frame.panes.first() {
            assert_eq!(pane.dirty, expected, "dirty flag mismatch");
        }
        self
    }

    pub fn has_picker(self) -> Self {
        assert!(self.frame.picker.is_some(), "expected picker to be open");
        self
    }

    pub fn no_picker(self) -> Self {
        assert!(self.frame.picker.is_none(), "expected picker to be closed");
        self
    }
}
