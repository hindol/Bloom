//! Screenshot integration tests.
//!
//! Renders the editor to a `ratatui::backend::TestBackend` and writes
//! the resulting text to `screenshots/` at the repository root.

use std::time::Duration;

use bloom_core::config::Config;
use bloom_core::BloomEditor;
use bloom_test_harness::{parse_key_sequence, TestVault};
use bloom_tui::render::draw;
use bloom_tui::theme::TuiTheme;
use ratatui::backend::TestBackend;
use ratatui::Terminal;

/// Repo-root-relative path for screenshot output.
/// Integration tests run from the crate directory (`crates/bloom-tui/`).
const SCREENSHOT_DIR: &str = "../../screenshots";

/// Render the editor into a `TestBackend` and return the buffer contents
/// as a plain-text string (one line per row).
fn render_to_string(editor: &mut BloomEditor, width: u16, height: u16, config: &Config) -> String {
    let frame = editor.render(width, height);
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();

    let theme = TuiTheme::new(editor.theme());
    terminal
        .draw(|f| {
            draw(f, &frame, &theme, config);
        })
        .unwrap();

    let buf = terminal.backend().buffer();
    let mut output = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            output.push_str(buf[(x, y)].symbol());
        }
        output.push('\n');
    }
    output
}

/// Send a key-sequence string (e.g. `"SPC f f"`) to the editor.
fn send_keys(editor: &mut BloomEditor, seq: &str) {
    for key in parse_key_sequence(seq) {
        editor.handle_key(key);
    }
}

/// Wait for the background indexer to finish (best-effort, bounded).
fn drain_indexer(editor: &mut BloomEditor) {
    let channels = editor.channels();
    if let Some(rx) = &channels.indexer_rx {
        // Give the indexer up to 5 seconds to complete.
        while let Ok(complete) = rx.recv_timeout(Duration::from_secs(5)) {
            editor.handle_index_complete(complete);
        }
    }
}

#[test]
fn screenshots() {
    // ---------------------------------------------------------------
    // 1. Build a test vault with interesting sample pages.
    // ---------------------------------------------------------------
    let vault = TestVault::new()
        .page("Rope Data Structure")
        .tags(&["rust", "editors"])
        .with_content(
            "\
# Rope Data Structure

A **rope** is a binary tree used to efficiently store and manipulate
very long strings. It is commonly used in [[id|Text Editor Theory]].

## Properties

- O(log n) insert / delete
- O(log n) random access
- Immutable snapshots via structural sharing

## Tasks

- [x] Implement basic rope @due(2026-03-01)
- [ ] Add line-index cache @due(2026-03-15)
- [ ] Benchmark against gap buffer @due(2026-04-01)

## Code Example

```rust
pub struct Rope {
    root: Node,
    len: usize,
}
```

#rust #editors
",
        )
        .page("2026-03-07")
        .tags(&["journal"])
        .with_content(
            "\
# 2026-03-07

- Morning standup: discussed [[id|Rope Data Structure]] refactor
- Reviewed PR on gap-buffer fallback #review
- [ ] Write benchmarks @due(2026-03-10)
- [x] Fix cursor math bug

#journal #daily
",
        )
        .page("Text Editor Theory")
        .tags(&["editors", "cs"])
        .with_content(
            "\
# Text Editor Theory

Classic text-editor data structures include gap buffers,
piece tables, and [[id|Rope Data Structure]]s.

## Topics

- Gap buffers
- Piece tables
- Ropes
- CRDT-based collaborative editing

#editors #cs
",
        );

    let vault = vault.build();
    let vault_root = vault.root();

    // ---------------------------------------------------------------
    // 2. Create editor, initialise vault, open the first page.
    // ---------------------------------------------------------------
    let config = Config::defaults();
    let mut editor = BloomEditor::new(config).unwrap();
    let _ = editor.init_vault(vault_root);
    drain_indexer(&mut editor);

    // Open a page with rich content so the screenshot is interesting.
    let id = bloom_core::uuid::generate_hex_id();
    editor.open_page_with_content(
        &id,
        "Rope Data Structure",
        &vault_root.join("pages").join("rope-data-structure.md"),
        "\
# Rope Data Structure

A **rope** is a binary tree used to efficiently store and manipulate
very long strings. It is commonly used in [[id|Text Editor Theory]].

## Properties

- O(log n) insert / delete
- O(log n) random access
- Immutable snapshots via structural sharing

## Tasks

- [x] Implement basic rope @due(2026-03-01)
- [ ] Add line-index cache @due(2026-03-15)
- [ ] Benchmark against gap buffer @due(2026-04-01)

## Code Example

```rust
pub struct Rope {
    root: Node,
    len: usize,
}
```

#rust #editors
",
    );

    let cfg = editor.config.clone();

    // ---------------------------------------------------------------
    // 3. Screenshot 1 — editor view.
    // ---------------------------------------------------------------
    std::fs::create_dir_all(SCREENSHOT_DIR).ok();

    let editor_txt = render_to_string(&mut editor, 100, 30, &cfg);
    let editor_path = format!("{}/editor.txt", SCREENSHOT_DIR);
    std::fs::write(&editor_path, &editor_txt).unwrap();
    assert!(
        !editor_txt.trim().is_empty(),
        "editor.txt should contain visible text"
    );
    assert!(
        editor_txt.contains("Rope"),
        "editor.txt should contain page content"
    );

    // ---------------------------------------------------------------
    // 4. Screenshot 2 — find-page picker (SPC f f).
    // ---------------------------------------------------------------
    send_keys(&mut editor, "SPC f f");
    let picker_txt = render_to_string(&mut editor, 100, 30, &cfg);
    let picker_path = format!("{}/picker.txt", SCREENSHOT_DIR);
    std::fs::write(&picker_path, &picker_txt).unwrap();
    assert!(
        !picker_txt.trim().is_empty(),
        "picker.txt should contain visible text"
    );

    // Close the picker before the next action.
    send_keys(&mut editor, "<Esc>");

    // ---------------------------------------------------------------
    // 5. Screenshot 3 — vertical split (SPC w v).
    // ---------------------------------------------------------------
    send_keys(&mut editor, "SPC w v");
    let split_txt = render_to_string(&mut editor, 100, 30, &cfg);
    let split_path = format!("{}/split.txt", SCREENSHOT_DIR);
    std::fs::write(&split_path, &split_txt).unwrap();
    assert!(
        !split_txt.trim().is_empty(),
        "split.txt should contain visible text"
    );

    println!("Screenshots written to {SCREENSHOT_DIR}/");
    println!(
        "  editor.txt: {} bytes",
        std::fs::metadata(&editor_path).unwrap().len()
    );
    println!(
        "  picker.txt: {} bytes",
        std::fs::metadata(&picker_path).unwrap().len()
    );
    println!(
        "  split.txt:  {} bytes",
        std::fs::metadata(&split_path).unwrap().len()
    );
}
