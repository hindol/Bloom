//! Smoke tests against the test-vault with real-world Bloom content.
//!
//! Three cross-cutting tests that exercise
//! store → parser → index → resolver → hint_updater → diagnostics.

use std::path::{Path, PathBuf};

use bloom_core::editor::{EditorState, Key};
use bloom_core::hint_updater;
use bloom_core::index::SqliteIndex;
use bloom_core::parser::parse;
use bloom_core::render::DiagnosticKind;
use bloom_core::resolver::Resolver;
use bloom_core::store::{LocalFileStore, NoteStore};

fn vault_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("test-vault")
}

fn build_index(vault: &Path) -> SqliteIndex {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("smoke.db");
    let mut index = SqliteIndex::open(&db_path).unwrap();

    for dir in &["pages", "journal"] {
        let dir_path = vault.join(dir);
        if !dir_path.exists() {
            continue;
        }
        for entry in std::fs::read_dir(&dir_path).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.extension().map(|e| e == "md").unwrap_or(false) {
                let content = std::fs::read_to_string(&path).unwrap();
                let doc = parse(&content).unwrap();
                index.index_document(&path, &doc).unwrap();
            }
        }
    }
    // Leak the TempDir so the DB stays alive for the test's duration.
    std::mem::forget(tmp);
    index
}

fn copy_dir_recursive(src: &Path, dst: &Path) {
    for entry in std::fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            std::fs::create_dir_all(&dst_path).unwrap();
            copy_dir_recursive(&src_path, &dst_path);
        } else {
            std::fs::copy(&src_path, &dst_path).unwrap();
        }
    }
}

// ── Test 1: Full user journey ──────────────────────────────────────────

#[test]
fn user_journey_create_link_search_rename_orphan() {
    let vault = vault_path();

    // 1. Index the entire vault (pages/ + journal/).
    let index = build_index(&vault);

    // 2. Verify cross-file link integrity: every [[uuid]] resolves.
    let resolver = Resolver::new(&index);
    let mut total_links = 0;
    let mut broken = Vec::new();
    for dir in &["pages", "journal"] {
        let dir_path = vault.join(dir);
        if !dir_path.exists() {
            continue;
        }
        for entry in std::fs::read_dir(&dir_path).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().map(|e| e == "md").unwrap_or(false) {
                let content = std::fs::read_to_string(&path).unwrap();
                let doc = parse(&content).unwrap();
                for block in &doc.blocks {
                    for link in &block.links {
                        total_links += 1;
                        if resolver.resolve_page_id(&link.page_id).unwrap().is_none() {
                            broken.push(format!(
                                "{}:[[{}]]",
                                path.file_name().unwrap().to_string_lossy(),
                                link.page_id
                            ));
                        }
                    }
                }
            }
        }
    }
    assert!(total_links > 10, "expected many cross-links, found {total_links}");
    assert!(broken.is_empty(), "broken links: {broken:?}");

    // 3. Verify backlinks: "Text Editor Theory" (a1b2c3d4) has ≥4 backlinks.
    let backlinks = resolver.backlinks_for_page_id("a1b2c3d4").unwrap();
    assert!(
        backlinks.len() >= 4,
        "expected ≥4 backlinks to Text Editor Theory, found {}",
        backlinks.len()
    );

    // 4. Verify FTS search: "ropey" returns ≥2 hits.
    let hits = index.search("ropey").unwrap();
    assert!(hits.len() >= 2, "expected ≥2 hits for 'ropey', found {}", hits.len());

    // 5. Verify tags: #editors exists with count ≥3.
    let tags = index.list_tags().unwrap();
    let editors = tags.iter().find(|t| t.tag == "editors").expect("missing #editors tag");
    assert!(editors.count >= 3, "#editors count {}, expected ≥3", editors.count);

    // 6. Simulate rename on a COPY of the vault (never modify real test-vault).
    let tmp = tempfile::TempDir::new().unwrap();
    copy_dir_recursive(&vault, tmp.path());
    let store = LocalFileStore::new(tmp.path().to_path_buf()).unwrap();
    let updated =
        hint_updater::update_display_hints(&store, "a1b2c3d4", "Editor Design Theory").unwrap();
    assert!(updated > 0, "hint_updater should have updated ≥1 file");
    let found = store.list_pages().unwrap().iter().any(|p| {
        store
            .read(p)
            .map(|c| c.contains("Editor Design Theory"))
            .unwrap_or(false)
    });
    assert!(found, "new display hint should appear in at least one file");

    // 7. Verify orphan detection: fake link [[deadbeef|Missing]] → BrokenLink.
    let orphan_content =
        "---\nid: orphantest\ntitle: \"Orphan\"\ncreated: 2026-01-01T00:00:00Z\ntags: []\n---\n\n[[deadbeef|Missing]]\n";
    let state = EditorState::new(orphan_content);
    let diags = state.link_diagnostics(&index);
    assert!(!diags.is_empty(), "expected BrokenLink diagnostic for [[deadbeef]]");
    assert_eq!(diags[0].kind, DiagnosticKind::BrokenLink);
}

// ── Test 2: Adversarial vault corruption recovery ──────────────────────

#[test]
fn adversarial_vault_corruption_recovery() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = LocalFileStore::new(tmp.path().to_path_buf()).unwrap();

    // Valid page
    let valid = "---\nid: valid001\ntitle: \"Valid\"\ncreated: 2026-01-01T00:00:00Z\ntags: [test]\n---\n\n# Valid page\nSome searchable content\n";
    store.write(&store.pages_dir().join("Valid.md"), valid).unwrap();

    // Corrupt YAML frontmatter
    let corrupt_yaml = "---\ntitle: [unclosed\n---\n\nBody text\n";
    store.write(&store.pages_dir().join("Corrupt.md"), corrupt_yaml).unwrap();

    // No frontmatter (just markdown)
    let no_fm = "# Just markdown\nNo frontmatter here.\n";
    store.write(&store.pages_dir().join("NoFM.md"), no_fm).unwrap();

    // Empty file (0 bytes)
    store.write(&store.pages_dir().join("Empty.md"), "").unwrap();

    // Two files with the SAME UUID
    let dup1 = "---\nid: dupid001\ntitle: \"Dup One\"\ncreated: 2026-01-01T00:00:00Z\ntags: []\n---\n\nFirst duplicate\n";
    let dup2 = "---\nid: dupid001\ntitle: \"Dup Two\"\ncreated: 2026-01-02T00:00:00Z\ntags: []\n---\n\nSecond duplicate\n";
    store.write(&store.pages_dir().join("Dup-One.md"), dup1).unwrap();
    store.write(&store.pages_dir().join("Dup-Two.md"), dup2).unwrap();

    // Binary file accidentally in pages/
    let binary_data: Vec<u8> = (0..256).map(|i| i as u8).collect();
    std::fs::write(store.pages_dir().join("image.md"), &binary_data).unwrap();

    // Index everything — gracefully handle failures.
    let db_path = tmp.path().join(".index").join("idx.db");
    let mut index = SqliteIndex::open(&db_path).unwrap();
    let pages = store.list_pages().unwrap();
    let mut indexed = 0;
    let mut skipped = 0;
    for page_path in &pages {
        let raw = match std::fs::read_to_string(page_path) {
            Ok(s) => s,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        match parse(&raw) {
            Ok(doc) => {
                index.index_document(page_path, &doc).unwrap();
                indexed += 1;
            }
            Err(_) => {
                skipped += 1;
            }
        }
    }

    // Valid, NoFM, Empty, Dup-One, Dup-Two should index; Corrupt + binary skipped.
    assert!(indexed >= 4, "expected ≥4 indexed, got {indexed}");
    assert!(skipped >= 1, "expected ≥1 skipped, got {skipped}");

    // Valid page is findable.
    let hits = index.search("searchable").unwrap();
    assert!(!hits.is_empty(), "valid page should be searchable");

    // :rebuild-index on this adversarial vault completes without panic.
    let rebuild_db = tmp.path().join(".index").join("rebuild.db");
    let mut state = EditorState::new("");
    state.vault_root = Some(tmp.path().to_path_buf());
    state.index = Some(SqliteIndex::open(&rebuild_db).unwrap());
    state.handle_key(Key::Char(':'));
    for c in "rebuild-index".chars() {
        state.handle_key(Key::Char(c));
    }
    state.handle_key(Key::Enter);
    // If we reach here without panic, rebuild succeeded.
}

// ── Test 3: Large file performance ─────────────────────────────────────

#[test]
fn large_file_performance() {
    use std::fmt::Write;
    use std::time::Instant;

    // Generate a 5,000-line markdown file.
    let mut content = String::new();
    writeln!(content, "---").unwrap();
    writeln!(content, "id: bigfile1").unwrap();
    writeln!(content, "title: \"Large File\"").unwrap();
    writeln!(content, "created: 2026-01-01T00:00:00Z").unwrap();
    writeln!(content, "tags: [perf, test]").unwrap();
    writeln!(content, "---").unwrap();
    writeln!(content).unwrap();
    let mut line_count = 7;

    for i in 0..200 {
        writeln!(content, "## Section {i}").unwrap();
        line_count += 1;
    }
    while line_count < 5000 {
        let idx = line_count;
        writeln!(
            content,
            "Paragraph {idx}: text with [[a1b2c3d4|link]] and #tag-{} @due(2026-06-{:02})",
            idx % 50,
            (idx % 28) + 1
        )
        .unwrap();
        line_count += 1;
    }

    // Parse — verify it completes in reasonable time (catches O(n²) regressions).
    let start = Instant::now();
    let doc = parse(&content).unwrap();
    let elapsed = start.elapsed();
    assert!(elapsed.as_secs() < 10, "parse took {elapsed:?}, expected < 10s");
    assert!(!doc.blocks.is_empty());

    // Open in EditorState with viewport_height=50.
    let mut state = EditorState::new(&content);
    state.viewport_height = 50;

    // G → go to last line
    state.handle_key(Key::Char('G'));
    let (row, _) = state.cursor_row_col();
    assert!(row > 4900, "G should jump near end, got row {row}");

    // gg → go to first line
    state.handle_key(Key::Char('g'));
    state.handle_key(Key::Char('g'));
    let (row, _) = state.cursor_row_col();
    assert_eq!(row, 0, "gg should return to first line");

    // Verify render frame respects viewport.
    let frame = state.render();
    let pane = frame.focused_pane().unwrap();
    assert!(
        pane.lines.len() <= state.viewport_height + 1,
        "rendered {} lines, viewport is {}",
        pane.lines.len(),
        state.viewport_height
    );
}
