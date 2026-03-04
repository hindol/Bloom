# Bloom 🌱 — Picker Surfaces

> Detailed wireframes for every fuzzy picker surface in Bloom.
> See GOALS.md G16 for architecture and keybinding reference.

Every picker shares a single layout component and input handler. Individual pickers only supply **data** (items, columns, preview content) — they never define their own layout or keybindings.

### Two layout variants

**Modal** (default) — centered overlay, 60% × 70% of screen, with optional preview pane:

```
┌─ [Title] ─────────────────────────────────────────────────────┐
│ > query text_                                     [filters]   │
│                                                               │
│ ▸ [label]             [middle col]          [right col]       │
│   [label]             [middle col]          [right col]       │
│   [label]             [middle col]          [right col]       │
│                                                               │
│   N of M [noun]                                               │  ← status line
├───────────────────────────────────────────────────────────────┤
│                                                               │
│   [preview of highlighted item]                               │  ← preview pane
│                                                               │
└───────────────────────────────────────────────────────────────┘
```

**Inline** (link picker only) — compact, cursor-anchored, no preview, no status line:

```
   text before cursor|
                      ┌─ Link to… ───────────────────────┐
                      │ > query_                          │
                      │                                   │
                      │ ▸ [label]                [right]  │
                      │   [label]                [right]  │
                      │                                   │
                      │ ↵ select  ⎋ cancel  + new page   │  ← hint line
                      └───────────────────────────────────┘
```

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

Triggered while typing in Insert mode. Appears inline, anchored to the cursor position. Smaller than full-screen pickers.

```
   Today I learned about |
                         ┌─ Link to… ───────────────────────┐
                         │ > rope_                           │
                         │                                   │
                         │ ▸ Text Editor Theory     #rust    │
                         │   Rope Data Structures   #rust    │
                         │   Rust Programming       #lang    │
                         │                                   │
                         │ ↵ select  ⎋ cancel  + new page   │
                         └───────────────────────────────────┘
```

| Element | Content |
|---------|---------|
| Position | Anchored below cursor, inline |
| Result row | Page title |
| Marginalia | Tags (compact) |
| Sort | Recency + fuzzy score |
| No preview | Inline picker is compact — no preview pane |
| On select (`Enter`) | Inserts `[[uuid\|title]]` at cursor |
| Create new (`+` or `Ctrl+Enter` on non-matching query) | Creates new page with typed text as title, inserts link |
| On cancel (`Escape`) | Leaves `[[` as typed text |

---

## 10. Quick Capture — `SPC j a` / `SPC j t`

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

For `SPC j t` (task variant):
```
├──────────────────────────────────────────────────────────────┤
│ ☐ Append task to journal > Review the ropey crate API_       │
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

```
┌─ Title ───────────────────────────────────────────────────────┐
│ > [query input]                          [filter] [filter]    │  ← input + filters
│                                                               │
│ ▸ [label]             [middle col]          [right col]       │  ← result list
│   [label]             [middle col]          [right col]       │
│   [label]             [middle col]          [right col]       │
│                                                               │
│   [filtered] of [total] [noun]                                │  ← status line
├───────────────────────────────────────────────────────────────┤
│                                                               │
│   [preview of highlighted item]                               │  ← preview pane
│                                                               │
└───────────────────────────────────────────────────────────────┘

Navigation:          Ctrl+N/P or Ctrl+J/K or ↑/↓    move highlight
Confirm:             Enter               select / execute
Cancel:              Escape / Ctrl+G     close picker
Action menu:         Tab                 on highlighted result
Batch select:        Tab (marks item)    then Enter to act on all
Filters:             Ctrl+T/D/L/S        add filter
Filter navigation:   Ctrl+←/→            move between filter pills
Clear filter:        Backspace on pill
Clear all filters:   Ctrl+Backspace
Clear input:         Ctrl+U
Top/bottom:          gg / G
```

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
| Templates | Template name | — | Description | templates | ✓ |
| Theme | Theme name | — | Description / `(current)` | themes | ✓ |

---

## 11. Which-Key Popup — `SPC` + timeout / pending Vim key

Not a picker — a bottom-anchored panel that appears after a brief timeout (~300ms) when a key prefix is pending. Provides progressive disclosure of available next keys.

### Leader Which-Key (after `SPC`)

**Step 1: User presses `SPC` and pauses**

```
┌──────────────────────────────────────────────────────────────┐
│                                                              │
│  # Text Editor Theory                                        │
│                                                              │
│  ## Rope Data Structure                                      │
│  Ropes are O(log n) for inserts. They use balanced           │
│  binary trees to represent text.                             │
│                                                              │
├── SPC ───────────────────────────────────────────────────────┤
│                                                              │
│  f  files         s  search        l  links        j journal │
│  t  tags          a  agenda        n  new page     w windows │
│  u  undo          r  refactor      i  insert       T toggles │
│  b  buffers       h  help          ?  all commands            │
│  SPC  commands (M-x)                                         │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

**Step 2: User presses `f` — drills into the `files` group**

```
├── SPC f ─────────────────────────────────────────────────────┤
│                                                              │
│  f  find page     r  rename        D  delete                 │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

**Step 3: User presses `f` again — `SPC f f` executes (Find Page picker opens)**

| Element | Style |
|---------|-------|
| Panel position | Bottom of screen, above status bar |
| `SPC` / `SPC f` header | `faded` prefix showing the keys typed so far |
| Key character (`f`, `s`, etc.) | `strong` (bold) — the actionable key |
| Description (`files`, `search`) | `foreground` — what it does |
| Group vs action | Groups (contain sub-keys) show as label only; actions execute immediately |
| Grid layout | Keys arranged in columns, max 4 columns wide, rows wrap as needed |
| Timeout | Popup appears ~300ms after the pending key. Typing before timeout skips the popup — the key is processed normally. |

### Vim Grammar Which-Key (after pending operator)

**User presses `d` in Normal mode and pauses:**

```
├── d ─────────────────────────────────────────────────────────┤
│                                                              │
│  motions                          text objects               │
│  w  word          b  back word    iw  inner word             │
│  e  end of word   $  end of line  aw  around word            │
│  0  start of line gg top of file  ip  inner paragraph        │
│  j  line down     k  line up      ap  around paragraph       │
│  G  end of file   %  matching     il  inner link             │
│  f… find char     t… till char    al  around link             │
│                                   i#  inner tag              │
│  operators                        a#  around tag             │
│  d  delete line (dd)              i@  inner timestamp        │
│                                   ih  inner heading section  │
│                                   ah  around heading section │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

**User presses `c` (change) and pauses — similar layout:**

```
├── c ─────────────────────────────────────────────────────────┤
│                                                              │
│  motions                          text objects               │
│  w  word          b  back word    iw  inner word             │
│  e  end of word   $  end of line  aw  around word            │
│  ...                              ...                        │
│                                                              │
│  operators                                                   │
│  c  change line (cc)                                         │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

| Element | Style |
|---------|-------|
| Operator header (`d`, `c`) | `salient` — the pending operator |
| Section labels (`motions`, `text objects`, `operators`) | `faded`, italic — category headers |
| Key character | `strong` (bold) |
| Description | `foreground` |
| `…` suffix on `f` and `t` | Indicates another key follows (e.g., `fa` = find 'a') |
| Bloom-specific objects | Highlighted subtly — `il`, `al`, `i#`, `a#`, `i@`, `ih`, `ah` appear alongside standard Vim objects |

### Behavior

- **No interaction required.** The popup is read-only. The user just presses the next key.
- **Instant dismiss.** Any keypress closes the popup and processes the key. No Escape needed.
- **Timeout only.** Popup ONLY appears after ~300ms of inactivity. Fast typists never see it — `SPC f f` typed quickly opens Find Page directly.
- **Configurable timeout.** `which_key_timeout_ms = 300` in `config.toml`.
- **Nested groups.** Leader which-key supports arbitrary depth: `SPC` → `w` → `=` (balance windows). Each level replaces the popup content.

---

## 12. Command Line — `:` mode

Triggered by pressing `:` in Normal mode. A single-line input at the bottom of the screen (same position as Vim's command line). Supports tab completion.

### Basic Command

```
┌──────────────────────────────────────────────────────────────┐
│                                                              │
│  (editor content undisturbed)                                │
│                                                              │
├──────────────────────────────────────────────────────────────┤
│ :rebuild-index_                                              │
└──────────────────────────────────────────────────────────────┘
```

### Tab Completion

**User types `:reb` and presses Tab:**

```
├──────────────────────────────────────────────────────────────┤
│ :rebuild-index_                                              │
│  rebuild-index    Rebuild the search index from scratch       │
└──────────────────────────────────────────────────────────────┘
```

If multiple matches, Tab cycles through them:

```
├──────────────────────────────────────────────────────────────┤
│ :theme_                                                      │
│  theme            Switch or cycle themes                      │
│  theme-reload     Reload theme from config                    │
└──────────────────────────────────────────────────────────────┘
```

### Command with Arguments

**`:theme` accepts a theme name. User types `:theme ` and presses Tab:**

```
├──────────────────────────────────────────────────────────────┤
│ :theme bloom-dark_                                           │
│  bloom-dark       bloom-dark-faded       bloom-light          │
│  bloom-light-faded                                            │
└──────────────────────────────────────────────────────────────┘
```

### Error Display

**User types `:nonexistent` and presses Enter:**

```
├──────────────────────────────────────────────────────────────┤
│ E: Unknown command: nonexistent                    :_        │
└──────────────────────────────────────────────────────────────┘
```

Error message shown briefly (`critical` colour), then the command line closes.

### Available Commands

| Command | Arguments | Description |
|---------|-----------|-------------|
| `:rebuild-index` | — | Rebuild SQLite index from scratch |
| `:theme` | `<name>?` | Switch theme (no arg = cycle) |
| `:theme-reload` | — | Reload current theme from config |
| `:import-logseq` | `<path>` | Import from Logseq directory |
| `:set` | `<key> <value>` | Change a config setting for this session |
| `:write` / `:w` | — | Save current buffer |
| `:quit` / `:q` | — | Close current window |
| `:wq` | — | Save and close |
| `:qa` | — | Quit all windows |

| Element | Style |
|---------|-------|
| `:` prompt | `faded` |
| Command text | `foreground` |
| Completion popup | `subtle` background, `foreground` text, highlighted match in `strong` |
| Error message | `critical` foreground |
| Position | Bottom of screen, replaces status bar while active |

### Interaction

| Binding | Action |
|---------|--------|
| `Enter` | Execute command |
| `Escape` | Cancel, close command line |
| `Tab` | Cycle through completions |
| `Shift+Tab` | Cycle completions in reverse |
| `↑` / `↓` | Command history (previous/next) |
| `Ctrl+U` | Clear command line |

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
