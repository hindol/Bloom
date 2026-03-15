# Journal Redesign 📓

> Daily journal files in `journal/`, navigated by time with a rich scrubber panel.
> Status: **Implemented** — core features shipped, calendar and scrubber live.
> See also: [TIME_TRAVEL.md](TIME_TRAVEL.md) for git-backed history.

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

- `SPC j t` opens today's journal. Created lazily — the file appears only after the first edit triggers auto-save.
- After midnight, `SPC j t` targets the new date.
- Journal pages are regular Bloom pages with frontmatter, UUID, and full link/tag/task support.

### Picker Hierarchy

Three entry points into one picker component, differing only in scope:

| Keybinding | Scope | Shows |
|-----------|-------|-------|
| `SPC f f` | All files | Pages + journal entries |
| `SPC p p` | Pages only | Named pages in `pages/` |
| `SPC j j` | Journal only | Journal entries in `journal/`, newest first |

`SPC p p` and `SPC j j` are pre-filtered views of the same picker that powers `SPC f f`.

### Quick Capture and Direct Access

| Keybinding | Action |
|-----------|--------|
| `SPC j t` | Open today's journal (t = today) |
| `SPC j a` | Quick-append a line to today's journal (without leaving current buffer) |
| `SPC x a` | Quick-append a task to today's journal |
| `SPC j c` | Open journal calendar |

---

## Navigation

The journal is navigated by **time**, not by filename. Two mechanisms:

### Flow

```
SPC j t → journal/2026-03-09.md (today, editable, JRNL mode)
           │
           [d / ]d  →  prev / next journal day (skip empty days)
           │
           scrubber panel appears (3-line, auto-hides after 3s)
           │
           SPC j c  →  journal calendar (month grid overlay)
           │
           h/l/j/k navigate calendar, [d/]d skip to journal days
           Enter    →  open that day, Esc → restore original page

SPC j j → journal picker (search/browse all journal entries)
```

### Day-Hopping (`[d` / `]d`)

Vim-style bracket motions hop to the previous/next day **that has a journal file** — empty days are skipped. The scrubber panel appears showing adjacent days with stats:

<div style="font-family: 'JetBrains Mono', 'Fira Code', 'Consolas', monospace; font-size: 13px; line-height: 1.5; background: #141414; color: #EBE9E7; border-radius: 6px; overflow: hidden; max-width: 680px; margin: 16px 0;">
  <!-- Journal content -->
  <div style="padding: 8px 16px;">
    <div><span style="color: #EBE9E7;">- </span>Explored ropey crate for buffer model</div>
    <div><span style="color: #EBE9E7;">- </span>Read about Xi Editor architecture</div>
    <div><span style="color: #EBE9E7;">- </span><span style="color: #F2DA61;">[ ]</span> Review gap buffer tradeoffs <span style="color: #A3A3A3;">@due</span><span style="color: #A3A3A3; opacity: 0.5;">(</span>03-10<span style="color: #A3A3A3; opacity: 0.5;">)</span></div>
    <div><span style="color: #EBE9E7;">- </span><span style="color: #62C554; text-decoration: line-through;">[x]</span><span style="color: #A3A3A3; text-decoration: line-through;"> Compare with PieceTable</span></div>
    <div><span style="color: #A3A3A3;">#rust #editors #data-structures</span></div>
    <div>&nbsp;</div>
  </div>
  <!-- Separator -->
  <div style="padding: 0 16px; color: #37373E; letter-spacing: 2px; font-size: 10px;">┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄</div>
  <!-- Scrubber (3 lines, buffer background) -->
  <div style="padding: 4px 16px;">
    <div style="display: flex;">
      <div style="flex: 1; color: #A3A3A3;"><span style="opacity: 0.7;">◄</span> Mar 6 Thu</div>
      <div style="flex: 1;"><span style="color: #F4BF4F;">▸</span> <span style="color: #EBE9E7; font-weight: bold;">Mar 8 Sat</span></div>
      <div style="flex: 1; color: #A3A3A3;">Mar 12 Wed <span style="opacity: 0.7;">►</span></div>
    </div>
    <div style="display: flex;">
      <div style="flex: 1; color: #A3A3A3;">2 items · #rust</div>
      <div style="flex: 1; color: #EBE9E7;">5 items · #rust #editors</div>
      <div style="flex: 1; color: #A3A3A3;">2 items</div>
    </div>
    <div style="display: flex;">
      <div style="flex: 1; color: #A3A3A3;"><span style="color: #A3A3A3;">[ ]</span> Read DDIA chapter</div>
      <div style="flex: 1; color: #EBE9E7;"><span style="color: #F2DA61;">[ ]</span> Review gap buffer</div>
      <div style="flex: 1; color: #A3A3A3;"><span style="color: #A3A3A3;">[x]</span> Write blog post</div>
    </div>
  </div>
  <!-- Separator -->
  <div style="padding: 0 16px; color: #37373E; letter-spacing: 2px; font-size: 10px;">┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄</div>
  <!-- Status bar -->
  <div style="background: #212228; padding: 3px 16px; display: flex; justify-content: space-between; font-size: 12px;">
    <div>
      <span style="background: #F2DA61; color: #141414; font-weight: bold; padding: 0 4px;">JRNL</span>
      <span style="color: #37373E;"> │ </span>
      <span style="color: #EBE9E7;">2026-03-08</span>
    </div>
    <div style="color: #A3A3A3;">↵:calendar  [d/]d</div>
  </div>
</div>

**Scrubber panel** (3 lines + separator lines):
- **Line 1:** Date labels — prev ◄, current ▸ (bold), next ►
- **Line 2:** Stats — item count + up to 3 unique tags
- **Line 3:** First unchecked task (or first body line)
- Uses buffer background with faded `┄` separator lines
- **Auto-hides after 3 seconds** of no journal navigation

| Key | Action |
|-----|--------|
| `]d` | Jump to next day with a journal |
| `[d` | Jump to previous day with a journal |
| `j` / `k` | Scroll within the current day |
| `SPC j c` | Open journal calendar |

`[d` / `]d` work from any buffer — they open the journal for the day before/after the **most recently viewed journal day** (defaulting to today if no journal has been viewed this session).

### Journal Calendar (`SPC j c`)

The calendar grid allows spatial date navigation. Opens as a centered overlay. Navigating the calendar loads journal files into the buffer as a live preview — the editor content behind the calendar updates as you move.

<div style="font-family: 'JetBrains Mono', 'Fira Code', 'Consolas', monospace; font-size: 13px; line-height: 1.5; background: #141414; color: #EBE9E7; border-radius: 6px; overflow: hidden; max-width: 680px; margin: 16px 0;">
  <!-- Editor content (live preview of selected day) -->
  <div style="padding: 12px 16px;">
    <div><span style="color: #EBE9E7;">- </span>Explored ropey crate for buffer model</div>
    <div><span style="color: #EBE9E7;">- </span><span style="color: #F2DA61;">[ ]</span> Review gap buffer tradeoffs</div>
    <div><span style="color: #A3A3A3;">#rust #editors</span></div>
    <div>&nbsp;</div>
  </div>
  <!-- Calendar grid overlay -->
  <div style="border: 1px solid #37373E; margin: 0 auto; max-width: 280px; padding: 8px 12px; background: #141414;">
    <div style="text-align: center; margin-bottom: 6px;">
      <span style="color: #EBE9E7; font-weight: bold;">March 2026</span>
    </div>
    <div style="color: #A3A3A3; text-align: center; font-size: 12px;">
      <div style="margin-bottom: 2px;">Mo Tu We Th Fr Sa Su</div>
      <div>                              <span style="color: #EBE9E7;">1</span></div>
      <div> 2  3  4  5 <span style="color: #F4BF4F;">◆</span><span style="color: #EBE9E7;">6</span>  7 <span style="background: #F4BF4F; color: #141414; font-weight: bold; padding: 0 2px;">◆ 8</span></div>
      <div> 9 10 11 <span style="color: #F4BF4F;">◆</span><span style="color: #EBE9E7;">12</span> 13 14 15</div>
      <div>16 17 18 19 20 21 22</div>
      <div>23 24 25 <span style="color: #F4BF4F;">◆</span><span style="color: #EBE9E7;">26</span> 27 28 29</div>
      <div>30 31</div>
    </div>
    <div style="text-align: center; margin-top: 6px; color: #A3A3A3; font-size: 11px;">3 entries  [d/]d:skip  ↵:open</div>
  </div>
  <!-- Status bar -->
  <div style="background: #212228; padding: 3px 16px; display: flex; justify-content: space-between; font-size: 12px; margin-top: 8px;">
    <div>
      <span style="background: #F2DA61; color: #141414; font-weight: bold; padding: 0 4px;">JRNL</span>
      <span style="color: #37373E;"> │ </span>
      <span style="color: #EBE9E7;">2026-03-08</span>
    </div>
    <div style="color: #A3A3A3;">h/l j/k H/L  ↵:open  Esc:close</div>
  </div>
</div>

- Selected day shown with `salient` background (inverse highlight)
- `◆` = days with a journal file
- Navigating the calendar **loads journal files as live preview** in the editor behind the overlay
- `Enter` confirms — keeps the buffer open, enters JRNL mode. Preview buffers for other days are silently closed.
- `Esc` cancels — closes all preview buffers, restores the original page.

| Key | Action |
|-----|--------|
| `h` / `l` | Previous / next day |
| `j` / `k` | Next / previous week |
| `H` / `L` | Previous / next month |
| `[d` / `]d` | Skip to previous / next day with a journal |
| `Enter` | Open journal for selected day (confirms preview) |
| `q` / `Esc` | Close calendar (reverts to original page) |

---

## JRNL Mode

When the user enters journal navigation via `SPC j t`, `[d`/`]d`, or calendar Enter:

- Status bar shows `JRNL` mode badge with `accent_yellow` background (badge only, not full bar)
- Right-aligned hints: `↵:calendar  [d/]d`
- Mode is cleared when navigating to a non-journal page
- Scrubber panel appears on navigation, auto-hides after 3 seconds

---

## Journal + Pages: Separate Namespaces

| Namespace | Contents | Picker | Navigation | Index |
|-----------|----------|--------|------------|-------|
| **All files** | Everything | `SPC f f` | — | Full |
| **Pages** (`pages/`) | Named ideas with identity | `SPC p p` | — | Full |
| **Journal** (`journal/`) | Daily stream, temporal | `SPC j j` | `SPC j c` (calendar), `[d`/`]d` | Full |

Pages are things you navigate by *name*. The journal is something you navigate by *time*. The "all files" picker is for when you don't care which namespace — search everything.

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

---

## References

- Current design: [GOALS.md G14](../GOALS.md) (Daily Journal)
- [TIME_TRAVEL.md](TIME_TRAVEL.md) — git-backed history, context strip component, calendar
- [BLOCK_IDENTITY.md](../BLOCK_IDENTITY.md) — stable IDs for task actions
