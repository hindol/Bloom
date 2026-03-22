# Bloom 🌱 — Design Decisions

> All major design decisions with rationale and alternatives considered.
> See [GOALS.md](GOALS.md) for goals and non-goals.

| Decision | Choice | Alternatives Considered |
|----------|--------|------------------------|
| Language | Rust | Go (+Wails), C++ (+Qt) |
| UI | Iced (GUI) built over shared `RenderFrame` | Tauri, Electron, egui |
| File format | Markdown + Bloom extensions | Org-mode, AsciiDoc |
| Link identity | UUID (8-char hex), regenerate on collision | Filename-as-slug, name-based + aliases |
| Rename strategy | UUIDs permanent; display hints eagerly updated in background | Lazy update on open, alias registry, rename journal |
| Source of truth | Frontmatter title; filename derived + sanitized | Filename as source, two-way sync |
| Text buffer | Rope (`ropey`) | Piece table, gap buffer, CRDT (automerge, diamond-types) |
| Undo model | Undo tree (branching), persisted to SQLite | Linear undo/redo, RAM-only |
| Link discovery | Explicit `[[links]]` + unlinked mentions (FTS5-backed) | Fully implicit linking, naive O(N×M) scan |
| Link creation UX | `[[` triggers inline fuzzy picker | Manual UUID entry |
| Transclusion | Deferred to post-v1; links + timeline + backlinks provide sufficient navigability | Inline expansion, collapsed by default, hover preview |
| Tag syntax | `#tag` with Unicode letter start rule | Org-mode `:tag:`, `@tag` |
| Timestamps | `@due` / `@start` / `@at` | Org-mode `<date>`, `{date}` |
| Threading | OS threads + crossbeam channels | Tokio async tasks, green threads |
| Discoverability | Which-key style popups | Command palette (Cmd+Shift+P) |
| Search | Fuzzy picker + composable filters | Dedicated query DSL (datalog) |
| MCP concurrency | Virtual editor (shared rope buffer) with 60s buffer eviction | Direct disk writes, file locking |
| MCP API | Title-based with `resolve_journal` for date-based pages | UUID-based |
| MCP edit targeting | Search-and-replace (`old_text` → `new_text`) — content-addressed, no byte offsets | Position-based edits, version counter, CRDT |
| MCP security | Read-only / read-write modes + path exclusion in config.toml | All-or-nothing enable/disable |
| Abstraction | Traits for format, storage, keybindings | Hard-coded implementations |
| Journal | `journal/` directory, one file per day, lazy file creation, quick-capture | Inbox page, single journal file |
| Vault structure | Single `~/bloom/` directory, auto `.gitignore`, everything inside | Separate config/data directories |
| Data safety | Atomic writes (write→fsync→rename), 300ms debounced auto-save | Direct writes, no crash protection |
| Filename sanitization | Invalid chars → `-`, capped at 200 chars, case-collision detection | No sanitization, UUID-only filenames |
| Unicode | NFC normalization for filenames and lookups, diacritic-insensitive search | No normalization |
| Session | Full restore (buffers, layout, cursors), configurable startup mode | No session persistence |
| Logseq import | Non-destructive copy, full syntax mapping table, import report | In-place transformation |
| Logseq namespaces | Flatten to title + auto-tags | Subdirectories, drop hierarchy |
| Logseq properties | Preserve as arbitrary YAML frontmatter | Drop, convert to tags |
| Template placeholders | `${N:description}` snippet-style tab-stops with code-block-aware expansion | No placeholders, manual editing, double-brace syntax |
| Template mirroring | Search-and-replace on Tab advance (not real-time sync) | Real-time mirror, no mirroring |
| Code-block safety | All Bloom extensions ignored inside code spans, fences, frontmatter | Tags only |
| Theming | Rougier-inspired semantic highlighting: monochrome base, typography-driven, sparing color | Rainbow syntax highlighting, no highlighting |
| Workspace | Cargo workspace with separate crates per frontend | Single crate with feature flags |
| Core granularity | Monolithic `bloom-core` with internal modules | Fine-grained crates (`bloom-parser`, `bloom-vim`, etc.) |
| Logging | `tracing` crate with structured spans/events | `log` crate, `env_logger`, println debugging |
| Snapshot testing | `insta` crate for RenderFrame snapshot tests | Manual assert-based tests only |
| Import crate | Separate `bloom-import` crate | Module inside bloom-core |
| Test utilities | Dedicated `bloom-test-harness` crate (dev-dependency) | Ad-hoc test helpers per crate |
| Syntax markers | Three-tier semantic weight system: structural (visible), contextual (subdued), noise (dimmed) | Uniform dimming, no dimming, hidden syntax |
| List marker style | Full visibility (`foreground`) — `-` is structural | `faded` like other markers |
| Tag `#` style | Same style as tag text — `#` is part of tag identity | Dim `#`, show only tag name |
| Link UUID display | Suppressed (rendered as `SyntaxNoise` / hidden) — meaningless to reader | Show UUID, dim UUID |
| Font strategy | Monospace-only, GUI uses size variation for headings | Mixed-pitch (proportional body + monospace code), proportional everywhere |
| Window navigation | Nearest spatial neighbor (ray cast from cursor position) | Tree-based parent/sibling traversal |
| Splittable panes | Only editor panes can be split; special views (timeline, agenda, undo tree) are leaf-only | Any pane can be split |
| Clipboard model | Vim registers + system clipboard (`arboard`) + kill ring (32 entries, `SPC i y` picker) | Vim registers only, system clipboard only, Emacs `M-y` cycling |
| New page creation | `SPC n` = template picker with title prompt. Title auto-derived from content on save if blank. Lazy file creation. | Prompt-free scratch buffer, name-on-save only |
| Scratch buffer | No separate scratch — today's journal IS the scratch. Extract to a page with `SPC r s`. | Dedicated scratch buffer, multiple unnamed buffers |
| Empty state | Dashboard with recent pages, quick actions, today's stats, random tip | Blank screen, auto-open journal, splash screen |
| Journal ID stability | Cache today's journal PageId in memory. `SPC j t` reopens same ID. No churn on close/reopen. | New ID per open, persist across restart |
