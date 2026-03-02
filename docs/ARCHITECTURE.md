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

The core library produces a `RenderFrame` — a UI-agnostic snapshot of everything to draw. Frontends never query editor state directly; they consume frames.

```
EditorState::render() → RenderFrame
    │
    ├─→ bloom-tui    (ratatui reads RenderFrame → terminal cells)
    ├─→ bloom-gui    (Tauri reads RenderFrame → HTML/CSS)
    └─→ tests        (assert directly on RenderFrame fields)
```

A `RenderFrame` contains:

| Field | Type | Description |
|-------|------|-------------|
| `visible_lines` | `Vec<RenderedLine>` | Lines in the viewport with syntax spans |
| `cursor` | `CursorState` | Position, shape (block/bar/underline), blink |
| `status_bar` | `StatusBar` | Mode, filename, dirty flag, cursor pos, pending keys |
| `picker` | `Option<PickerFrame>` | Active picker: query, results, selected index, filter pills |
| `which_key` | `Option<WhichKeyFrame>` | Popup: available key bindings in current prefix |
| `diagnostics` | `Vec<Diagnostic>` | Orphaned links, broken refs (inline indicators) |
| `splits` | `Vec<PaneFrame>` | Window layout tree (for multi-pane) |

This means:
- **Tests assert on `RenderFrame`** — no terminal or browser needed.
- **TUI and GUI are thin** — they map `RenderFrame` fields to their native primitives.
- **Design issues surface early** — both frontends consume the same contract.

---

## Threading Model

```
┌────────────────────────────────────────────────────┐
│                   UI Thread                         │
│  Renders UI, handles input, edits rope buffer       │
│  Rule: NEVER blocks. All I/O is async via channels. │
└───────────┬──────────────┬──────────────┬──────────┘
      channel         channel        channel
            │              │              │
            ▼              ▼              ▼
   ┌────────────┐  ┌────────────┐  ┌────────────┐
   │ Disk Writer│  │  Indexer   │  │File Watcher│
   │ (OS thread)│  │ (OS thread)│  │ (OS thread)│
   │            │  │            │  │            │
   │ Single     │  │ Rebuilds   │  │ Watches    │
   │ writer,    │  │ search     │  │ notes dir, │
   │ debounced  │  │ index,     │  │ detects    │
   │ auto-save, │  │ backlinks, │  │ external   │
   │ no write   │  │ unlinked   │  │ changes    │
   │ conflicts  │  │ mentions   │  │            │
   └────────────┘  └────────────┘  └────────────┘
```

- All threads are OS threads.
- Inter-thread communication via `crossbeam` channels.
- The core library has zero dependency on any async runtime.
- Rope edits are O(log n) ≈ microseconds → safe on the UI thread.

---

## Theming and Syntax Highlighting

Bloom uses **semantic highlighting** inspired by Nicolas Rougier's nano-emacs/elegant-emacs philosophy: mostly monochrome, typography-driven, with sparing use of color for semantic meaning.

### Pipeline

```
Buffer text → highlight.rs (per-line scan) → StyledSpan[] → RenderedLine
                                                              ↓
                                              Theme::props_for(style) → StyleProps
                                                              ↓
                                              TUI: ratatui::Style  |  GUI: CSS classes
```

### Design Principles

1. **Typography over color.** Headings are bold, not neon blue. Emphasis comes from weight (bold/italic/dim), not hue.
2. **De-emphasize metadata.** Frontmatter, block IDs, and timestamps are dimmed — present but not screaming.
3. **Links are subtle.** Underline + soft teal, not bright blue. You see them when you look for them.
4. **Completed tasks fade.** Checked items get dim + strikethrough — done means out of mind.
5. **Color = semantic signal.** Broken links are red. Tags are peach italic. Embeds are mauve. Each color means one thing.
6. **Code blocks recede.** Monochrome dim — content, not code decoration.

### Style Variants

| Style | Visual Treatment |
|-------|-----------------|
| H1 | Bold, lavender |
| H2 | Bold, blue |
| H3 | Bold, subtle gray |
| H4–H6 | Bold only |
| `code` | Green (inline) |
| Code block | Dim gray |
| `[[link]]` | Teal, underline |
| `![[embed]]` | Mauve, underline |
| `#tag` | Peach, italic |
| `@timestamp` | Yellow, dim |
| `^block-id` | Gray, dim |
| `- [ ]` | Blue checkbox |
| `- [x]` | Gray, dim, strikethrough |
| Frontmatter | Very dim gray |
| Broken link | Red, strikethrough |

### Theme Struct

Themes are data — a `Theme` struct maps each `Style` variant to `StyleProps` (fg, bg, bold, italic, underline, dim, strikethrough). The default "Bloom" theme uses a Catppuccin Mocha base palette. Themes are swappable and will be user-configurable via `config.toml`.

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
