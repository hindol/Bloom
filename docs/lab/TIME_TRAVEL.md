# Time Travel 🕰️

> Git-backed history with cached day views for instant cognitive time travel.
> Status: **Draft** — exploratory, not committed.

---

## The Problem

You wrote something three weeks ago. Maybe it was a journal entry, maybe you edited a page, maybe you created a new page and jotted a few ideas. You don't remember exactly when, or exactly where. You just remember *roughly* when you were thinking about it.

Today's tools give you two options: full-text search (requires remembering keywords) or manually browsing files sorted by modification date (tedious, no context). Neither reconstructs **what was on your mind that day** — the full picture of your thinking at a point in time.

---

## The Vision

Bloom maintains a complete, automatic history of every change to your vault. Not as a backup feature — as a **thinking tool.** You can travel to any past day and see a rich summary of your mental activity: what you journaled, which pages you edited, what you created, what tasks you completed. It feels like flipping through a diary, except the diary writes itself.

Navigation is temporal — a calendar to land approximately, then day-hopping to browse fluidly. Past day views load instantly because they're pre-computed and cached.

---

## Architecture

### Git as the Time-Series Store

Bloom silently maintains a git repository in the vault. The user never interacts with git directly — Bloom auto-commits in the background using `gix` (pure Rust git implementation, compiled into the binary, zero external dependencies).

**Why git:**
- Battle-tested delta compression — a year of daily changes to 10K files stays small
- Line-level diffs for free — exactly what the day view needs
- Portable — the vault is a valid git repo, browsable with any git tool
- No new storage format to invent, debug, or migrate

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

### The Day View

The day view is a **rich summary of your mental activity on a given day.** It combines journal content, page edit diffs, page creation, and task completions into a single scrollable document.

```
═══ Saturday, March 8, 2026 ═══════════════════════════════════════

 📓 Journal
 ─────────────────────────────────────────────────────────────────
  - Explored ropey crate for buffer model
  - Read about Xi Editor architecture
  - [ ] Review gap buffer tradeoffs @due(03-10)
  - [x] Compare with PieceTable
  #rust #editors #data-structures

 ✏️  Edited
 ─────────────────────────────────────────────────────────────────
  Text Editor Theory                            +12 lines
    + "Ropes are O(log n) for inserts. They use balanced
       binary trees to represent text."
    + "See Xi Editor for a real-world implementation."

  Rust Programming                               +3 lines
    + "The ropey crate handles Unicode correctly via
       grapheme cluster boundaries."

 🌱 Created
 ─────────────────────────────────────────────────────────────────
  Gap Buffer Tradeoffs                    #data-structures

 ✅ Completed
 ─────────────────────────────────────────────────────────────────
  [x] Compare with PieceTable             Text Editor Theory
  [x] Read Neovim buffer internals        Rust Programming

═══════════════════════════════════════════════════════════════════
  3 pages edited · 1 page created · 2 tasks completed
```

**Sections:**

| Section | Source | Content |
|---------|--------|---------|
| 📓 Journal | Archived journal file for that day | Full journal content |
| ✏️ Edited | Git diff (first commit of day → last commit of day) | Page name, lines added/removed, 2-3 line snippets of additions |
| 🌱 Created | Git diff (new files) | Page titles + tags |
| ✅ Completed | Index: tasks toggled from `[ ]` to `[x]` on that day | Task text + source page |

**Philosophy: over-surface, recall over precision.** The day view shows everything, because when you're browsing back through time you're trying to re-enter a mental context. The stray detail is what triggers the memory.

### Eager Caching

A past day's view is **immutable** — once midnight passes, that day's summary never changes. We compute it once and store it forever.

```sql
CREATE TABLE day_views (
    date        TEXT PRIMARY KEY,   -- "2026-03-08"
    journal     TEXT,               -- archived journal content (markdown)
    edits       TEXT NOT NULL,       -- JSON: [{page, added, removed, snippets}]
    created     TEXT NOT NULL,       -- JSON: [{title, tags}]
    completed   TEXT NOT NULL,       -- JSON: [{task_text, source_page}]
    computed_at TEXT NOT NULL
);
```

**Cache lifecycle:**

| Event | Action |
|-------|--------|
| Journal rotation (new day starts) | Background thread computes yesterday's day view, writes to `day_views` |
| Calendar navigation to past day | Read from `day_views` — single SQLite lookup, sub-millisecond |
| Today's day view | Computed live from git diff since this morning's first commit (cheap — narrow window) |
| First launch with existing vault | Background backfill: walk git history, compute all historical day views. Heavy once (~seconds for a year of history), never again |
| `:rebuild-index` | Invalidate and recompute all `day_views` from git history |

**Result:** flipping between days with `[d` / `]d` feels like scrolling a local file. Zero git operations, zero diff computation for cached days.

**Cache invalidation:** If the user amends git history outside Bloom (rebase, squash), Bloom detects that the commit range for a cached date no longer matches and invalidates that entry. For normal use this never happens.

### Calendar Navigation

`SPC j c` opens a month-grid calendar.

```
         March 2026
  Mo Tu We Th Fr Sa Su
                    1
   2  3  4  5 ◆6  7 ◆8
   9 10 11 ◆12 13 14 15
  16 17 18 19 20 21 22
  23 24 25 ◆26 27 28 29
  30 31

  ◆ = has activity (journal or page edits)
```

| Key | Action |
|-----|--------|
| `h` / `l` | Previous / next day |
| `j` / `k` | Next / previous week |
| `H` / `L` | Previous / next month |
| `Enter` | Open day view |
| `q` / `Esc` | Close calendar |

The `◆` markers come from the `day_views` cache — days with non-empty entries are highlighted. This is a single query: `SELECT date FROM day_views`.

### Day-Hopping

Once inside a day view, temporal navigation keys let you browse fluidly:

| Key | Action |
|-----|--------|
| `]d` | Jump to next day with activity (skip empty days) |
| `[d` | Jump to previous day with activity |
| `j` / `k` | Scroll within the current day view |
| `Enter` | On an edit snippet or page name — jump to that page |
| `q` | Back to calendar (or close if opened directly) |

Day-hopping queries: `SELECT date FROM day_views WHERE date > ? ORDER BY date LIMIT 1`. Instant.

---

## Threading Model

```text
UI Thread
    │
    │  "show day view for 2026-03-08"
    │
    ▼
History Thread (new, or shared with indexer)
    │
    ├── Cache hit? → return from day_views table
    │
    └── Cache miss? → gix: revwalk + tree diff → compute → cache → return
    │
    ▼
UI Thread: render DayViewFrame
```

The history thread owns the `gix::Repository` handle (read-only for queries) and the read-write connection to the `day_views` cache table. Auto-commits also go through this thread.

For auto-commits, the disk writer thread signals the history thread (via channel) after each successful write. The history thread debounces these signals (5-minute idle window) and commits when appropriate.

---

## Vault Structure (revised)

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

Day views are expressible as queries:

```
blocks | where modified on 2026-03-08                    -- everything touched that day
blocks | where page = $journal | where created on 2026-03-08  -- just journal entries
tasks  | where completed on 2026-03-08                   -- tasks checked off that day
pages  | where created on 2026-03-08                     -- pages created that day
```

The cached day view is the *pre-computed, rendered* version of these queries. BQL gives you ad-hoc flexibility; the day view gives you instant, opinionated, over-surfaced context.

### Emergence (Semantic Embeddings)

With time as a first-class dimension, emergence detection gains temporal awareness:

- "You wrote about X in March and independently arrived at the same idea in June" — the temporal gap is what makes it interesting
- Cognitive drift: how has the embedding cluster for a concept shifted over months?
- The day view could surface emergence discoveries for that day: "On this day, you wrote something that connects to what you'd write 3 months later"

---

## Configuration

```toml
[history]
enabled = true                  # default: true
auto_commit_idle_minutes = 5    # commit after N minutes of inactivity
```

Users who manage their own git workflow can set `enabled = false` — Bloom won't touch `.git/`. Time travel features degrade gracefully (calendar shows journal archive only, no edit diffs).

---

## Open Questions

1. **User's existing git repo.** If the vault already has a `.git/` with the user's manual commits, Bloom's auto-commits would interleave. Options: (a) Bloom uses a separate orphan branch `bloom/history`, (b) Bloom's commits use a distinct author and the user filters in their own tooling, (c) Bloom only auto-inits git if no `.git/` exists. Leaning towards (c) — respect the user's existing setup.

2. **Commit message richness.** Auto-generated messages like "edited Text Editor Theory, journal" are useful for the day view summary. But should we include more? Tags changed, tasks created, links added? Richer messages = richer day view cache, but more computation per commit.

3. **Storage budget.** Git packfiles are efficient, but a year of daily commits with 10K files — how large does `.git/` get? Need to benchmark. Periodic `git gc` (via `gix`) could run on the history thread during idle time.

4. **Day boundary.** What defines "a day"? Local timezone midnight? Configurable? If you work past midnight, do those edits belong to yesterday or today? Leaning towards: the day boundary is when journal rotation happens (first launch of the new calendar day), not strict midnight.

5. **Deleted content.** The day view shows additions and edits. Should it also show deletions? "You removed the section about gap buffers from Text Editor Theory." Useful for recall ("oh right, I decided that wasn't relevant") but potentially noisy.

6. **Day view for today.** Today's view is computed live (not cached). How often to refresh? On every render? On a timer? On index-complete? Leaning towards: on index-complete (same trigger as backlinks refresh), so it updates when files are saved.

---

## New Dependency

| Crate | Purpose | Size impact |
|-------|---------|-------------|
| `gix` | Pure Rust git: init, commit, revwalk, tree diff | ~2-3 MB binary size |

No external runtime dependencies. No `git` binary required. Works on macOS and Windows identically.

---

## References

- [`gix` crate](https://github.com/GitoxideLabs/gitoxide) — pure Rust git implementation, used by `cargo`
- Current journal design: [GOALS.md G14](../GOALS.md)
- Bullet Journal migration: the inspiration for task carry-forward
- [Journal Redesign](JOURNAL_REDESIGN.md) — companion doc covering the `journal.md` rotation model
