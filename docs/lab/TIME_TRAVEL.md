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

This document covers the **infrastructure layer**: git as the time-series store, auto-commit strategy, file and block history, and the threading model. The [Day View](DAY_VIEW.md) document covers the vault-wide daily activity summary built on top of this infrastructure.

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

### Auto-Commit Strategy

Bloom commits automatically. The user never thinks about it.

**When to commit:**

| Trigger | Rationale |
|---------|-----------|
| On quit | Capture the final state of the session |
| After 5 minutes of inactivity | Natural pause boundary — you've context-switched |
| On journal rotation (start of new day) | Close out the day cleanly before rotating |

**Not** on every auto-save. The 300ms auto-save debounce writes to disk for crash safety, but committing every 300ms would create hundreds of commits per day. The 5-minute idle window batches edits into meaningful chunks — typically 3–10 commits per active day.

**Commit details:**

```
Author:  Bloom <bloom@local>
Message: "2026-03-08 14:32 — edited Text Editor Theory, journal"
```

- Machine-authored (filterable if the user also commits manually)
- Timestamp + summary of what changed (auto-generated from the staged diff)
- All files staged with `git add -A` before each commit

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

Each entry is a commit that touched this file. The summary is auto-generated from the diff (`+N / -M` lines, first changed line as a description hint).

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

When you press `Enter` on a history entry, Bloom retrieves the file content at that commit via `gix` (a single blob lookup — instant) and opens it in a **read-only buffer**. The status bar shows:

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
═══ ^a3 — Block History ═══════════════════════════════════════════

  ◆ Mar 8    - [ ] Review the ropey API @due(2026-03-10)
  ◆ Mar 6    - [ ] Review the ropey API @due(2026-03-08)
  ◆ Mar 1    - [ ] Review the ropey crate

  3 versions · first appeared Mar 1
```

This uses `gix` blame to trace the block ID through commits — finding every commit that changed the line containing `^a3`. The block ID is the stable anchor that makes this possible even when lines shift around it.

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
| **Page history list** | ~50-100 ms | Prefetch on `SPC l` keypress (which-key delay = free compute time) | < 10 ms ✓ |
| **View past version** | < 5 ms | Prefetch adjacent entries while browsing history list | < 5 ms ✓ |
| **Diff two versions** | < 10 ms | Prefetch on history list navigation | < 10 ms ✓ |
| **Block history (blame)** | ~200-500 ms | Prefetch on `SPC H` keypress; which-key delay = free compute time | < 10 ms (prefetched) |
| **Calendar markers** | < 1 ms | Read from day view cache | < 1 ms ✓ |

### Prefetch Strategy

The principle: **never compute on demand — compute before the user asks.** Two mechanisms:

**1. Which-key prefix prefetch.** When the user presses a leader prefix, the history thread starts computing what that prefix's commands will need — before the user presses the second key.

| Prefix | Prefetches |
|--------|-----------|
| `SPC H` | Page history for current page + block blame for cursor line |
| `SPC j` | Today's journal content + yesterday's day view |
| `SPC a` | Agenda task query results |

The which-key popup appears after 300ms. The user reads it and decides for 300-800ms. Total free compute time: 600-1100ms — enough for page history and block blame.

**2. Adjacency prefetch inside temporal views.** Once inside a browsing context (day view, history list, calendar), prefetch the adjacent entries in the direction of navigation.

| Context | On navigate to item N | Prefetch |
|---------|----------------------|----------|
| Day view (`[d`/`]d`) | Opened day N | Day N-1 and N+1 |
| History list (`j`/`k`) | Selected entry N | Blob + diff for N-1 and N+1 |
| Calendar (arrow keys) | Hovered day N | Day view for N |
| Calendar (month change) | Entered new month | Day views for days with `◆` markers |

**What we don't prefetch:** anything outside an active temporal context. Opening a page does not prefetch its history. Moving the cursor does not prefetch block blame. These operations are infrequent enough that a one-time spinner (< 500ms) on first access is acceptable. Prefetch only kicks in once the user has entered a temporal browsing mode.

### Storage Budget

All history data lives in `.index/` (rebuildable, gitignored):

| Component | Size (10-year reference vault) |
|-----------|-------------------------------|
| Git repo (`.index/.git/`) | ~8-10 MB |
| Day view LRU cache (50 entries) | ~250 KB |
| SQLite index (FTS5 + metadata) | ~15-20 MB |
| **Total `.index/`** | **~25-30 MB** |

Periodic `git gc` runs on the history thread during idle time (no more than once per day) to repack loose objects. This keeps the git repo compact.

If `.index/` is deleted, Bloom rebuilds everything: SQLite index from files on disk, git repo from current file state (historical day views are lost but future ones accumulate again).

---

## Threading Model

```text
UI Thread
    │
    │  requests (page history, day view, block blame, etc.)
    │  prefix hints (SPC H pressed → prefetch)
    │
    ▼
History Thread (new)
    │
    │  Owns: gix::Repository handle (GIT_DIR=.index/.git/)
    │  Owns: day_view_cache + prefetch_cache (SQLite)
    │
    ├── Read queries: revwalk, blob lookup, diff, blame
    │
    ├── Prefetch: triggered by prefix keys and navigation
    │
    ├── Auto-commits: debounced from disk writer signals
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
├── journal.md              ← today's journal (always this name)
├── pages/                  ← named pages
│   ├── Text Editor Theory.md
│   └── Rust Programming.md
├── .journal/               ← hidden archive (auto-rotated daily journals)
│   ├── 2026-03-07.md
│   ├── 2026-03-06.md
│   └── ...
├── .git/                   ← user's own repo (optional, theirs entirely)
├── .index/                 ← Bloom internals (gitignored, rebuildable)
│   ├── bloom.db            ← SQLite index
│   └── .git/               ← Bloom's history repo (separate from user's)
├── templates/
├── images/
├── .gitignore              ← excludes .index/ (so user's git ignores Bloom internals)
└── config.toml
```

Bloom opens its repo with `GIT_DIR=.index/.git/` and working tree at the vault root. The user's `.git/` (if present) is completely independent — different objects, different history, different commits. `git log` in the vault shows the user's commits, not Bloom's.

`.index/` is already excluded by the auto-generated `.gitignore` (from G21), so no additional gitignore entries are needed.

---

## Integration with Other Lab Ideas

### BQL (Live Views)

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
enabled = true                  # default: true
auto_commit_idle_minutes = 5    # commit after N minutes of inactivity
```

Users who manage their own git workflow can set `enabled = false` — Bloom won't touch `.git/`. Time travel features degrade gracefully (calendar shows journal archive only, no edit diffs, no self-healing).

---

## Open Questions

1. **Commit message richness.** Auto-generated messages like "edited Text Editor Theory, journal" are useful. But should we include more? Tags changed, tasks created, links added? Richer messages = more context, but more computation per commit.

2. **Day boundary.** What defines "a day"? Local timezone midnight? Configurable? If you work past midnight, do those edits belong to yesterday or today? Leaning towards: the day boundary is when journal rotation happens (first launch of the new calendar day), not strict midnight.

3. **Prefetch cancellation.** When `SPC H` triggers a prefetch but the user presses `Esc` (abandons the prefix), should the history thread cancel in-flight work? Or let it finish and cache anyway? Leaning towards: let it finish — the work is cheap and the cache may be useful next time.

4. **Cold start.** A brand new vault has no git history. Time travel features show empty results gracefully. After the first day of use, the first day view is computed on journal rotation. No special cold-start logic needed beyond graceful empty states.

---

## Testing and Demo Vault

Time travel features need realistic historical data for both automated tests and the demo experience. Since production auto-commits always use `now()`, we need a way to create backdated commits.

### Backdated Commits

`gix` commit objects accept explicit `author_date` and `committer_date` timestamps. A test helper in `bloom-test-harness` exposes this:

```rust
/// Create a commit with a backdated timestamp.
/// Writes files, stages them, and commits with the given date.
pub fn commit_at(
    repo: &gix::Repository,
    files: &[(&str, &str)],  // (path, content) pairs
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
- Page history: create 5 versions of one file, verify revwalk
- Block history: edit a line across commits, verify blame chain
- Calendar markers: verify `◆` appears only on days with activity
- Cache: verify LRU eviction and predictive prefetch behaviour

---

## New Dependency

| Crate | Purpose | Size impact |
|-------|---------|-------------|
| `gix` | Pure Rust git: init, commit, revwalk, tree diff, blame | ~2-3 MB binary size |

No external runtime dependencies. No `git` binary required. Works on macOS and Windows identically.

---

## References

- [`gix` crate](https://github.com/GitoxideLabs/gitoxide) — pure Rust git implementation, used by `cargo`
- [DAY_VIEW.md](DAY_VIEW.md) — daily activity summary built on this layer
- [BLOCK_IDENTITY.md](BLOCK_IDENTITY.md) — self-healing block IDs powered by git history
- [Journal Redesign](JOURNAL_REDESIGN.md) — `journal.md` rotation model
