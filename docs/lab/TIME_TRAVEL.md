# Time Travel 🕰️

> Git-backed history via `gix` — the infrastructure layer for temporal features.
> Status: **Draft** — exploratory, not committed.
> See also: [DAY_VIEW.md](DAY_VIEW.md) for the daily activity summary built on this layer.

---

## The Problem

You wrote something three weeks ago. Maybe it was a journal entry, maybe you edited a page, maybe you created a new page and jotted a few ideas. You don't remember exactly when, or exactly where. You just remember *roughly* when you were thinking about it.

Today's tools give you two options: full-text search (requires remembering keywords) or manually browsing files sorted by modification date (tedious, no context). Neither reconstructs **what was on your mind that day** — the full picture of your thinking at a point in time.

---

## The Vision

Bloom maintains a complete, automatic history of every change to your vault. Not as a backup feature — as a **thinking tool.** Time becomes a first-class dimension you can navigate — per-file version history, per-block evolution, and vault-wide daily activity summaries.

**Fearless editing.** Every version of every thought is recoverable. Split pages, merge pages, delete sections — knowing you can always get back to any previous state. The undo tree handles per-keystroke recovery within a session; git handles everything beyond that.

This document covers the **infrastructure layer**: git as the time-series store, auto-commit strategy, file and block history, and the threading model. The [Day View](DAY_VIEW.md) document covers the vault-wide daily activity summary built on top of this infrastructure.

---

## Two Layers of History

| Layer | Granularity | Persistence | Branching | Purpose |
|-------|-------------|-------------|-----------|---------|
| **Undo tree** (SQLite) | Per-edit | Survives restarts, pruned on buffer close or after 24h | Full branching | "Undo what I just did" |
| **Git history** | 5-minute snapshots | Permanent (in `.index/.git/`) | Linear | "What did this look like last month?" |

The undo tree is the fine-grained, branching history — same as VS Code's persistent undo model. It's serialized to SQLite on quit and restored on next launch.

Git provides the coarse-grained, permanent record. Linear (no git branches), automatic, invisible. Every 5 minutes of inactivity, Bloom commits the current vault state. These commits are the substrate for page history, day view, and block-level time travel.

The two layers are complementary, not competing. `u` in Vim walks the undo tree. `SPC H h` browses git history.

---

## Architecture

### Git as the Time-Series Store

Bloom silently maintains a git repository in the vault. The user never interacts with git directly — Bloom auto-commits in the background using `gix` (pure Rust git implementation, compiled into the binary, zero external dependencies).

**Why git:**
- Battle-tested delta compression — a year of daily changes to 10K files stays small
- Line-level diffs for free — exactly what temporal features need
- Portable — the vault is a valid git repo, browsable with any git tool
- No new storage format to invent, debug, or migrate
- Enables self-healing block IDs (see [BLOCK_IDENTITY.md](BLOCK_IDENTITY.md))

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

### Page History (`SPC H h`)

While viewing any page, `SPC H h` opens its **commit history** — a list of every version of that file, newest first.

```
═══ Text Editor Theory — History ══════════════════════════════════

  ◆ Mar 8, 14:32                                    +12 / -0
    Added section on rope data structures

  ◆ Mar 6, 09:15                                     +3 / -1
    Updated Xi Editor reference

  ◆ Mar 1, 20:48                                    +45 / -0
    Initial creation

══════════════════════════════════════════════════════════════════
  3 versions · created Mar 1
```

Each entry is a commit that touched this page's UUID. The summary is auto-generated from the diff (`+N / -M` lines, first changed line as a description hint). Rename-proof — the UUID never changes.

**Navigation:**

| Key | Action |
|-----|--------|
| `j` / `k` | Move between versions |
| `Enter` | Open that version read-only (full page at that point in time) |
| `d` | Show diff between selected version and current (or between two selected versions) |
| `r` | Restore — copy this version's content into the current buffer (undo-able) |
| `e` | Toggle between compact and expanded diff view |
| `q` | Close history |

### Viewing a Past Version

When you press `Enter` on a history entry, Bloom retrieves the file content at that commit via `gix` (a single blob lookup by UUID — instant) and opens it in a **read-only buffer**. The status bar shows:

```
 HISTORY  Text Editor Theory  Mar 6, 09:15  (read-only)
```

You can scroll, search, even yank text — but not edit. This is a snapshot, not a live document. `q` returns to the current version.

### Side-by-Side Diff (`SPC H d`)

From the history view, pressing `d` opens a **split diff** between the selected version and the current page:

```
┌─ Text Editor Theory (Mar 6) ──────┬─ Text Editor Theory (current) ─────┐
│  ## Rope Data Structure            │  ## Rope Data Structure             │
│                                    │                                     │
│                                    │+ Ropes are O(log n) for inserts.   │
│                                    │+ They use balanced binary trees     │
│                                    │+ to represent text.                 │
│                                    │+                                    │
│  See Xi Editor for details.        │  See Xi Editor for a real-world     │
│                                    │  implementation.                    │
│                                    │                                     │
├────────────────────────────────────┼─────────────────────────────────────┤
│ HISTORY  Mar 6, 09:15             │ NORMAL  Text Editor Theory          │
└────────────────────────────────────┴─────────────────────────────────────┘
```

Added lines highlighted in `accent_green`, removed in `accent_red`. The diff is computed by `gix` (blob diff between two commits) and rendered using Bloom's existing split pane infrastructure.

With two versions selected in the history list (mark with `Tab`), `d` diffs those two versions against each other — not against current.

### Restore

Pressing `r` on a history entry copies that version's full content into the current buffer. This is a normal edit — it goes through the rope, it's undo-able, it triggers auto-save. You can restore a past version and then `u` to undo if you change your mind. The git history gains a new commit showing the restore.

### Block-Level History

With universal block IDs (see [BLOCK_IDENTITY.md](BLOCK_IDENTITY.md)), file time travel extends to individual blocks. Place your cursor on any block and `SPC H H` (block history) shows every version of *that specific block* across time:

```
═══ ^k7m2x — Block History ════════════════════════════════════════

  ◆ Mar 8    - [ ] Review the ropey API @due(2026-03-10)
  ◆ Mar 6    - [ ] Review the ropey API @due(2026-03-08)
  ◆ Mar 1    - [ ] Review the ropey crate

  3 versions · first appeared Mar 1
```

This uses pickaxe search (`-S "^k7m2x"`) scoped to the page's UUID file (`-- 8f3a1b2c.md`). Because the git tree uses UUIDs, the search is scoped to one file's history, not the entire tree. Estimated: <10ms for a typical page.

For cross-page block moves, an unscoped pickaxe search finds the block ID across all UUID files — revealing which page it lived in at each point in time.

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
| `SPC j` | Today's journal content + yesterday's day view |
| `SPC a` | Agenda task query results |

The which-key popup appears after 300ms. The user reads it and decides for 300-800ms. Total free compute time: 600-1100ms — enough for page history and block pickaxe.

**2. Adjacency prefetch inside temporal views.** Once inside a browsing context (day view, history list, calendar), prefetch the adjacent entries in the direction of navigation.

| Context | On navigate to item N | Prefetch |
|---------|----------------------|----------|
| Day view (`[d`/`]d`) | Opened day N | Day N-1 and N+1 |
| History list (`j`/`k`) | Selected entry N | Blob + diff for N-1 and N+1 |
| Calendar (arrow keys) | Hovered day N | Day view for N |
| Calendar (month change) | Entered new month | Day views for days with `◆` markers |

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

See [DAY_VIEW.md](DAY_VIEW.md) for day-level BQL queries.

### Emergence (Semantic Embeddings)

With time as a first-class dimension, emergence detection gains temporal awareness:

- "You wrote about X in March and independently arrived at the same idea in June" — the temporal gap is what makes it interesting
- Cognitive drift: how has the embedding cluster for a concept shifted over months?

### Block Identity (Self-Healing)

Git history is the backstop that makes block ID self-healing possible. See [BLOCK_IDENTITY.md](BLOCK_IDENTITY.md).

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
- [DAY_VIEW.md](DAY_VIEW.md) — daily activity summary built on this layer
- [BLOCK_IDENTITY.md](BLOCK_IDENTITY.md) — self-healing block IDs powered by git history
- [Journal Redesign](JOURNAL_REDESIGN.md) — `journal.md` rotation model
