# Bloom 🌱

[![Docs](https://img.shields.io/badge/docs-latest-blue)](https://hindol.github.io/Bloom/bloom_core/)
[![License: AGPL-3.0](https://img.shields.io/badge/license-AGPL--3.0-green)](LICENSE)

> A local-first, Vim-modal note editor for networked thinking.

Bloom is a keyboard-driven note editor built in Rust. Your notes stay as plain Markdown on disk, linked with stable IDs, indexed locally with SQLite, and owned entirely by you. Think Obsidian's linking model with Neovim's editing feel and Doom Emacs' discoverability.

![Split panes showing Bloom Markdown with semantic highlighting, wiki-links, tags, and architecture diagrams](screenshots/split-panes-dark.png)

## Features

### Editing

- **Full Vim grammar** — `[count][operator][motion/text-object]` with Normal, Insert, Visual, and Command modes
- **Bloom-specific text objects** — `il`/`al` (links), `i#`/`a#` (tags), `i@`/`a@` (timestamps), `ih`/`ah` (headings)
- **Branching undo tree** — never lose an edit path
- **Word wrap** — visual wrapping with `↪` continuation indicators
- **Inline completion** — `[[` triggers the page-link picker, `#` triggers tag completion

### Navigation and Search

- **Fuzzy picker** — find pages, search full text, browse tags, backlinks, and unlinked mentions
- **Full-text search** — FTS5-backed search with line-level previews
- **Which-key discoverability** — press `Space` and see what is available
- **Link following** — `[[page|display]]` links resolve instantly via the local index
- **Timeline** — chronological context for the page you are in

### Knowledge Work

- **Bloom Markdown** — standard Markdown plus `[[wiki-links]]`, `#tags`, `@due(...)`, and block IDs
- **Daily journal** — one page per day, with quick capture from anywhere
- **Agenda view** — overdue, today, and upcoming tasks across the vault
- **Templates** — tab stops, placeholder mirroring, and magic variables like `${AUTO}` and `${DATE}`
- **Refactoring** — move blocks and pages without turning links into dead text

### Interface

- **Window splits** — binary splits with resize, swap, rotate, maximize, and spatial navigation
- **12 built-in themes** — dark and light palettes with strong semantic contrast
- **Adaptive layout** — picker density and preview width adapt to the available space
- **Session restore** — layout, open panes, cursors, and scroll offsets survive restart

### Architecture

- **Event-driven core** — no busy polling while idle
- **In-editor diagnostics** — notifications, `:messages`, `:log`, and `:stats`
- **Local-only by default** — no network calls; MCP integration is optional and explicit
- **Desktop GUI** — shipped through the `bloom-gui` frontend and installed as `bloom`

## Install

### macOS / Linux

```sh
curl -fsSL https://raw.githubusercontent.com/hindol/Bloom/main/install.sh | sh
```

### Windows (PowerShell)

```powershell
irm https://raw.githubusercontent.com/hindol/Bloom/main/install.ps1 | iex
```

The install scripts place the app on your `PATH` as `bloom`.

### Build from source

Requires [Rust](https://rustup.rs/) 1.75+:

```sh
git clone https://github.com/hindol/Bloom.git
cd Bloom
cargo run --release -p bloom-gui
```

On first launch, Bloom opens a setup wizard to create or import a vault.

## Keybindings

| Key | Action |
|-----|--------|
| `i` | Enter Insert mode |
| `Esc` | Back to Normal mode / dismiss notifications |
| `SPC f f` | Find page |
| `SPC j j` | Open today's journal |
| `SPC j a` | Quick-capture a note |
| `SPC s s` | Full-text search |
| `SPC s l` | Backlinks to current page |
| `SPC s t` | Browse tags |
| `SPC l l` | Insert link (inline picker) |
| `SPC w v` | Vertical split |
| `SPC w s` | Horizontal split |
| `SPC T t` | Theme selector (live preview) |
| `SPC n` | New page from template |
| `SPC a a` | Agenda |
| `:w` | Save |
| `:q` | Quit |
| `:messages` | Notification history |
| `:log` | Open the log buffer |

See [docs/KEYBINDINGS.md](docs/KEYBINDINGS.md) for the full reference.

## Performance

Benchmarked on a 10,365-page vault (24 MB of Markdown), Windows 11, NVMe SSD:

| Operation | Time | Notes |
|-----------|------|-------|
| First-run full index | **2.4s** | Scan 820ms + read/parse 150ms + SQLite write 1,470ms |
| Incremental startup (0 changed) | **0.7s** | 10K `stat()` calls against fingerprint cache |
| File save → watcher ack | **<1ms** | Fingerprint match, no file I/O |
| Render (idle) | **0 CPU** | Event-driven core, no polling |

## Themes

12 built-in themes, selectable with `SPC T t` and previewed live while you move through the picker.

**Dark:** Bloom Dark, Aurora, Ember, Twilight, Verdant, Ink

**Light:** Bloom Light, Frost, Solarium, Sakura, Lichen, Paper

All themes share a semantic palette model rather than per-screen color hacks. See [docs/THEMING.md](docs/THEMING.md).

## Configuration

Bloom stores configuration in `config.toml` at the vault root. The checked-in template is comment-rich, versioned, and migrated forward when new keys are introduced.

```toml
config_version = 1

[startup]
mode = "journal"

autosave_debounce_ms = 300
which_key_timeout_ms = 500
scrolloff = 3
word_wrap = true
wrap_indicator = "↪"
auto_align = "page"

[theme]
name = "bloom-dark"
```

See [docs/FILE_FORMAT.md](docs/FILE_FORMAT.md) for Bloom Markdown and config details.

## Project Structure

```
crates/
├── bloom-core/          # Core editor, parser, index, views, history, themes
├── bloom-gui/           # Iced desktop frontend
├── bloom-mcp/           # MCP server for local LLM workflows
├── bloom-import/        # Logseq vault importer
└── bloom-test-harness/  # SimInput / e2e infrastructure
```

## Core Documents

| Document | Description |
|----------|-------------|
| [ARCHITECTURE.md](docs/ARCHITECTURE.md) | How Bloom is divided into editor core, services, and frontends |
| [GOALS.md](docs/GOALS.md) | Product goals and deliberate non-goals |
| [JOURNAL.md](docs/JOURNAL.md) | Daily notes, quick capture, and journal navigation |
| [HISTORY.md](docs/HISTORY.md) | Undo, page history, block history, and time travel |
| [DEBUGGABILITY.md](docs/DEBUGGABILITY.md) | Diagnostics that exist today: notifications, logs, and stats |
| [FILE_FORMAT.md](docs/FILE_FORMAT.md) | Bloom Markdown extensions, templates, and config shape |
| [BLOCK_IDENTITY.md](docs/BLOCK_IDENTITY.md) | Stable block IDs and mirror semantics |
| [WINDOW_LAYOUTS.md](docs/WINDOW_LAYOUTS.md) | How panes split, move, and persist |
| [KEYBINDINGS.md](docs/KEYBINDINGS.md) | Full keybinding reference |
| [THEMING.md](docs/THEMING.md) | Theme system and palette philosophy |
| [PICKER_SURFACES.md](docs/PICKER_SURFACES.md) | Detailed picker reference |

Future and exploratory work lives under [`docs/planning/`](docs/planning/).

For contributor workflow and doc-first development policy, see [docs/AGENT_WORKFLOW.md](docs/AGENT_WORKFLOW.md).

API documentation: [hindol.github.io/Bloom/bloom_core](https://hindol.github.io/Bloom/bloom_core/)

## Contributing

1. Fork the repo and create a branch.
2. Make focused changes.
3. Run `cargo test --workspace && cargo clippy --workspace && cargo fmt --all -- --check`.
4. Open a pull request.

The docs in `docs/` describe the current editor. The docs in `docs/planning/` are where future work belongs.

## Known Bugs

| Bug | Root Cause | Impact |
|-----|-----------|--------|
| macOS: window focus doesn't return to launching app on quit | winit/Iced `NSApplication` lifecycle doesn't activate the previous app on window close | Use Cmd+Tab to return to terminal after closing Bloom |
| Picker composable filters not wired | `PickerFilter` types defined but Ctrl+T/D/L/S not handled | Users can't narrow results by tag/date in pickers |
| No horizontal scrolling | When `word_wrap = false`, long lines truncate at pane edge | Cursor can move past visible area |
| Inline link picker has no hint keys | `InlineMenuFrame.hint` is always `None`; there is no `^` drill-down to blocks yet | No discoverability for block deep-linking from the `[[` picker |
| Undo doesn't re-assign block IDs | Undo can remove a `^id` but autosave doesn't re-add it until the next edit | Undoing a block split can leave the merged block without an ID briefly |

## License

Bloom is licensed under the [GNU Affero General Public License v3.0](LICENSE).
