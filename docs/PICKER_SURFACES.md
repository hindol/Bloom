# Bloom 🌱 — Picker Surfaces

> Per-picker data definitions, columns, ranking, and preview content.
> See [WINDOW_LAYOUTS.md](WINDOW_LAYOUTS.md) for spatial layout, dimensions, and chrome styling.

Every picker shares a single layout component and input handler. Individual pickers only supply **data** (items, columns, preview content) — they never define their own layout or keybindings. There are two layout variants — **modal** (centered overlay with optional preview pane) and **inline menu** (compact, anchored to cursor) — both defined in [WINDOW_LAYOUTS.md](WINDOW_LAYOUTS.md).

### Row columns

Each result row has a **label** (left-aligned, takes remaining space) plus up to two additional columns:

| Column | Alignment | Truncation | Purpose |
|--------|-----------|------------|---------|
| Label | Left | Truncates with `…` first | Primary text (title, command name, date, matching line) |
| Middle | Left (after label gap) | Truncates after label | Secondary metadata (tags, `[+]` marker, keybinding, item count) |
| Right | Right-aligned to edge | Hidden if no space | Tertiary metadata (date, time-ago, category, note count) |

Not every picker uses all three columns — see individual wireframes below.

### Status line

Bottom of the results section, faded. Format: `{filtered} of {total} {noun}`, where **noun** varies per picker (pages, buffers, themes, etc.).

### Common properties

- `▸` marks the highlighted (selected) result.
- **Filters** are shown as pills: `[tag:rust] [date:this-week]`. `Ctrl+←/→` navigates pills, `Backspace` removes.
- **Preview** pane shows the content of the highlighted result. Auto-updates as you move.
- `Ctrl+N/P` or `Ctrl+J/K` or `↑/↓` moves the highlight. `Enter` confirms. `Escape` closes. `Tab` opens action menu. `Alt+Enter` creates a new page from the query text.
- **All pickers**, including the theme selector, share one input handler and one rendering path.

---

## 1. Find Page — `SPC f f`

Search all pages by title. The default entry point for "open something."

```
┌─ Find Page ───────────────────────────────────────────────────┐
│ > edt thry_                                                   │
│                                                               │
│ ▸ Text Editor Theory              #rust #editors     Feb 28   │
│   Threading Models                #architecture      Feb 25   │
│   Editor Architecture Notes       #design            Feb 20   │
│   Introduction to EDT             #history           Jan 15   │
│                                                               │
│   4 of 147 pages                                              │
├───────────────────────────────────────────────────────────────┤
│                                                               │
│   # Text Editor Theory                                        │
│                                                               │
│   ## Rope Data Structure                                      │
│   Ropes are O(log n) for inserts. They use balanced           │
│   binary trees to represent text...                           │
│                                                               │
│   ## Piece Table                                              │
│   Used by VS Code. Append-only, good for undo...             │
│                                                               │
└───────────────────────────────────────────────────────────────┘
```

| Element | Content |
|---------|---------|
| Result row | Page title |
| Marginalia | Tags (dimmed), last modified date |
| Sort | Recency-biased fuzzy score (recently opened pages rank higher) |
| Preview | Rendered page content (headings, first ~20 lines) |
| Actions (`Tab`) | Open, Open in split, Rename, Delete, Copy link, Copy page ID |

---

## 2. Switch Buffer — `SPC b b`

Search open buffers. For fast switching between what you're working on.

```
┌─ Switch Buffer ───────────────────────────────────────────────┐
│ > _                                                           │
│                                                               │
│ ▸ 2026-03-01 (journal)            [+]                 active  │
│   Text Editor Theory                                  3m ago  │
│   Rust Programming                [+]                 5m ago  │
│   Meeting Notes - Q1 Review                          12m ago  │
│                                                               │
│   4 open buffers                                              │
├───────────────────────────────────────────────────────────────┤
│                                                               │
│   # 2026-03-01                                                │
│                                                               │
│   - Explored ropey crate for Bloom's buffer model             │
│   - [ ] Review PR for authentication module @due(2026-03-02)  │
│   - The borrow checker finally clicked...                     │
│                                                               │
└───────────────────────────────────────────────────────────────┘
```

| Element | Content |
|---------|---------|
| Result row | Page title (journal pages show date + "journal" label) |
| Marginalia | `[+]` if unsaved changes, "active" / time since last focused |
| Sort | Most recently focused first |
| Preview | Current buffer content at current scroll position |
| Actions (`Tab`) | Open, Open in split, Close buffer, Save, Diff unsaved changes |

---

## 3. Full-Text Search — `SPC s s`

Search note *contents* across all files. Each result is a matching line, not a page.

```
┌─ Search ──────────────────────────────────────────────────────┐
│ > rope data structure_                   [tag:rust]           │
│                                                               │
│ ▸ Ropes are O(log n) for inserts         Text Editor Theory   │
│   Rope vs gap buffer tradeoffs           2026-02-28 (journal) │
│   "rope" crate is the standard in Rust   Rust Programming     │
│   Xi Editor used a rope-based CRDT       2026-02-20 (journal) │
│                                                               │
│   4 matches across 3 pages                                    │
├───────────────────────────────────────────────────────────────┤
│                                                               │
│   ## Rope Data Structure                                      │
│                                                               │
│   ❯ Ropes are O(log n) for inserts. They use balanced        │
│     binary trees to represent text. Each leaf holds a         │
│     string fragment, and internal nodes store the weight      │
│     (character count of left subtree).                        │
│                                                               │
│   Good for large files. Used by Xi Editor and Zed.            │
│                                                               │
└───────────────────────────────────────────────────────────────┘
```

| Element | Content |
|---------|---------|
| Result row | Matching line (truncated), highlighted match |
| Marginalia | Source page title |
| Sort | Relevance score from FTS5 |
| Preview | Surrounding context (±5 lines) with match highlighted (`❯`) |
| Filters | `Ctrl+T` tag, `Ctrl+D` date range, `Ctrl+L` links-to, `Ctrl+S` task status |
| Actions (`Tab`) | Open at line, Open in split, Copy block link |

---

## 4. Search Journal Entries — `SPC s j`

Browse and search journal pages by date. Sorted chronologically.

```
┌─ Journal ─────────────────────────────────────────────────────┐
│ > feb_                                                        │
│                                                               │
│ ▸ 2026-02-28                      5 items  #rust #editors     │
│   2026-02-25                      3 items  #architecture      │
│   2026-02-20                      8 items  #rust #crdt        │
│   2026-02-14                      4 items  #project-bloom     │
│   2026-02-10                      2 items                     │
│                                                               │
│   5 of 28 journal entries                                     │
├───────────────────────────────────────────────────────────────┤
│                                                               │
│   # 2026-02-28                                                │
│                                                               │
│   - Explored ropey crate for Bloom's buffer model             │
│   - Read about Rope data structures — O(log n) insert         │
│   - [ ] Review PR for authentication module                   │
│   - The Rust borrow checker finally clicked for me            │
│   - Need to buy groceries                                     │
│                                                               │
└───────────────────────────────────────────────────────────────┘
```

| Element | Content |
|---------|---------|
| Result row | Date |
| Marginalia | Item count, tags used that day |
| Sort | Reverse chronological (newest first) |
| Preview | Full journal content |
| Actions (`Tab`) | Open, Open in split |

---

## 5. Search Tags — `SPC s t`

Browse all tags. On select, transitions to the full-text search picker pre-filtered by that tag — maintaining UI consistency (every picker ultimately leads to opening an editable page).

**Step 1: Pick a tag**
```
┌─ Tags ────────────────────────────────────────────────────────┐
│ > ru_                                                         │
│                                                               │
│ ▸ #rust                                            23 notes   │
│   #rust-crates                                      5 notes   │
│   #running                                          2 notes   │
│                                                               │
│   3 of 34 tags                                                │
├───────────────────────────────────────────────────────────────┤
│                                                               │
│   #rust — 23 notes                                            │
│                                                               │
│   Text Editor Theory              Feb 28                      │
│   Rust Programming                Feb 25                      │
│   Ownership and Borrowing         Feb 20                      │
│   2026-02-28 (journal)            Feb 28                      │
│   ...                                                         │
│                                                               │
└───────────────────────────────────────────────────────────────┘
```

**Step 2: Press `Enter` → seamlessly transitions to search with tag filter applied**
```
┌─ Search ──────────────────────────────────────────────────────┐
│ > _                                              [tag:rust]   │
│                                                               │
│ ▸ Text Editor Theory              #rust #editors     Feb 28   │
│   Rust Programming                #rust #lang        Feb 25   │
│   Ownership and Borrowing         #rust              Feb 20   │
│   2026-02-28 (journal)            #rust #editors     Feb 28   │
│   2026-02-14 (journal)            #rust              Feb 14   │
│                                                               │
│   23 of 23 (filtered by #rust)                                │
├───────────────────────────────────────────────────────────────┤
│                                                               │
│   # Text Editor Theory                                        │
│                                                               │
│   ## Rope Data Structure                                      │
│   Ropes are O(log n) for inserts...                           │
│                                                               │
└───────────────────────────────────────────────────────────────┘
```

From here, the user can further narrow by typing, add more filters (`Ctrl+D` for date), and `Enter` opens an editable page — fully consistent with every other picker.

| Element | Content |
|---------|---------|
| **Step 1** | |
| Result row | Tag name |
| Marginalia | Note count |
| Sort | Frequency (most used first), then alphabetical |
| Preview | List of pages using this tag, with dates |
| On select (`Enter`) | Transitions to Search picker with `[tag:X]` filter pre-applied |
| Actions (`Tab`) | Rename tag (across all files), Delete tag (across all files) |
| **Step 2** | |
| Behavior | Standard Search picker (§3) with tag filter pre-populated |

---

## 6. Backlinks — `SPC s l`

All pages that link TO the current page.

```
┌─ Backlinks to: Text Editor Theory ────────────────────────────┐
│ > _                                                           │
│                                                               │
│ ▸ 2026-02-28 (journal)            "Rope data structure is…"  │
│   2026-02-25 (journal)            "Piece table used by VS…"  │
│   2026-02-20 (journal)            "Read: Xi Editor retros…"  │
│   Rust Programming                "See [[Text Editor Theor…" │
│   CRDT Notes                      "Related to [[Text Edit…"  │
│                                                               │
│   5 backlinks                                                 │
├───────────────────────────────────────────────────────────────┤
│                                                               │
│   # 2026-02-28                                                │
│                                                               │
│   ...                                                         │
│   - Read about ❯ Rope data structure — O(log n) insert       │
│     makes them ideal for [[Text Editor Theory]].              │
│   ...                                                         │
│                                                               │
└───────────────────────────────────────────────────────────────┘
```

| Element | Content |
|---------|---------|
| Result row | Source page title |
| Marginalia | Truncated context around the link |
| Sort | Reverse chronological |
| Preview | Source page content with the linking line highlighted (`❯`) |
| Actions (`Tab`) | Open, Open in split, Copy block link |

---

## 7. Unlinked Mentions — `SPC s u`

Text matches for the current page's title that AREN'T explicit links yet. The discovery zone.

```
┌─ Unlinked Mentions of: Text Editor Theory ────────────────────┐
│ > _                                                           │
│                                                               │
│ ▸ 2026-02-14 (journal)            "started exploring text…"  │
│   2026-01-30 (journal)            "text editor for large …"  │
│   Programming Languages           "relates to text editor…"   │
│                                                               │
│   3 unlinked mentions                                         │
├───────────────────────────────────────────────────────────────┤
│                                                               │
│   # 2026-02-14                                                │
│                                                               │
│   ...                                                         │
│   - Started exploring how ❯ text editors handle large         │
│     files. Need to look into rope vs gap buffer.              │
│   ...                                                         │
│                                                               │
│   [Tab: Promote to link | Ignore | Open | Open in split]     │
└───────────────────────────────────────────────────────────────┘
```

| Element | Content |
|---------|---------|
| Result row | Source page title |
| Marginalia | Truncated context around the text match |
| Sort | Reverse chronological |
| Preview | Source page with match highlighted |
| Batch select | `Tab` marks items, `Enter` promotes all marked to explicit `[[links]]` |
| Actions (`Tab` on single) | **Promote to link**, Ignore (permanently dismiss this mention), Open, Open in split |

---

## 8. All Commands — `SPC SPC`

Emacs `M-x` equivalent. Every Bloom command is searchable.

```
┌─ Commands ────────────────────────────────────────────────────┐
│ > split_                                                      │
│                                                               │
│ ▸ Window: Vertical Split              SPC w v        window   │
│   Window: Horizontal Split            SPC w s        window   │
│   Refactor: Split Page                SPC r s        refactor │
│                                                               │
│   3 of 87 commands                                            │
├───────────────────────────────────────────────────────────────┤
│                                                               │
│   Window: Vertical Split                                      │
│                                                               │
│   Split the current window vertically, creating a new         │
│   window to the right. The new window shows the same          │
│   buffer. Use SPC b b or SPC f f to open a different page.    │
│                                                               │
└───────────────────────────────────────────────────────────────┘
```

| Element | Content |
|---------|---------|
| Result row | Command name (human-readable) |
| Marginalia | Keybinding (if any), category |
| Sort | Frequency of use (most-used first), then fuzzy score |
| Preview | Command description / help text |
| On select | Executes the command |

---

## 9. Inline Link Picker — `[[` trigger

Triggered while typing in Insert mode. Appears inline, anchored to the cursor position. Uses the shared **inline menu** component.

```
   Today I learned about |
                         ┌───────────────────────────────┐
                         │ ▸ Text Editor Theory    #rust  │
                         │   Rope Data Structures  #rust  │
                         │   Rust Programming      #lang  │
                         │                                │
                         │ ↵ select  ⎋ cancel  + new     │
                         └───────────────────────────────┘
```

| Element | Content |
|---------|---------|
| Trigger | `[[` typed in Insert mode |
| Anchor | Below cursor |
| Data | Page titles from index, sorted by recency + fuzzy score |
| Right column | Tags (compact) |
| On select (`Enter`/`Tab`) | Inserts `[[uuid\|title]]` at cursor |
| Create new (`Ctrl+Enter`) | Creates new page with query as title, inserts link |
| On cancel (`Escape`) | Leaves `[[` as typed text |

---

## 10. Inline Tag Completion — `#` trigger

Triggered while typing in Insert mode. Same inline menu, anchored to cursor. Completes tag names from the index.

```
   Reviewed the ropey crate API #ru|
                                    ┌────────────────────┐
                                    │ ▸ rust          12  │
                                    │   ruby           3  │
                                    │   runtime        1  │
                                    └────────────────────┘
```

| Element | Content |
|---------|---------|
| Trigger | `#` followed by a letter in Insert mode |
| Anchor | Below cursor |
| Data | All tags from index, fuzzy-matched against text after `#` |
| Right column | Usage count |
| On select (`Enter`/`Tab`) | Completes the tag text (e.g. `ru` → `rust`), cursor moves past tag |
| On cancel (`Escape`) | Leaves partial text as-is |

---

## 11. Command Completion — `:` command line

Appears immediately when typing in Command mode. Anchored above the status bar. Replaces the which-key timeout-based hint grid for commands.

```
  ┌──────────────────────────────────────┐
  │ ▸ theme        switch theme          │
  │   theme-list   list all themes       │
  └──────────────────────────────────────┘
 ────────────────────────────────────────────
 :th█
```

After `Tab` fills `:theme `, argument completion kicks in:

```
  ┌──────────────────────────────────────┐
  │ ▸ bloom-dark                         │
  │   bloom-dark-faded                   │
  │   bloom-light                        │
  │   parchment                          │
  │   moss                               │
  └──────────────────────────────────────┘
 ────────────────────────────────────────────
 :theme █
```

| Element | Content |
|---------|---------|
| Trigger | Entering Command mode (`:`) |
| Anchor | Above status bar |
| Data | Registered ex commands, prefix-filtered. After a known command + space, switches to argument completions (e.g. theme names). |
| Right column | Description (commands) or — (arguments) |
| `Tab` | Fills selected completion into command line (does not execute) |
| `Enter` | Executes whatever is in the command line |
| On cancel (`Escape`) | Exits Command mode |

---

## 12. Add Tag — `SPC t a`

Inline menu anchored above the status bar. Adds a tag to the current page's frontmatter.

```
 ────────────────────────────────────────────
 Tag: ru█
  ┌──────────────────────────────────────┐
  │ ▸ rust                           12  │
  │   ruby                            3  │
  └──────────────────────────────────────┘
```

| Element | Content |
|---------|---------|
| Trigger | `SPC t a` |
| Anchor | Above status bar (single-line input) |
| Data | All tags from index, fuzzy-matched |
| Right column | Usage count |
| On select (`Enter`) | Adds tag to current page frontmatter |
| On cancel (`Escape`) | Closes without changes |

---

## 13. Remove Tag — `SPC t r`

Same inline menu, but sourced from the current page's tags only.

```
 ────────────────────────────────────────────
 Remove tag:
  ┌──────────────────────────────────────┐
  │ ▸ rust                               │
  │   editors                            │
  │   bloom                              │
  └──────────────────────────────────────┘
```

| Element | Content |
|---------|---------|
| Trigger | `SPC t r` |
| Anchor | Above status bar |
| Data | Current page's frontmatter tags (no fuzzy filter — list is short) |
| Right column | — |
| On select (`Enter`) | Removes tag from current page frontmatter |
| On cancel (`Escape`) | Closes without changes |

---

## 14. Quick Capture — `SPC j a` / `SPC x a`

Not a full picker — a minimal single-line input anchored to the bottom of the screen.

```
┌──────────────────────────────────────────────────────────────┐
│                                                              │
│   (current buffer content remains visible and undisturbed)   │
│                                                              │
│                                                              │
│                                                              │
├──────────────────────────────────────────────────────────────┤
│ 📓 Append to journal > Rope data structure is ideal for…_   │
└──────────────────────────────────────────────────────────────┘
```

For `SPC x a` (task variant):
```
├──────────────────────────────────────────────────────────────┤
│ - [ ] Append task to journal > Review the ropey crate API_       │
└──────────────────────────────────────────────────────────────┘
```

| Element | Content |
|---------|---------|
| Position | Bottom bar, single line |
| On `Enter` | Appends line (or `- [ ] line`) to today's journal. Shows brief confirmation: "✓ Added to Mar 1 journal" |
| On `Escape` | Cancels, returns to buffer |
| Buffer | Completely undisturbed — no context switch |
| `[[` trigger | Works inside quick capture too — can link while capturing |

---

## Shared Anatomy Summary

Every modal picker has: a query input with filter pills, a result list with up to three columns, a status line (`{filtered} of {total} {noun}`), and an optional preview pane. For spatial layout and dimensions, see [WINDOW_LAYOUTS.md](WINDOW_LAYOUTS.md).

**Navigation:**

| Binding | Action |
|---------|--------|
| `Ctrl+N/P` or `Ctrl+J/K` or `↑/↓` | Move highlight |
| `Enter` | Select / execute |
| `Escape` / `Ctrl+G` | Close picker |
| `Tab` | Action menu on highlighted result (or batch-mark) |
| `Ctrl+T/D/L/S` | Add filter |
| `Ctrl+←/→` | Move between filter pills |
| `Backspace` on pill | Clear filter |
| `Ctrl+Backspace` | Clear all filters |
| `Ctrl+U` | Clear input |
| `gg` / `G` | Top / bottom |

### Column usage by picker

| Picker | Label | Middle | Right | Status noun | Preview |
|--------|-------|--------|-------|-------------|---------|
| Find Page | Page title | Tags | Date | pages | ✓ |
| Switch Buffer | Title | `[+]` marker | time ago | open buffers | ✓ |
| Search | Matching line | — | Source page | matches | ✓ |
| Journal | Date | Item count | Tags | journal entries | ✓ |
| Tags | `#tag-name` | — | Note count | tags | ✓ |
| Backlinks | Source page | — | Context quote | backlinks | ✓ |
| Unlinked Mentions | Source page | — | Context quote | unlinked mentions | ✓ |
| All Commands | Command name | Keybinding | Category | commands | ✓ |
| Inline Link | Page title | — | Tags | (hint line) | — |
| Inline Tag | Tag name | — | Usage count | — | — |
| Command Completion | Command name | — | Description | — | — |
| Add Tag | Tag name | — | Usage count | — | — |
| Remove Tag | Tag name | — | — | — | — |
| Templates | Template name | — | Description | templates | ✓ |
| Theme | Theme name | — | Description / `(current)` | themes | ✓ |

---

## Ranking & Large Vault Ergonomics

With 1000+ pages, raw fuzzy matching alone produces too many results. Every modal picker uses a **multi-signal ranking** pipeline that combines fuzzy score with contextual signals. The goal: the page you want is almost always in the first 3 results — even before you type.

### Zero-query state: curated, not overwhelming

When a picker opens with an empty query (or a restored query), results are not "all 1000 pages sorted alphabetically." Instead, each picker shows a **curated default list**:

| Picker | Zero-query results |
|--------|-------------------|
| Find Page | Last 10 recently accessed pages |
| Switch Buffer | Open buffers, most recently focused first |
| Search | Restored previous query results, or "Type to search…" |
| Journal | Last 30 journal entries, reverse chronological |
| Tags | All tags, sorted by usage frequency |

As the user types, the curated list transitions smoothly to full fuzzy search results. The transition is seamless — the same ranking pipeline handles both states.

### Frecency: frequency × recency

The primary ranking signal for Find Page is **frecency** — a score combining how often and how recently a page was accessed. Inspired by Firefox's awesome bar:

```
frecency_score = Σ (weight × recency_bucket)

Recency buckets:
  Accessed in the last 4 hours  → weight × 100
  Accessed in the last day      → weight × 70
  Accessed in the last week     → weight × 50
  Accessed in the last month    → weight × 30
  Accessed longer ago           → weight × 10

Weight: each access adds 1 visit. Pages visited 5× this week rank
higher than pages visited 1× today.
```

Frecency scores are stored in the SQLite index (`page_access` table) and updated on every `open_page`. The score is cheap to compute — a single `UPDATE` on page open, a `JOIN` on picker query.

### Multi-signal ranking pipeline

When a query is present, the final score combines multiple signals:

```
final_score = fuzzy_score(query, title)
            + 0.3 × frecency_normalized
            + word_boundary_bonus
            + exact_prefix_bonus
```

| Signal | Weight | Purpose |
|--------|--------|---------|
| Fuzzy score | 1.0 | Core relevance — how well the query matches |
| Frecency | 0.3 | Personal relevance — pages you use more rank higher |
| Word boundary | bonus | "ed th" → "**Ed**itor **Th**eory" ranks above "r**ed** pa**th**" |
| Exact prefix | bonus | "Rust" → "**Rust** Notes" ranks above "T**rust** Issues" |

The 0.3 weight for frecency means: a perfect fuzzy match always wins, but among similarly-scored results, your frequently-used pages bubble up.

### Search picker: FTS5 candidates → fuzzy re-rank

Search (`SPC s s`) uses a two-phase pipeline optimized for large vaults:

1. **Phase 1 — FTS5 prefix candidates**: Query words become prefix terms (`"mem pat"` → `"mem* OR pat*"`). FTS5 returns matching pages + content from SQLite. This narrows 1000 pages to ~20 candidates in <1ms.

2. **Phase 2 — Per-word fuzzy scoring**: Each candidate's content lines are scored with `fuzzy_words_score()` — every query word is fuzzy-matched independently against the line. `"mem pat re"` matches `"Deep research on memory usage patterns"` because `mem→memory`, `pat→patterns`, `re→research`.

Results are ranked by fuzzy score, capped at 500 for responsiveness. The search query is persisted across sessions — reopening `SPC s s` restores the last query with select-all highlighting (typing replaces, arrows preserve).

### Preview pane: lazy content loading

Preview panes show content for the highlighted result. For index-backed pickers, content is loaded lazily:

- **Find Page**: First ~20 lines from the page file, with semantic highlighting
- **Search**: ±5 lines of context around the matching line, with `❯` marker on the match
- **Journal**: Full journal content with semantic highlighting
- **Backlinks**: Source page with the linking line highlighted

Preview content is loaded on highlight change (not upfront for all results). For pages already in a buffer, preview reads from the in-memory rope. For others, a single file read from disk (the OS page cache makes this fast after indexing).

---

## Inline Menu — Shared Data Model

All inline menus (§9–§13) share one rendering component and one input handler. For layout dimensions and styling, see the Inline Menu section in [WINDOW_LAYOUTS.md](WINDOW_LAYOUTS.md).

Each inline menu supplies: a list of items with a **label** and an optional **right column**, plus an **anchor point** (cursor position or command line). Navigation: `↓`/`Ctrl+n` next, `↑`/`Ctrl+p` previous. `Tab` fills input (no execute), `Enter` confirms, `Escape` closes.

---

## 13. Theme Selector — `SPC T t`

A picker that lists all available themes with **live preview**: as the highlight moves, the entire editor re-renders in the highlighted theme. Pressing `Enter` confirms; `Escape` reverts to the previous theme.

```
┌─ Theme ────────────────────────────────────────────────────────┐
│ > _                                                            │
│                                                                │
│ ▸ Bloom Dark                                        (current)  │
│   Bloom Dark Faded                          softer, Nord-like  │
│   Bloom Light                              warm white, strong  │
│   Bloom Light Faded                         cool, muted light  │
│                                                                │
│   4 themes                                                     │
├────────────────────────────────────────────────────────────────┤
│                                                                │
│   ## Preview                                                   │
│                                                                │
│   - [ ] Sample task @due(2026-03-05)                           │
│   - [x] Completed task                                         │
│   See [[abc123|Text Editor Theory]] for background.            │
│   #rust #editors                                               │
│                                                                │
└────────────────────────────────────────────────────────────────┘
```

The preview pane shows a fixed sample document rendered in the highlighted theme, so the user can compare colours without switching to their own content.

**Live preview behavior:**
- Moving the highlight immediately applies the highlighted theme to the **entire editor** — the picker itself, the panes behind it, all chrome.
- This gives a true WYSIWYG preview, not just a swatch.
- `Enter` confirms the selection, writes `theme.name` to `config.toml`.
- `Escape` reverts to the theme that was active when the picker opened.

| Element | Style |
|---------|-------|
| Current theme marker `(current)` | `faded` |
| Theme description | `faded` |
| Preview content | Rendered with the highlighted theme's palette |

Standard picker navigation applies (see **Shared Anatomy Summary** above). `Enter` persists the selection to `config.toml`; `Escape` reverts to the theme that was active when the picker opened.
