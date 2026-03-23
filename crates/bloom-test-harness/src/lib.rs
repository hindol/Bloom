//! Bloom test harness — SimInput, TestScreen, TestVault, and assertion helpers.
//!
//! Provides end-to-end testing infrastructure that drives `BloomEditor` through
//! key sequences and asserts on the visual output (RenderFrame), without any
//! terminal or GUI dependency.

use bloom_core::config::Config;
use bloom_core::render::RenderFrame;
use bloom_core::types::KeyEvent;
use bloom_core::BloomEditor;
use std::path::Path;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// TestVault — creates a temporary vault with pages
// ---------------------------------------------------------------------------

/// Creates a temporary vault with pre-populated pages. Auto-cleanup on drop.
pub struct TestVault {
    dir: TempDir,
}

impl TestVault {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> TestVaultBuilder {
        TestVaultBuilder { pages: Vec::new() }
    }

    pub fn root(&self) -> &Path {
        self.dir.path()
    }
}

#[derive(Default)]
pub struct TestVaultBuilder {
    pages: Vec<(String, String)>, // (filename, content)
}

impl TestVaultBuilder {
    /// Add a page with auto-generated frontmatter.
    pub fn page(mut self, title: &str) -> Self {
        let id = bloom_core::uuid::generate_hex_id();
        let filename = format!("{}.md", title.to_lowercase().replace(' ', "-"));
        let content = format!(
            "---\nid: {}\ntitle: \"{}\"\ncreated: 2026-01-01\ntags: []\n---\n\n",
            id.to_hex(),
            title,
        );
        self.pages.push((filename, content));
        self
    }

    /// Append content to the last added page.
    pub fn with_content(mut self, extra: &str) -> Self {
        if let Some(last) = self.pages.last_mut() {
            last.1.push_str(extra);
        }
        self
    }

    /// Add tags to the last added page (modifies frontmatter).
    pub fn tags(mut self, tags: &[&str]) -> Self {
        if let Some(last) = self.pages.last_mut() {
            let tag_str = tags.join(", ");
            last.1 = last.1.replace("tags: []", &format!("tags: [{}]", tag_str));
        }
        self
    }

    /// Add a raw file (filename + full content, no auto-frontmatter).
    pub fn raw_file(mut self, filename: &str, content: &str) -> Self {
        self.pages.push((filename.to_string(), content.to_string()));
        self
    }

    /// Build the vault on disk and return a TestVault handle.
    pub fn build(self) -> TestVault {
        let dir = TempDir::new().expect("failed to create temp dir");
        let pages_dir = dir.path().join("pages");
        let journal_dir = dir.path().join("journal");
        std::fs::create_dir_all(&pages_dir).unwrap();
        std::fs::create_dir_all(&journal_dir).unwrap();

        for (filename, content) in &self.pages {
            let path = if filename.starts_with("journal/") {
                dir.path().join(filename)
            } else {
                pages_dir.join(filename)
            };
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&path, content).unwrap();
        }

        TestVault { dir }
    }
}

// ---------------------------------------------------------------------------
// Pre-built vault fixtures
// ---------------------------------------------------------------------------

/// 3 pages with cross-links. For testing link following, backlinks, unlinked mentions.
pub fn linked_vault() -> TestVault {
    let id_rust = "aa110001";
    let id_editor = "aa110002";
    let id_orphan = "aa110003";

    TestVault::new()
        .raw_file("rust-notes.md", &format!(
            "---\nid: {id_rust}\ntitle: \"Rust Notes\"\ncreated: 2026-01-15\ntags: [rust, programming]\n---\n\n\
            # Rust Notes\n\n\
            Rust is a systems programming language.\n\n\
            See [[{id_editor}|Text Editor Theory]] for editor architecture.\n\n\
            Memory safety is key.\n"
        ))
        .raw_file("text-editor-theory.md", &format!(
            "---\nid: {id_editor}\ntitle: \"Text Editor Theory\"\ncreated: 2026-02-01\ntags: [editors, rust]\n---\n\n\
            # Text Editor Theory\n\n\
            Ropes are O(log n) for inserts.\n\n\
            See [[{id_rust}|Rust Notes]] for language details.\n\n\
            Piece tables are used by VS Code.\n"
        ))
        .raw_file("orphan-page.md", &format!(
            "---\nid: {id_orphan}\ntitle: \"Orphan Page\"\ncreated: 2026-03-01\ntags: []\n---\n\n\
            # Orphan Page\n\n\
            This page has no links to or from other pages.\n"
        ))
        .build()
}

/// Pages with tasks in various states. For testing agenda, task toggle, search by status.
pub fn task_vault() -> TestVault {
    TestVault::new()
        .raw_file(
            "project-a.md",
            "---\nid: bb220001\ntitle: \"Project A\"\ncreated: 2026-01-10\ntags: [project, work]\n---\n\n# Project A\n\n- [ ] Review the API design @due(2026-03-10)\n- [ ] Write unit tests @due(2026-03-15)\n- [x] Set up CI pipeline\n- [ ] Deploy to staging\n",
        )
        .raw_file(
            "project-b.md",
            "---\nid: bb220002\ntitle: \"Project B\"\ncreated: 2026-02-01\ntags: [project]\n---\n\n# Project B\n\n- [ ] Design the database schema\n- [ ] Implement auth module @due(2026-04-01)\n- [x] Write the RFC\n",
        )
        .raw_file(
            "journal/2026-03-10.md",
            "---\nid: bb220003\ntitle: \"2026-03-10\"\ncreated: 2026-03-10\ntags: []\n---\n\n- Worked on Project A today\n- [ ] Follow up with team @due(2026-03-12)\n- [x] Read the design doc\n",
        )
        .build()
}

/// Pages with diverse tags. For testing tag search, filter by tag.
pub fn tagged_vault() -> TestVault {
    TestVault::new()
        .raw_file(
            "rust-notes.md",
            "---\nid: cc330001\ntitle: \"Rust Notes\"\ncreated: 2026-01-15\ntags: [rust, editors]\n---\n\n# Rust Notes\n\nContent about Rust.\n",
        )
        .raw_file(
            "python-notes.md",
            "---\nid: cc330002\ntitle: \"Python Notes\"\ncreated: 2026-02-01\ntags: [python]\n---\n\n# Python Notes\n\nContent about Python.\n",
        )
        .raw_file(
            "meeting-notes.md",
            "---\nid: cc330003\ntitle: \"Meeting Notes\"\ncreated: 2026-03-01\ntags: [rust, meetings]\n---\n\n# Meeting Notes\n\nDiscussed Rust architecture.\n",
        )
        .raw_file(
            "untagged.md",
            "---\nid: cc330004\ntitle: \"Untagged Page\"\ncreated: 2026-03-05\ntags: []\n---\n\n# Untagged\n\nNo tags here.\n",
        )
        .build()
}

// ---------------------------------------------------------------------------
// SimInput — drives BloomEditor with key sequences
// ---------------------------------------------------------------------------

/// Drives a `BloomEditor` with simulated key sequences.
/// Owns the editor and provides methods to send keys and inspect state.
pub struct SimInput {
    pub editor: BloomEditor,
    vault: Option<TestVault>,
}

impl SimInput {
    /// Access the vault root (if a vault was provided).
    pub fn vault_root(&self) -> Option<&Path> {
        self.vault.as_ref().map(|v| v.root())
    }

    /// Create a SimInput with an empty scratch buffer (no vault).
    pub fn new() -> Self {
        let config = Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = bloom_core::uuid::generate_hex_id();
        editor.open_page_with_content(&id, "Scratch", std::path::Path::new("[scratch]"), "");
        Self {
            editor,
            vault: None,
        }
    }

    /// Create a SimInput with content in a scratch buffer (no vault).
    pub fn with_content(content: &str) -> Self {
        let config = Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = bloom_core::uuid::generate_hex_id();
        editor.open_page_with_content(&id, "Scratch", std::path::Path::new("[scratch]"), content);
        Self {
            editor,
            vault: None,
        }
    }

    /// Create a SimInput backed by a TestVault (vault initialized, indexed).
    pub fn with_vault(vault: TestVault) -> Self {
        let config = Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let _ = editor.init_vault(vault.root());

        // Wait for indexer to complete
        let ch = editor.channels();
        if let Some(rx) = &ch.indexer_rx {
            for _ in 0..300 {
                if let Ok(complete) = rx.try_recv() {
                    editor.handle_index_complete(complete);
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }

        editor.startup();

        Self {
            editor,
            vault: Some(vault),
        }
    }

    /// Create a SimInput with a pre-configured editor and vault.
    pub fn with_editor_and_vault(editor: BloomEditor, vault: TestVault) -> Self {
        Self {
            editor,
            vault: Some(vault),
        }
    }

    /// Send a key sequence. Supports: `"dw"`, `"SPC f f"`, `"<Esc>"`,
    /// `"<CR>"`, `"C-r"`, etc.
    pub fn keys(&mut self, sequence: &str) -> &mut Self {
        let keys = parse_key_sequence(sequence);
        for key in keys {
            self.editor.handle_key(key);
        }
        self
    }

    /// Type literal text (each char sent as an Insert-mode key event).
    pub fn type_text(&mut self, text: &str) -> &mut Self {
        for c in text.chars() {
            self.editor.handle_key(KeyEvent::char(c));
        }
        self
    }

    /// Advance time (triggers notification expiry, which-key popup, etc.).
    pub fn tick(&mut self, millis: u64) -> &mut Self {
        let future = std::time::Instant::now() + std::time::Duration::from_millis(millis);
        self.editor.tick(future);
        self
    }

    /// Drain background channels (indexer, file writer) until quiescent.
    /// Call this before any test that relies on search results, agenda, or
    /// other index-dependent features.
    pub fn flush_background(&mut self) -> &mut Self {
        let timeout = std::time::Duration::from_millis(50);
        for _ in 0..100 {
            let mut progressed = false;
            let channels = self.editor.channels();

            if let Some(rx) = &channels.write_result_rx {
                while let Ok(result) = rx.try_recv() {
                    self.editor.handle_write_result(result);
                    progressed = true;
                }
            }

            if let Some(rx) = &channels.indexer_rx {
                // Use blocking recv_timeout so we yield CPU to the indexer thread
                match rx.recv_timeout(timeout) {
                    Ok(complete) => {
                        self.editor.handle_index_complete(complete);
                        progressed = true;
                        // Drain any additional messages
                        while let Ok(c) = rx.try_recv() {
                            self.editor.handle_index_complete(c);
                        }
                    }
                    Err(_) => {}
                }
            }

            if !progressed && !self.editor.is_indexing() {
                break;
            }
        }
        self
    }

    /// Render and return a TestScreen for assertions.
    pub fn screen(&mut self, width: u16, height: u16) -> TestScreen {
        self.editor.update_layout(width, height);
        let frame = self.editor.render(width, height);
        TestScreen::from_frame(frame, width, height)
    }

    /// Get the active page's content as text.
    pub fn buffer_text(&self) -> String {
        self.editor.active_buffer_text().unwrap_or_default()
    }
}

impl Default for SimInput {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// TestScreen — visual assertions on a RenderFrame
// ---------------------------------------------------------------------------

/// A rendered screen extracted from a RenderFrame.
/// Provides assertion methods on the visual output.
pub struct TestScreen {
    pub frame: RenderFrame,
    pub width: u16,
    pub height: u16,
}

impl TestScreen {
    pub fn from_frame(frame: RenderFrame, width: u16, height: u16) -> Self {
        Self {
            frame,
            width,
            height,
        }
    }

    /// Get the text content of a visible line (0-indexed) in the active pane.
    pub fn line_text(&self, row: usize) -> String {
        self.active_pane()
            .and_then(|p| p.visible_lines.get(row))
            .map(|l| l.text.trim_end_matches(['\n', '\r']).to_string())
            .unwrap_or_default()
    }

    /// Get the number of visible lines in the active pane.
    pub fn line_count(&self) -> usize {
        self.active_pane()
            .map(|p| p.visible_lines.len())
            .unwrap_or(0)
    }

    /// Get all visible line texts joined by newline.
    pub fn all_lines(&self) -> String {
        self.active_pane()
            .map(|p| {
                p.visible_lines
                    .iter()
                    .map(|l| l.text.trim_end_matches(['\n', '\r']))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default()
    }

    /// Get the page title shown in the active pane.
    pub fn title(&self) -> &str {
        self.active_pane().map(|p| p.title.as_str()).unwrap_or("")
    }

    /// Get the mode string (NORMAL, INSERT, VISUAL, COMMAND).
    pub fn mode(&self) -> &str {
        self.active_pane()
            .map(|p| p.status_bar.mode.as_str())
            .unwrap_or("")
    }

    /// Get cursor position (line, column).
    pub fn cursor(&self) -> (usize, usize) {
        self.active_pane()
            .map(|p| (p.cursor.line, p.cursor.column))
            .unwrap_or((0, 0))
    }

    /// Whether the active buffer is dirty.
    pub fn is_dirty(&self) -> bool {
        self.active_pane().map(|p| p.dirty).unwrap_or(false)
    }

    /// Whether a picker overlay is visible.
    pub fn has_picker(&self) -> bool {
        self.frame.picker.is_some()
    }

    /// Get picker query text.
    pub fn picker_query(&self) -> &str {
        self.frame
            .picker
            .as_ref()
            .map(|p| p.query.as_str())
            .unwrap_or("")
    }

    /// Get picker result labels.
    pub fn picker_results(&self) -> Vec<&str> {
        self.frame
            .picker
            .as_ref()
            .map(|p| p.results.iter().map(|r| r.label.as_str()).collect())
            .unwrap_or_default()
    }

    /// Get the selected picker result label.
    pub fn picker_selected(&self) -> Option<&str> {
        self.frame
            .picker
            .as_ref()
            .and_then(|p| p.results.get(p.selected_index).map(|r| r.label.as_str()))
    }

    /// Whether a which-key popup is visible.
    pub fn has_which_key(&self) -> bool {
        self.frame.which_key.is_some()
    }

    /// Whether a dialog is visible.
    pub fn has_dialog(&self) -> bool {
        self.frame.dialog.is_some()
    }

    /// Number of panes.
    pub fn pane_count(&self) -> usize {
        self.frame.panes.len()
    }

    fn active_pane(&self) -> Option<&bloom_core::render::PaneFrame> {
        self.frame.panes.iter().find(|p| p.is_active)
    }

    /// Whether a context strip is visible.
    pub fn has_context_strip(&self) -> bool {
        self.frame.context_strip.is_some()
    }

    /// Whether a temporal strip (history scrubber) is visible.
    pub fn has_temporal_strip(&self) -> bool {
        self.frame.temporal_strip.is_some()
    }

    /// Number of diff preview lines in the temporal strip.
    pub fn temporal_preview_line_count(&self) -> usize {
        self.frame
            .temporal_strip
            .as_ref()
            .map(|s| s.preview_lines.len())
            .unwrap_or(0)
    }

    /// Number of nodes in the temporal strip.
    pub fn temporal_strip_node_count(&self) -> usize {
        self.frame
            .temporal_strip
            .as_ref()
            .map(|s| s.items.len())
            .unwrap_or(0)
    }

    /// Whether a date picker / calendar is visible.
    pub fn has_date_picker(&self) -> bool {
        self.frame.date_picker.is_some()
    }

    /// Get the right_hints from the active pane's status bar.
    pub fn right_hints(&self) -> Option<&str> {
        self.active_pane()
            .and_then(|p| p.status_bar.right_hints.as_deref())
    }

    /// Get the active theme name from the render frame.
    pub fn theme_name(&self) -> &str {
        &self.frame.theme_name
    }

    /// Whether a view overlay is visible.
    pub fn has_view(&self) -> bool {
        self.frame.view.is_some()
    }

    /// Get the view title if a view is open.
    pub fn view_title(&self) -> Option<&str> {
        self.frame.view.as_ref().map(|v| v.title.as_str())
    }

    /// Get the number of result rows in the view.
    pub fn view_row_count(&self) -> usize {
        self.frame.view.as_ref().map(|v| v.rows.len()).unwrap_or(0)
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
// History test helpers
// ---------------------------------------------------------------------------

/// Create a commit with a backdated timestamp in a [`bloom_history::HistoryRepo`].
pub fn commit_at(
    repo: &bloom_history::HistoryRepo,
    files: &[(&str, &str)],
    timestamp: i64,
    message: &str,
) -> Option<String> {
    repo.commit_all(files, message, Some(timestamp))
        .expect("commit_at failed")
}

// ---------------------------------------------------------------------------
// FrameRecorder — captures animation frames from editor sessions
// ---------------------------------------------------------------------------

/// A recorded animation frame — one step in an animated documentation GIF.
#[derive(serde::Serialize)]
pub struct AnimationFrame {
    /// Frame index (0-based).
    pub index: usize,
    /// Milliseconds to display this frame before advancing.
    pub duration_ms: u64,
    /// Optional caption text rendered below the wireframe.
    pub caption: Option<String>,
    /// The key sequence that produced this frame (for debugging).
    pub keys: Option<String>,
    /// The full render frame.
    pub frame: RenderFrame,
    /// Screen dimensions.
    pub width: u16,
    pub height: u16,
}

/// Records a sequence of editor frames for animated documentation.
///
/// Wraps `SimInput` and captures a `RenderFrame` after each step.
/// Outputs a JSON array of frames with timing metadata, which a
/// renderer script converts to SVG frames and then an animated GIF.
///
/// ```rust,ignore
/// let mut rec = FrameRecorder::new(SimInput::with_content("hello world"));
/// rec.step("w");           // move to next word
/// rec.step("dw");          // delete word
/// rec.caption("Word deleted");
/// rec.save("word-delete"); // → target/animations/word-delete.json
/// ```
pub struct FrameRecorder {
    sim: SimInput,
    frames: Vec<AnimationFrame>,
    width: u16,
    height: u16,
    /// Default frame duration (ms).
    default_duration: u64,
    /// Typing speed per character (ms).
    typing_speed: u64,
    /// Current caption (sticky — applied to subsequent frames until changed).
    current_caption: Option<String>,
}

impl FrameRecorder {
    /// Create a new recorder wrapping a SimInput.
    pub fn new(sim: SimInput) -> Self {
        Self::with_size(sim, 80, 24)
    }

    /// Create a recorder with custom screen dimensions.
    pub fn with_size(sim: SimInput, width: u16, height: u16) -> Self {
        Self {
            sim,
            frames: Vec::new(),
            width,
            height,
            default_duration: 100,
            typing_speed: 60,
            current_caption: None,
        }
    }

    /// Record a frame after executing a key sequence.
    pub fn step(&mut self, keys: &str) -> &mut Self {
        self.sim.keys(keys);
        self.capture_frame(Some(keys.to_string()), self.default_duration);
        self
    }

    /// Record frames for each character typed (natural typing animation).
    pub fn step_type(&mut self, text: &str) -> &mut Self {
        for ch in text.chars() {
            self.sim.type_text(&ch.to_string());
            self.capture_frame(Some(format!("type '{}'", ch)), self.typing_speed);
        }
        self
    }

    /// Record a pause frame (holds the current state for visual emphasis).
    pub fn pause(&mut self, ms: u64) -> &mut Self {
        self.capture_frame(None, ms);
        self
    }

    /// Set a caption that appears on subsequent frames.
    pub fn caption(&mut self, text: &str) -> &mut Self {
        self.current_caption = Some(text.to_string());
        // Re-capture current state with the new caption
        self.capture_frame(None, 1500);
        self
    }

    /// Clear the current caption.
    pub fn clear_caption(&mut self) -> &mut Self {
        self.current_caption = None;
        self
    }

    /// Access the underlying SimInput for assertions.
    pub fn sim(&self) -> &SimInput {
        &self.sim
    }

    /// Access the underlying SimInput mutably.
    pub fn sim_mut(&mut self) -> &mut SimInput {
        &mut self.sim
    }

    /// Capture the current editor state as a frame.
    fn capture_frame(&mut self, keys: Option<String>, duration_ms: u64) {
        self.sim.editor.update_layout(self.width, self.height);
        let frame = self.sim.editor.render(self.width, self.height);
        let index = self.frames.len();
        self.frames.push(AnimationFrame {
            index,
            duration_ms,
            caption: self.current_caption.clone(),
            keys,
            frame,
            width: self.width,
            height: self.height,
        });
    }

    /// Save recorded frames as JSON to `target/animations/{name}.json`.
    /// Uses the workspace root's target directory.
    pub fn save(&self, name: &str) -> std::path::PathBuf {
        // Find workspace root by looking for the workspace Cargo.toml
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let workspace_root = manifest_dir
            .parent() // crates/
            .and_then(|p| p.parent()) // workspace root
            .unwrap_or(manifest_dir);
        let dir = workspace_root.join("target/animations");
        std::fs::create_dir_all(&dir).expect("create animations dir");
        let path = dir.join(format!("{}.json", name));
        let json = serde_json::to_string_pretty(&self.frames).expect("serialize frames");
        std::fs::write(&path, json).expect("write animation JSON");
        eprintln!(
            "Animation saved: {} ({} frames, {}KB)",
            path.display(),
            self.frames.len(),
            std::fs::metadata(&path)
                .map(|m| m.len() / 1024)
                .unwrap_or(0),
        );
        path
    }

    /// Total number of captured frames.
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    /// Total animation duration in milliseconds.
    pub fn total_duration_ms(&self) -> u64 {
        self.frames.iter().map(|f| f.duration_ms).sum()
    }
}
