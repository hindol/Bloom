# Journal Redesign 📓

> Daily journal files in `journal/`, excluded from the page picker, navigated by time.
> Status: **Draft** — exploratory, not committed.
> See also: [TIME_TRAVEL.md](TIME_TRAVEL.md) for git-backed history and the context strip component.

---

## The Problem

Today Bloom creates one file per day in `journal/`: `2026-03-08.md`, `2026-03-09.md`, etc. This has two problems:

1. **Date-named files are meaningless in a picker.** `SPC f f` shows "2026-03-08" alongside "Text Editor Theory" — one evokes an idea, the other evokes nothing. Journal files pollute the page namespace with noise.

2. **Users think about files instead of writing.** "Which daily file was that thought in?" is the wrong question. You should be thinking about *when* or *what*, not *which file*.

---

## The Design

### One File Per Day (unchanged from G14)

The journal file layout stays the same as [GOALS.md G14](../GOALS.md):

```
~/bloom/
├── journal/
│   ├── 2026-03-09.md       ← today
│   ├── 2026-03-08.md
│   ├── 2026-03-06.md       ← March 7 had no entries, no file
│   └── ...
├── pages/
└── ...
```

- `SPC j j` opens today's journal (`journal/2026-03-09.md`). Created lazily — the file appears only after the first edit triggers auto-save.
- After midnight, `SPC j j` targets the new date. No rotation, no file moves — a new file is simply created for the new day.
- Journal pages are regular Bloom pages with frontmatter, UUID, and full link/tag/task support.

### Picker Hierarchy

Three entry points into one picker component, differing only in scope:

| Keybinding | Scope | Shows |
|-----------|-------|-------|
| `SPC f f` | All files | Pages + journal entries |
| `SPC p p` | Pages only | Named pages in `pages/` |
| `SPC j j` | Journal only | Journal entries in `journal/`, newest first |

`SPC p p` and `SPC j j` are pre-filtered views of the same picker that powers `SPC f f`. The underlying component is identical — only the file scope differs.

**Quick filters in `SPC f f`:** typing `/p` at the start of the query narrows to pages; `/j` narrows to journals. These filters only work in the "all files" picker — in `SPC p p` and `SPC j j` the scope is fixed.

**Journal entry display:** In any picker, journal entries are displayed with a human-readable date ("Saturday, March 8, 2026") instead of the raw filename. Today's journal appears in `SPC j j` even if not yet created on disk (lazy creation), shown at the top.

This replaces the earlier "exclude journal from SPC f f" approach. Nothing is excluded — users choose their scope.

> **Note:** This changes `SPC f f` from "Find page" (current G14) to "Find file." `SPC p p` provides the old `SPC f f` behavior. GOALS.md, KEYBINDINGS.md, and PICKER_SURFACES.md will need updates when this design is adopted.

### Quick Capture and Direct Access

| Keybinding | Action |
|-----------|--------|
| `SPC j t` | Open today's journal (t = today) |
| `SPC j a` | Quick-append a line to today's journal (without leaving current buffer) |
| `SPC x a` | Quick-append a task to today's journal |

> **Note:** `SPC j t` replaces the old `SPC j j` (open today). `SPC x a` replaces the old `SPC j t` (append task) — tasks move to the `SPC x` prefix (mnemonic: checkbox). Tags stay at `SPC t`. These are keybinding changes to GOALS.md G14.

---

## Navigation

The journal is navigated by **time**, not by filename. Three mechanisms, from lightweight to spatial:

### Flow

```
SPC j t → journal/2026-03-09.md (today, editable)
           │
           SPC j p / SPC j n  →  prev / next journal day
           │
           context strip shows adjacent journal days at bottom
           │
           Enter on strip  →  journal calendar
           SPC j c          →  journal calendar (direct entry)
           │
           type date in calendar  →  jump to that day
           Enter on a day        →  open that day's journal

SPC j j → journal picker (search/browse all journal entries)
```

### Day-Hopping (`SPC j p` / `SPC j n`)

From any journal, these keys hop to the previous/next day **that has a journal file** — empty days are skipped. The context strip from [TIME_TRAVEL.md](TIME_TRAVEL.md) appears at the bottom showing adjacent days:

<div style="font-family: 'JetBrains Mono', 'Fira Code', 'Consolas', monospace; font-size: 13px; line-height: 1.5; background: #141414; color: #EBE9E7; border-radius: 6px; overflow: hidden; max-width: 680px; margin: 16px 0;">
  <!-- Journal content -->
  <div style="padding: 12px 16px;">
    <div style="color: #A3A3A3; font-size: 11px; margin-bottom: 8px;">journal/2026-03-08.md</div>
    <div><span style="color: #EBE9E7;">- </span>Explored ropey crate for buffer model</div>
    <div><span style="color: #EBE9E7;">- </span>Read about Xi Editor architecture</div>
    <div><span style="color: #EBE9E7;">- </span><span style="color: #F2DA61;">[ ]</span> Review gap buffer tradeoffs <span style="color: #A3A3A3;">@due</span><span style="color: #A3A3A3; opacity: 0.5;">(</span>03-10<span style="color: #A3A3A3; opacity: 0.5;">)</span></div>
    <div><span style="color: #EBE9E7;">- </span><span style="color: #62C554; text-decoration: line-through;">[x]</span><span style="color: #A3A3A3; text-decoration: line-through;"> Compare with PieceTable</span></div>
    <div><span style="color: #A3A3A3;">#rust #editors #data-structures</span></div>
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
  <!-- Status bar (JRNL mode) -->
  <div style="background: #F2DA61; color: #141414; padding: 3px 16px; display: flex; justify-content: space-between; font-size: 12px;">
    <div>
      <span style="font-weight: bold;">JRNL</span>
      <span style="opacity: 0.4;"> │ </span>
      <span>Saturday, March 8, 2026</span>
    </div>
    <div style="opacity: 0.7;">↵:calendar  SPC j p/n</div>
  </div>
</div>

| Key | Action |
|-----|--------|
| `SPC j n` | Jump to next day with a journal |
| `SPC j p` | Jump to previous day with a journal |
| `j` / `k` | Scroll within the current day |
| `Enter` | On a page name — jump to that page. On context strip — expand to calendar. |
| `x` | On a task — toggle it in the source file |
| `o` | On a page — open in a split |
| `q` | Close journal, return to previous buffer |

`SPC j p` / `SPC j n` work from any buffer — they open the journal for the day before/after the **most recently viewed journal day** (defaulting to today if no journal has been viewed this session).

### Journal Calendar (`SPC j c` or `Enter` on context strip)

The calendar grid allows spatial date navigation. Shows only journal entries — for vault-wide activity by date, see [TIME_TRAVEL.md § Day Activity](TIME_TRAVEL.md#day-activity) (`SPC H c`). Opens as a panel above the status bar:

<div style="font-family: 'JetBrains Mono', 'Fira Code', 'Consolas', monospace; font-size: 13px; line-height: 1.5; background: #141414; color: #EBE9E7; border-radius: 6px; overflow: hidden; max-width: 680px; margin: 16px 0;">
  <!-- Editor content (unchanged) -->
  <div style="padding: 12px 16px; color: #A3A3A3;">
    <div><span style="opacity: 0.5;">##</span> <span style="color: #F4BF4F; font-weight: bold;">Rope Data Structure</span></div>
    <div style="color: #EBE9E7;">Ropes are O(log n) for inserts. They use balanced binary trees.</div>
    <div>&nbsp;</div>
  </div>
  <!-- Calendar grid -->
  <div style="border-top: 1px solid #37373E; padding: 12px 16px;">
    <div style="text-align: center; margin-bottom: 8px;">
      <span style="color: #EBE9E7; font-weight: bold;">March 2026</span>
    </div>
    <div style="color: #A3A3A3; text-align: center; letter-spacing: 0.5px;">
      <div style="margin-bottom: 4px;"><span style="display: inline-block; width: 36px;">Mo</span><span style="display: inline-block; width: 36px;">Tu</span><span style="display: inline-block; width: 36px;">We</span><span style="display: inline-block; width: 36px;">Th</span><span style="display: inline-block; width: 36px;">Fr</span><span style="display: inline-block; width: 36px;">Sa</span><span style="display: inline-block; width: 36px;">Su</span></div>
      <div><span style="display: inline-block; width: 36px;"></span><span style="display: inline-block; width: 36px;"></span><span style="display: inline-block; width: 36px;"></span><span style="display: inline-block; width: 36px;"></span><span style="display: inline-block; width: 36px;"></span><span style="display: inline-block; width: 36px;"></span><span style="display: inline-block; width: 36px; color: #EBE9E7;">1</span></div>
      <div><span style="display: inline-block; width: 36px;">2</span><span style="display: inline-block; width: 36px;">3</span><span style="display: inline-block; width: 36px;">4</span><span style="display: inline-block; width: 36px;">5</span><span style="display: inline-block; width: 36px;"><span style="color: #F4BF4F;">◆</span><span style="color: #EBE9E7;">6</span></span><span style="display: inline-block; width: 36px;">7</span><span style="display: inline-block; width: 36px; background: #7A9EFF; color: #141414; border-radius: 3px; font-weight: bold;">8</span></div>
      <div><span style="display: inline-block; width: 36px;">9</span><span style="display: inline-block; width: 36px;">10</span><span style="display: inline-block; width: 36px;">11</span><span style="display: inline-block; width: 36px;"><span style="color: #F4BF4F;">◆</span><span style="color: #EBE9E7;">12</span></span><span style="display: inline-block; width: 36px;">13</span><span style="display: inline-block; width: 36px;">14</span><span style="display: inline-block; width: 36px;">15</span></div>
      <div><span style="display: inline-block; width: 36px;">16</span><span style="display: inline-block; width: 36px;">17</span><span style="display: inline-block; width: 36px;">18</span><span style="display: inline-block; width: 36px;">19</span><span style="display: inline-block; width: 36px;">20</span><span style="display: inline-block; width: 36px;">21</span><span style="display: inline-block; width: 36px;">22</span></div>
      <div><span style="display: inline-block; width: 36px;">23</span><span style="display: inline-block; width: 36px;">24</span><span style="display: inline-block; width: 36px;">25</span><span style="display: inline-block; width: 36px;"><span style="color: #F4BF4F;">◆</span><span style="color: #EBE9E7;">26</span></span><span style="display: inline-block; width: 36px;">27</span><span style="display: inline-block; width: 36px;">28</span><span style="display: inline-block; width: 36px;">29</span></div>
      <div><span style="display: inline-block; width: 36px;">30</span><span style="display: inline-block; width: 36px;">31</span></div>
    </div>
    <div style="text-align: center; margin-top: 8px; color: #A3A3A3; font-size: 12px;">3 journal entries this month</div>
  </div>
  <!-- Status bar (JRNL mode) -->
  <div style="background: #F2DA61; color: #141414; padding: 3px 16px; display: flex; justify-content: space-between; font-size: 12px;">
    <div>
      <span style="font-weight: bold;">JRNL</span>
      <span style="opacity: 0.4;"> │ </span>
      <span>March 2026</span>
    </div>
    <div style="opacity: 0.7;">h/l j/k H/L  ↵:open day  Esc:close</div>
  </div>
</div>

- Selected day (`8`) shown with `popout` background (inverse highlight)
- `◆` = days with a journal file in `journal/`
- Today is marked even if the file hasn't been created yet (lazy creation)
- `Enter` on a day with `◆` opens that journal. On a day without `◆`, no action.
- `Esc` closes the calendar

| Key | Action |
|-----|--------|
| `h` / `l` | Previous / next day |
| `j` / `k` | Next / previous week |
| `H` / `L` | Previous / next month |
| `Enter` | Open journal for selected day (only if journal exists) |
| `/` | Type a date to jump directly (e.g., `2026-01-15` or `jan 15`) |
| `q` / `Esc` | Close calendar |

### Expanded Calendar (from context strip)

Pressing `Enter` on the context strip while viewing a journal day re-opens the calendar. The journal compresses to a compact summary above:

<div style="font-family: 'JetBrains Mono', 'Fira Code', 'Consolas', monospace; font-size: 13px; line-height: 1.5; background: #141414; color: #EBE9E7; border-radius: 6px; overflow: hidden; max-width: 680px; margin: 16px 0;">
  <!-- Compact journal summary -->
  <div style="padding: 8px 16px; color: #A3A3A3; font-size: 12px;">
    <span style="color: #EBE9E7;">Mar 8</span> — Explored ropey · Read about Xi Editor · ...
  </div>
  <!-- Calendar grid -->
  <div style="border-top: 1px solid #37373E; padding: 12px 16px;">
    <div style="text-align: center; margin-bottom: 8px;">
      <span style="color: #EBE9E7; font-weight: bold;">March 2026</span>
    </div>
    <div style="color: #A3A3A3; text-align: center; letter-spacing: 0.5px;">
      <div style="margin-bottom: 4px;"><span style="display: inline-block; width: 36px;">Mo</span><span style="display: inline-block; width: 36px;">Tu</span><span style="display: inline-block; width: 36px;">We</span><span style="display: inline-block; width: 36px;">Th</span><span style="display: inline-block; width: 36px;">Fr</span><span style="display: inline-block; width: 36px;">Sa</span><span style="display: inline-block; width: 36px;">Su</span></div>
      <div><span style="display: inline-block; width: 36px;"></span><span style="display: inline-block; width: 36px;"></span><span style="display: inline-block; width: 36px;"></span><span style="display: inline-block; width: 36px;"></span><span style="display: inline-block; width: 36px;"></span><span style="display: inline-block; width: 36px;"></span><span style="display: inline-block; width: 36px; color: #EBE9E7;">1</span></div>
      <div><span style="display: inline-block; width: 36px;">2</span><span style="display: inline-block; width: 36px;">3</span><span style="display: inline-block; width: 36px;">4</span><span style="display: inline-block; width: 36px;">5</span><span style="display: inline-block; width: 36px;"><span style="color: #F4BF4F;">◆</span><span style="color: #EBE9E7;">6</span></span><span style="display: inline-block; width: 36px;">7</span><span style="display: inline-block; width: 36px; background: #7A9EFF; color: #141414; border-radius: 3px; font-weight: bold;">8</span></div>
      <div><span style="display: inline-block; width: 36px;">9</span><span style="display: inline-block; width: 36px;">10</span><span style="display: inline-block; width: 36px;">11</span><span style="display: inline-block; width: 36px;"><span style="color: #F4BF4F;">◆</span><span style="color: #EBE9E7;">12</span></span><span style="display: inline-block; width: 36px;">13</span><span style="display: inline-block; width: 36px;">14</span><span style="display: inline-block; width: 36px;">15</span></div>
      <div><span style="display: inline-block; width: 36px;">16</span><span style="display: inline-block; width: 36px;">17</span><span style="display: inline-block; width: 36px;">18</span><span style="display: inline-block; width: 36px;">19</span><span style="display: inline-block; width: 36px;">20</span><span style="display: inline-block; width: 36px;">21</span><span style="display: inline-block; width: 36px;">22</span></div>
      <div><span style="display: inline-block; width: 36px;">23</span><span style="display: inline-block; width: 36px;">24</span><span style="display: inline-block; width: 36px;">25</span><span style="display: inline-block; width: 36px;"><span style="color: #F4BF4F;">◆</span><span style="color: #EBE9E7;">26</span></span><span style="display: inline-block; width: 36px;">27</span><span style="display: inline-block; width: 36px;">28</span><span style="display: inline-block; width: 36px;">29</span></div>
      <div><span style="display: inline-block; width: 36px;">30</span><span style="display: inline-block; width: 36px;">31</span></div>
    </div>
  </div>
  <!-- Status bar (JRNL mode) -->
  <div style="background: #F2DA61; color: #141414; padding: 3px 16px; display: flex; justify-content: space-between; font-size: 12px;">
    <div>
      <span style="font-weight: bold;">JRNL</span>
      <span style="opacity: 0.4;"> │ </span>
      <span>March 2026</span>
    </div>
    <div style="opacity: 0.7;">h/l j/k H/L  ↵:open day  Esc:strip</div>
  </div>
</div>

---

## Journal + Pages: Separate Namespaces

| Namespace | Contents | Picker | Navigation | Index |
|-----------|----------|--------|------------|-------|
| **All files** | Everything | `SPC f f` | — | Full |
| **Pages** (`pages/`) | Named ideas with identity | `SPC p p` | — | Full |
| **Journal** (`journal/`) | Daily stream, temporal | `SPC j j` | `SPC j c` (calendar), `SPC j p`/`SPC j n` | Full |

Pages are things you navigate by *name*. The journal is something you navigate by *time*. The "all files" picker is for when you don't care which namespace — search everything.

All namespaces are fully searchable via `SPC s s` (full-text) and BQL queries.

---

## BQL Integration

The `journal` source in BQL targets the journal namespace:

```
journal | where date = today                          -- today's journal
journal | where date this week                        -- this week's entries
blocks  | where page in $journal | where tags has "rust"  -- all journal blocks tagged #rust
tasks   | where page in $journal | where not done     -- open tasks from any journal day
```

`$journal` is a context variable representing the journal namespace (all files in `journal/`).

---

## Open Questions

1. **Linking to journal entries.** If a page wants to reference "what I wrote on March 8", how? The journal file has a UUID — you can link by UUID as with any page. But the user doesn't know the UUID. Maybe: `[[journal:2026-03-08]]` as a special link syntax that resolves to the UUID? Or rely on the existing `[[` picker with a journal filter?

2. **Days with no journal in the calendar.** The `◆` marker means "has a journal file." Should the calendar also show a lighter marker for days with only page edits (no journal)? Or keep it purely journal-focused?

---

## References

- Current design: [GOALS.md G14](../GOALS.md) (Daily Journal)
- [TIME_TRAVEL.md](TIME_TRAVEL.md) — git-backed history, context strip component, calendar
- [BLOCK_IDENTITY.md](BLOCK_IDENTITY.md) — stable IDs for task actions
