# Bloom 🌱

A local-first, Vim-modal note-taking editor built in Rust. Bloom keeps your notes as plain Markdown files on disk, links them with stable UUIDs, and indexes everything with SQLite — so your knowledge graph is fast, portable, and entirely yours.

## Features

- **Vim-modal editing** — full `[count][operator][motion]` grammar with Normal, Insert, Visual, and Command modes
- **Bloom Markdown** — standard Markdown extended with `[[wiki-links]]`, `#tags`, `@due(dates)`, and `^block-ids`
- **Fuzzy everything** — find pages, search full-text, browse tags and backlinks through a unified picker
- **Daily journal** — one file per day with quick-capture from anywhere
- **Undo tree** — branching, not linear — never lose an edit path
- **Which-key discoverability** — press `Space` and see what's possible
- **Window splits** — Doom Emacs-style binary splits with spatial navigation
- **Local-only** — zero network calls by default; optional MCP server for LLM integration

## Install

### Prerequisites

- [Rust](https://rustup.rs/) 1.75+

### Build from source

```sh
git clone https://github.com/hindol/Bloom.git
cd Bloom
cargo build --release
```

The TUI binary is at `target/release/bloom-tui`.

### Run

```sh
# Launch the TUI editor
cargo run -p bloom-tui

# Or run the release build directly
./target/release/bloom-tui
```

On first launch Bloom creates a vault at `~/.bloom/` with `pages/`, `journal/`, and `.bloom/` directories.

## Quick Start

| Key | Action |
|-----|--------|
| `i` | Enter Insert mode |
| `Esc` | Back to Normal mode |
| `Space f f` | Find page |
| `Space j j` | Open today's journal |
| `Space j a` | Quick-capture a note |
| `Space s s` | Full-text search |
| `Space w v` | Vertical split |
| `:w` | Save |
| `:q` | Quit |

See [docs/KEYBINDINGS.md](docs/KEYBINDINGS.md) for the full reference.

## Project Structure

```
crates/
├── bloom-core/          # Core library — all logic, no UI deps
├── bloom-tui/           # TUI frontend (ratatui + crossterm)
├── bloom-gui/           # GUI frontend (Tauri) — planned
├── bloom-mcp/           # MCP server — planned
├── bloom-import/        # Logseq importer — planned
└── bloom-test-harness/  # Test utilities
```

## Contributing

Contributions are welcome! To get started:

1. Fork the repo and create a branch from `feature/v2`
2. Make your changes — keep commits small and focused
3. Run `cargo test --workspace` and ensure everything passes
4. Open a pull request with a clear description of what you changed and why

Please follow the existing code style: Rust 2021 edition, `thiserror` for errors, `tracing` for instrumentation, and trait-based abstractions where the docs call for them. The design documents in `docs/` are the source of truth for architecture decisions — read them before proposing structural changes.

## Known Bugs

| Bug | Root Cause | Impact |
|-----|-----------|--------|
| `x` in Normal mode deletes 2 chars instead of 1 | `ordered_range()` adds `+1` for inclusive Vim semantics, but `motion_l` already returns an exclusive position. Double-counting. Affects all operator+motion combos — needs careful audit of every motion before fixing. | All `d`+motion commands delete one char too many |
| Picker composable filters not wired | `PickerFilter` types defined (Tag, DateRange, LinksTo, TaskStatus) but Ctrl+T/D/L/S not handled in `handle_picker_key()`. No filter pill UI. | Users can't narrow results by tag/date in pickers |
| No "file changed on disk" prompt | File watcher detects changes and triggers re-indexing, but the editor doesn't prompt when an open buffer's file is modified externally. The buffer keeps stale content. | External edits (git checkout, manual saves) silently diverge from the in-memory buffer |

## License

Bloom is licensed under the [GNU Affero General Public License v3.0](LICENSE).
