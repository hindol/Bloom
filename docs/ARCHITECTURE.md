# Bloom 🌱 — Architecture

> Technical architecture for Bloom. See [GOALS.md](GOALS.md) for goals and non-goals.

---

## Tech Stack

| Component | Choice | Rationale |
|-----------|--------|-----------|
| Language | Rust | Memory safety, performance, cross-platform, zero-cost abstractions |
| UI framework | Iced (GUI) | Built over a shared `RenderFrame` abstraction. The GUI is a thin render target. |
| Text buffer | Rope (`ropey` crate) | O(log n) operations, natural undo tree support via persistent snapshots, composable for transclusion |
| Storage | Markdown files on disk + SQLite index | Files for portability, SQLite for fast search/backlinks/metadata queries |
| Channels | `crossbeam` | Inter-thread communication, no async runtime dependency in core |

---

## Layered Architecture

```text
┌─────────────────────────────────────────────────────┐
│                  Frontend Layer                       │
│  (Iced GUI / MCP Server — all swappable)              │
│  Responsibility: consume RenderFrame, capture input   │
└──────────────────────┬──────────────────────────────┘
                       │ RenderFrame (UI-agnostic snapshot)
┌──────────────────────▼──────────────────────────────┐
│               bloom-core (orchestrator)               │
│                                                      │
│  • Editor engine (key dispatch, save, session)       │
│  • RenderFrame producer (visible lines, cursor,      │
│    status bar, picker state, diagnostics)             │
│  • Which-key discoverability                         │
│  • Link resolver + backlink tracker                  │
│  • Search / query engine (BQL)                       │
│  • Index (SQLite FTS5, backlinks, tags)              │
│  • Unlinked mentions scanner                         │
│  • Window manager (splits, panes)                    │
└──────────────────────┬──────────────────────────────┘
                       │ depends on
  ┌────────────────────┼──────────────────────┐
  │                    │                      │
┌─▼──────────┐ ┌──────▼──────┐ ┌─────────────▼──┐
│ bloom-vim   │ │  bloom-md   │ │  bloom-store   │
│             │ │             │ │                │
│ Vim state   │ │ Parser,     │ │ LocalFileStore,│
│ machine,    │ │ highlighter,│ │ DiskWriter,    │
│ motions,    │ │ frontmatter,│ │ FileWatcher    │
│ operators,  │ │ 12 themes,  │ │                │
│ text objects│ │ Markdown    │ │                │
│ input types │ │ types       │ │                │
└─────┬───────┘ └─────────────┘ └────────┬───────┘
      │                                  │
┌─────▼───────┐                 ┌────────▼───────┐
│ bloom-buffer │                 │  bloom-error   │
│              │                 │                │
│ Rope,        │                 │ BloomError     │
│ cursors,     │                 │ (shared across │
│ undo tree,   │                 │  all crates)   │
│ block IDs    │                 └────────────────┘
└──────────────┘
```

### Crate Responsibilities

| Crate | Owns | Key invariant |
|-------|------|---------------|
| **bloom-error** | `BloomError` enum | Single error type shared across all crates |
| **bloom-buffer** | Rope + cursors + undo tree + block ID generation | **Buffer owns cursors.** All mutations (insert/delete/replace) auto-adjust every tracked cursor. No manual cursor shifts. |
| **bloom-md** | Markdown parser, highlighter, frontmatter, themes, `PageId`/`BlockId`/`TagName`/`Timestamp` | Pure parsing — no state, no I/O. Leaf crate. |
| **bloom-vim** | Vim state machine, grammar, motions, operators, text objects, `KeyEvent`/`KeyCode` | Produces `EditOp` descriptors. Never mutates buffers — read-only access. |
| **bloom-store** | `LocalFileStore`, `DiskWriter` (atomic writes), `FileWatcher` | File I/O abstraction. No editor knowledge. |
| **bloom-core** | Editor orchestrator: key dispatch, save, session, pickers, notifications, window manager, index, BQL | Composes all other crates. Thin — delegates to specialized crates. |

### Why This Structure

The crate boundaries enforce three architectural invariants that prevent bugs:

1. **Buffer-owned cursors** (bloom-buffer): `buf.insert()` adjusts all cursors atomically. Since bloom-buffer is a separate crate, no external code can reach into the rope and mutate it without going through the cursor-adjusting API.

2. **Vim produces, editor applies** (bloom-vim): The Vim state machine produces `EditOp` descriptors but never mutates buffers. The editor applies them. This separation means Vim logic can't create buffer/cursor inconsistencies.

3. **Save is read-only** (bloom-core): The save path reads buffer content and writes to disk. It never mutates the buffer. Block ID assignment happens on edit-group close (leaving Insert mode), not during save.

### RenderFrame Abstraction

The core library produces a `RenderFrame` — a UI-agnostic snapshot of everything to draw. Frontends never query editor state directly; they consume frames. Layout is computed in `update_layout()` (state mutation); rendering in `render()` (read-only snapshot).

```rust,ignore
terminal.draw(|f| {
    let (w, h) = f.area();
    editor.update_layout(w, h);          // state: viewport dims, cursor scroll
    let frame = editor.render(w, h);     // read-only: produces the snapshot
    gui::draw(f, &frame, &theme);        // GUI reads rects, renders widgets
});
```

A `RenderFrame` contains:

| Field | Type | Description |
|-------|------|-------------|
| `panes` | `Vec<PaneFrame>` | Pane content, cursor, status bar, and **layout rect** |
| `maximized` | `bool` | Whether a pane is maximized (hides others) |
| `hidden_pane_count` | `usize` | Number of hidden panes when maximized |
| `picker` | `Option<PickerFrame>` | Active picker: query, results, selected index, preview |
| `inline_menu` | `Option<InlineMenuFrame>` | Cursor-anchored inline suggestions and completions |
| `which_key` | `Option<WhichKeyFrame>` | Popup: available key bindings in current prefix |
| `date_picker` | `Option<DatePickerFrame>` | Date picker overlay |
| `dialog` | `Option<DialogFrame>` | Confirmation dialog |
| `notifications` | `Vec<Notification>` | Active transient notifications |
| `scrolloff` | `usize` | Scroll margin forwarded to the frontend renderer |

Each `PaneFrame` includes a `PaneRectFrame` with `x, y, width, content_height, total_height` — concrete cell positions computed by the core's `WindowManager::compute_pane_rects()`. The GUI reads these directly instead of computing its own layout.

This means:
- **Tests assert on `RenderFrame`** — no browser needed.
- **GUI is thin** — it maps `RenderFrame` fields to Iced Canvas primitives.
- **Layout lives in one place** — the core computes pane dimensions; the GUI never splits areas.

---

## Rendering Model

### Render Loop

The GUI render loop runs on the UI thread at ~60fps (or on input):

```rust,ignore
loop {
    let (w, h) = window_size();                         // actual window dimensions
    editor.update_layout(w, h);                         // state: viewport + scroll
    let frame = editor.render(w, h);                    // read-only: snapshot
    gui::draw(canvas, &frame, &theme);                  // GUI: primitives onto Iced Canvas
    wait_for_input_or_tick();
}
```

**Key property:** The window dimensions flow into `update_layout()` which sets viewport dimensions, then into `render()` which uses them to compute pane rects. The same dimensions are used by the GUI to draw. No stored state to drift.

### Cell Painting Strategy

The Iced Canvas GUI uses **GPU-accelerated rendering** — it redraws the visible frame each tick using the `RenderFrame` snapshot. The Canvas API batches draw calls efficiently so full-frame repaints are cheap.

Each frame follows a three-layer painting strategy:

```text
Layer 1: Clear + Background    ← fills the canvas with the background colour (clean slate)
Layer 2: Pane content          ← editor lines, status bars, drawn into pane rects
Layer 3: Overlays              ← picker, inline menu, date picker, dialog, notifications
```

**Layer 1** ensures no stale content from previous frames bleeds through. The canvas is cleared and filled with the background colour.

**Layer 2** renders each pane into its core-computed rect. The pane content (editor lines, syntax-highlighted spans) overwrites the background layer. Each pane includes its own status bar at the bottom of its rect.

**Layer 3** renders overlays on top of panes. Overlays draw last, so their cursor-positioning calls override the pane cursor — each overlay owns its cursor (picker query input, inline selection, date picker choice, dialog choice).

### Viewport and Scrolling

The viewport tracks which lines of the buffer are visible. On each render cycle:

1. `update_layout(w, h)` computes pane rects from the real terminal/window size.
2. Each pane viewport is refreshed from its `content_height` and `width`.
3. The active pane's `viewport.ensure_visible(cursor_line)` updates buffer-line scroll state.
4. `render()` consumes that state and `render_buffer_lines()` produces `RenderedLine`s for the visible range.
5. Frontends may add display-specific refinement (for example `ScreenScroll` for wrapped screen rows) without mutating the core viewport.

The viewport height is never guessed or stored separately — it's refreshed from layout computation using the actual window dimensions before `render()` consumes it.

### Semantic Highlighting Pipeline

All content — editor text, picker preview, and other highlighted text surfaces — uses the same highlighting path:

```text
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

```text
┌──────────────────────────────────────────────────────┐
│                    UI Thread                          │
│                                                      │
│  ┌─ Event Loop ─────────────────────────────────┐    │
│  │  1. Poll for input (Iced/winit)               │    │
│  │  2. Poll indexer completion channel           │    │
│  │  3. Poll file watcher, debounce, forward      │    │
│  │  4. Poll MCP edit channel (if enabled)        │    │
│  │  5. Dispatch key → BloomEditor::handle_key() │    │
│  │  6. Process Vim grammar, apply edits to rope  │    │
│  │  7. Call editor.update_layout(w, h)           │    │
│  │  8. Call editor.render(w, h) → RenderFrame    │    │
│  │  9. GUI draws RenderFrame onto Iced Canvas    │    │
│  │ 10. Iced presents the frame to the window     │    │
│  └──────────────────────────────────────────────┘    │
│                                                      │
│  Rule: NEVER blocks. All I/O dispatched via channels.│
│  Rope edits are O(log n) ≈ microseconds.             │
│  Render produces a snapshot — no locks held.         │
│  Index queries are read-only — no write contention.  │
└──────┬──────────┬──────────────┬──────────┬──────────┘
  channel     channel        channel    channel
       │          │              │          │
       ▼          ▼              ▼          ▼
┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────────┐
│Disk Writer│ │ Indexer  │ │  File    │ │ MCP Server   │
│(OS thread)│ │(OS thread)│ │ Watcher │ │ (OS thread)  │
│           │ │          │ │(OS thread)│ │              │
│ Debounced │ │ Scan →   │ │          │ │ Listens on   │
│ atomic    │ │ Read →   │ │ Watches  │ │ localhost.   │
│ write→    │ │ Write    │ │ vault,   │ │ Translates   │
│ fsync→    │ │ pipeline.│ │ sends    │ │ MCP tool     │
│ rename    │ │ See      │ │ events   │ │ calls into   │
│           │ │ below.   │ │ to UI.   │ │ edit requests│
│           │ │          │ │          │ │ via channel  │
│           │ │          │ │          │ │ to UI thread.│
│           │ │          │ │          │ │ Opt-in only. │
└──────────┘ └──────────┘ └──────────┘ └──────────────┘
```

### Thread Responsibilities

| Thread | Input | Output | Blocking? |
|--------|-------|--------|-----------|
| **UI** | Terminal key events, file watcher events, indexer completion, MCP edits | Rope edits, RenderFrame, index requests | Never — all I/O via channels |
| **Disk Writer** | `WriteRequest` via channel | Atomic file writes (write→fsync→rename) | Blocks on disk I/O (own thread) |
| **Indexer** | `IndexRequest` via channel (startup scan, file change batches, full rebuild) | Updated SQLite FTS5 index, `IndexComplete` notification | Blocks on file I/O + SQLite (own thread) |
| **File Watcher** | Filesystem notifications (notify crate) | `FileEvent` via channel to UI thread | Blocks waiting for OS events (own thread) |
| **MCP Server** | HTTP/stdio MCP tool calls on localhost | Edit requests via channel to UI thread, responses back to client | Blocks on network I/O (own thread, opt-in) |

### Indexer Architecture

The indexer is a **long-lived background thread** that keeps the SQLite index in sync with the vault. It processes two kinds of requests from the UI thread:

```text
UI Thread                              Indexer Thread
    │                                       │
    │  FullRebuild ────────────────────────▶ │  Invalidate all fingerprints,
    │  (user: :rebuild-index)               │  scan + read + parse + write
    │                                       │
    │  IncrementalBatch(paths) ────────────▶ │  Read + parse + write just
    │  (file watcher events,                │  the listed paths
    │   debounced 300ms)                    │
    │                                       │
    │  ◀──────────────── IndexComplete ──── │  Stats + timing
    │                                       │
    │  Shutdown ───────────────────────────▶ │  break
    └───────────────────────────────────────┘
```

**Long-lived loop:** The indexer thread starts on `init_vault()` and runs until the editor exits. On startup it performs the initial incremental scan (same as before). Then it blocks on the request channel, waking only when the UI forwards file changes or a full rebuild.

```text
Indexer Thread
    │
    ├── Startup: run_incremental() — scan all files, compare fingerprints
    │   └── Send IndexComplete to UI
    │
    └── Loop:
        ├── recv(IndexRequest::IncrementalBatch(paths))
        │   For each path:
        │     Read file, parse, extract IndexEntry
        │     remove_page_data + insert_page_data in one transaction
        │   Update fingerprints for changed files
        │   └── Send IndexComplete to UI
        │
        ├── recv(IndexRequest::FullRebuild)
        │   Invalidate all fingerprints → run_incremental()
        │   Prune orphaned page_access rows
        │   └── Send IndexComplete to UI
        │
        └── recv(IndexRequest::Shutdown) → break
```

**Single SQLite connection:** The indexer thread owns one read-write connection for its entire lifetime. No connection churn, no WAL checkpoint storms. The UI thread holds a separate read-only connection for queries (WAL mode allows concurrent readers).

### File Watcher → Indexer Pipeline

The file watcher detects changes on disk. The UI thread debounces them and forwards batches to the indexer:

```text
File Watcher (OS thread)
    │
    │  FileEvent::Modified("pages/rust-notes.md")
    │  FileEvent::Modified("pages/rust-notes.md")  ← duplicate from rename
    │  FileEvent::Created("journal/2026-03-06.md")
    │
    ▼
UI Thread: poll_file_events()
    │
    │  Drains watcher_rx (non-blocking)
    │  Filters: only .md files in pages/ or journal/
    │  Deduplicates by path
    │  Debounces: waits 300ms of quiet before sending
    │
    ▼
Indexer Thread (via channel)
    │
    │  IncrementalBatch(["pages/rust-notes.md", "journal/2026-03-06.md"])
    │
    ▼
SQLite index updated
```

**Debouncing:** File saves (especially atomic write→fsync→rename) generate 2-3 events per file. The UI thread collects events into a pending set and starts a 300ms timer. If more events arrive within 300ms, the timer resets. When 300ms of quiet passes, the batch is sent to the indexer. This matches the autosave debounce window.

**Self-triggering on save:** When `save_current()` writes a file to disk, the file watcher picks it up → UI debounces → indexer re-indexes that page. No special "update index after save" code in the save path. The file watcher is the single source of truth for all disk changes.

**External changes:** `git checkout`, manual edits, Syncthing sync — all produce file events that flow through the same pipeline. The editor gets a consistent, always-up-to-date index without polling.

**`:rebuild-index`:** Sends `IndexRequest::FullRebuild` to the indexer thread. The UI shows `⟳` and returns immediately. The indexer invalidates all fingerprints (forcing a full re-scan), prunes orphaned `page_access` rows, and sends `IndexComplete` when done. Same UX as startup indexing, but user-triggered.

### Fingerprint Cache

The index stores `(path, mtime_secs, size_bytes)` for each indexed file. On startup, the indexer `stat()`s each file (fast — no content read) and compares against stored fingerprints. Only files with changed mtime or size are re-read. For the common "nothing changed" case, startup goes from reading 1050 files to 1050 `stat()` calls — typically <10ms.

### Parallel Reads

Changed files are read and parsed concurrently using `rayon::par_iter()`. This mitigates NTFS per-file overhead on Windows where sequential small-file reads are slow. On a 4-core machine, 4 files are read simultaneously.

### Batched Writes

All SQLite mutations happen in a single `BEGIN ... COMMIT` transaction. This turns N fsyncs into 1, which is the largest SQLite bulk-insert optimization (~100x for 1000+ rows).

### Graceful Degradation

While the indexer runs, the editor is fully usable. Features that depend on the index (backlinks, unlinked mentions, agenda, tag queries) return empty results gracefully. Features that read files directly (find page, search, buffer editing) work immediately. The status bar shows `⟳` while indexing and a brief "Index ready" notification on completion.

### Communication Pattern

All inter-thread communication uses `crossbeam` channels (bounded, lock-free):

- **Input → UI**: Iced/winit delivers keyboard and mouse events on the UI thread via its subscription model
- **UI → Disk Writer**: `Sender<WriteRequest>` — debounced auto-save and explicit `:w` both route here
- **Disk Writer → UI**: `Sender<WriteComplete>` — ack with mtime/size fingerprint after each successful write
- **UI → Indexer**: `Sender<IndexRequest>` — `FullRebuild`, `IncrementalBatch(paths)`, `Shutdown`
- **Indexer → UI**: `Sender<IndexComplete>` — completion notification with timing
- **File Watcher → UI**: `Receiver<FileEvent>` — debounced, forwarded to indexer
- **MCP Server → UI**: `Sender<McpEditRequest>` — edit requests applied to the shared rope buffer; results sent back via a one-shot channel
- **No shared mutable state** — threads communicate exclusively via channels
- **SQLite access** — the indexer thread owns the read-write connection; the UI thread holds a separate read-only connection (SQLite WAL mode supports concurrent readers)

### Event-Driven Rendering

The GUI event loop uses `crossbeam::select!` to block until **any** channel fires or a timer expires — no polling, no frame-budget spinning. The loop renders only when state changes, sleeps with zero CPU cost between events, and wakes with sub-millisecond latency on any channel input. Timeouts are computed dynamically from active timers (notification expiry, which-key popup delay, file-event debounce).

The core library has **zero dependency on any async runtime**. All concurrency is OS threads + channels. This keeps the dependency tree small, debugging straightforward, and latency predictable.

---

## Data Safety

- **Atomic writes**: The disk writer uses a write→fsync→rename pattern. Content is written to a temporary file, fsynced to disk, then atomically renamed over the target. A crash at any point leaves either the old or new file intact — never a half-written file.
- **Auto-save**: Debounced at 300ms after last keystroke. Dirty-buffer indicator shown in the status bar (dot or [+] next to filename).
- **Self-write detection**: When the file watcher reports a change to an open buffer, the editor first checks a recorded write fingerprint (mtime + size from `DiskWriter`'s ack channel) — a single `stat()` syscall with no file I/O. If the fingerprint matches, the event is from our own save and is skipped. If no fingerprint matches, the editor falls back to reading the file and comparing content. This two-tier approach (stat-first, read-only-if-needed) matches the pattern used by VS Code and Neovim.
- **External file changes vs dirty buffer**: If the disk content *differs* from the buffer and the buffer has unsaved edits, Bloom shows a prompt: "File changed on disk. Reload (losing edits) or keep buffer version?" If the buffer is clean (no unsaved edits), Bloom silently reloads the new content. This handles `git checkout`, Syncthing sync, and manual external edits.

---

## Keybinding Architecture

1. **Platform shortcuts** (Cmd+S / Ctrl+S) — checked first, always work.
2. **Vim grammar state machine** — checked second, handles modal editing.
3. **Insert mode passthrough** — if in insert mode, character goes to buffer.
4. **Which-key popup** — appears after timeout during pending key sequences.
5. **User customization** — keymap config file (`~/bloom/keymap.toml`) for overrides.

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
