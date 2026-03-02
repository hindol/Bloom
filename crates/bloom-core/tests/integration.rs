//! Integration tests: store + parser working together (TestVault helper).

use std::path::PathBuf;

use bloom_core::buffer::Buffer;
use bloom_core::editor::{EditorState, Key, KeyResult};
use bloom_core::parser::{parse, serialize};
use bloom_core::store::{LocalFileStore, NoteStore};

/// A temporary vault wired with store + parser for integration testing.
struct TestVault {
    _tmp: tempfile::TempDir,
    store: LocalFileStore,
}

impl TestVault {
    fn new() -> Self {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = LocalFileStore::new(tmp.path().to_path_buf()).unwrap();
        Self { _tmp: tmp, store }
    }

    /// Write a page with the given filename and raw Markdown content.
    fn write_page(&self, filename: &str, content: &str) {
        let path = self.pages_dir().join(filename);
        self.store.write(&path, content).unwrap();
    }

    /// Write a journal entry for a given date (YYYY-MM-DD).
    fn write_journal(&self, date: &str, content: &str) {
        let path = self.journal_dir().join(format!("{date}.md"));
        self.store.write(&path, content).unwrap();
    }

    /// Read and parse a page by filename.
    fn read_and_parse(&self, filename: &str) -> bloom_core::document::Document {
        let path = self.pages_dir().join(filename);
        let raw = self.store.read(&path).unwrap();
        parse(&raw).unwrap()
    }

    /// Read and parse a journal entry by date.
    fn read_and_parse_journal(&self, date: &str) -> bloom_core::document::Document {
        let path = self.journal_dir().join(format!("{date}.md"));
        let raw = self.store.read(&path).unwrap();
        parse(&raw).unwrap()
    }

    fn pages_dir(&self) -> PathBuf {
        self._tmp.path().join("pages")
    }

    fn journal_dir(&self) -> PathBuf {
        self._tmp.path().join("journal")
    }
}

// ── Scenario: write → read → parse roundtrip ──────────────────────────────

#[test]
fn store_parser_roundtrip() {
    let vault = TestVault::new();
    let content = r#"---
id: aabb1122
title: "Test Page"
created: 2026-01-01T00:00:00Z
tags: [rust, notes]
---

# Introduction
Some text with a [[deadbeef|link]] and #tag
"#;
    vault.write_page("Test-Page.md", content);
    let doc = vault.read_and_parse("Test-Page.md");

    assert_eq!(doc.frontmatter.id, "aabb1122");
    assert_eq!(doc.frontmatter.title, "Test Page");
    assert_eq!(doc.frontmatter.tags, vec!["rust", "notes"]);
    assert_eq!(doc.blocks.len(), 2);
    assert_eq!(doc.blocks[1].links.len(), 1);
    assert_eq!(doc.blocks[1].tags.len(), 1);
}

// ── Scenario: journal entry with links and tags ────────────────────────────

#[test]
fn journal_entry_with_extensions() {
    let vault = TestVault::new();
    let content = r#"---
id: jrnl0001
title: "2026-03-01"
created: 2026-03-01T00:00:00Z
tags: []
---

- Read about [[aabb1122|Text Editor Theory]] today #research
- [ ] Follow up on [[ccdd3344|Rope Buffers]] @due(2026-03-05)
"#;
    vault.write_journal("2026-03-01", content);
    let doc = vault.read_and_parse_journal("2026-03-01");

    assert_eq!(doc.frontmatter.id, "jrnl0001");
    assert_eq!(doc.blocks.len(), 2);
    // First bullet: 1 link, 1 tag
    assert_eq!(doc.blocks[0].links.len(), 1);
    assert_eq!(doc.blocks[0].links[0].page_id, "aabb1122");
    assert_eq!(doc.blocks[0].tags.len(), 1);
    assert_eq!(doc.blocks[0].tags[0].name, "research");
    // Second bullet: 1 link, 1 timestamp, is unchecked task
    assert_eq!(doc.blocks[1].links.len(), 1);
    assert_eq!(doc.blocks[1].timestamps.len(), 1);
    assert!(matches!(
        doc.blocks[1].kind,
        bloom_core::document::BlockKind::ListItem {
            checked: Some(false)
        }
    ));
}

// ── Scenario: modify a page in memory and write back ───────────────────────

#[test]
fn modify_and_rewrite_preserves_structure() {
    let vault = TestVault::new();
    let content = r#"---
id: mod00001
title: "Editable"
created: 2026-01-01T00:00:00Z
tags: []
---

# Original heading
Original paragraph
"#;
    vault.write_page("Editable.md", content);
    let doc = vault.read_and_parse("Editable.md");

    // Serialize back (simulating save after in-memory edit)
    let serialized = serialize(&doc);
    vault.write_page("Editable.md", &serialized);

    let doc2 = vault.read_and_parse("Editable.md");
    assert_eq!(doc2.frontmatter.id, "mod00001");
    assert_eq!(doc2.blocks.len(), doc.blocks.len());
}

// ── Scenario: multiple pages can cross-reference each other ────────────────

#[test]
fn cross_references_between_pages() {
    let vault = TestVault::new();

    let page_a = r#"---
id: aaaaaaaa
title: "Page A"
created: 2026-01-01T00:00:00Z
tags: []
---

Links to [[bbbbbbbb|Page B]]
"#;
    let page_b = r#"---
id: bbbbbbbb
title: "Page B"
created: 2026-01-01T00:00:00Z
tags: []
---

Links back to [[aaaaaaaa|Page A]]
"#;
    vault.write_page("Page-A.md", page_a);
    vault.write_page("Page-B.md", page_b);

    let doc_a = vault.read_and_parse("Page-A.md");
    let doc_b = vault.read_and_parse("Page-B.md");

    // A links to B's UUID
    assert_eq!(doc_a.blocks[0].links[0].page_id, "bbbbbbbb");
    // B links to A's UUID
    assert_eq!(doc_b.blocks[0].links[0].page_id, "aaaaaaaa");
}

// ── Scenario: page with embeds ─────────────────────────────────────────────

#[test]
fn page_with_embeds_parses_correctly() {
    let vault = TestVault::new();
    let content = r#"---
id: emb00001
title: "Embed Test"
created: 2026-01-01T00:00:00Z
tags: []
---

## Notes
![[aabb1122|Text Editor Theory]]
![[ccdd3344#sec-1|Rope section]]
Regular text [[aabb1122|just a link]]
"#;
    vault.write_page("Embed-Test.md", content);
    let doc = vault.read_and_parse("Embed-Test.md");

    assert_eq!(doc.blocks.len(), 4); // heading + 2 embeds + 1 paragraph
    assert_eq!(doc.blocks[1].embeds.len(), 1);
    assert_eq!(doc.blocks[2].embeds.len(), 1);
    assert_eq!(doc.blocks[2].embeds[0].sub_id.as_deref(), Some("sec-1"));
    assert_eq!(doc.blocks[3].links.len(), 1);
    assert_eq!(doc.blocks[3].embeds.len(), 0);
}

// ── Scenario: empty page (just frontmatter) ────────────────────────────────

#[test]
fn empty_page_only_frontmatter() {
    let vault = TestVault::new();
    let content = "---\nid: empty001\ntitle: Empty\ncreated: 2026-01-01T00:00:00Z\ntags: []\n---\n";
    vault.write_page("Empty.md", content);
    let doc = vault.read_and_parse("Empty.md");

    assert_eq!(doc.frontmatter.id, "empty001");
    assert!(doc.blocks.is_empty());
}

// ── Scenario: unlinked mentions → promote → verify backlinks ───────────────

#[test]
fn unlinked_mentions_promote_to_linked() {
    use bloom_core::index::SqliteIndex;
    use bloom_core::resolver::Resolver;
    use std::path::Path;

    let vault = TestVault::new();

    // 1. Create 4 pages: target, source A (unlinked mention), source B (unlinked mention), source C (already linked)
    let target = "---\nid: target01\ntitle: \"Rust Notes\"\ncreated: 2026-01-01T00:00:00Z\ntags: []\n---\n\nThis is the target page about Rust.\n";
    let source_a = "---\nid: srca0001\ntitle: \"Source A\"\ncreated: 2026-01-01T00:00:00Z\ntags: []\n---\n\nI was reading about Rust Notes the other day.\n";
    let source_b = "---\nid: srcb0001\ntitle: \"Source B\"\ncreated: 2026-01-01T00:00:00Z\ntags: []\n---\n\nMy Rust Notes are scattered everywhere.\n";
    let source_c = "---\nid: srcc0001\ntitle: \"Source C\"\ncreated: 2026-01-01T00:00:00Z\ntags: []\n---\n\nSee [[target01|Rust Notes]] for reference.\n";

    vault.write_page("Rust-Notes.md", target);
    vault.write_page("Source-A.md", source_a);
    vault.write_page("Source-B.md", source_b);
    vault.write_page("Source-C.md", source_c);

    // 2. Index all pages into SqliteIndex
    let db_path = vault._tmp.path().join(".index").join("core.db");
    let mut index = SqliteIndex::open(&db_path).unwrap();

    let pages = [
        ("Rust-Notes.md", "pages/Rust-Notes.md"),
        ("Source-A.md", "pages/Source-A.md"),
        ("Source-B.md", "pages/Source-B.md"),
        ("Source-C.md", "pages/Source-C.md"),
    ];
    for (filename, idx_path) in &pages {
        let doc = vault.read_and_parse(filename);
        index
            .index_document(Path::new(idx_path), &doc)
            .unwrap();
    }

    // 3. Unlinked mentions for "Rust Notes" should return sources A and B, not C
    let resolver = Resolver::new(&index);
    let mentions = resolver.unlinked_mentions_for_title("Rust Notes").unwrap();
    let mut mention_ids: Vec<_> = mentions.iter().map(|m| m.source_page_id.as_str()).collect();
    mention_ids.sort();
    assert_eq!(mention_ids, vec!["srca0001", "srcb0001"]);

    // 4. Promote: replace raw "Rust Notes" text with [[target01|Rust Notes]] in each mentioned source
    fn promote_mention(vault: &TestVault, filename: &str, title: &str, target_id: &str) {
        let path = vault.pages_dir().join(filename);
        let raw = vault.store.read(&path).unwrap();
        let link = format!("[[{target_id}|{title}]]");
        let promoted = raw.replacen(title, &link, 1);
        vault.store.write(&path, &promoted).unwrap();
    }

    for mention in &mentions {
        let filename = mention
            .source_path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap();
        promote_mention(&vault, filename, "Rust Notes", "target01");
    }

    // 5. Re-index and verify
    for (filename, idx_path) in &pages {
        let doc = vault.read_and_parse(filename);
        index
            .index_document(Path::new(idx_path), &doc)
            .unwrap();
    }

    let resolver = Resolver::new(&index);

    // Unlinked mentions should now be empty
    let mentions_after = resolver.unlinked_mentions_for_title("Rust Notes").unwrap();
    assert!(
        mentions_after.is_empty(),
        "Expected no unlinked mentions after promote, got: {mentions_after:?}"
    );

    // Backlinks for target01 should include all 3 sources
    let backlinks = resolver.backlinks_for_page_id("target01").unwrap();
    let mut backlink_ids: Vec<_> = backlinks.iter().map(|b| b.source_page_id.as_str()).collect();
    backlink_ids.sort();
    assert_eq!(backlink_ids, vec!["srca0001", "srcb0001", "srcc0001"]);
}

// ── Scenario: lazy journal — SPC j t creates journal buffer ─────────────────
//
// Spec (gap-lazy-journal): SPC j t should create the journal buffer in memory
// WITHOUT writing to disk; the file should only appear after an explicit :w.
//
// KNOWN DEVIATION: The current implementation (`load_journal_file`) writes the
// initial frontmatter to disk immediately when the file doesn't exist.  The
// assertions below capture the *current* behaviour.  When the spec is
// implemented, flip the two marked assertions.

#[test]
fn lazy_journal_current_behavior() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = LocalFileStore::new(tmp.path().to_path_buf()).unwrap();

    let mut editor = EditorState::new("");
    editor.vault_root = Some(tmp.path().to_path_buf());

    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let journal_path = tmp.path().join("journal").join(format!("{today}.md"));

    // Precondition: no journal file yet.
    assert!(!journal_path.exists(), "journal file should not exist before SPC j t");

    // Feed SPC j t  (journal.today keybinding).
    editor.handle_key(Key::Char(' '));
    editor.handle_key(Key::Char('j'));
    editor.handle_key(Key::Char('t'));

    // SPEC: journal file should NOT exist on disk yet (buffer is in-memory only).
    // CURRENT BEHAVIOUR: load_journal_file writes frontmatter immediately.
    // TODO(gap-lazy-journal): change `assert!(journal_path.exists())` to
    //   `assert!(!journal_path.exists())` once lazy creation is implemented.
    assert!(
        journal_path.exists(),
        "CURRENT: load_journal_file writes immediately; spec says it should not"
    );

    // The buffer should contain the frontmatter regardless.
    let buf_text = editor.text();
    assert!(buf_text.contains("---"), "buffer should have frontmatter");
    assert!(buf_text.contains(&today), "buffer should contain today's date");

    // Feed :w + Enter to explicitly save.
    editor.handle_key(Key::Char(':'));
    editor.handle_key(Key::Char('w'));
    let result = editor.handle_key(Key::Enter);
    assert_eq!(result, KeyResult::Save);

    // After :w the caller would persist; simulate that with the store.
    if let Some(fp) = editor.buffer.file_path.as_ref() {
        store.write(fp, &editor.text()).unwrap();
    }

    // The journal file should now exist with correct frontmatter.
    assert!(journal_path.exists(), "journal file must exist after :w");
    let on_disk = std::fs::read_to_string(&journal_path).unwrap();
    assert!(on_disk.contains("---"), "saved file should have frontmatter");
    assert!(on_disk.contains(&today), "saved file should contain today's date");
}

// ── Scenario: page with code blocks containing fake extensions ─────────────

#[test]
fn code_blocks_in_page_hide_extensions() {
    let vault = TestVault::new();
    let content = r#"---
id: code0001
title: "Code Safety"
created: 2026-01-01T00:00:00Z
tags: []
---

Real link [[aabb1122|ok]]

```rust
let fake = "[[deadbeef|not a link]]";
// #not-a-tag @due(2026-01-01)
```

Another real #tag here
"#;
    vault.write_page("Code-Safety.md", content);
    let doc = vault.read_and_parse("Code-Safety.md");

    let all_links: Vec<_> = doc.blocks.iter().flat_map(|b| &b.links).collect();
    let all_tags: Vec<_> = doc.blocks.iter().flat_map(|b| &b.tags).collect();
    let all_ts: Vec<_> = doc.blocks.iter().flat_map(|b| &b.timestamps).collect();

    assert_eq!(all_links.len(), 1);
    assert_eq!(all_links[0].page_id, "aabb1122");
    assert_eq!(all_tags.len(), 1);
    assert_eq!(all_tags[0].name, "tag");
    assert_eq!(all_ts.len(), 0);
}

// ── Scenario: :rebuild-index populates a fresh index ───────────────────────

#[test]
fn rebuild_index_populates_empty_index() {
    use bloom_core::editor::{EditorState, Key};
    use bloom_core::index::SqliteIndex;

    let vault = TestVault::new();

    // Write two .md files via store.
    vault.write_page(
        "Alpha.md",
        "---\nid: alpha001\ntitle: \"Alpha\"\ncreated: 2026-01-01T00:00:00Z\ntags: []\n---\n\nAlpha unique content\n",
    );
    vault.write_journal(
        "2026-05-01",
        "---\nid: jrnl0501\ntitle: \"2026-05-01\"\ncreated: 2026-05-01T00:00:00Z\ntags: []\n---\n\nJournal unique entry\n",
    );

    // Create an EditorState with index + vault_root.
    let db_path = vault._tmp.path().join(".index").join("core.db");
    let index = SqliteIndex::open(&db_path).unwrap();

    let mut state = EditorState::new("");
    state.index = Some(index);
    state.vault_root = Some(vault._tmp.path().to_path_buf());

    // Index is empty initially.
    assert_eq!(
        state.index.as_ref().unwrap().search("Alpha").unwrap().len(),
        0,
    );

    // Feed :rebuild-index + Enter.
    let keys: Vec<Key> = ":rebuild-index\n"
        .chars()
        .map(|c| match c {
            '\n' => Key::Enter,
            _ => Key::Char(c),
        })
        .collect();
    state.feed_keys(&keys);

    // Search now finds content from both files.
    let alpha_hits = state.index.as_ref().unwrap().search("Alpha").unwrap();
    assert!(!alpha_hits.is_empty(), "should find Alpha after rebuild");

    let journal_hits = state.index.as_ref().unwrap().search("Journal").unwrap();
    assert!(!journal_hits.is_empty(), "should find Journal after rebuild");
}

// ── Scenario: open → edit → save → reopen (E2E file flow) ─────────────────

#[test]
fn e2e_open_edit_save_reopen() {
    // 1. Create a temp vault with a .md file containing frontmatter + body.
    let vault = TestVault::new();
    let original_content = "\
---
id: e2e00001
title: \"E2E Test\"
created: 2026-06-01T00:00:00Z
tags: [integration]
---

# Hello
World
";
    vault.write_page("E2E-Test.md", original_content);
    let file_path = vault.pages_dir().join("E2E-Test.md");

    // 2. Create an EditorState using Buffer::from_file() on that path.
    let buf = Buffer::from_file(&file_path).unwrap();
    assert_eq!(buf.file_path.as_deref(), Some(file_path.as_path()));
    let mut editor = EditorState::new(&buf.text());
    editor.buffer.file_path = Some(file_path.clone());

    // 3. Feed keys to navigate (j, l) and edit (i, type, Esc).
    //    Lines (0-indexed): 0:"---" … 7:"# Hello" 8:"World"
    //    Navigate to line 7 with 7×j, then 2×l to col 2 ('H' in "# Hello"),
    //    then 1×j to line 8 ("World"), then press 'i' to enter insert mode,
    //    type "EDITED ", then Esc.
    let mut keys: Vec<Key> = Vec::new();
    for _ in 0..7 {
        keys.push(Key::Char('j'));
    }
    for _ in 0..2 {
        keys.push(Key::Char('l'));
    }
    keys.push(Key::Char('j'));
    // Now on line 8, col clamped to min(2, len-1) = 2 → 'r' in "World"
    // 'i' enters insert mode before cursor
    keys.push(Key::Char('i'));
    for ch in "EDITED ".chars() {
        keys.push(Key::Char(ch));
    }
    keys.push(Key::Escape);
    editor.feed_keys(&keys);

    // Verify buffer text has the edit (inserted "EDITED " before 'r' → "WoEDITED rld")
    let edited_text = editor.buffer.text();
    assert!(
        edited_text.contains("EDITED"),
        "Buffer should contain the edit, got: {edited_text}"
    );

    // 4. Feed `:w` + Enter to save.
    let save_result = {
        editor.handle_key(Key::Char(':'));
        editor.handle_key(Key::Char('w'));
        editor.handle_key(Key::Enter)
    };
    assert_eq!(save_result, KeyResult::Save);

    // The editor signals Save but doesn't write to disk itself;
    // simulate what the frontend does: write buffer content to file_path.
    std::fs::write(&file_path, editor.buffer.text()).unwrap();

    // 5. Read the file back from disk and verify.
    let on_disk = std::fs::read_to_string(&file_path).unwrap();

    // The edit is present.
    assert!(
        on_disk.contains("EDITED"),
        "Saved file should contain the edit"
    );

    // Original content around the edit is preserved.
    assert!(on_disk.contains("# Hello"), "Heading should be preserved");

    // Frontmatter is intact.
    let doc = parse(&on_disk).unwrap();
    assert_eq!(doc.frontmatter.id, "e2e00001");
    assert_eq!(doc.frontmatter.title, "E2E Test");
    assert_eq!(doc.frontmatter.tags, vec!["integration"]);

    // 6. Reopen: create a new EditorState from the same file and verify.
    let buf2 = Buffer::from_file(&file_path).unwrap();
    let editor2 = EditorState::new(&buf2.text());
    assert_eq!(editor2.buffer.text(), editor.buffer.text());
    assert!(editor2.buffer.text().contains("EDITED"));
}

// ── Scenario: external file modification detected ──────────────────────────

#[test]
fn external_file_modification_detected() {
    use bloom_core::index::SqliteIndex;
    use std::path::Path;

    let vault = TestVault::new();
    let original = "---\nid: extmod01\ntitle: \"External Mod\"\ncreated: 2026-01-01T00:00:00Z\ntags: []\n---\n\nOriginal content searchable\n";
    vault.write_page("ExtMod.md", original);

    // Index it.
    let db_path = vault._tmp.path().join(".index").join("core.db");
    let mut index = SqliteIndex::open(&db_path).unwrap();
    let doc = vault.read_and_parse("ExtMod.md");
    index
        .index_document(Path::new("pages/ExtMod.md"), &doc)
        .unwrap();

    // Verify original is searchable.
    let hits = index.search("Original").unwrap();
    assert!(!hits.is_empty(), "original content should be indexed");

    // Modify the file directly (simulating external editor change).
    let modified = "---\nid: extmod01\ntitle: \"External Mod\"\ncreated: 2026-01-01T00:00:00Z\ntags: []\n---\n\nReplaced content uniqueword\n";
    std::fs::write(vault.pages_dir().join("ExtMod.md"), modified).unwrap();

    // Re-read and re-parse — content should be the new content.
    let raw = std::fs::read_to_string(vault.pages_dir().join("ExtMod.md")).unwrap();
    assert!(raw.contains("uniqueword"), "file should have new content");
    let doc2 = parse(&raw).unwrap();
    assert_eq!(doc2.frontmatter.id, "extmod01");

    // Re-index — search should find the new content, not old.
    index
        .index_document(Path::new("pages/ExtMod.md"), &doc2)
        .unwrap();
    let hits = index.search("uniqueword").unwrap();
    assert!(
        !hits.is_empty(),
        "new content should be searchable after re-index"
    );
}

// ── Scenario: circular embeds don't stack overflow ─────────────────────────

#[test]
fn circular_embed_does_not_stack_overflow() {
    let vault = TestVault::new();

    let page_a = "---\nid: circAAAA\ntitle: \"Circle A\"\ncreated: 2026-01-01T00:00:00Z\ntags: []\n---\n\n![[circBBBB|Circle B]]\n";
    let page_b = "---\nid: circBBBB\ntitle: \"Circle B\"\ncreated: 2026-01-01T00:00:00Z\ntags: []\n---\n\n![[circAAAA|Circle A]]\n";

    vault.write_page("Circle-A.md", page_a);
    vault.write_page("Circle-B.md", page_b);

    // Parse both — verify embeds are extracted without panic or stack overflow.
    let doc_a = vault.read_and_parse("Circle-A.md");
    let doc_b = vault.read_and_parse("Circle-B.md");

    assert_eq!(doc_a.blocks[0].embeds.len(), 1);
    assert_eq!(doc_a.blocks[0].embeds[0].page_id, "circBBBB");
    assert_eq!(doc_b.blocks[0].embeds.len(), 1);
    assert_eq!(doc_b.blocks[0].embeds[0].page_id, "circAAAA");
}

// ── Picker data population from index ──────────────────────────────────

#[test]
fn spc_ff_picker_populated_from_index() {
    use bloom_core::editor::{EditorState, Key};
    use bloom_core::index::SqliteIndex;
    use bloom_core::parser::parse;
    use bloom_core::store::{LocalFileStore, NoteStore};

    let tmp = tempfile::TempDir::new().unwrap();
    let store = LocalFileStore::new(tmp.path().to_path_buf()).unwrap();

    // Write 3 pages
    let pages = [
        ("pages/Alpha.md", "aaa11111", "Alpha Page"),
        ("pages/Beta.md", "bbb22222", "Beta Page"),
        ("pages/Gamma.md", "ccc33333", "Gamma Page"),
    ];
    for (file, id, title) in &pages {
        let content = format!(
            "---\nid: {id}\ntitle: \"{title}\"\ncreated: 2026-01-01T00:00:00Z\ntags: []\n---\n\nBody of {title}\n"
        );
        store.write(&tmp.path().join(file), &content).unwrap();
    }

    // Build index
    let db_path = tmp.path().join(".index/test.db");
    let mut index = SqliteIndex::open(&db_path).unwrap();
    for (file, _, _) in &pages {
        let path = tmp.path().join(file);
        let content = store.read(&path).unwrap();
        let doc = parse(&content).unwrap();
        index.index_document(&path, &doc).unwrap();
    }

    // Create editor with index
    let mut state = EditorState::new("test");
    state.index = Some(index);

    // SPC f f → Find Page picker
    state.handle_key(Key::Char(' '));
    state.handle_key(Key::Char('f'));
    state.handle_key(Key::Char('f'));

    // Picker should exist and have 3 results (all pages)
    assert!(state.active_picker.is_some(), "picker should be open");
    let picker = state.active_picker.as_ref().unwrap();
    assert_eq!(picker.title, "Find Page");
    let results = picker.inner.results();
    assert_eq!(
        results.len(), 3,
        "picker should have 3 results from index, got {}",
        results.len()
    );
}

#[test]
fn spc_ff_picker_empty_without_index() {
    use bloom_core::editor::{EditorState, Key};

    let mut state = EditorState::new("test");
    // No index set

    state.handle_key(Key::Char(' '));
    state.handle_key(Key::Char('f'));
    state.handle_key(Key::Char('f'));

    assert!(state.active_picker.is_some());
    let results = state.active_picker.as_ref().unwrap().inner.results();
    assert_eq!(
        results.len(), 0,
        "picker should be empty without index, got {}",
        results.len()
    );
}

#[test]
fn spc_spc_all_commands_picker_populated() {
    use bloom_core::editor::{EditorState, Key};

    let mut state = EditorState::new("test");

    state.handle_key(Key::Char(' '));
    state.handle_key(Key::Char(' '));

    assert!(state.active_picker.is_some());
    let picker = state.active_picker.as_ref().unwrap();
    assert_eq!(picker.title, "All Commands");
    let results = picker.inner.results();
    assert!(
        results.len() >= 10,
        "all-commands picker should have ≥10 commands, got {}",
        results.len()
    );
}

#[test]
fn spc_st_search_tags_picker_populated_from_index() {
    use bloom_core::editor::{EditorState, Key};
    use bloom_core::index::SqliteIndex;
    use bloom_core::parser::parse;
    use bloom_core::store::{LocalFileStore, NoteStore};

    let tmp = tempfile::TempDir::new().unwrap();
    let store = LocalFileStore::new(tmp.path().to_path_buf()).unwrap();

    let content = "---\nid: aaa11111\ntitle: \"Tagged\"\ncreated: 2026-01-01T00:00:00Z\ntags: []\n---\n\n#rust #editors #bloom\n";
    store.write(&tmp.path().join("pages/Tagged.md"), content).unwrap();

    let db_path = tmp.path().join(".index/test.db");
    let mut index = SqliteIndex::open(&db_path).unwrap();
    let path = tmp.path().join("pages/Tagged.md");
    let raw = store.read(&path).unwrap();
    let doc = parse(&raw).unwrap();
    index.index_document(&path, &doc).unwrap();

    let mut state = EditorState::new("test");
    state.index = Some(index);

    // SPC t s → Search Tags
    state.handle_key(Key::Char(' '));
    state.handle_key(Key::Char('t'));
    state.handle_key(Key::Char('s'));

    assert!(state.active_picker.is_some(), "tags picker should open");
    let results = state.active_picker.as_ref().unwrap().inner.results();
    assert!(
        results.len() >= 3,
        "tags picker should have ≥3 tags, got {}",
        results.len()
    );
}

#[test]
fn inline_link_picker_populated_from_index() {
    use bloom_core::editor::{EditorState, Key};
    use bloom_core::index::SqliteIndex;
    use bloom_core::parser::parse;
    use bloom_core::store::{LocalFileStore, NoteStore};

    let tmp = tempfile::TempDir::new().unwrap();
    let store = LocalFileStore::new(tmp.path().to_path_buf()).unwrap();

    let content = "---\nid: page0001\ntitle: \"Target Page\"\ncreated: 2026-01-01T00:00:00Z\ntags: []\n---\n\nSome content\n";
    store.write(&tmp.path().join("pages/Target.md"), content).unwrap();

    let db_path = tmp.path().join(".index/test.db");
    let mut index = SqliteIndex::open(&db_path).unwrap();
    let path = tmp.path().join("pages/Target.md");
    let raw = store.read(&path).unwrap();
    let doc = parse(&raw).unwrap();
    index.index_document(&path, &doc).unwrap();

    let mut state = EditorState::new("");
    state.index = Some(index);

    // Enter insert mode, type [[
    state.handle_key(Key::Char('i'));
    state.handle_key(Key::Char('['));
    state.handle_key(Key::Char('['));

    assert!(state.active_picker.is_some(), "inline picker should open");
    let results = state.active_picker.as_ref().unwrap().inner.results();
    assert_eq!(
        results.len(), 1,
        "inline picker should show 1 page from index, got {}",
        results.len()
    );
}
