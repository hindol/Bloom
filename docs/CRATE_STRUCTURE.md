# Bloom 🌱 — Crate & Module Structure

> Workspace layout, crate responsibilities, and internal module organization.
> See [ARCHITECTURE.md](ARCHITECTURE.md) for the layered architecture and [GOALS.md](GOALS.md) for goals.

---

## Workspace Overview

```
bloom/                          # workspace root
├── Cargo.toml                  # [workspace] manifest
├── crates/
│   ├── bloom-core/             # Core library — all logic, no UI deps
│   ├── bloom-tui/              # TUI frontend (ratatui)
│   ├── bloom-gui/              # GUI frontend (Tauri)
│   ├── bloom-mcp/              # MCP server (localhost, opt-in)
│   ├── bloom-import/           # Logseq importer
│   └── bloom-test-harness/     # Test utilities (dev-dependency only)
├── docs/                       # Design documents
└── README.md
```

---

## Crate Dependency Graph

```
bloom-tui ──────┐
bloom-gui ──────┼──→ bloom-core ←── bloom-test-harness (dev)
bloom-mcp ──────┤
bloom-import ───┘
```

All frontends and tools depend on `bloom-core`. No crate depends on another frontend crate. `bloom-test-harness` is a dev-dependency of `bloom-core` and any crate that needs test utilities.

---

## Crate Responsibilities

### `bloom-core`

The monolithic core. Pure Rust, no UI framework dependencies, no async runtime. Everything that isn't a frontend or an external tool lives here.

**Key design properties for testability:**
- All public APIs accept and return plain Rust types — no framework types leak out.
- The `RenderFrame` abstraction means tests assert on structured data, not terminal output.
- Trait-based abstractions (`DocumentParser`, `NoteStore`, `KeyMapper`) allow test doubles.
- `tracing` instrumentation on all state transitions (Vim mode changes, index updates, link resolution) enables structured debugging.

**Key design properties for debuggability:**
- Every command/keystroke flows through a single `dispatch()` entry point with a `tracing::span`, making it easy to trace exactly what happened.
- `RenderFrame` is serializable — a bug report can include the frame that showed incorrect output.
- The Vim state machine emits tracing events on every transition: `mode_change`, `operator_apply`, `motion_resolve`, `pending_key`.

### `bloom-tui`

Thin binary crate. Reads `RenderFrame`, maps it to `ratatui` widgets, captures terminal input, sends it to `bloom-core`. Minimal logic — rendering and event loop only.

### `bloom-gui`

Thin binary crate. Reads `RenderFrame`, maps it to Tauri webview (HTML/CSS), captures input. Same contract as TUI, different render target.

### `bloom-mcp`

Binary crate. Exposes `bloom-core` functionality over the Model Context Protocol on localhost. Translates MCP tool calls into `bloom-core` API calls. All edits go through the same rope/undo path as the UI.

### `bloom-import`

Library + binary crate. Reads a Logseq vault directory, transforms files into Bloom format, writes to the Bloom vault. One-directional pipeline. Depends on `bloom-core` for types (frontmatter, UUID generation, link syntax) but not on the editor engine.

### `bloom-test-harness`

Dev-dependency only. Provides:

| Utility | Purpose |
|---------|---------|
| `TestVault` | Creates a temp vault with pre-populated pages. Auto-cleanup on drop. |
| `SimInput` | Simulates keystrokes through the full editor pipeline, returns `RenderFrame`. |
| `SnapshotHelpers` | Formats `RenderFrame` into deterministic strings for `insta` snapshots. |
| `PageBuilder` | Builder pattern for creating test pages with links, tags, tasks. |
| `AssertFrame` | Fluent assertions on `RenderFrame` fields (cursor position, status bar, picker state). |

---

## `bloom-core` Internal Modules

```
bloom-core/src/
├── lib.rs                  # Public API surface (BloomEditor, BloomAPI trait)
│
├── buffer/                 # Text buffer and undo
│   ├── mod.rs
│   ├── rope.rs             # Rope wrapper over ropey, Bloom-specific operations
│   ├── undo.rs             # Branching undo tree (RAM-only)
│   └── edit.rs             # Edit operations (insert, delete, replace) as commands
│
├── vim/                    # Modal editing state machine
│   ├── mod.rs
│   ├── state.rs            # Mode enum (Normal, Insert, Visual, Command), transitions
│   ├── grammar.rs          # [count][operator][motion/text-object] parser
│   ├── operator.rs         # d, c, y, >, <, = etc.
│   ├── motion.rs           # w, b, e, f, t, %, gg, G etc.
│   ├── text_object.rs      # Standard (iw, aw, ip, i") + Bloom-specific (il, al, ie, i#, i@, ih)
│   ├── register.rs         # Named registers, system clipboard
│   └── macro.rs            # Macro recording/playback
│
├── parser/                 # Document parsing
│   ├── mod.rs
│   ├── traits.rs           # DocumentParser trait
│   ├── markdown.rs         # BloomMarkdownParser — standard Markdown + extensions
│   ├── frontmatter.rs      # YAML frontmatter parsing/serialization
│   ├── extensions.rs       # [[links]], ^block-ids, #tags, @timestamps
│   └── highlight.rs        # Per-line scan → StyledSpan[] for rendering
│
├── index/                  # SQLite-backed index
│   ├── mod.rs
│   ├── schema.rs           # Table definitions, migrations
│   ├── writer.rs           # Index update operations (called by indexer thread)
│   ├── query.rs            # Search queries, backlink lookups, tag queries
│   └── fts.rs              # FTS5 full-text search, unlinked mentions
│
├── linker/                 # Link resolution and management
│   ├── mod.rs
│   ├── resolver.rs         # UUID → page/section/block resolution
│   ├── backlinks.rs        # Backlink tracking and queries
│   ├── hints.rs            # Display hint updater (background scan on rename)
│   └── orphan.rs           # Orphaned/broken link detection
│
├── picker/                 # Fuzzy picker framework
│   ├── mod.rs
│   ├── picker.rs           # Generic Picker<T> state machine
│   ├── source.rs           # PickerSource trait
│   ├── filter.rs           # Composable filter system (tag, date, links-to, status)
│   └── nucleo.rs           # nucleo integration for fuzzy matching
│
├── which_key/              # Command discoverability
│   ├── mod.rs
│   └── tree.rs             # Hierarchical key tree, timeout logic, popup generation
│
├── render/                 # RenderFrame production
│   ├── mod.rs
│   ├── frame.rs            # RenderFrame struct and all sub-types
│   ├── viewport.rs         # Viewport calculation (scroll, visible lines)
│   └── layout.rs           # Window split layout tree
│
├── store/                  # File storage
│   ├── mod.rs
│   ├── traits.rs           # NoteStore trait (read, write, list, watch)
│   ├── local.rs            # LocalFileStore — filesystem implementation
│   ├── watcher.rs          # File watcher (external change detection)
│   └── disk_writer.rs      # Atomic write (write→fsync→rename), debounced auto-save
│
├── keymap/                 # Keybinding dispatch
│   ├── mod.rs
│   ├── traits.rs           # KeyMapper trait
│   ├── dispatch.rs         # Priority chain: platform → vim → insert → which-key
│   ├── platform.rs         # Platform-specific shortcuts (Cmd/Ctrl)
│   └── user.rs             # User keymap overrides from config
│
├── journal/                # Daily journal
│   ├── mod.rs
│   └── journal.rs          # Today's page, navigation, quick capture, lazy file creation
│
├── agenda/                 # Agenda view
│   ├── mod.rs
│   └── agenda.rs           # Task aggregation, timestamp grouping, filters
│
├── timeline/               # Timeline view
│   ├── mod.rs
│   └── timeline.rs         # Chronological list of linking notes
│
├── template/               # Template engine
│   ├── mod.rs
│   └── template.rs         # Template loading, placeholder expansion, tab-stop cursor
│
├── refactor/               # Note refactoring operations
│   ├── mod.rs
│   ├── split.rs            # Split page (extract section)
│   ├── merge.rs            # Merge pages
│   └── move_block.rs       # Move block between pages
│
├── config/                 # Configuration
│   ├── mod.rs
│   └── config.rs           # config.toml parsing, defaults, validation
│
├── vault/                  # Vault management
│   ├── mod.rs
│   ├── setup.rs            # Setup wizard logic, vault creation, .gitignore generation
│   ├── adopt.rs            # File adoption (add frontmatter to unrecognized .md files)
│   └── conflict.rs         # Git merge conflict detection
│
├── session/                # Session persistence
│   ├── mod.rs
│   └── session.rs          # Save/restore open buffers, layout, cursors, scroll positions
│
├── window/                 # Window/split management
│   ├── mod.rs
│   └── window.rs           # Split, navigate, resize, balance, maximize
│
├── uuid.rs                 # 8-char hex UUID generation, collision detection
├── types.rs                # Shared types (PageId, BlockId, TagName, Timestamp, etc.)
└── error.rs                # Error types (thiserror-based)
```

### Module Dependency Rules

To keep the monolithic crate manageable, these internal dependency rules apply:

1. **`types`, `uuid`, `error`** — Leaf modules. Depended on by everything, depend on nothing.
2. **`buffer`** — Depends only on `types`. No knowledge of Markdown, links, or Vim.
3. **`parser`** — Depends on `types`. No knowledge of the editor, buffer, or index.
4. **`vim`** — Depends on `buffer`, `types`. No knowledge of Markdown, links, or files.
5. **`index`**, **`linker`**, **`store`** — Depend on `types`, `parser`. May depend on each other.
6. **`picker`**, **`which_key`**, **`render`** — Depend on `types`. UI-adjacent but framework-free.
7. **`keymap`** — Depends on `vim`, `picker`, `which_key`. Orchestrates input dispatch.
8. **`journal`**, **`agenda`**, **`timeline`**, **`template`**, **`refactor`** — Feature modules. Depend on lower layers as needed.
9. **`config`**, **`vault`**, **`session`**, **`window`** — Infrastructure modules.
10. **`lib.rs`** — The `BloomEditor` type that wires everything together. Depends on all modules.

### Tracing Instrumentation Strategy

Every module annotates key functions with `#[tracing::instrument]`:

```rust
// vim/state.rs
#[tracing::instrument(skip(self), fields(from = ?self.mode, to = ?new_mode))]
pub fn transition(&mut self, new_mode: Mode) { ... }

// keymap/dispatch.rs
#[tracing::instrument(skip(self), fields(key = %event, mode = ?self.vim.mode()))]
pub fn dispatch(&mut self, event: KeyEvent) -> Option<Action> { ... }

// index/writer.rs
#[tracing::instrument(skip(self, content), fields(page = %page_id))]
pub fn index_page(&mut self, page_id: &PageId, content: &str) -> Result<()> { ... }
```

At runtime, `BLOOM_LOG=bloom_core::vim=debug,bloom_core::keymap=trace` gives you:
```
  vim::state::transition  from=Normal to=Insert
    keymap::dispatch  key=i mode=Normal
```

### Snapshot Testing Strategy (insta)

Tests use `SimInput` from `bloom-test-harness` to drive the editor, then snapshot the `RenderFrame`:

```rust
#[test]
fn test_heading_renders_bold() {
    let mut sim = SimInput::with_page("# Hello World\n\nSome text.");
    let frame = sim.render();
    insta::assert_snapshot!(SnapshotHelpers::format_lines(&frame));
}

#[test]
fn test_vim_delete_word() {
    let mut sim = SimInput::with_page("hello world");
    sim.keys("dw");
    insta::assert_snapshot!(SnapshotHelpers::format_buffer(&sim.render()));
    // Snapshot: "world" with cursor at position 0
}

#[test]
fn test_picker_filters_by_tag() {
    let mut sim = SimInput::with_vault(TestVault::new()
        .page("Rust Notes").tags(&["rust"])
        .page("Python Notes").tags(&["python"])
    );
    sim.keys("SPC f f").type_text("rust");
    let frame = sim.render();
    insta::assert_snapshot!(SnapshotHelpers::format_picker(&frame));
    // Snapshot shows only "Rust Notes" in results
}
```

---

## Key Cargo Dependencies

| Crate | Used by | Purpose |
|-------|---------|---------|
| `ropey` | bloom-core | Rope text buffer |
| `rusqlite` | bloom-core | SQLite index (with FTS5) |
| `nucleo` | bloom-core | Fuzzy matching (from Helix) |
| `serde` + `toml` | bloom-core | Config parsing, frontmatter |
| `crossbeam` | bloom-core | Inter-thread channels |
| `notify` | bloom-core | File system watching |
| `thiserror` | bloom-core | Error types |
| `tracing` | all crates | Structured logging/debugging |
| `uuid` | bloom-core | UUID generation (v4, truncated to 8 hex) |
| `ratatui` + `crossterm` | bloom-tui | Terminal UI |
| `tauri` | bloom-gui | GUI framework |
| `insta` | all crates (dev) | Snapshot testing |
| `tempfile` | bloom-test-harness | Temp directories for test vaults |

---

## Related Documents

| Document | Contents |
|----------|----------|
| [ARCHITECTURE.md](ARCHITECTURE.md) | Layered architecture, RenderFrame, threading model |
| [GOALS.md](GOALS.md) | Feature goals and non-goals |
| [DESIGN_DECISIONS.md](DESIGN_DECISIONS.md) | All 30 design decisions |
