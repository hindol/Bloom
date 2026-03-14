# Changelog

All notable changes to Bloom.

## 0.2.0-alpha — 2026-03-14

### Features
- **Journal redesign**: SPC j t (today), SPC j j (picker), SPC j c (calendar), [d/]d day-hopping
- **Journal scrubber**: 3-line panel with stats, first task, separator lines, 3s auto-hide
- **Live views**: BQL query execution with SPC v v (query prompt), SPC v l (list), SPC a a (agenda)
- **Agenda section headers**: Overdue / Today / Upcoming based on due dates
- **Unified buffer architecture**: BufferWriter struct, BufferMessage enum, all mutations via apply()
- **Block mirroring**: toggle propagates to all pages containing same block ID
- **Read-only buffers**: ReadOnly<Buffer> wrapper with freeze()/thaw(), full Vim navigation
- **Per-pane cursors**: split panes navigate independently on the same buffer
- **Theme live preview**: theme picker updates colors in real-time
- **Autosave notification**: "✓ Saved filename.md" on each save
- **Bracket motions**: [d/]d for journal, [l/]l for broken links (framework)
- **BQL default limit**: 100 rows, configurable via max_results
- **Doom Emacs keybindings**: SPC b k (buffer close), :q (close pane), :qa (quit app)

### Bug Fixes
- Command mode status bar no longer hidden by which-key space reservation
- Mode badge background limited to badge only (not full status bar)
- Auto-alignment includes non-task list items in width calculation
- Block IDs excluded from alignment width calculation
- Vim 'o' places cursor correctly at end of file with trailing newline
- Which-key space reservation only for leader sequences, not Vim pending
- Autosave deferred during Insert mode
- Navigation works on frozen (read-only) view buffers

### Improvements
- Auto-alignment requires 2+ timestamps for padding (single = no whitespace)
- Block IDs vertically aligned in list blocks
- Alignment engine rewritten (876 → 677 lines, generic align_segments)
- Index notification only on first startup, not every incremental update
- Removed ink theme (too similar to bloom-dark), 11 themes remain
- Status bar mode styling centralized in theme resolver
- Zero compiler warnings across workspace

### Architecture
- BufferSlot enum: Mutable(Buffer) / Frozen(ReadOnly<Buffer>)
- BufferWriter centralizes mutations behind apply(BufferMessage)
- BufferMessage: Edit, MirrorEdit, ToggleTask, Open, Close, Reload, Align, etc.
- Block-level event bus (HashMap) for view notifications
- MirrorEdit variant prevents circular BlockChanged events
- Per-pane cursor_idx on PaneState, Buffer::ensure_cursors()
- Composite PK on block_ids table for mirror support

### Docs
- UNIFIED_BUFFER.md: Elm-inspired architecture, stress tests, industry survey
- JOURNAL_REDESIGN.md: updated to match implementation with HTML mocks
- Mirroring stress test: 5/5 problems solved by unified buffer

## 0.1.0-alpha — 2026-03-08

### Bug Fixes
- always restore window layout on startup regardless of mode
- only allocate side preview panel when preview content exists
- picker column layout — fixed right column, capped middle, wide mode
- dd on last empty line now deletes the trailing newline
- use INSERT OR IGNORE for tags to handle duplicate inline tags
- wire Esc to dismiss persistent error notifications
- suppress false reload prompt after autosave
- correct Vim operator-motion range semantics
- add scrolling with scrolloff=3 (Vim-style margin)
- filter out done tasks, fix column alignment
- agenda footer shows 0/0 when no tasks found
- wire SPC a a to open agenda view in split pane
- show selection position in picker footer
- use salient colour for H1 instead of background wash
- parse block IDs in heading lines
- scroll picker results to follow selection
- add background wash to H1 headings for visual hierarchy
- frontmatter padding 1 space after longest key, not 2
- capture mode before vim processes key
- remaining Unicode issues — text objects, tag parsing, alignment
- comprehensive Unicode correctness pass
- only trigger on Insert→Normal, not Command→Normal
- earliest timestamp position, frontmatter idempotency
- include non-@ lines in alignment column calculation
- clamp cursor position to prevent buffer index panic
- sync drawer reservation with timeout, persist visibility
- resolve doc drift — contrast violations and missing styles
- render ~ in content region, right of line number gutter
- drawer below status bar via layout split
- respect timeout before showing popup
- load config.toml from vault on startup
- track frontmatter and code block context across lines
- separate list marker from checkbox in style docs
- use unicode display width for picker column alignment

### CI
- add workflow_dispatch for manual docs trigger
- add GitHub Pages docs workflow, README badge
- add CI, release workflows and install scripts

### Chore
- fix all warnings, apply rustfmt, clippy auto-fixes
- add pre-commit and pre-push hooks from Graphite

### Documentation
- add performance benchmarks to README
- add WORD_WRAP.md — frontend-owned wrapping with MeasureWidth trait
- replace hand-maintained API docs with cargo doc comments
- add notification UX design to DEBUGGABILITY.md
- add DEBUGGABILITY.md — logging, rotation, instrumentation spec
- remove fixed x-deletes-two-chars from Known Bugs
- unify inline menu design for links, tags, commands
- add agenda view wireframe to WINDOW_LAYOUTS.md
- add G24 — Full Unicode Support
- add configuration section, update for page-level scan
- remove viewport cap, alignment is presentation-agnostic
- add long line scenarios with wireframes
- add AUTO_ALIGNMENT.md design doc
- fix drift in UC-41 and UC-42
- sync API_SURFACES.md with actual code
- remove Built-In Themes section
- add contrast ratio targets and measurements
- revise syntax semantic weights for all constructs
- differentiate frontmatter field weights
- document unified layout model and column structure
- remove duplicate keybindings from theme selector

### Features
- auto-generated terminal screenshots via TestBackend
- add 4 nature-inspired dimmed themes
- inline completion for [[ (link) and # (tag) triggers in Insert mode
- wire all 11 missing window management keybindings
- implement visual word wrapping in TUI
- structured logging, notification stack, error surfacing
- wire remaining picker surfaces and Logseq import edge cases
- add inline menu component with command-line completion
- add scrolloff config (default 3), apply to editor and agenda
- populate preview pane with source file context
- implement rebuild_index — scan vault files into SQLite index
- full-screen takeover with columnar layout and preview
- implement auto-alignment on Insert→Normal transition
- add nature-inspired themes — driftwood, twilight, lichen, ember
- add moss, slate, solarium, ink themes
- implement Logseq import with idempotent re-run
- implement MCP server with all tools
- implement session save and restore
- wire auto-save via DiskWriter on buffer edits
- wire which-key actions for tags, timestamps, journal nav, links
- implement UC-12, UC-26, UC-29, UC-30, UC-31
- implement task toggle at cursor
- bottom panel, vim grammar hints, 1s timeout
- show : prefix and which-key hints in command mode
- add paper-inspired themes — parchment, newsprint, aged-paper
- implement semantic weight system for all constructs
- add Ctrl+N/P/J/K/G/U to picker navigation

### Performance
- wrap fingerprint batch writes in a transaction
- measure FTS vs structured write time in rebuild
- use bulk rebuild path for first-run indexing
- skip redraws when file events produce no visual change
- event-driven rendering with crossbeam select

### Refactor
- per-pane state for cursor, active_page, viewport
- centralize autosave trigger, move undo/redo to execute_actions
- split god classes into focused modules
- remove legacy poll methods, use channels() in tests
- deduplicate poll methods into thin wrappers
- unified save architecture with DiskWriter ack channel
- extract EX_COMMANDS constant, remove duplication
- use LocalFileStore in rebuild_index instead of raw fs
- unify all pickers into single layout and input path

### Reverts
- restore session only in Restore mode, update vault config

### Config
- revert which-key timeout to 500ms

### Style
- add padding for readability
- show ~ in gutter instead of line numbers beyond EOF
- remove pane title bar, reclaim line for content
- render drawer below status bar
- add 2-char right padding to picker rows
