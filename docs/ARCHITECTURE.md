# Bloom 🌱 — Architecture

> Technical architecture for Bloom. See [GOALS.md](GOALS.md) for goals and non-goals.

---

## Tech Stack

| Component | Choice | Rationale |
|-----------|--------|-----------|
| Language | Rust | Memory safety, performance, cross-platform, zero-cost abstractions |
| UI framework | Tauri (GUI) + ratatui (TUI) | Both built in parallel over a shared `RenderFrame` abstraction. Frontends are thin render targets. |
| Text buffer | Rope (`ropey` crate) | O(log n) operations, natural undo tree support via persistent snapshots, composable for transclusion |
| Storage | Markdown files on disk + SQLite index | Files for portability, SQLite for fast search/backlinks/metadata queries |
| Channels | `crossbeam` | Inter-thread communication, no async runtime dependency in core |

---

## Layered Architecture

```
┌─────────────────────────────────────────────────────┐
│                  Frontend Layer                       │
│  (Tauri GUI / TUI / MCP Server — all swappable)      │
│  Responsibility: consume RenderFrame, capture input   │
└──────────────────────┬──────────────────────────────┘
                       │ RenderFrame (UI-agnostic snapshot)
┌──────────────────────▼──────────────────────────────┐
│                   Core Library                       │
│            (pure Rust crate, no UI deps)             │
│                                                      │
│  • Editor engine (rope + undo tree)                  │
│  • Vim modal editing state machine                   │
│  • RenderFrame producer (visible lines, cursor,      │
│    status bar, picker state, diagnostics)             │
│  • Which-key discoverability                         │
│  • Theme engine (palettes, style resolution)         │
│  • Link resolver + backlink tracker                  │
│  • Bloom Markdown parser                             │
│  • Search / query engine                             │
│  • Unlinked mentions scanner                         │
└──────────────────────┬──────────────────────────────┘
                       │ traits
┌──────────────────────▼──────────────────────────────┐
│              Abstraction Traits                       │
│                                                      │
│  DocumentParser  — parse/serialize file format        │
│  NoteStore       — read/write/list/watch storage      │
│  KeyMapper       — platform-specific key mapping      │
└──────────────────────┬──────────────────────────────┘
                       │
┌──────────────────────▼──────────────────────────────┐
│           Concrete Implementations                    │
│                                                      │
│  BloomMarkdownParser + LocalFileStore + MacOS/Win    │
│  (swap any independently)                            │
└─────────────────────────────────────────────────────┘
```

### RenderFrame Abstraction

The core library produces a `RenderFrame` — a UI-agnostic snapshot of everything to draw. Frontends never query editor state directly; they consume frames. The core owns layout computation (Vim/Emacs model); the TUI reads positions from the frame.

```
terminal.draw(|f| {
    let (w, h) = f.area();
    let frame = editor.render(w, h);    // core computes layout for this exact size
    tui::draw(f, &frame, &theme);       // TUI reads rects, renders widgets
});
```

A `RenderFrame` contains:

| Field | Type | Description |
|-------|------|-------------|
| `panes` | `Vec<PaneFrame>` | Pane content, cursor, status bar, and **layout rect** |
| `maximized` | `bool` | Whether a pane is maximized (hides others) |
| `hidden_pane_count` | `usize` | Number of hidden panes when maximized |
| `picker` | `Option<PickerFrame>` | Active picker: query, results, selected index, preview |
| `agenda` | `Option<AgendaFrame>` | Agenda overlay: tasks grouped by overdue/today/upcoming |
| `which_key` | `Option<WhichKeyFrame>` | Popup: available key bindings in current prefix |
| `dialog` | `Option<DialogFrame>` | Confirmation dialog |
| `notification` | `Option<Notification>` | Transient status message |

Each `PaneFrame` includes a `PaneRectFrame` with `x, y, width, content_height, total_height` — concrete cell positions computed by the core's `WindowManager::compute_pane_rects()`. The TUI reads these directly instead of computing its own layout.

This means:
- **Tests assert on `RenderFrame`** — no terminal or browser needed.
- **TUI and GUI are thin** — they map `RenderFrame` fields to their native primitives.
- **Layout lives in one place** — the core computes pane dimensions; the TUI never splits areas.
- **Design issues surface early** — both frontends consume the same contract.

---

## Rendering Model

### Render Loop

The TUI render loop runs synchronously on the UI thread at ~60fps (or on input):

```
loop {
    terminal.draw(|f| {
        let area = f.area();                        // actual terminal dimensions
        let frame = editor.render(area.w, area.h);  // core: layout + viewport + content
        tui::draw(f, &frame, &theme);               // TUI: widgets into ratatui buffer
    });                                              // ratatui diffs and flushes to terminal
    wait_for_input_or_tick();
}
```

**Key property:** The terminal dimensions (`f.area()`) flow directly into `render()`, which uses them to compute pane rects. The same dimensions are used by the TUI to draw. No stored state to drift.

### Cell Painting Strategy

ratatui uses **differential rendering** — it maintains an in-memory buffer and only flushes cells that changed since the last frame to the terminal. This makes full-screen repaints cheap in the common case (most cells don't change).

Each frame follows a three-layer painting strategy:

```
Layer 1: Clear + Background    ← writes ' ' with bg to every cell (clean slate)
Layer 2: Pane content          ← editor lines, status bars, written into pane rects
Layer 3: Overlays              ← picker, agenda, dialog, notification (drawn last, wins)
```

**Layer 1** ensures no stale content from previous frames bleeds through. `Clear` writes space characters (content reset), then `Block::default().style(bg)` sets the background colour. Both operate on the in-memory buffer — the terminal only receives the final diff.

**Layer 2** renders each pane into its core-computed rect. The pane content (editor lines, syntax-highlighted spans) overwrites the background layer. Each pane includes its own status bar at the bottom of its rect.

**Layer 3** renders overlays on top of panes. Overlays draw last, so their `set_cursor_position()` calls override the pane cursor — each overlay owns its cursor (picker query input, agenda selected row, dialog choice).

### Viewport and Scrolling

The viewport tracks which lines of the buffer are visible. On each `render()` call:

1. `compute_pane_rects(w, h)` → active pane's `content_height`
2. `viewport.height = content_height` (always matches the real screen area)
3. `viewport.ensure_visible(cursor_line)` (scrolls if cursor moved past the edge)
4. `render_buffer_lines()` produces `RenderedLine`s for the visible range

The viewport height is never guessed or stored separately — it's derived from the layout computation on every frame, using the actual terminal dimensions.

### Semantic Highlighting Pipeline

All content — editor, picker preview, agenda tasks — uses the same highlighting path:

```
                        ┌─────────────────────┐
                        │     text line        │
                        └──────────┬──────────┘
                                   │
                                   ▼
                    ┌──────────────────────────────┐
                    │  parser.highlight_line(line)  │
                    └──────────────┬───────────────┘
                                   │
                                   ▼
                          Vec<StyledSpan>
                                   │
                    ┌──────────────┴───────────────┐
                    │                              │
                    ▼                              ▼
         ┌──────────────────┐           ┌──────────────────────┐
         │ theme.style_for()│           │ search_highlight::   │
         │ (syntax styles)  │           │ highlight_matches()  │
         └────────┬─────────┘           │ (SearchMatch spans)  │
                  │                     └──────────┬───────────┘
                  │                                │
                  └──────────┬─────────────────────┘
                             │  overlay / merge
                             ▼
                    ┌──────────────────┐
                    │  Span::styled()  │
                    │  → rendered cell │
                    └──────────────────┘
```

Search match highlighting overlays on top via `render::search_highlight::highlight_matches()`, which produces `SearchMatch` spans that split or override base syntax spans.

---

## Threading Model

```
┌──────────────────────────────────────────────────────┐
│                    UI Thread                          │
│                                                      │
│  ┌─ Event Loop ─────────────────────────────────┐    │
│  │  1. Poll for input (crossterm)               │    │
│  │  2. Poll indexer completion channel           │    │
│  │  3. Dispatch key → BloomEditor::handle_key() │    │
│  │  4. Process Vim grammar, apply edits to rope  │    │
│  │  5. Call editor.render(w, h) → RenderFrame    │    │
│  │  6. TUI draws RenderFrame into ratatui buffer │    │
│  │  7. ratatui diffs and flushes to terminal     │    │
│  └──────────────────────────────────────────────┘    │
│                                                      │
│  Rule: NEVER blocks. All I/O dispatched via channels.│
│  Rope edits are O(log n) ≈ microseconds.             │
│  Render produces a snapshot — no locks held.         │
│  Index queries are read-only — no write contention.  │
└────────┬─────────────────┬──────────────┬────────────┘
    channel            channel        channel
         │                 │              │
         ▼                 ▼              ▼
┌────────────┐    ┌────────────┐   ┌────────────┐
│ Disk Writer│    │  Indexer   │   │File Watcher│
│ (OS thread)│    │ (OS thread)│   │ (OS thread)│
│            │    │            │   │            │
│ Receives   │    │ Orchestrat-│   │ Watches    │
│ write reqs │    │ or thread  │   │ vault dir, │
│ via channel│    │ that coord-│   │ sends file │
│ Debounced  │    │ inates     │   │ events via │
│ 300ms,     │    │ NoteStore, │   │ channel to │
│ atomic     │    │ Parser,    │   │ UI thread  │
│ write→     │    │ and Index  │   │            │
│ fsync→     │    │ layers.    │   │ Uses notify│
│ rename     │    │ See below. │   │ crate      │
└────────────┘    └────────────┘   └────────────┘
```

### Thread Responsibilities

| Thread | Input | Output | Blocking? |
|--------|-------|--------|-----------|
| **UI** | Terminal key events, file watcher events, indexer completion | Rope edits, RenderFrame, write/index requests | Never — all I/O via channels |
| **Disk Writer** | `WriteRequest` via channel | Atomic file writes (write→fsync→rename) | Blocks on disk I/O (own thread) |
| **Indexer** | Triggered on startup and on file change events | Updated SQLite FTS5 index, completion notification | Blocks on file I/O + SQLite (own thread) |
| **File Watcher** | Filesystem notifications (notify crate) | `FileEvent` via channel to UI thread | Blocks waiting for OS events (own thread) |

### Indexer Architecture

The indexer is a background thread that coordinates three existing layers to build the search index without blocking the UI:

```
Indexer Thread
    │
    ├── Phase 1: Scan
    │   NoteStore::list_pages() + list_journals()
    │   stat() each file for (mtime, size)
    │   Compare against fingerprints stored in SQLite
    │   → changed[], deleted[], unchanged[]
    │
    ├── Phase 2: Read + Parse (parallel via rayon)
    │   For each changed file:
    │     NoteStore::read(path) → content
    │     DocumentParser::parse(content) → Document
    │     Extract IndexEntry (frontmatter, links, tags, tasks)
    │   Multiple files processed concurrently on the rayon thread pool
    │
    ├── Phase 3: Write (batched single transaction)
    │   BEGIN TRANSACTION
    │     DELETE entries for deleted files
    │     INSERT/REPLACE entries for changed files
    │     UPDATE fingerprints (mtime, size)
    │   COMMIT
    │   Single fsync instead of one per file
    │
    └── Send IndexComplete { timing, stats } to UI via channel
```

**Fingerprint cache:** The index stores `(path, mtime_secs, size_bytes)` for each indexed file. On startup, the indexer `stat()`s each file (fast — no content read) and compares against stored fingerprints. Only files with changed mtime or size are re-read. For the common "nothing changed" case, startup goes from reading 1050 files to 1050 `stat()` calls — typically <10ms.

**Parallel reads:** Changed files are read and parsed concurrently using `rayon::par_iter()`. This mitigates NTFS per-file overhead on Windows where sequential small-file reads are slow. On a 4-core machine, 4 files are read simultaneously.

**Batched writes:** All SQLite mutations happen in a single `BEGIN ... COMMIT` transaction. This turns N fsyncs into 1, which is the largest SQLite bulk-insert optimization (~100x for 1000+ rows).

**Graceful degradation:** While the indexer runs, the editor is fully usable. Features that depend on the index (backlinks, unlinked mentions, agenda, tag queries) return empty results gracefully. Features that read files directly (find page, search, buffer editing) work immediately. The status bar shows `⟳` while indexing and a "Index ready" notification on completion.

### Communication Pattern

All inter-thread communication uses `crossbeam` channels (bounded, lock-free):

- **UI → Disk Writer**: `crossbeam::Sender<WriteRequest>` — fire-and-forget, debounced
- **Indexer → UI**: `crossbeam::Sender<IndexComplete>` — completion notification with timing
- **File Watcher → UI**: `crossbeam::Receiver<FileEvent>` — polled in event loop; triggers re-index of changed files
- **No shared mutable state** — threads communicate exclusively via channels
- **SQLite access** — the Index handle lives on the indexer thread during writes, UI thread reads use a separate read-only connection (SQLite WAL mode supports concurrent readers)

The core library has **zero dependency on any async runtime**. All concurrency is OS threads + channels. This keeps the dependency tree small, debugging straightforward, and latency predictable.

---

## Data Safety

- **Atomic writes**: The disk writer uses a write→fsync→rename pattern. Content is written to a temporary file, fsynced to disk, then atomically renamed over the target. A crash at any point leaves either the old or new file intact — never a half-written file.
- **Auto-save**: Debounced at 300ms after last keystroke. Dirty-buffer indicator shown in the status bar (dot or [+] next to filename).
- **External file changes vs dirty buffer**: If the file watcher detects an external change to a file with unsaved in-memory edits, Bloom shows a prompt: "File changed on disk. Reload (losing edits) or keep buffer version?" This also handles `git checkout` scenarios.

---

## Keybinding Architecture

1. **Platform shortcuts** (Cmd+S / Ctrl+S) — checked first, always work.
2. **Vim grammar state machine** — checked second, handles modal editing.
3. **Insert mode passthrough** — if in insert mode, character goes to buffer.
4. **Which-key popup** — appears after timeout during pending key sequences.
5. **User customization** — keymap config file (`~/.bloom/keymap.toml`) for overrides.

---

## Unicode and Filesystem Handling

- **Unicode normalization**: All filenames and page title lookups use NFC normalization. This prevents macOS NFD decomposition issues with non-ASCII filenames (e.g., Japanese, accented characters).
- **Diacritic-insensitive search**: Fuzzy matching normalizes diacritics — `cafe` matches `café`. Powered by `nucleo`'s Unicode-aware matching.
- **Filename sanitization**: See [GOALS.md G3](GOALS.md#g3-uuid-based-stable-linking) for title→filename derivation rules.
- **Symlinks**: Ignored in vault directories. Not followed, not indexed.
- **Non-.md files**: Ignored outside `images/`. The file watcher and indexer only process `.md` files.

---

## Unlinked Mentions Scaling

Unlinked mentions (G5) use SQLite FTS5 full-text search — not a naive O(N×M) scan. Page titles are registered as search terms; the FTS5 index is updated by the indexer thread on file save. This scales to 10K+ pages with sub-millisecond query times.

---

## Background Buffer Lifecycle

Background buffers (created by MCP edits or background hint updates to files not open in the UI) are evicted from memory 60 seconds after their last edit, once saved to disk. This prevents unbounded memory growth from bulk operations.
