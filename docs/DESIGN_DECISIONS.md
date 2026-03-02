# Bloom 🌱 — Design Decisions

> All major design decisions with rationale and alternatives considered.
> See [GOALS.md](GOALS.md) for goals and non-goals.

| Decision | Choice | Alternatives Considered |
|----------|--------|------------------------|
| Language | Rust | Go (+Wails), C++ (+Qt) |
| UI | Tauri (GUI) + ratatui (TUI) built in parallel over shared `RenderFrame` | Pure Rust GUI (egui/iced), Electron, TUI-only |
| File format | Markdown + Bloom extensions | Org-mode, AsciiDoc |
| Link identity | UUID (8-char hex), regenerate on collision | Filename-as-slug, name-based + aliases |
| Rename strategy | UUIDs permanent; display hints eagerly updated in background | Lazy update on open, alias registry, rename journal |
| Source of truth | Frontmatter title; filename derived + sanitized | Filename as source, two-way sync |
| Text buffer | Rope (`ropey`) | Piece table, gap buffer, CRDT (automerge, diamond-types) |
| Undo model | Undo tree (branching), RAM-only | Linear undo/redo, persisted undo |
| Link discovery | Explicit `[[links]]` + unlinked mentions (FTS5-backed) | Fully implicit linking, naive O(N×M) scan |
| Link creation UX | `[[` / `![[` triggers inline fuzzy picker (drills into blocks for embeds) | Manual UUID entry |
| Transclusion depth | One level only | Recursive, configurable depth |
| Tag syntax | `#tag` with Unicode letter start rule | Org-mode `:tag:`, `@tag` |
| Timestamps | `@due` / `@start` / `@at` | Org-mode `<date>`, `{date}` |
| Threading | OS threads + crossbeam channels | Tokio async tasks, green threads |
| Discoverability | Which-key style popups | Command palette (Cmd+Shift+P) |
| Search | Fuzzy picker + composable filters | Dedicated query DSL (datalog) |
| MCP concurrency | Virtual editor (shared rope buffer) with 60s buffer eviction | Direct disk writes, file locking |
| MCP API | Title-based with `resolve_journal` for date-based pages | UUID-based |
| MCP security | Read-only / read-write modes + path exclusion in config.toml | All-or-nothing enable/disable |
| Abstraction | Traits for format, storage, keybindings | Hard-coded implementations |
| Journal | `journal/` directory, one file per day, lazy file creation, quick-capture | Inbox page, single journal file |
| Vault structure | Single `~/.bloom/` directory, auto `.gitignore`, everything inside | Separate config/data directories |
| Data safety | Atomic writes (write→fsync→rename), 300ms debounced auto-save | Direct writes, no crash protection |
| Filename sanitization | Invalid chars → `-`, capped at 200 chars, case-collision detection | No sanitization, UUID-only filenames |
| Unicode | NFC normalization for filenames and lookups, diacritic-insensitive search | No normalization |
| Session | Full restore (buffers, layout, cursors), configurable startup mode | No session persistence |
| Logseq import | Non-destructive copy, full syntax mapping table, import report | In-place transformation |
| Logseq namespaces | Flatten to title + auto-tags | Subdirectories, drop hierarchy |
| Logseq properties | Preserve as arbitrary YAML frontmatter | Drop, convert to tags |
| Template placeholders | `${N:description}` snippet-style tab-stops | No placeholders, manual editing |
| Code-block safety | All Bloom extensions ignored inside code spans, fences, frontmatter | Tags only |
| Theming | Rougier-inspired semantic highlighting: monochrome base, typography-driven, sparing color | Rainbow syntax highlighting, no highlighting |
