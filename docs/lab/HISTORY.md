# History 🕰️

> Unified history — undo tree for recent edits, git for permanent record.
> Status: **Draft** — undo tree implemented, git layer via bloom-history crate.
> See also: [JOURNAL.md](../JOURNAL.md) for journal navigation and calendar.

---

## The Problem

You wrote something three weeks ago. Maybe it was a journal entry, maybe you edited a page, maybe you created a new page and jotted a few ideas. You don't remember exactly when, or exactly where. You just remember *roughly* when you were thinking about it.

Today's tools give you two options: full-text search (requires remembering keywords) or manually browsing files sorted by modification date (tedious, no context). Neither reconstructs **what was on your mind that day** — the full picture of your thinking at a point in time.

---

## The Vision

Bloom maintains a complete, automatic history of every change to your vault. Not as a backup feature — as a **thinking tool.** Time becomes a first-class dimension you can navigate — per-file version history, per-block evolution, and vault-wide daily activity summaries.

**Fearless editing.** Every version of every thought is recoverable. The undo tree handles per-keystroke recovery within a session; git handles everything beyond that. The user sees one seamless timeline — they never need to know which system is serving the history.

---

## Unified History Model

The user's mental model: **"I can go back to any point in time."**

```
Now ─────────────────── 24h ago ──────────────────── weeks ago
│                         │                            │
│  Undo tree              │  Git commits               │
│  (full branching,       │  (linear, per-save,        │
│   per-edit-group,       │   one snapshot per          │
│   in-memory + SQLite)   │   auto-save cycle)          │
│                         │                            │
└── rich, interactive ───►└── degraded, read-only ────►│
```

| Layer | Granularity | Time range | Branching | Storage |
|-------|-------------|------------|-----------|---------|
| **Undo tree** | Per-edit-group (Insert session, `dd`, etc.) | Session + 24h (persisted to SQLite) | Full branching | In-memory, serialized to SQLite |
| **Git history** | Per-save (auto-commit on save) | Permanent | Linear (one branch) | `.index/.git/` via gix |

**The transition is seamless.** The undo tree's root node corresponds to the buffer state at the last git commit. When you scroll past the undo tree into older history, you're looking at git commits. No visual break — just `●` (undo node) becomes `○` (git commit).

**Restore behavior differs silently:**
- Restore from undo node → `buf.restore_state(node_id)`. Cursor restored. Branching preserved.
- Restore from git commit → load content from `history_repo.blob_at(oid, uuid)`, replace buffer. Cursor at line 0. Creates a new undo tree branch ("restored from Mar 12").

---

## Architecture

### Git as the Time-Series Store

Bloom silently maintains a git repository in the vault. The user never interacts with git directly — Bloom auto-commits in the background using `gix` (pure Rust git implementation, compiled into the binary, zero external dependencies).

**Why git:**
- Battle-tested delta compression — a year of daily changes to 10K files stays small
- Line-level diffs for free — exactly what temporal features need
- Portable — the vault is a valid git repo, browsable with any git tool
- No new storage format to invent, debug, or migrate
- Enables self-healing block IDs (see [BLOCK_IDENTITY.md](../BLOCK_IDENTITY.md))

**Why `gix` (not subprocess):**
- No requirement that the user has git installed
- In-process — no fork/exec overhead, typed Rust APIs
- Same cross-platform story as the rest of Bloom (especially Windows)
- Commit, revwalk, tree diff, and blame are all supported

### UUID-Based Git Tree

Files are stored in git under their **page UUID**, not their filesystem path. This eliminates rename tracking entirely:

```
.index/.git/ tree:
├── 8f3a1b2c.md    ← "Text Editor Theory"
├── deadbeef.md    ← "Rust Programming"
├── a1b2c3d4.md    ← "Meeting Notes"
└── ...
```

The UUID is permanent (G3). The file can be renamed a hundred times on disk — `git log -- 8f3a1b2c.md` always gives the complete history. No rename detection, no heuristics, no following.

The SQLite index provides the bidirectional mapping:

```sql
SELECT path FROM pages WHERE id = '8f3a1b2c';   -- UUID → disk path
SELECT id FROM pages WHERE path = 'pages/Rust Programming.md';  -- disk path → UUID
```

When `commit_all()` stages files, it reads each vault file, looks up its UUID in the index, and writes the content to the git tree under `{uuid}.md`. The git tree never uses filesystem paths. Human-readable titles go in the commit message.

### Auto-Commit Strategy

Bloom commits automatically. The user never thinks about it.

**When to commit:**

| Trigger | Rationale |
|---------|-----------|
| On quit | Capture the final state of the session |
| After 5 minutes of inactivity | Natural pause boundary — you've context-switched |
| On journal rotation (start of new day) | Close out the day cleanly before rotating |
| After 1 hour regardless of activity | Safety net for long uninterrupted sessions |

**Not** on every auto-save. The 300ms auto-save debounce writes to disk for crash safety, but committing every 300ms would create hundreds of commits per day. The 5-minute idle window batches edits into meaningful chunks — typically 3–10 commits per active day.

**Commit details:**

```
Author:  Bloom <bloom@local>
Message: "2026-03-08 14:32 — edited Text Editor Theory, journal"
```

- Machine-authored (filterable if the user also commits manually)
- Timestamp + summary of what changed (auto-generated from the staged diff)
- Staged by UUID: index lookup maps each changed vault file to its UUID

### Single-Instance Lock

Only one Bloom process may access a vault at a time (TUI or GUI, not both). On startup, Bloom creates `.index/bloom.lock` exclusively. The lock file contains the PID. On shutdown, deleted. If Bloom crashes, the stale lock's PID is checked — if the process isn't running, the lock is taken.

This prevents concurrent writes to both the SQLite index and the git repo.

### Persistent Undo Tree

The in-memory undo tree (G9) is serialized to SQLite on quit and restored on next launch:

```sql
CREATE TABLE undo_tree (
    page_id    TEXT NOT NULL,
    node_id    INTEGER NOT NULL,
    parent_id  INTEGER,           -- NULL for root
    content    BLOB NOT NULL,     -- rope snapshot or delta
    timestamp  TEXT NOT NULL,
    PRIMARY KEY (page_id, node_id)
);
```

On restart, the undo tree is deserialized. `u` and `Ctrl-R` work across sessions. The undo tree is pruned when the buffer is closed or after 24 hours — beyond that, git history provides recovery.

---

## File Time Travel

The history of a single page over time.

### Context Strip

Bloom uses a **context strip** — a 3-line panel above the status bar for navigating through ordered items (history versions, calendar days). The same component powers page history (`SPC H h`), day activity browsing (`SPC H c` → `[d`/`]d`), and journal day-hopping (`SPC j p`/`SPC j n`). See [JOURNAL.md](../JOURNAL.md) for journal-specific navigation.

The strip shows the **selected item plus its neighbors** — one before, one after — giving temporal context at a glance. Neighbors are rendered in `faded` text. The status bar stays at the very bottom (always present) and becomes **mode-aware**: `HIST`, `DAY`, or `JRNL` mode replaces `NORMAL`, with key hints in the right section replacing cursor position and thread indicators (both irrelevant during temporal browsing). See [WINDOW_LAYOUTS.md](../../WINDOW_LAYOUTS.md) § Status Bar Anatomy for mode colour assignments.

**Three states:**

| State | Chrome overhead | Trigger |
|-------|----------------|---------|
| **Context strip** (default) | 3 lines above status bar | `SPC H h` or `]d`/`[d` |
| **Expanded list** | ~40% of terminal above status bar | `Enter` from strip |
| **Dismissed** | 0 (status bar returns to normal mode) | `Esc` / `q` |

### Page History (`SPC H h`)

While viewing any page, `SPC H h` opens the unified history — undo tree entries (recent, branching) seamlessly followed by git commits (older, linear).

```
┌─ Rust Project ─────────────────────────────────────┐
│ ## Rope Data Structure                             │
│                                                     │
│ Ropes are O(log n) for inserts.                    │  ← live preview
│ They use balanced binary trees.                     │     of selected
│ See Xi Editor for details.                         │     version
│                                                     │
├─────────────────────────────────────────────────────┤
│  Mar 8 14:32   Restructured headings      +5 / -8  │  ← faded
│▸ Mar 8 16:01   Added rope section        +12 / -0  │  ← selected
│  Mar 8 21:00   Fixed typo                 +1 / -1  │  ← faded
├─────────────────────────────────────────────────────┤
│ HIST  Rust Project          d:diff  r:restore  3/12│
└─────────────────────────────────────────────────────┘
```

#### Expanded history list (`Enter` from strip)

```
┌─ Rust Project (preview) ───────────────────────────┐
│ ## Rope Data Structure                             │
│ Ropes are O(log n) for inserts.                    │
├─────────────────────────────────────────────────────┤
│  ● 2 min ago    "insert session"    (3 branches)   │  ← undo tree
│  ├─● 5 min ago  "delete line"                      │
│  │ └─● 5 min ago "change word" (abandoned)         │
│  ├─● 8 min ago  "insert session"                   │
│  ● 15 min ago   auto-save                          │  ← undo root = git
│  ○ 1 hour ago   auto-save            +3 / -1       │  ← git commits
│  ○ 3 hours ago  auto-save            +8 / -2       │
│  ○ yesterday    auto-save           +12 / -0       │
│  ○ Mar 12       auto-save           +28 / -0       │
│                                                     │
│  12 versions · Mar 1 – now                         │
├─────────────────────────────────────────────────────┤
│ HIST  Rust Project       j/k:nav  r:restore  3/12  │
└─────────────────────────────────────────────────────┘
```

- **● = undo node** (recent, rich). Full branching visible. Can restore to any node including abandoned branches.
- **○ = git commit** (older, linear). Restore replaces buffer. Creates a new undo branch.
- The transition is seamless — no visual break, just `●` → `○`.

**Interaction model:**

| Key | Action |
|-----|--------|
| `h` / `←` | Older version (strip mode, live preview updates) |
| `l` / `→` | Newer version (strip mode) |
| `j` / `k` | Navigate up/down (expanded mode) |
| `d` | Toggle inline diff (green = added, red = removed vs current) |
| `Enter` | Expand strip → scrollable list / collapse back |
| `r` | Restore selected version to buffer (undo-able) |
| `Esc` / `q` | Dismiss, return to current version |

**Live preview:** While scrubbing, the editor pane displays historical content read-only. The actual buffer is never modified. On `Esc`, original content reappears. On `r`, preview replaces buffer (one undo step).

### Restore

Pressing `r` on the context strip copies the selected version's full content into the current buffer. This is a normal edit — it goes through the rope, it's undo-able, it triggers auto-save. You can restore a past version and then `u` to undo if you change your mind. The git history gains a new commit showing the restore.

### Block-Level History

`SPC H H` (cursor on any block) opens history filtered to that specific block ID.

```
┌─ History: ^k7m2x ──────────────────────────────────┐
│                                                     │
│  ● 2 min ago    "Review ropey + petgraph API"       │  ← undo tree
│  ● 8 min ago    "Review ropey API"                  │  ← undo tree
│  ○ 1 hour ago   "Review the ropey API @due(03-16)"  │  ← git commit
│  ○ yesterday    (created)                           │  ← git: block born
│                                                     │
│  ─── moved: Weekly Review → Rust Project ───        │  ← cross-page move
│  ○ Mar 10       "Review rope libraries @due(03-12)" │  ← git: original form
│                                                     │
├─────────────────────────────────────────────────────┤
│  ↑/↓ navigate  Enter: preview  r: restore  q: close│
└─────────────────────────────────────────────────────┘
```

**How it works at each layer:**

**Undo tree (recent):** Walk the tree. At each node, extract the line containing `^k7m2x`. If content differs from child → this node changed the block. Show it. Skip nodes that didn't touch this block. Branching preserved.

**Git (older):** For each commit, `blob_at(oid, uuid)` → file content → grep for `^k7m2x` → extract line. If changed from previous commit → show it. If `^k7m2x` absent in older commit → creation point.

**Cross-page moves:** Block ID disappears from page A, appears in page B between two commits. Detected by scanning git diffs for the block ID across all changed files. Shown as a "moved" event in the timeline.

**Restore:** Replaces ONLY that line in the current buffer (same MirrorEdit-style line replacement). Rest of the page is untouched.

**Performance:** Undo tree walk: µs. Git per-block scan: ~1ms/commit. 100 commits ≈ 100ms. Cacheable.

---

## Day Activity

A git-derived summary of vault-wide activity for any given day. Available via `SPC H c` (day activity calendar). This is a **separate feature** from the journal (`SPC j c`) — it shows what happened across the entire vault, not just what you journaled.

| Keybinding | Action |
|-----------|--------|
| `SPC H c` | Open day activity calendar (◆ = days with git activity) |
| `[d` / `]d` | Hop to previous / next day with activity (from within day activity view) |

### What the Activity View Shows

| Section | Source | Content |
|---------|--------|---------|
| ✏️ Edited | Git diff: first commit of day → last commit of day | Page name, `+N / -M` lines, content snippets |
| 🌱 Created | Git diff: new files that day | Page titles + tags |
| ✅ Completed | Git diff: task lines that changed from `[ ]` to `[x]` | Task text + source page (identified by block ID) |

**Philosophy: over-surface, recall over precision.** When you're browsing back through time, too much context is better than too little. The stray detail is what triggers the memory.

### Wireframe

<div style="font-family: 'JetBrains Mono', 'Fira Code', 'Consolas', monospace; font-size: 13px; line-height: 1.5; background: #141414; color: #EBE9E7; border-radius: 6px; overflow: hidden; max-width: 680px; margin: 16px 0;">
  <!-- Day activity content -->
  <div style="padding: 12px 16px;">
    <div style="color: #A3A3A3; font-size: 11px; margin-bottom: 8px;">Day Activity — Saturday, March 8, 2026</div>
    <div>&nbsp;</div>
    <div><span style="font-weight: bold;">✏️ &nbsp;Edited</span></div>
    <div style="display: flex; justify-content: space-between;"><span>Text Editor Theory</span><span style="color: #62C554;">+12 lines</span></div>
    <div style="display: flex; justify-content: space-between;"><span>Rust Programming</span><span style="color: #62C554;">+3 lines</span></div>
    <div>&nbsp;</div>
    <div><span style="font-weight: bold;">🌱 &nbsp;Created</span></div>
    <div>Gap Buffer Tradeoffs <span style="color: #A3A3A3;">#data-structures</span></div>
    <div>&nbsp;</div>
    <div><span style="font-weight: bold;">✅ &nbsp;Completed</span></div>
    <div style="display: flex; justify-content: space-between;"><span><span style="color: #62C554;">[x]</span> Compare with PieceTable</span><span style="color: #A3A3A3;">Text Editor Theory</span></div>
    <div style="display: flex; justify-content: space-between;"><span><span style="color: #62C554;">[x]</span> Read Neovim buffer internals</span><span style="color: #A3A3A3;">Rust Programming</span></div>
    <div>&nbsp;</div>
    <div style="color: #A3A3A3; font-size: 12px;">3 pages edited · 1 page created · 2 tasks completed</div>
  </div>
  <!-- Context strip -->
  <div style="border-top: 1px solid #37373E;">
    <div style="padding: 4px 16px; color: #A3A3A3;">
      <div><span style="color: #F4BF4F;">◆</span> Mar 6 Thu</div>
    </div>
    <div style="padding: 4px 16px; background: #212228;">
      <div><span style="color: #EBE9E7;">▸</span> <span style="color: #F4BF4F;">◆</span> <span style="color: #EBE9E7; font-weight: bold;">Mar 8 Sat</span></div>
    </div>
    <div style="padding: 4px 16px; color: #A3A3A3;">
      <div><span style="color: #F4BF4F;">◆</span> Mar 12 Wed</div>
    </div>
  </div>
  <!-- Status bar (DAY mode) -->
  <div style="background: #F2DA61; color: #141414; padding: 3px 16px; display: flex; justify-content: space-between; font-size: 12px;">
    <div>
      <span style="font-weight: bold;">DAY</span>
      <span style="opacity: 0.4;"> │ </span>
      <span>Saturday, March 8, 2026</span>
    </div>
    <div style="opacity: 0.7;">e:detail &nbsp; ↵:calendar &nbsp; [d ]d</div>
  </div>
</div>

### Detail Levels

The activity sections support three density levels, toggled with `e`:

| Press | Mode | What activity shows |
|-------|------|----------------|
| (default) | **compact** | `Text Editor Theory  +12 lines` |
| `e` | **expanded** | + 2-3 line snippets of additions |
| `e` again | **full diff** | complete added/removed lines, colour-coded |

One key cycles through densities. Same data, different zoom. Not a configuration — a keybinding.

### Actions on Tasks

Tasks in the activity view are **actionable.** Pressing `x` on a task toggles it in the source file.

**How it works:** The activity view stores tasks by block ID (see [BLOCK_IDENTITY.md](../BLOCK_IDENTITY.md)). The toggle resolves `page_id^block_id` → current line in the index → flip `[ ]` ↔ `[x]` in the rope buffer. Same code path as the agenda's toggle.

If the block ID is orphaned (the content was deleted since that day), the task renders as historical — no action available, dimmed styling.

### Today's Activity

Today is the only day whose activity changes. Today's activity is **computed live** — a git diff from this morning's first commit to HEAD. This is cheap (narrow time window, few commits) and ensures it stays current as you work.

Refresh triggers: on index-complete (same trigger as backlinks refresh), so it updates when files are saved.

### Day Activity Cache

The cache stores **only the immutable parts** — data derived from git history, which cannot change after the day has passed.

```sql
CREATE TABLE day_activity_cache (
    date        TEXT PRIMARY KEY,   -- "2026-03-08"
    edits       TEXT NOT NULL,       -- JSON: [{page_id, page_title, added, removed, snippets, task_block_ids}]
    created     TEXT NOT NULL,       -- JSON: [{page_id, title, tags}]
    completed   TEXT NOT NULL,       -- JSON: [{block_id, page_id, task_text}]
    computed_at TEXT NOT NULL
);
```

Task *toggle state* is NOT cached — it's resolved live from the index at render time by block ID. This means toggling a task today that you wrote last month is immediately reflected. The cache stores *which* tasks appeared; the index provides *current* state.

**Predictive prefetch.** No eager backfill. A small hot window follows your attention:

| Trigger | Action |
|---------|--------|
| Open day N | Compute & cache N (if miss), then pre-compute N-1 and N+1 in background |
| Calendar hover on day N | Pre-compute N in background |
| `]d` from day N | Pre-compute N+2 (the hop *after* next) in background |
| `[d` from day N | Pre-compute N-2 in background |

The history thread does speculative work. If the user moves faster than the cache can fill (rapid `]d]d]d`), they see a brief spinner on cache misses — ~100ms, barely noticeable. In normal browsing (land, read, hop), the next day is always pre-computed.

**LRU eviction.** Fixed budget: 50 entries (~250 KB). When full, the least-recently-accessed entry is evicted. Evicted entries are recomputed on demand (~100ms). No growing database — the cache is a sliding window, bounded at 50 rows forever.

**Cache invalidation.** Past activity never changes — git history is append-only. If the user amends git history outside Bloom (rebase, force-push), Bloom detects the mismatch on next access (stored commit SHA vs. current) and recomputes.

---

## Performance

### Design Target

All performance estimates assume the **reference vault**: 10,000 pages (~25 MB of Markdown), 10 years of history, ~18,000 commits (~5 commits/day), ~8-10 MB git repo after pack compression.

**Assumptions:**

| Parameter | Value | Rationale |
|-----------|-------|-----------|
| Pages | 10,000 | Heavy long-term user |
| Average page size | 2.5 KB | Markdown notes, not novels |
| Total vault size | ~25 MB | 10K × 2.5 KB |
| Daily edit volume | 5-20 pages/day, ~5 KB net change | Active daily use |
| Commits per day | ~5 (idle-debounced) | 5-min idle window batches edits |
| History duration | 10 years (3,650 days) | Long-term use |
| Total commits | ~18,000 | 3,650 × 5 |
| Git repo size (packed) | ~8-10 MB | Delta compression + zlib on text |
| Versions per page (median) | ~20 | Most pages have light edit history |
| Versions per block (median) | < 5 | Blocks are rarely rewritten many times |

### Latency Budget

The target: **every user-initiated operation completes in < 10 ms,** or the result is already pre-computed when the user asks.

| Operation | Raw cost | Mitigation | User-perceived |
|-----------|----------|------------|----------------|
| **Commit** (auto, 5-min idle) | ~10-20 ms | Background thread, user never waits | 0 ms (invisible) |
| **Day view (cache hit)** | < 1 ms | LRU cache in SQLite | < 1 ms ✓ |
| **Day view (cache miss)** | ~100-200 ms | Predictive prefetch on `[d`/`]d` and calendar hover | < 1 ms (prefetched) |
| **Page history list** | ~50-100 ms | Prefetch on `SPC H` keypress (which-key delay = free compute time) | < 10 ms ✓ |
| **View past version** | < 5 ms | Prefetch adjacent entries while browsing history list | < 5 ms ✓ |
| **Diff two versions** | < 10 ms | Prefetch on history list navigation | < 10 ms ✓ |
| **Block history (pickaxe)** | ~10-50 ms | Scoped to one UUID file, not full tree | < 10 ms ✓ |
| **Calendar markers** | < 1 ms | Read from day view cache | < 1 ms ✓ |

### Prefetch Strategy

The principle: **never compute on demand — compute before the user asks.** Two mechanisms:

**1. Which-key prefix prefetch.** When the user presses a leader prefix, the history thread starts computing what that prefix's commands will need — before the user presses the second key.

| Prefix | Prefetches |
|--------|-----------|
| `SPC H` | Page history for current page + block pickaxe for cursor line |
| `SPC j` | Today's journal content |
| `SPC a` | Agenda task query results |

The which-key popup appears after 300ms. The user reads it and decides for 300-800ms. Total free compute time: 600-1100ms — enough for page history and block pickaxe.

**2. Adjacency prefetch inside temporal views.** Once inside a browsing context (day view, history list, calendar), prefetch the adjacent entries in the direction of navigation.

| Context | On navigate to item N | Prefetch |
|---------|----------------------|----------|
| Day activity (`[d`/`]d`) | Opened day N | Day N-1 and N+1 |
| History list (`j`/`k`) | Selected entry N | Blob + diff for N-1 and N+1 |
| Calendar (arrow keys) | Hovered day N | Day activity for N |
| Calendar (month change) | Entered new month | Day activity for days with `◆` markers |

**What we don't prefetch:** anything outside an active temporal context. Opening a page does not prefetch its history. Moving the cursor does not prefetch block history. These operations are infrequent enough that a one-time spinner (< 500ms) on first access is acceptable. Prefetch only kicks in once the user has entered a temporal browsing mode.

### Storage Budget

All history data lives in `.index/` (git history is non-rebuildable; SQLite is rebuildable):

| Component | Size (10-year reference vault) |
|-----------|-------------------------------|
| Git repo (`.index/.git/`) | ~8-10 MB |
| Undo tree (SQLite) | ~1-5 MB (pruned after 24h per buffer) |
| Day view LRU cache (50 entries) | ~250 KB |
| SQLite index (FTS5 + metadata) | ~15-20 MB |
| **Total `.index/`** | **~25-35 MB** |

Periodic `git gc` runs on the history thread during idle time (no more than once per day) to repack loose objects. This keeps the git repo compact.

**If `.index/` is deleted:** SQLite index rebuilds from files on disk. Undo tree is lost (same as clearing VS Code's undo history). Git history is lost — Bloom reinitializes with the current vault state as the first commit. The vault files themselves are always intact. This is documented: `.index/` contains non-rebuildable history data.

---

## Threading Model

```text
UI Thread
    │
    │  requests (page history, day view, block pickaxe, etc.)
    │  prefix hints (SPC H pressed → prefetch)
    │
    ▼
History Thread (new)
    │
    │  Owns: gix::Repository handle (GIT_DIR=.index/.git/, work tree = vault root)
    │  Owns: day_view_cache (SQLite)
    │
    ├── Read queries: revwalk, blob lookup, diff, pickaxe
    │
    ├── Prefetch: triggered by prefix keys and navigation
    │
    ├── Auto-commits: debounced from disk writer signals
    │   (UUID lookup via index for staging)
    │
    └── Periodic git gc (idle, max once/day)
    │
    ▼
UI Thread: render result frames
```

The history thread owns the `gix::Repository` handle and all caches. Auto-commits also go through this thread — the disk writer signals it (via channel) after each successful write, and the history thread debounces these signals (5-minute idle window) before committing.

---

## Vault Structure

Bloom's git repo lives inside `.index/` — separate from any user-managed `.git/` repo. This means users who `git init` their vault for backup/sync have zero conflicts with Bloom's history.

```
~/bloom/
├── pages/                  ← named pages (human-readable filenames)
│   ├── Text Editor Theory.md
│   └── Rust Programming.md
├── journal/                ← daily journal pages
│   └── 2026-03-09.md
├── .git/                   ← user's own repo (optional, theirs entirely)
├── .index/                 ← Bloom internals
│   ├── bloom.db            ← SQLite index (rebuildable from files)
│   ├── bloom.lock          ← single-instance lock (PID)
│   └── .git/               ← Bloom's history repo (UUID-based tree, non-rebuildable)
│       └── objects/        ← git object store (packed)
├── templates/
├── images/
├── .gitignore              ← excludes .index/
└── config.toml
```

Bloom opens its repo with `GIT_DIR=.index/.git/` and working tree at the vault root. The user's `.git/` (if present) is completely independent — different objects, different history, different commits. `git log` in the vault shows the user's commits, not Bloom's.

---

## Integration with Other Lab Ideas

### BQL (Named Views)

File history extends the query surface:

```
history | where page = "Text Editor Theory"              -- all versions of a page
history | where page = "Text Editor Theory" | where date before 2026-03-01
```

See [JOURNAL.md](../JOURNAL.md) for journal-level BQL queries.

### Emergence (Semantic Embeddings)

With time as a first-class dimension, emergence detection gains temporal awareness:

- "You wrote about X in March and independently arrived at the same idea in June" — the temporal gap is what makes it interesting
- Cognitive drift: how has the embedding cluster for a concept shifted over months?

### Block Identity (Self-Healing)

Git history is the backstop that makes block ID self-healing possible. See [BLOCK_IDENTITY.md](../BLOCK_IDENTITY.md).

---

## Configuration

```toml
[history]
auto_commit_idle_minutes = 5    # commit after N minutes of inactivity
max_commit_interval_minutes = 60  # safety net for long uninterrupted sessions
```

Git history is always on — it powers self-healing block IDs, time travel, and the day view. Bloom's internal repo (`.index/.git/`) is separate from any user-managed `.git/`, so there is no conflict with the user's own git workflow.

---

## Decisions

1. **UUID-based git tree.** Files stored under their page UUID, not filesystem path. Eliminates rename tracking. The index provides bidirectional UUID↔path mapping, rebuildable from frontmatter.
2. **Linear git, branching undo tree.** Git history is linear (no git branches). Branching is the undo tree's job — persisted to SQLite, VS Code model. Two layers, clean separation.
3. **Single-instance lock.** `.index/bloom.lock` with PID. Only one Bloom process per vault. TUI + GUI simultaneously is not supported.
4. **Block history via pickaxe.** `git log -S "^block_id" -- {uuid}.md` — scoped to one UUID file, fast. No blame needed.
5. **`.index/` contains non-rebuildable data.** Git history and undo tree are lost if `.index/` is deleted. Documented, acceptable — vault files are always the source of truth.

## Open Questions

1. **Commit message richness.** Auto-generated messages like "edited Text Editor Theory, journal" are useful. But should we include more? Tags changed, tasks created, links added? Richer messages = more context, but more computation per commit.

2. **Day boundary.** What defines "a day"? Local timezone midnight? Configurable? If you work past midnight, do those edits belong to yesterday or today? Leaning towards: the day boundary is when journal rotation happens (first launch of the new calendar day), not strict midnight.

3. **Undo tree pruning strategy.** Prune on buffer close? After 24 hours? After N nodes? VS Code prunes on file close + restart. Leaning towards: persist until buffer is closed, then prune.

---

## Testing and Demo Vault

Time travel features need realistic historical data for both automated tests and the demo experience. Since production auto-commits always use `now()`, we need a way to create backdated commits.

### Backdated Commits

`gix` commit objects accept explicit `author_date` and `committer_date` timestamps. A test helper in `bloom-test-harness` exposes this:

```rust
/// Create a commit with a backdated timestamp.
/// Writes files to the UUID-based tree and commits with the given date.
pub fn commit_at(
    repo: &gix::Repository,
    files: &[(&str, &str)],  // (uuid, content) pairs
    date: NaiveDateTime,
    message: &str,
)
```

This is a **dev-dependency only** — `bloom-test-harness` is never shipped in the release binary. Production code in `bloom-core` and `bloom-tui` has no access to backdating.

### Demo Vault

A `:demo-vault` command (or a setup wizard option) generates a realistic vault with months of simulated history:

- Creates pages with evolving content across simulated days
- Generates journal entries with tasks, links, and tags
- Backdates all commits to produce a rich calendar and day view
- Page history, block history, and day views all light up immediately

This gives new users an instant feel for time travel features without needing weeks of real usage first.

### Test Plan

Integration tests use `commit_at` to set up controlled histories:

- Day view: create commits across 3 days, verify correct grouping
- Page history: create 5 versions of one file (by UUID), verify revwalk
- Block history: edit a line across commits, verify pickaxe results
- Calendar markers: verify `◆` appears only on days with activity
- Cache: verify LRU eviction and predictive prefetch behaviour
- Rename survival: rename a page, verify history follows the UUID

---

## New Dependency

| Crate | Purpose | Size impact |
|-------|---------|-------------|
| `gix` | Pure Rust git: init, commit, revwalk, tree diff, pickaxe | ~2-3 MB binary size |

No external runtime dependencies. No `git` binary required. Works on macOS and Windows identically.

---

## References

- [`gix` crate](https://github.com/GitoxideLabs/gitoxide) — pure Rust git implementation, used by `cargo`
- [JOURNAL.md](../JOURNAL.md) — journal file model, calendar navigation
- [BLOCK_IDENTITY.md](../BLOCK_IDENTITY.md) — self-healing block IDs powered by git history
- [JOURNAL.md](../JOURNAL.md) — `journal.md` rotation model
