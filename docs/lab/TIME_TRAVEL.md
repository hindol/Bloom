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

### Page History (`SPC l h`)

While viewing any page, `SPC l h` opens its **commit history** — a list of every version of that file, newest first.

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

### Side-by-Side Diff (`SPC l d`)

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

With universal block IDs (see [BLOCK_IDENTITY.md](BLOCK_IDENTITY.md)), file time travel extends to individual blocks. Place your cursor on any block and `SPC l H` (block history) shows every version of *that specific block* across time:

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

| Operation | Source | Cost |
|-----------|--------|------|
| Page history list | `gix` revwalk filtered by path | ~10-50 ms |
| View past version | `gix` blob lookup at commit | < 5 ms |
| Diff two versions | `gix` blob diff | < 10 ms |
| Block history | `gix` blame on single file | ~50-100 ms |
| Day view (cache hit) | SQLite row lookup | < 1 ms |
| Day view (cache miss) | `gix` revwalk + tree diff | ~100-200 ms |

All operations run on the history thread. The UI shows a spinner for anything over ~50 ms, but in practice most operations feel instant.

**Caching rule:** cache only when latency matters for interactive browsing. The day view is cached because `[d`/`]d` hopping needs sub-millisecond response. File history, past versions, and diffs are computed live from `gix` — fast enough and accessed infrequently.

---

## Threading Model

```text
UI Thread
    │
    │  requests (page history, day view, block blame, etc.)
    │
    ▼
History Thread (new)
    │
    │  Owns: gix::Repository handle
    │  Owns: day_views cache (read-write SQLite connection)
    │
    ├── Read queries: revwalk, blob lookup, diff, blame
    │
    ├── Auto-commits: debounced from disk writer signals
    │
    └── Day view computation: on-demand + predictive prefetch
    │
    ▼
UI Thread: render result frames
```

The history thread owns the `gix::Repository` handle and the day view cache. Auto-commits also go through this thread — the disk writer signals it (via channel) after each successful write, and the history thread debounces these signals (5-minute idle window) before committing.

---

## Vault Structure

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
├── .git/                   ← auto-managed by Bloom via gix (user never touches)
├── .index/                 ← SQLite index + day_views cache
│   └── bloom.db
├── templates/
├── images/
├── .gitignore
└── config.toml
```

`.gitignore` excludes `.index/` (rebuildable) but NOT `.journal/` (archived content, should be versioned).

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

1. **User's existing git repo.** If the vault already has a `.git/` with the user's manual commits, Bloom's auto-commits would interleave. Options: (a) Bloom uses a separate orphan branch `bloom/history`, (b) Bloom's commits use a distinct author and the user filters in their own tooling, (c) Bloom only auto-inits git if no `.git/` exists. Leaning towards (c) — respect the user's existing setup.

2. **Commit message richness.** Auto-generated messages like "edited Text Editor Theory, journal" are useful. But should we include more? Tags changed, tasks created, links added? Richer messages = more context, but more computation per commit.

3. **Storage budget.** Git packfiles are efficient, but a year of daily commits with 10K files — how large does `.git/` get? Need to benchmark. Periodic `git gc` (via `gix`) could run on the history thread during idle time.

4. **Day boundary.** What defines "a day"? Local timezone midnight? Configurable? If you work past midnight, do those edits belong to yesterday or today? Leaning towards: the day boundary is when journal rotation happens (first launch of the new calendar day), not strict midnight.

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
