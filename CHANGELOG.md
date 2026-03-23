# Changelog

All notable changes to Bloom.



## 0.4.0-alpha — 2026-03-23

### Bug Fixes
- replace Modifier::DIM with blended color for cross-platform dim
- picker border/faded styles use subtle bg, not editor bg
- fill picker overlay with surface bg before content
- StyleProps — fg non-optional, bg optional (layered correctly)
- auto-detect true-color on Windows Terminal
- exclude bloom-gui from Docs CI to avoid GTK deps

### CI
- exclude bloom-gui from coverage threshold
- native installers, fix bloom-tui refs, clean Tauri legacy

### Design
- self-documenting config.toml with migration
- centralized layout manager for GUI frame rects
- section mirroring via structural sync + leaf mirrors

### Documentation
- remove stale TUI/ratatui references — GUI (Iced Canvas) is the only frontend

### Features
- self-documenting config.toml with version-based migration
- system clipboard + kill ring with SPC i y picker
- implement dot repeat with insert-mode recording
- implement dashboard empty state when no buffers are open
- implement SPC f r (rename page) and SPC f D (delete page)
- implement Phase 3 features
- inline word diff for modifications in page history
- dual-column line numbers + word-level diff in page history
- demo vault generator with backdated git history
- GIF-first landing page + Docusaurus deployment
- complete animated docs pipeline — GIF assembly + Docusaurus scaffold
- animated docs pipeline — FrameRecorder + SVG renderer

### Fix
- block ID dimming on checked tasks + code fence as SyntaxNoise

### Refactor
- draw functions accept only Rectangle, never Size or window dims
- use similar's DiffOp::Replace for word-level diff pairing
- make StyleProps fg/bg non-optional

### Strikethrough
- trim both leading and trailing whitespace

### Content
- weave local-first philosophy into safety section
- rewrite landing page — safety, speed, mirroring, journal

### Gui
- remote session detection + fix Ctrl+N/P in inline menus
- fix theming mismatches, reduce scrim opacity, add cursor tick to scroll bar

### Polish
- landing page visual overhaul + structured footer


## 0.3.0-alpha — 2026-03-16

### Bug Fixes
- strip block ID suffix in dedup comparison for block history
- block history extracts single line from git blobs, falls back to line number
- word diff uses LCS — only actually changed words are red/green
- day-hopping and calendar preview reuse buffers via find_by_path
- expanded temporal strip (e key) — core reserves 3 lines, not 2
- word diff shows red (removed) before green (added)
- restore diff preview in history mode, add e2e tests
- non-markdown files render as plaintext, not markdown-highlighted
- HIST mode badge gets accent_yellow — matches JRNL/DAY family
- render() accounts for temporal strip height in pane layout
- test harness calls update_layout before render
- status bar visible in HIST mode, add SPC H b/d keybindings
- temporal strip — remove duplicate mode badge, show HIST in status bar
- temporal strip renders below status bar, not above
- block IDs and inline elements styled correctly in blockquotes
- tests accept JRNL mode alongside NORMAL
- buffer picker [+] only shown for actually dirty buffers
- JRNL mode is page-based, not action-based
- journal and day-hopping reuse buffers — no duplicates
- reuse existing buffer when opening same page — no duplicates
- self-write detection race — pending_writes closes the window
- theme persist corrupted view names — section-aware replacement
- consistent search highlight contrast — mild bg for all matches
- search picker — highlight selected item, boost exact matches
- cursor landing bugs — dd last line, J join, undo restore
- undo restores cursor position (Vim behavior)

### CI
- install Tauri system dependencies (glib, gtk, webkit)

### Documentation
- promote HISTORY.md from lab/ to docs/
- write pipeline — indexer reads, DiskWriter writes, UI decides
- [●] branch marker, auto-expand on cursor arrival
- branch rules from stress test
- branch expand on j/k at ⑂ — no extra key needed
- history keybindings — SPC H h/b/d (page/block/day)
- create TEMPORAL_NAVIGATION.md — unified timeline UX
- scrub stale content — auto-commit, keybindings, wireframes
- rename TIME_TRAVEL → HISTORY, unified history model, ASCII wireframes
- promote JOURNAL_REDESIGN → docs/JOURNAL.md — implemented
- promote BLOCK_IDENTITY from lab to docs/ — implemented
- promote UNIFIED_BUFFER from lab to docs/ — fully implemented
- animated documentation pipeline design
- cursor-only gutter coloring for mirrors
- gutter — colored line number, no extra column
- move gutter = to left of line numbers
- drop SPC m i — redundant with SPC m g picker
- fix gutter — additive indicator, keep line numbers
- Mirror UX — gutter, status bar, keybindings
- restore full explanations for scenarios 7-12
- retired ID recovery, stale row cleanup
- merge BLOCK_IDENTITY + MIRRORING into one doc
- rewrite — pure ^= design, no BQL/Agenda/View content
- Agenda keybindings from Doom Emacs / evil-org
- add prior art — Org Agenda, Babel, Notion, Roam
- decision — views stay read-only, no mixed concerns
- target region model, drop BQL group
- redo stress test — editable Agenda via block IDs
- stress test editable views via block mirroring
- refresh with implemented architecture
- reconcile doc with benchmark findings
- add benchmarks — parsing is already sub-ms
- fix memory model — 9KB not 100KB per buffer
- add PARSE_TREE.md — persistent incremental parse tree design

### Features
- skip unchanged commits in block history via eager loading + dim
- skip unchanged git commits in block history strip
- block history shows inline word-diff on cursor line only
- block-level history — SPC H b scrubs a single block's versions
- richer temporal strip — 4/6 lines, horizontal scrolling, wider nodes
- inline word diff in history preview, inverted diff direction
- temporal strip TUI renderer — horizontal timeline with diff preview
- temporal strip — unified page history (undo + git)
- full in-buffer search — / ? n N SPC * with live highlighting
- mirror UX — gutter, status hint, sever, go-to, notifications
- general text mirroring — edit any ^= line, peers update
- retired IDs, stale row cleanup, mirror promotion/demotion
- ^= mirror markers — parser, index, highlighter, e2e tests

### Refactor
- switch to similar crate for diffs — Myers algorithm
- block diff flows through normal render pipeline — wraps correctly
- centralize drawer_height() — one method, three call sites
- write pipeline — write IDs, content comparison, no fingerprints
- mirror menu → inline popup, SPC m g → SPC m m
- decouple picker view from selection logic via PickerAction
- centralize line parsing — LineElements, parse_line(), extract_link_at_col

### Release
- 0.3.0-alpha — search, temporal navigation, block history

### Test
- block history diff verified — no extra content in long files
- block history diff contains only block content, no unrelated lines
- mirror integrity e2e, unit test for ^= not double-assigned
- comprehensive undo e2e tests, document edit group ordering


## 0.2.0-alpha — 2026-03-14

### Bug Fixes
- defer autosave during Insert mode
- add MirrorEdit to prevent circular BlockChanged events
- 'o' places cursor on new line at end of file
- cursor movement works on frozen (read-only) buffers
- :q quits when last buffer, closes otherwise (like Vim)
- navigation works on frozen (read-only) buffers
- full Vim navigation in views, close via :q not q
- add section headers to Agenda based on due date category
- column-aligned results and section headers for group by
- register auto-hide deadline so tick triggers re-render
- which-key space reservation only for leader sequences
- use page ID from frontmatter instead of generating new ones
- JRNL mode yellow background now only covers the badge
- exclude block IDs from alignment width calculation
- include non-task list items in alignment width calculation
- limit mode background color to the mode badge only
- prevent which-key space reservation in Command mode
- fix live preview in theme picker
- editor unresponsive when all buffers closed
- clip long lines to terminal width in no-wrap mode
- config parse fails silently on lowercase enum variants
- log viewer shows local time instead of UTC
- theme persists to config.toml, not session; skip block IDs on non-.md
- persist active theme across restarts via session state
- clean cursor positioning on trailing empty line
- Insert mode on trailing empty line edits wrong line
- adjust cursor after block ID insertion to prevent panic
- add macOS .icns and PNG icons for Tauri GUI bundle
- move unicode-width, rayon, bloom-history to general dependencies
- add missing libc dep and improve vault lock error messages

### Chore
- clean all compiler warnings
- remove ink theme (too similar to bloom-dark)
- clean up dead code and warnings

### Documentation
- mirroring conflict uses existing reload-or-keep dialog
- performance analysis for extreme typing speeds
- mirroring stress test — unified buffer solves 4/5 problems
- one-level undo for view toggle (task may be filtered out)
- toggle is self-undoing, no undo stack needed
- final stress test of unified buffer architecture
- remove conflicting threading info from UNIFIED_BUFFER.md
- resolve event bus granularity — block-level only
- resolve open questions in UNIFIED_BUFFER.md
- cursor ownership is a settled decision, not an open question
- add comprehensive stress test results to UNIFIED_BUFFER.md
- add threading and event bus sections to UNIFIED_BUFFER.md
- add UNIFIED_BUFFER.md — Elm-inspired buffer architecture
- update JOURNAL_REDESIGN.md to match implementation
- add HTML mocks for journal scrubber placement options
- redesign temporal bar, journal, and day activity
- update ARCHITECTURE.md for new crate structure
- replace split-pane history UI with temporal bar design

### Features
- require 2+ timestamps for padding, align block IDs in lists
- per-pane cursors — split panes navigate independently
- block mirroring support via unified buffer writer
- add ToggleTask + event bus + x key in Agenda
- default 100-row limit, configurable via max_results
- Doom Emacs buffer management — SPC b k, :q closes pane
- full keybinding support on read-only buffers
- add ReadBuffer trait and read-only buffer support
- implement live views with BQL query execution
- Option A — separator lines with buffer background
- rich 3-line journal scrubber with stats and first task
- add preview pane and [d/]d skip navigation
- replace SPC j p/n with [d/]d for journal day-hopping
- show autosave notification, limit index notifications
- implement journal redesign from lab doc
- extract bloom-vim crate (Vim engine + input types)
- extract bloom-md crate (parser, theme, Markdown types)
- new bloom-buffer crate with cursor-owning Buffer
- ghost text completion in command line
- command-line prefix resolution on Enter
- :config command, human-readable log viewer, autoscroll to end
- block-only links and retired block IDs
- add page history UI (SPC H h)
- add time travel infrastructure (Layer 1)

### Fix
- block IDs not assigned on autosave after block split

### Performance
- add core/draw breakdown to slow frame logging

### Redesign
- queries live in pages, not separate views

### Refactor
- rewrite alignment engine — 876 → 677 lines
- remove public buffers_mut() — all mutations via apply()
- add BufferMessage enum and apply() method
- introduce BufferWriter struct wrapping BufferManager
- clean cursor API on ReadOnly — no as_buffer_mut exposed
- simplify API — get() works for both mutable and frozen
- replace StaticBuffer with ReadOnly<Buffer> wrapper
- replace read_only flag with BufferSlot enum
- migrate to read-only buffer surface
- centralize mode styling in theme resolver
- horizontal context strip with auto-hide
- preview by opening buffer, close on cancel
- extract bloom-error and bloom-store, separate layout from render, clean block_id_gen
- block IDs on edit-group close, which-key in tick not render
- cursor owned by Buffer, re-export shim removed
- bloom-core uses bloom-buffer, old buffer/ removed
- move block detection into parser, simplify block_id_gen

### Release
- 0.2.0-alpha


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
- add git-cliff config and generated CHANGELOG.md
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

