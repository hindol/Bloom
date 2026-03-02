# Bloom — Implementation & Testing Phases

This document defines the phased build plan for Bloom, mapping every goal (G1–G23) to a concrete implementation phase with its testing tier. Each phase has a clear exit criterion: what must work and how it is verified before moving on.

Cross-references: [GOALS.md](GOALS.md) · [ARCHITECTURE.md](ARCHITECTURE.md) · [FILE_FORMAT.md](FILE_FORMAT.md) · [KEYBINDINGS.md](KEYBINDINGS.md) · [PICKER_SURFACES.md](PICKER_SURFACES.md) · [DESIGN_DECISIONS.md](DESIGN_DECISIONS.md)

---

## Guiding Principles

1. **Each phase produces a usable artifact.** Phase 0 gives you a text editor that opens files. Phase 1 gives you a note-taking app. Phase 2 makes it powerful. Phase 3 makes it interoperable.
2. **Tests are not an afterthought.** Every phase defines its test tier. No phase is "done" until its tests pass.
3. **Dependencies flow downward.** Later phases build on earlier ones — never the reverse. A bug in Phase 0 is a bug in everything above it.

---

## Current Status

**10,092+ lines of Rust · 297 tests · 0 failures**

| Component | Module | LOC | Tests | Status |
|-----------|--------|-----|-------|--------|
| Document model | `document.rs` | 154 | 2 | ✅ Done |
| Rope buffer + undo tree | `buffer.rs` | 456 | 12 | ✅ Done |
| File store + atomic writes | `store.rs` | 337 | 13 | ✅ Done |
| Markdown parser | `parser.rs` | 1,004 | 40 | ✅ Done |
| Vim engine + leader + pickers + journal + windows | `editor.rs` | 2,975 | 63 | ✅ Done |
| RenderFrame abstraction | `render.rs` | 295 | 2 | ✅ Done |
| SQLite index (FTS5) | `index.rs` | 653 | 9 | ✅ Done |
| Journal service | `journal.rs` | 134 | 3 | ✅ Done |
| Generic picker + surfaces + DynPicker | `picker.rs` | 1,008 | 18 | ✅ Done |
| Link resolver | `resolver.rs` | 278 | 5 | ✅ Done |
| Timeline aggregation | `timeline.rs` | 295 | 4 | ✅ Done |
| File watcher | `watcher.rs` | 269 | 3 | ✅ Done |
| Which-key state machine | `whichkey.rs` | 381 | 4 | ✅ Done |
| Window layout model | `window.rs` | 632 | 5 | ✅ Done |
| Display hint updater | `hint_updater.rs` | 201 | 3 | ✅ Done |
| Syntax highlighting + theme | `highlight.rs` + `render.rs` | 500+ | 9 | ✅ Done |
| TUI frontend (pickers, which-key, multi-pane, capture bar) | `bloom-tui` | 457 | — | ✅ Done |
| GUI frontend (Tauri v2) | `bloom-gui` | 155 | — | ✅ Done |
| Integration tests | `tests/integration.rs` | 255 | 7 | ✅ Done |
| Property tests | `tests/properties.rs` | 89 | 4 | ✅ Done |
| **Totals** | **17 modules + 2 test suites** | **10,092** | **260** | |

---

## Phase 0 — Foundation ✅ Complete

> **Goal**: A modal text editor that opens, edits, and saves Markdown files with undo — testable without any UI.

### Implementation

| Task | Module | Goals | Status |
|------|--------|-------|--------|
| Rust workspace + crate layout | `Cargo.toml` | — | ✅ Done |
| Document model (BloomId, Frontmatter, Block, Link, Embed, Tag, Timestamp) | `document.rs` | G3, G4 | ✅ Done |
| Rope buffer + branching undo tree | `buffer.rs` | G7, G9 | ✅ Done |
| File store (atomic writes, vault structure, filename sanitization) | `store.rs` | G2, G3, G21 | ✅ Done |
| Bloom Markdown parser (frontmatter, extensions, code-block safety) | `parser.rs` | G4 | ✅ Done |
| Vim editing engine (4 modes, motions, operators, counts, commands) | `editor.rs` | G7 | ✅ Done |
| RenderFrame abstraction (UI-agnostic snapshot: lines, cursor, status bar) | `render.rs` | — | ✅ Done |
| TUI frontend (ratatui, consumes RenderFrame, color status bar) | `bloom-tui` | G7, G10 | ✅ Done |
| GUI frontend scaffold (Tauri v2, consumes RenderFrame via JSON commands) | `bloom-gui` | G7, G10 | ✅ Done |

### Vim Engine Coverage

| Category | Implemented |
|----------|------------|
| Modes | Normal, Insert, Visual, Command |
| Mode transitions | `i`, `a`, `I`, `A`, `o`, `O`, `v`, `:`, `Esc` |
| Motions | `h`/`j`/`k`/`l`, `w`/`b`/`e`, `0`/`$`/`^`, `gg`/`G`, arrow keys |
| Count prefixes | `3j`, `3x`, `2dw`, etc. |
| Operators | `d`+motion, `c`+motion, `y`+motion, `dd`, `cc` |
| Single-key actions | `x`, `X`, `r`, `J`, `u`, `Ctrl-R` |
| Insert editing | chars, Enter, Backspace, Tab, arrows |
| Visual mode | select + `d`/`c`/`y` on selection |
| Commands | `:w`, `:q`, `:wq`, `:x`, `:q!`, `:N` (goto line) |

### Testing — Tier 0

| Layer | Tool | What it covers | Count |
|-------|------|----------------|-------|
| Unit | `#[test]` inline | Parser edge cases, buffer ops, store CRUD, document model, Vim motions/operators/modes | 168 |
| Integration | `tests/integration.rs` | `TestVault` helper: store ↔ parser roundtrip, journal entries, cross-references, code-block safety | 7 |
| Property | `tests/properties.rs` + `proptest` | `parse(any_string)` never panics, roundtrip, `sanitize_filename(any_unicode)` always valid, deterministic | 4 |

### Smoke Test

```
# TUI (terminal)
cargo run -p bloom-tui -- docs/GOALS.md

# GUI (Tauri desktop window)
cd crates/bloom-gui/src-tauri && cargo tauri dev
```

**Exit criterion** ✅: Open a `.md` file, navigate with hjkl, edit in Insert mode, undo/redo, save with `:w`, quit with `:q`. Both TUI and GUI consume the same `RenderFrame`. 221 tests pass.

---

## Phase 1 — Note-Taking Core ✅ Complete

> **Goal**: A usable note-taking app with linking, search, journal, and the picker system.

### Implementation — Core Modules ✅

| Task | Module | Goals | Status |
|------|--------|-------|--------|
| SQLite index (FTS5 for full-text, backlinks table, tag index) | `index.rs` | G5, G12, G22 | ✅ Done |
| Link resolver (UUID → file path, backlink lookup, unlinked mention scan) | `resolver.rs` | G4, G5, G6 | ✅ Done |
| Generic `Picker<T>` + `PickerSource` trait + nucleo fuzzy | `picker.rs` | G16 | ✅ Done |
| Concrete picker sources (pages, tags, journal, backlinks, unlinked mentions, commands) | `picker.rs` | G12, G16 | ✅ Done |
| Journal service (today/prev/next, quick-append, quick-task) | `journal.rs` | G14 | ✅ Done |
| Timeline aggregation (chronological backlinks for a page) | `timeline.rs` | G6 | ✅ Done |
| Which-key hierarchical state machine (prefix tree, timeout config) | `whichkey.rs` | G8 | ✅ Done |
| Window layout model (split/navigate/close/maximize/balance) | `window.rs` | G11 | ✅ Done |
| File watcher (notify events, path eligibility, `.md` filtering) | `watcher.rs` | G2, G21 | ✅ Done |

### Implementation — UI Wiring ✅

| Task | Module | Goals | Status |
|------|--------|-------|--------|
| SPC leader dispatch + which-key integration | `editor.rs` | G8 | ✅ Done |
| Wire pickers (`SPC f f`, `SPC SPC`, picker key handling) | `editor.rs` | G4, G12, G16 | ✅ Done |
| `[[`/`![[` inline pickers in insert mode | `editor.rs` | G4 | ✅ Done |
| Wire journal commands (`SPC j j/p/n/a/t` + capture bar) | `editor.rs` | G14 | ✅ Done |
| Wire window commands (`SPC w v/s/h/j/k/l/d/m` + multi-pane) | `editor.rs` | G11 | ✅ Done |
| Display hint updates on page rename | `hint_updater.rs` | G3 | ✅ Done |
| Orphaned link indicators with diagnostics | `editor.rs` | G20 | ✅ Done |
| TUI picker + which-key overlay rendering | `bloom-tui` | G8, G16 | ✅ Done |
| TUI multi-pane + capture bar rendering | `bloom-tui` | G11, G14 | ✅ Done |

### Testing — Tier 1

| Layer | Tool | What it covers |
|-------|------|----------------|
| Unit | `#[test]` | Index CRUD, link resolution, FTS5 queries, picker scoring/filtering, journal path generation, which-key state machine |
| Integration | `TestVault` | **Scenario tests mapped to user journeys:** |
| | | • Write journal entry with `[[link]]` → create topic page → backlinks include journal |
| | | • Rename page → all `[[uuid\|old]]` display hints update to `[[uuid\|new]]` |
| | | • Delete page → backlinks for that UUID return empty, orphaned link detected |
| | | • `SPC s u` unlinked mentions → batch promote → links created in source files |
| | | • External file change (simulated) → watcher fires → index updated |
| | | • `:rebuild-index` → index matches filesystem state |
| TUI | `ratatui::TestBackend` | Picker rendering (result list, filter pills, marginalia), status bar content (mode, filename, dirty, pending keys), which-key popup layout |
| Concurrency | Stress tests | Concurrent write + index rebuild, file watcher during atomic write, rapid edits + debounced save |

**Exit criterion** ✅: Create a journal entry, link to a topic page, see backlinks, search by tag, use `[[` picker, use timeline view. All pickers navigable with keyboard only. 260 tests pass.

---

## Phase 2 — Power Features

> **Goal**: Templates, agenda, refactoring, Logseq import — features that make Bloom a daily driver.

### Implementation

| Task | Module | Goals | Depends on |
|------|--------|-------|------------|
| Template engine (`${N:description}`, `${AUTO}`, `${DATE}`, tab-stop cursor) | `template.rs` | G19 | store |
| Template picker (`SPC n`) | `picker.rs` | G19 | Picker, template engine |
| Agenda view (overdue / today / upcoming, filter by tag/page/date) | `agenda.rs` | G15 | index |
| Agenda interaction (check off `x`, reschedule `s`, jump `Enter`) | `agenda.rs` | G15 | agenda |
| Page split (`SPC r s`: extract section → new page + link/embed) | `refactor.rs` | G18 | resolver |
| Page merge (`SPC r m`: combine pages + redirect links) | `refactor.rs` | G18 | resolver |
| Block move (`SPC r b`: move block, UUID follows) | `refactor.rs` | G18 | resolver |
| Logseq import (non-destructive, syntax mapping, namespace flatten, import report) | `import.rs` | G13 | parser, store, index |
| Setup wizard (first-launch: vault location, optional Logseq import) | `wizard.rs` | G21 | import, store |
| Session restore (persist buffers, layout, cursors; startup mode config) | `session.rs` | G23 | window, store |
| Tag rename/delete across vault | `refactor.rs` | G12 | index |
| Vim text objects for Bloom extensions (`il/al`, `ie/ae`, `i#/a#`, `i@/a@`, `ah/ih`) | `editor.rs` | G7 | parser |
| `.` repeat, macros, registers, marks | `editor.rs` | G7 | editor |

### Testing — Tier 2

| Layer | Tool | What it covers |
|-------|------|----------------|
| Unit | `#[test]` | Template placeholder expansion + tab-stop ordering, agenda grouping logic, refactor link rewriting, Logseq syntax mapping per line |
| Fixtures | `tests/fixtures/logseq/` | Real Logseq vault files (5–10 `.md` files covering: wikilinks, block refs, embeds, namespaces, properties, TODO states, advanced queries). Import → verify every mapping rule. |
| Snapshot | `insta` | Logseq import output (full Document struct per file). Agenda view output for a known vault. Template expansion results. Catches regressions in complex transformations. |
| Property | `proptest` | Template `${N}` placeholders always produce valid cursor positions. Refactor split+merge is lossless (no content lost). |
| Scenario | `TestVault` | **User journeys:** |
| | | • Use template → fill tab-stops → save → re-parse intact |
| | | • Agenda: tasks with `@due` in past/today/future grouped correctly |
| | | • Split page → backlinks update → original has embed → embedded content resolves |
| | | • Merge two pages → all inbound links redirect to merged page |
| | | • Logseq import of 10-file vault → import report accurate, all links resolve |
| | | • Session restore: quit with 3 splits open → relaunch → same layout, cursors, content |
| | | • Vim text object `ci[` on `[[link]]` → correct range selected |

**Exit criterion**: Import a Logseq vault, use templates for new pages, manage tasks via agenda, split/merge pages without broken links. Session survives quit/relaunch.

---

## Phase 3 — Interoperability

> **Goal**: MCP server for LLM integration, cross-platform polish, and production hardening.

### Implementation

| Task | Module | Goals | Depends on |
|------|--------|-------|------------|
| MCP JSON-RPC server (localhost, stdio transport) | `mcp/server.rs` | G17 | — |
| MCP tool dispatch (16 tools: `search_notes`, `read_note`, `create_note`, etc.) | `mcp/tools.rs` | G17 | resolver, store, index |
| MCP shared rope buffer (MCP edits visible in UI, undo includes MCP edits) | `mcp/buffer.rs` | G17 | buffer |
| MCP security (read-only/read-write mode, path exclusion `exclude_paths` globs) | `mcp/security.rs` | G17 | — |
| MCP config (`[mcp]` section in `config.toml`, `SPC T m` toggle) | `mcp/config.rs` | G17 | — |
| Background buffer eviction (60s after last MCP edit, once saved) | `mcp/buffer.rs` | G17 | buffer |
| Title-based fuzzy resolution for MCP tools (ambiguous → return top-N) | `mcp/tools.rs` | G17 | index |
| Platform keybindings (Cmd on macOS, Ctrl on Windows) | `keymap.rs` | G10 | — |
| Cross-platform testing (macOS + Windows CI) | CI | G10 | — |
| Config file support (`config.toml`: startup mode, MCP settings, keybinding overrides) | `config.rs` | G21, G23 | — |
| Performance benchmarks (< 1s display hint update, FTS5 sub-ms for 10K pages) | benchmarks | G3, G5 | — |

### Testing — Tier 3

| Layer | Tool | What it covers |
|-------|------|----------------|
| Unit | `#[test]` | MCP tool dispatch (each of 16 tools), security filtering, title resolution with fuzzy matching, config parsing |
| Protocol | JSON-RPC test client | Send raw JSON-RPC requests → assert correct response schema, error codes, edge cases (missing page, ambiguous title, read-only mode blocks write tools) |
| Integration | `TestVault` + MCP client | **End-to-end MCP scenarios:** |
| | | • MCP `create_note` → note appears in vault → UI buffer exists |
| | | • MCP `edit_note` → undo in UI reverts MCP edit |
| | | • MCP `add_to_journal` → journal entry appended without disturbing open buffer |
| | | • MCP with `mode = "read-only"` → all write tools return error |
| | | • MCP with `exclude_paths = ["private/*"]` → `search_notes` never returns private files |
| | | • Background buffer: MCP opens note → 60s idle + saved → buffer evicted → memory freed |
| Concurrency | `loom` or stress | MCP edit + user edit same buffer simultaneously → no panics, undo tree consistent |
| Platform | CI matrix | macOS + Windows: filesystem case sensitivity, platform keybindings, atomic write behavior |
| Benchmarks | `criterion` | Display hint update latency (target: < 1s for 1000 pages). FTS5 query latency (target: < 1ms for 10K pages). Parser throughput. |

**Exit criterion**: LLM can create, read, edit, and search notes via MCP. Read-only mode enforced. Path exclusion hides private files. No regressions on macOS or Windows.

---

## Testing Infrastructure

### Crates & Tools

| Tool | Purpose | Added in |
|------|---------|----------|
| `#[test]` + `assert!` | Inline unit tests | Phase 0 ✅ |
| `tempfile` | Temp directories for `TestVault` and store tests | Phase 0 ✅ |
| `proptest` | Property-based: never-panic, roundtrip, sanitization | Phase 0 ✅ |
| `assert_keys()` harness | Table-driven Vim keybinding tests | Phase 0 ✅ (scaffold) |
| `TestVault` helper | Store + parser + index integration | Phase 0 ✅ (basic), Phase 1 (full) |
| `ratatui::TestBackend` | Headless TUI rendering assertions | Phase 1 |
| `insta` | Snapshot testing for parser/import/agenda output | Phase 2 |
| `criterion` | Benchmarks for latency-sensitive paths | Phase 3 |
| `loom` | Concurrency correctness (threading model) | Phase 3 |

### `TestVault` Helper

Lives in `crates/bloom-core/tests/integration.rs`. Provides:

```rust
let vault = TestVault::new();                          // temp dir + LocalFileStore
vault.write_page("Name.md", content);                  // write to pages/
vault.write_journal("2026-03-01", content);             // write to journal/
let doc = vault.read_and_parse("Name.md");              // read + parse
let doc = vault.read_and_parse_journal("2026-03-01");   // read + parse journal
```

Phase 1 extends this with:
```rust
vault.index();                                          // build SQLite index
vault.backlinks_for("uuid");                            // query backlinks
vault.search("query");                                  // FTS5 search
vault.unlinked_mentions("page-title");                  // unlinked mention scan
```

### `assert_keys` Harness

Lives in `crates/bloom-core/src/editor.rs`. Signature:

```rust
fn assert_keys(
    initial_text: &str,
    initial_cursor: usize,
    keys: &[Key],
    expected_text: &str,
    expected_cursor: usize,
    expected_mode: Mode,
);
```

Test cases are commented out in the module — uncomment one by one as each Vim motion/operator is implemented. Target: 100+ entries covering the full keybinding spec.

### Test Naming Convention

```
<module>::tests::<what>_<condition>_<expected>
```

Examples:
- `parser::tests::tags_follow_bloom_rules`
- `store::tests::sanitize_filename_with_unicode`
- `editor::tests::normal_mode_dw_deletes_word`
- `integration::cross_references_between_pages`

---

## Goal Coverage Matrix

Every goal (G1–G23) must be covered by at least one test by the end of its phase.

| Goal | Description | Phase | Test Layer |
|------|-------------|-------|------------|
| G1 | Local-only, no network | 0 | Architectural (no network deps in core) |
| G2 | Markdown files on disk | 0 | Store integration |
| G3 | UUID + filename sync | 1 | Integration (rename cascade) |
| G4 | Links, embeds, tags, timestamps | 0 | Parser unit (40 tests) |
| G5 | Unlinked mentions | 1 | Integration (FTS5 scan + promote) |
| G6 | Timeline view | 1 | Integration (chronological backlinks) |
| G7 | Vim modal editing | 0 | `assert_keys` harness (100+ entries) |
| G8 | Which-key | 1 | Unit (state machine) + TUI (rendering) |
| G9 | Undo tree | 0 | Buffer unit (12 tests) |
| G10 | Cross-platform | 3 | CI matrix (macOS + Windows) |
| G11 | Window management | 1 | Unit (layout math) + TUI |
| G12 | Structured search | 1 | Integration (filter stacking) |
| G13 | Logseq import | 2 | Fixtures + snapshots |
| G14 | Daily journal | 1 | Integration (create, navigate, quick-append) |
| G15 | Agenda view | 2 | Unit (grouping) + integration (interact) |
| G16 | Fuzzy picker | 1 | Unit (scoring) + TUI (rendering) |
| G17 | MCP server | 3 | Protocol + integration + concurrency |
| G18 | Refactoring | 2 | Integration (split/merge/move) |
| G19 | Templates | 2 | Unit (placeholder expansion) + integration |
| G20 | Orphaned links | 1 | Integration (detect + render) |
| G21 | Setup wizard | 2 | Integration (first-launch flow) |
| G22 | Index rebuild | 1 | Integration (`:rebuild-index` correctness) |
| G23 | Session restore | 2 | Integration (quit → relaunch → same state) |

---

## Phase Dependency Graph

```
Phase 0: Foundation ✅
  document ─┬─ parser ─┐
  buffer ───┤          ├─ vim engine ─── RenderFrame ─┬─ TUI (ratatui) ✅
  store ────┘          │                              ├─ GUI (Tauri) ✅
                       │                              └─ TestBackend ✅
Phase 1: Note-Taking ✅
  index ✅ ┬─ resolver ✅ ─── hint_updater ✅
  watcher ✅┘           ├─ picker surfaces ✅ ─── wired into editor ✅
  journal ✅────────────┤                       ├─ TUI overlays ✅
  whichkey ✅───────────┤                       ├─ inline [[ pickers ✅
  window ✅─────────────┤                       └─ orphan diagnostics ✅
  timeline ✅───────────┘

Phase 2: Power 🔲
  templates ───────────┤
  agenda ──────────────┤
  refactor ────────────┤
  import ──────────────┤
  session ─────────────┘

Phase 3: Interop 🔲
  mcp server ──────────┤
  platform keys ───────┤
  config ──────────────┤
  benchmarks ──────────┘
```
