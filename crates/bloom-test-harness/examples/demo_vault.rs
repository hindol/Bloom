#[allow(clippy::type_complexity)]
/// Demo vault generator — creates realistic pages, journal entries, and
/// backdated git history so that page history, block history, and day
/// activity views all light up immediately.
///
/// Run: cargo run -p bloom-test-harness --example demo_vault
fn main() {
    let vault = dirs::home_dir().unwrap().join("bloom");
    let index_dir = vault.join(".index");
    std::fs::create_dir_all(&index_dir).unwrap();

    let repo = bloom_history::HistoryRepo::open(&index_dir).expect("init history repo");

    // ── Pages (uuid, filename, [(content_version, unix_timestamp)]) ──

    let pages: Vec<(&str, &str, Vec<(&str, i64)>)> = vec![
        (
            "a1b2c3d4",
            "Rope Data Structures",
            vec![
                (
                    "---\nid: a1b2c3d4\ntitle: \"Rope Data Structures\"\ncreated: 2026-02-15\ntags: [rust, data-structures, editors]\n---\n\n\
                     ## Overview\n\nRopes are a tree-based data structure for representing long strings.\n\n\
                     - [ ] Read the ropey crate API docs ^k7m2x\n\
                     - [ ] Benchmark insert performance ^p3a9f\n",
                    1739577600, // Feb 15
                ),
                (
                    "---\nid: a1b2c3d4\ntitle: \"Rope Data Structures\"\ncreated: 2026-02-15\ntags: [rust, data-structures, editors]\n---\n\n\
                     ## Overview\n\nRopes are a tree-based data structure for representing long strings.\n\
                     They provide O(log n) insert and delete operations.\n\n\
                     ## Why Ropes?\n\nGap buffers are simpler but O(n) for inserts far from the gap.\n\
                     Piece tables are append-only — good for undo but complex.\n\
                     Ropes balance simplicity with performance.\n\n\
                     - [x] Read the ropey crate API docs ^k7m2x\n\
                     - [ ] Benchmark insert performance ^p3a9f\n\
                     - [ ] Compare with xi-rope ^w1b5q\n",
                    1739836800, // Feb 18
                ),
                (
                    "---\nid: a1b2c3d4\ntitle: \"Rope Data Structures\"\ncreated: 2026-02-15\ntags: [rust, data-structures, editors]\n---\n\n\
                     ## Overview\n\nRopes are a tree-based data structure for representing long strings.\n\
                     They provide O(log n) insert and delete operations.\n\n\
                     ## Why Ropes?\n\nGap buffers are simpler but O(n) for inserts far from the gap.\n\
                     Piece tables are append-only — good for undo but complex.\n\
                     Ropes balance simplicity with performance.\n\n\
                     ## Benchmarks\n\nTested ropey on a 50MB file:\n\
                     - Insert at position 0: 2.3us\n\
                     - Insert at midpoint: 2.1us\n\
                     - Append at end: 1.8us\n\
                     - Line iteration (all lines): 12ms\n\n\
                     - [x] Read the ropey crate API docs ^k7m2x\n\
                     - [x] Benchmark insert performance ^p3a9f\n\
                     - [x] Compare with xi-rope ^w1b5q\n",
                    1740268800, // Feb 23
                ),
            ],
        ),
        (
            "deadbeef",
            "Vim Modal Editing",
            vec![
                (
                    "---\nid: deadbeef\ntitle: \"Vim Modal Editing\"\ncreated: 2026-02-20\ntags: [vim, editors, ux]\n---\n\n\
                     ## The Grammar\n\nVim's power comes from composability: `[count] [operator] [motion]`.\n\n\
                     Operators: `d` (delete), `c` (change), `y` (yank)\n\
                     Motions: `w` (word), `$` (end of line), `}` (paragraph)\n\n\
                     ## Why Modal?\n\nModal editing separates navigation from insertion.\n\
                     Most time is spent reading and navigating, not typing.\n",
                    1740009600, // Feb 20
                ),
                (
                    "---\nid: deadbeef\ntitle: \"Vim Modal Editing\"\ncreated: 2026-02-20\ntags: [vim, editors, ux]\n---\n\n\
                     ## The Grammar\n\nVim's power comes from composability: `[count] [operator] [motion]`.\n\n\
                     Operators: `d` (delete), `c` (change), `y` (yank)\n\
                     Motions: `w` (word), `$` (end of line), `}` (paragraph)\n\n\
                     ## Why Modal?\n\nModal editing separates navigation from insertion.\n\
                     Most time is spent reading and navigating, not typing.\n\n\
                     ## Text Objects\n\nText objects select structured regions:\n\
                     - `iw` inner word, `aw` around word\n\
                     - `ip` inner paragraph, `ap` around paragraph\n\n\
                     Bloom adds custom text objects:\n\
                     - `il` / `al` — inside/around `[[link]]`\n\
                     - `i#` / `a#` — inside/around `#tag`\n\
                     - `i@` / `a@` — inside/around `@due(...)`\n",
                    1740441600, // Feb 25
                ),
            ],
        ),
        (
            "cafe0123",
            "Local-First Software",
            vec![(
                "---\nid: cafe0123\ntitle: \"Local-First Software\"\ncreated: 2026-03-01\ntags: [architecture, philosophy]\n---\n\n\
                 ## Principles\n\nFrom the Ink & Switch paper:\n\n\
                 1. No spinners — data is always available locally\n\
                 2. Your data, your device — no cloud dependency\n\
                 3. The network is optional, not required\n\
                 4. Collaboration is a feature, not a requirement\n\n\
                 Bloom follows principles 1-3. Collaboration is a non-goal.\n\n\
                 ## Why It Matters\n\nYour notes contain your most private thoughts.\n\
                 Company secrets, personal reflections, half-formed ideas.\n\
                 They should never leave your machine unless you choose.\n",
                1740787200, // Mar 1
            )],
        ),
        (
            "f00dcafe",
            "Block Identity Design",
            vec![
                (
                    "---\nid: f00dcafe\ntitle: \"Block Identity Design\"\ncreated: 2026-03-05\ntags: [bloom, architecture]\n---\n\n\
                     ## The Problem\n\nBlocks identified by line number break when lines shift.\n\
                     We need stable identity that survives edits and moves.\n\n\
                     ## Proposal: 5-char base36 IDs\n\n\
                     - 60.5M ID space for ~5M lifetime blocks\n\
                     - Vault-scoped, not page-scoped\n\
                     - Appended to line end: `^k7m2x`\n",
                    1741132800, // Mar 5
                ),
                (
                    "---\nid: f00dcafe\ntitle: \"Block Identity Design\"\ncreated: 2026-03-05\ntags: [bloom, architecture]\n---\n\n\
                     ## The Problem\n\nBlocks identified by line number break when lines shift.\n\
                     We need stable identity that survives edits and moves.\n\n\
                     ## Solution: 5-char base36 IDs\n\n\
                     - 60.5M ID space for ~5M lifetime blocks\n\
                     - Vault-scoped, not page-scoped\n\
                     - Appended to line end: `^k7m2x`\n\
                     - Never reused — retired IDs reserved permanently\n\n\
                     ## Mirroring\n\nSame block ID in multiple files = mirror.\n\
                     `^k7m2x` (solo) vs `^=k7m2x` (mirrored).\n\
                     Edit one, all copies update on mode transition.\n",
                    1741392000, // Mar 8
                ),
            ],
        ),
    ];

    // ── Journal entries (uuid, date, content, timestamp) ─

    let journals: Vec<(&str, &str, &str, i64)> = vec![
        (
            "j0000001",
            "2026-02-15",
            "---\nid: j0000001\ntitle: \"2026-02-15\"\ncreated: 2026-02-15\n---\n\n\
             - Started exploring rope data structures for Bloom's buffer model\n\
             - Read about [[a1b2c3d4|Rope Data Structures]] — O(log n) is promising\n\
             - [ ] Set up the Cargo workspace\n\
             #rust #editors\n",
            1739577600,
        ),
        (
            "j0000002",
            "2026-02-20",
            "---\nid: j0000002\ntitle: \"2026-02-20\"\ncreated: 2026-02-20\n---\n\n\
             - Deep dive into [[deadbeef|Vim Modal Editing]] grammar\n\
             - The composability of operators + motions is elegant\n\
             - [x] Set up the Cargo workspace\n\
             - [ ] Implement basic Normal/Insert mode switching\n\
             #vim #editors\n",
            1740009600,
        ),
        (
            "j0000003",
            "2026-03-01",
            "---\nid: j0000003\ntitle: \"2026-03-01\"\ncreated: 2026-03-01\n---\n\n\
             - Read the Ink & Switch [[cafe0123|Local-First Software]] paper\n\
             - Bloom should follow their principles — no cloud, no sync\n\
             - The SQLite index is just a cache — files are truth\n\
             - [x] Implement basic Normal/Insert mode switching\n\
             - [ ] Build the index layer @due(2026-03-10)\n\
             #architecture\n",
            1740787200,
        ),
        (
            "j0000004",
            "2026-03-08",
            "---\nid: j0000004\ntitle: \"2026-03-08\"\ncreated: 2026-03-08\n---\n\n\
             - Block identity design is coming together\n\
             - See [[f00dcafe|Block Identity Design]] for the full spec\n\
             - 5-char base36 gives 60M IDs — more than enough\n\
             - Mirroring is the killer feature: `^=` markers in file content\n\
             - [x] Build the index layer\n\
             - [ ] Implement block ID assignment @due(2026-03-15)\n\
             #bloom #architecture\n",
            1741392000,
        ),
        (
            "j0000005",
            "2026-03-14",
            "---\nid: j0000005\ntitle: \"2026-03-14\"\ncreated: 2026-03-14\n---\n\n\
             - Block IDs are working — assignment on save, 5-char base36\n\
             - Mirror promotion/demotion implemented\n\
             - [x] Implement block ID assignment\n\
             - [ ] Build temporal navigation strip @due(2026-03-20)\n\
             - [ ] Add page history view @due(2026-03-22)\n\
             #bloom\n",
            1741910400,
        ),
        (
            "j0000006",
            "2026-03-16",
            "---\nid: j0000006\ntitle: \"2026-03-16\"\ncreated: 2026-03-16\n---\n\n\
             - Working on the docs site today\n\
             - Animated GIFs from the test harness look great\n\
             - [ ] Polish the landing page\n\
             - [ ] Add more animation tests for mirroring and journal\n\
             #bloom #docs\n",
            1742083200,
        ),
    ];

    // ── Write files to disk ──────────────────────────────

    println!("Writing files...");
    for (_, filename, versions) in &pages {
        let last = versions.last().unwrap().0;
        let path = vault.join("pages").join(format!("{filename}.md"));
        std::fs::write(&path, last).unwrap();
        println!("  pages/{filename}.md");
    }

    for (_, date, content, _) in &journals {
        let path = vault.join("journal").join(format!("{date}.md"));
        std::fs::write(&path, content).unwrap();
        println!("  journal/{date}.md");
    }

    // ── Create backdated git history ─────────────────────

    println!("\nCreating history...");

    // Build a timeline sorted by timestamp
    let mut timeline: Vec<(i64, Vec<(&str, &str)>, String)> = Vec::new();

    for (uuid, filename, versions) in &pages {
        for (content, ts) in versions {
            timeline.push((*ts, vec![(*uuid, *content)], format!("edited {filename}")));
        }
    }

    for (uuid, date, content, ts) in &journals {
        timeline.push((*ts, vec![(*uuid, *content)], format!("journal {date}")));
    }

    timeline.sort_by_key(|(ts, _, _)| *ts);

    for (ts, files, msg) in &timeline {
        let files_ref: Vec<(&str, &str)> = files.iter().map(|(a, b)| (*a, *b)).collect();
        match repo.commit_all(&files_ref, msg, Some(*ts)) {
            Ok(Some(oid)) => println!("  {} {} ({})", &oid[..8], msg, ts),
            Ok(None) => println!("  skip: {}", msg),
            Err(e) => eprintln!("  ERROR: {} — {}", msg, e),
        }
    }

    println!("\n✓ Demo vault ready at {}", vault.display());
    println!(
        "  {} pages, {} journal entries, {} commits",
        pages.len(),
        journals.len(),
        timeline.len()
    );
    println!("\n  Launch: cargo run -r -p bloom-tui");
}
