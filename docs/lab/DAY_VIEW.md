# Day View 📅

> A rich daily activity summary — "what was on my mind that day."
> Status: **Draft** — exploratory, not committed.
> Built on: [TIME_TRAVEL.md](TIME_TRAVEL.md) (git layer), [BLOCK_IDENTITY.md](BLOCK_IDENTITY.md) (stable IDs for actions).

---

## The Problem

When you browse back through time, you're not doing a precise lookup — you're trying to **re-enter a mental context.** You want to see the full picture of what you were thinking about on a given day: what you journaled, which pages you edited, what you created, what tasks you completed.

No existing tool provides this. File browsers show modification dates. Git log shows raw diffs. Neither reconstructs the *shape* of a day's thinking.

---

## The Design

### What the Day View Shows

The day view is a **read-only, scrollable document** summarising all vault activity on a given day. It combines data from the journal archive and git diffs into a single view.

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

### Default Sections

| Section | Source | Content |
|---------|--------|---------|
| 📓 Journal | Archived journal file for that day (`.journal/`) | Full journal content, rendered with Bloom syntax highlighting |
| ✏️ Edited | Git diff: first commit of day → last commit of day | Page name, `+N / -M` lines, content snippets |
| 🌱 Created | Git diff: new files that day | Page titles + tags |
| ✅ Completed | Git diff: task lines that changed from `[ ]` to `[x]` | Task text + source page (identified by block ID) |

**Philosophy: over-surface, recall over precision.** When you're browsing back through time, too much context is better than too little. The stray detail is what triggers the memory.

### Detail Levels

The edit section supports three density levels, toggled with a single key:

| Key | Mode | What edits show |
|-----|------|----------------|
| default | **compact** | `Text Editor Theory  +12 lines` |
| `e` | **expanded** | + 2-3 line snippets of additions |
| `e` again | **full diff** | complete added/removed lines, colour-coded |

One key cycles through densities. Same data, different zoom. Not a configuration — a keybinding.

---

## Navigation

### Calendar (`SPC H c`)

`SPC H c` opens a month-grid calendar to land at an approximate date.

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

The `◆` markers come from the cache — a single query: `SELECT date FROM day_view_cache`.

### Day-Hopping

Once inside a day view:

| Key | Action |
|-----|--------|
| `]d` | Jump to next day with activity (skip empty days) |
| `[d` | Jump to previous day with activity |
| `j` / `k` | Scroll within the current day view |
| `e` | Cycle detail level (compact → expanded → full diff) |
| `Enter` | On a page name or edit snippet — jump to that page |
| `x` | On a task — toggle it in the source file |
| `o` | On a page — open in a split |
| `q` | Back to calendar (or close) |

Day-hopping between cached days is sub-millisecond. Between uncached days, the predictive cache ensures the next hop is pre-computed before you press the key.

### Actions on Tasks

Tasks in the day view are **actionable.** Pressing `x` on a task toggles it in the source file.

**How it works:** The day view stores tasks by block ID (see [BLOCK_IDENTITY.md](BLOCK_IDENTITY.md)). The toggle resolves `page_id^block_id` → current line in the index → flip `[ ]` ↔ `[x]` in the rope buffer. Same code path as the agenda's toggle.

If the block ID is orphaned (the content was deleted since that day), the task renders as historical — no action available, dimmed styling.

---

## Caching Strategy

### What's Cached

The day view cache stores **only the immutable parts** — data derived from git history and the journal archive, which cannot change after the day has passed.

```sql
CREATE TABLE day_view_cache (
    date        TEXT PRIMARY KEY,   -- "2026-03-08"
    journal     TEXT,               -- archived journal content (markdown)
    edits       TEXT NOT NULL,       -- JSON: [{page_id, page_title, added, removed, snippets, task_block_ids}]
    created     TEXT NOT NULL,       -- JSON: [{page_id, title, tags}]
    completed   TEXT NOT NULL,       -- JSON: [{block_id, page_id, task_text}]
    computed_at TEXT NOT NULL
);
```

Task *toggle state* is NOT cached — it's resolved live from the index at render time by block ID. This means toggling a task today that you wrote last month is immediately reflected when you view last month's day view. The cache stores *which* tasks appeared; the index provides *current* state.

### Predictive Prefetch

No eager backfill. No ever-growing cache. A small hot window follows your attention.

| Trigger | Action |
|---------|--------|
| Open day N | Compute & cache N (if miss), then pre-compute N-1 and N+1 in background |
| Calendar hover on day N | Pre-compute N in background |
| `]d` from day N | Pre-compute N+2 (the hop *after* next) in background |
| `[d` from day N | Pre-compute N-2 in background |

The history thread does speculative work. If the user moves faster than the cache can fill (rapid `]d]d]d`), they see a brief spinner on cache misses — ~100ms, barely noticeable. In normal browsing (land, read, hop), the next day is always pre-computed.

### LRU Eviction

The cache has a **fixed budget**: 50 entries (~250 KB). When full, the least-recently-accessed entry is evicted. Evicted entries are recomputed on demand if revisited (~100ms).

This means:
- Yesterday's view stays hot (recently accessed)
- A deep browse into March 2024 fills the cache with March 2024 entries, evicting distant dates
- Coming back to last week re-computes a few entries — fast and invisible

**No growing database.** The cache is a sliding window, bounded at 50 rows forever.

### Cache Invalidation

For normal use, past day views never change — git history is append-only.

If the user amends git history outside Bloom (rebase, force-push), Bloom detects the mismatch on next access (stored commit SHA vs. current) and recomputes. This is an edge case that rarely happens.

### Today's Day View

Today is the only day that changes. Today's view is **computed live** on each render — a git diff from this morning's first commit to HEAD. This is cheap (narrow time window, few commits) and ensures the view stays current as you work.

Refresh triggers: on index-complete (same trigger as backlinks refresh), so it updates when files are saved.

---

## Customisation via BQL

Once BQL exists ([LIVE_VIEWS.md](LIVE_VIEWS.md)), each section of the day view *is* a named query:

```
📓 Journal    →  journal | where date = $day
✏️ Edited     →  edits   | where date = $day
🌱 Created    →  pages   | where created on $day
✅ Completed  →  tasks   | where completed on $day
```

Power users can redefine the day view in `config.toml`:

```toml
[[day_view.sections]]
icon = "📓"
title = "Journal"
query = "journal | where date = $day"

[[day_view.sections]]
icon = "✏️"
title = "Edited"
query = "edits | where date = $day"

[[day_view.sections]]
icon = "🏷️"
title = "Work Activity"
query = "blocks | where modified on $day | where tags has 'work'"

# 'Created' section removed — this user doesn't care
# Custom 'Work Activity' section added
```

No new DSL. No "day view configuration language." Just BQL queries applied to a different context. Adding a section = adding a query. Removing = deleting a line.

Users who never learn BQL see the built-in defaults and zero difference. The customisation exists for power users who want it.

**Rendering is not customisable.** The query decides *what* to show. Bloom decides *how* to render it. Tasks look like tasks. Diffs look like diffs. Consistent visual language, no format strings.

---

## Integration with Other Lab Ideas

### BQL (Live Views)

Day-level queries extend the BQL surface:

```
blocks | where modified on 2026-03-08                    -- everything touched that day
tasks  | where completed on 2026-03-08                   -- tasks checked off that day
pages  | where created on 2026-03-08                     -- pages created that day
journal | where date = 2026-03-08                        -- just journal entries
```

The day view is the *pre-computed, rendered* version of these queries. BQL gives ad-hoc flexibility; the day view gives instant, opinionated, over-surfaced context.

### Block Identity

Task actions in the day view depend on stable block IDs. See [BLOCK_IDENTITY.md](BLOCK_IDENTITY.md). Without block IDs, tasks render as historical text with no toggle action.

### Emergence (Semantic Embeddings)

Future: the day view could include an emergence section: "On this day, you wrote something that connects to what you'd write 3 months later." Discovery timestamps from [EMERGENCE.md](EMERGENCE.md) would populate this section.

---

## Open Questions

1. **Deleted content.** Should the day view show deletions? "You removed the section about gap buffers from Text Editor Theory." Useful for recall ("oh right, I decided that wasn't relevant") but potentially noisy. Could be a 4th detail level or a separate section.

2. **Day view for days with no journal.** If you only edited pages (no journal entry), the 📓 section is empty. Show an empty section? Hide it? Show a message like "No journal entry"?

3. **Empty days in day-hopping.** `]d` / `[d` skip empty days. But what counts as "empty"? Any git commit? Or only days with journal content + meaningful edits (exclude auto-formatting, block ID assignment)?

4. **Day view as a page.** Should the day view be openable as a split pane alongside the editor? Or is it always a full-screen takeover? Split would let you reference the day view while editing.

---

## References

- [TIME_TRAVEL.md](TIME_TRAVEL.md) — git infrastructure layer that provides diffs and history
- [BLOCK_IDENTITY.md](BLOCK_IDENTITY.md) — stable IDs that make task actions reliable
- [LIVE_VIEWS.md](LIVE_VIEWS.md) — BQL query language for customisable sections
- [JOURNAL_REDESIGN.md](JOURNAL_REDESIGN.md) — journal archive that feeds the 📓 section
