# Bloom 🌱 — Window Layouts

> Wireframes for window management, split layouts, active pane indication, and navigation semantics.
> See [GOALS.md G11](GOALS.md) for the feature goal, [KEYBINDINGS.md](KEYBINDINGS.md) for keybindings.

---

## Layout Model

Bloom's window system uses a **binary split tree**. Every layout is a tree of `Split` and `Leaf` nodes:

```
LayoutTree:
  Leaf(PaneId)
  Split { direction: V|H, ratio: f32, left: LayoutTree, right: LayoutTree }
```

Starting from a single pane, each split divides one pane into two. This produces all common layouts without complexity.

**Rules:**
- Only editor panes can be split. Special panes (timeline, agenda, undo tree) are leaf panes that can be opened, closed, or replaced, but not subdivided.
- Navigation currently uses a **nearest spatial neighbor** heuristic — `SPC w l` (right) filters to panes on the right and picks the pane whose vertical center is closest to the active pane center. The current implementation does not yet use the exact cursor position within the pane.
- Minimum pane size: 20 columns wide, 5 lines tall. Splits that would violate this are rejected.

---

## Single Pane (default)

```
┌─ Text Editor Theory ─────────────────────────────────────────┐
│                                                               │
│  ## Rope Data Structure                                       │
│                                                               │
│  Ropes are O(log n) for inserts. They use balanced            │
│  binary trees to represent text.                              │
│                                                               │
│  - [ ] Review the ropey crate API @due(2026-03-05)            │
│  - [x] Read Xi Editor source                                  │
│                                                               │
│  ~                                                            │
│  ~                                                            │
├───────────────────────────────────────────────────────────────┤
│ NORMAL │ Text Editor Theory [+]                          12:1 │
└───────────────────────────────────────────────────────────────┘
```

Tree: `Leaf(A)`

---

## Two Panes — Vertical Split (`SPC w v`)

The most common layout: two editors side by side.

```
┌─ Text Editor Theory ─────────┬─ 2026-03-03 (journal) ───────┐
│                               │                               │
│  ## Rope Data Structure       │  - Explored ropey crate for   │
│                               │    Bloom's buffer model        │
│  Ropes are O(log n) for      │  - [ ] Review PR for auth      │
│  inserts. They use balanced   │    module @due(2026-03-05)     │
│  binary trees.                │  - The borrow checker finally  │
│                               │    clicked for me              │
│  ~                            │                               │
│  ~                            │  ~                            │
│  ~                            │  ~                            │
├───────────────────────────────┼───────────────────────────────┤
│ NORMAL │ Text Editor…           12:1 │ 2026-03-03                    │
└───────────────────────────────┴───────────────────────────────┘
```

Tree: `Split { V, 0.5, Leaf(A), Leaf(B) }`

| Element | Style |
|---------|-------|
| Vertical separator `│` | `faded` — subtle, doesn't draw the eye |
| Active pane | Status bar uses mode colour (Normal = `modeline` bg). Inactive pane status bar uses `subtle` bg. |
| Pane title | In the top border, `faded`. Shows page title (truncated if needed). |

---

## Two Panes — Horizontal Split (`SPC w s`)

```
┌─ Text Editor Theory ─────────────────────────────────────────┐
│                                                               │
│  ## Rope Data Structure                                       │
│                                                               │
│  Ropes are O(log n) for inserts. They use balanced            │
│  binary trees to represent text.                              │
│                                                               │
├─ Rust Programming ───────────────────────────────────────────┤
│                                                               │
│  ## Ownership and Borrowing                                   │
│                                                               │
│  The borrow checker ensures memory safety at compile time.    │
│                                                               │
├───────────────────────────────────────────────────────────────┤
│ NORMAL │ Rust Programming [+]                             8:1 │
└───────────────────────────────────────────────────────────────┘
```

Tree: `Split { H, 0.5, Leaf(A), Leaf(B) }`

| Element | Style |
|---------|-------|
| Horizontal separator `─` | `faded` with pane title embedded |
| Active pane status bar | Only the active pane shows the full status bar at the bottom |
| Inactive pane | Has a thin title bar at the split border, no full status bar |

---

## Three Panes — Editor + Editor + Timeline

A common workflow: editing a page, its related page in a second pane, and the timeline in a third.

```
┌─ Text Editor Theory ────────┬─ 2026-03-03 (journal) ────────┐
│                              │                                │
│  ## Rope Data Structure      │  - Explored ropey crate        │
│                              │  - [ ] Review PR for auth      │
│  Ropes are O(log n) for     │  - The borrow checker finally  │
│  inserts.                    │    clicked for me              │
│                              │                                │
│                              ├─ Timeline: Text Editor Theory ┤
│                              │                                │
│                              │  Mar 3 · 2026-03-03 (journal) │
│                              │  ┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄  │
│                              │  Explored ropey crate for      │
│                              │  Bloom's buffer model.         │
│                              │                                │
│                              │  Feb 28 · Rust Programming     │
│                              │  ┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄  │
│                              │  See [[Text Editor Theory]]    │
│                              │  for data structure comparison │
│  ~                           │                                │
├──────────────────────────────┼────────────────────────────────┤
│ NORMAL │ Text Editor… 12:1   │                                │
└──────────────────────────────┴────────────────────────────────┘
```

Tree: `Split { V, 0.5, Leaf(A), Split { H, 0.5, Leaf(B), Leaf(C:Timeline) } }`

---

## Three Panes — Tall Left + Two Right

Another common layout: a main editor on the left, two supplementary views on the right.

```
┌─ Text Editor Theory ────────┬─ Rust Programming ─────────────┐
│                              │                                │
│  ## Rope Data Structure      │  ## Ownership and Borrowing    │
│                              │                                │
│  Ropes are O(log n) for     │  The borrow checker ensures    │
│  inserts. They use balanced  │  memory safety at compile      │
│  binary trees to represent   │  time.                         │
│  text. Each leaf holds a     │                                │
│  string fragment.            ├─ Agenda ───────────────────────┤
│                              │                                │
│  Good for large files.       │  Overdue                       │
│  Used by Xi Editor and Zed.  │  - [ ] Read Xi Editor paper Feb 25 │
│                              │                                │
│  ## Piece Table              │  Today · Mar 3                 │
│                              │  - [ ] Review PR for auth          │
│  Used by VS Code.            │  - [ ] Buy groceries               │
│                              │                                │
│  ~                           │  3 open tasks across 2 pages   │
├──────────────────────────────┼────────────────────────────────┤
│ NORMAL │ Text Editor… 14:1   │                                │
└──────────────────────────────┴────────────────────────────────┘
```

Tree: `Split { V, 0.5, Leaf(A), Split { H, 0.5, Leaf(B), Leaf(C:Agenda) } }`

---

## Four Panes — Grid

The maximum practical layout. Four equal panes.

```
┌─ Text Editor Theory ────────┬─ Rust Programming ─────────────┐
│                              │                                │
│  ## Rope Data Structure      │  ## Ownership and Borrowing    │
│                              │                                │
│  Ropes are O(log n) for     │  The borrow checker ensures    │
│  inserts.                    │  memory safety.                │
│                              │                                │
├─ CRDT Notes ─────────────────┼─ 2026-03-03 (journal) ────────┤
│                              │                                │
│  ## Operational Transform    │  - Explored ropey crate        │
│                              │  - [ ] Review PR for auth      │
│  OT preceded CRDTs for       │                                │
│  collaborative editing.      │  ~                             │
│                              │                                │
├──────────────────────────────┼────────────────────────────────┤
│ NORMAL │ CRDT Notes    5:1   │                                │
└──────────────────────────────┴────────────────────────────────┘
```

Tree: `Split { H, 0.5, Split { V, 0.5, Leaf(A), Leaf(B) }, Split { V, 0.5, Leaf(C), Leaf(D) } }`

---

## Maximized Pane (`SPC w m`)

Hides all other panes. The active pane takes full screen. A subtle indicator shows that other panes exist.

```
┌─ Text Editor Theory ──────────────────────── [2 hidden panes] ┐
│                                                                │
│  ## Rope Data Structure                                        │
│                                                                │
│  Ropes are O(log n) for inserts. They use balanced             │
│  binary trees to represent text. Each leaf holds a             │
│  string fragment, and internal nodes store the weight          │
│  (character count of left subtree).                            │
│                                                                │
│  Good for large files. Used by Xi Editor and Zed.              │
│                                                                │
│  ## Piece Table                                                │
│                                                                │
│  Used by VS Code. Append-only, good for undo operations.       │
│                                                                │
│  ~                                                             │
│  ~                                                             │
├────────────────────────────────────────────────────────────────┤
│ NORMAL │ Text Editor Theory [+]                          14:1 │
└────────────────────────────────────────────────────────────────┘
```

| Element | Style |
|---------|-------|
| `[2 hidden panes]` | `faded`, in the top-right corner. Shows count of hidden panes. |
| `SPC w m` again | Restores the previous layout exactly. |

---

## Active Pane Indicator

The active pane is indicated by **status bar styling**, not borders:

| Element | Active pane | Inactive pane |
|---------|------------|---------------|
| Status bar background | Mode colour (Normal = `modeline`, Insert = `accent_green`, Visual = `popout`) | `subtle` (very dim) |
| Status bar content | Full: mode, filename, dirty, position, pending keys | Compact: filename only |
| Pane border | `faded` | `faded` (same — borders don't change) |
| Cursor | Visible (block/bar/underline per mode) | Hidden (no cursor shown) |

This means pane borders are always the same weight — the eye is drawn to the active pane by its cursor and bright status bar, not by border changes.

---

## Status Bar Anatomy

The status bar is a single line at the bottom of each pane. Active and inactive panes show different levels of detail.

### Active Pane

```
│ NORMAL │ Text Editor Theory [+]            @q  SPC f    12:1 │
  ├─1──┘   ├──────2──────────┘├3┘            ├4┘ ├─5──┘   ├─6─┘
```

| # | Element | Description | When shown |
|---|---------|-------------|------------|
| 1 | **Mode** | `NORMAL` / `INSERT` / `VISUAL` / `COMMAND` | Always |
| 2 | **Page title** | Frontmatter title, not filename or UUID | Always |
| 3 | **Dirty marker** | `[+]` | Buffer has unsaved changes |
| 4 | **Macro recording** | `@q` (register name) | While recording a macro |
| 5 | **Pending keys** | Keys accumulated so far: `d`, `SPC f`, `2d` | During incomplete Vim command or leader sequence |
| 6 | **Cursor position** | `line:col` (1-indexed) | Always |

Layout: mode + title + dirty are **left-aligned**. Macro, pending, position are **right-aligned**. The middle is empty space — the bar breathes.

### Inactive Pane

```
│ Text Editor Theory                                            │
```

Just the page title. No mode, no position, no pending keys. `subtle` background — the bar recedes visually.

### Mode Colours

| Mode | Foreground | Background | Rationale |
|------|------------|------------|-----------|
| NORMAL | `foreground` | `modeline` | Calm default — you're navigating, not editing |
| INSERT | `background` | `accent_green` | Green = "go" — you're actively writing |
| VISUAL | `background` | `popout` | Selection is happening — needs to stand out |
| COMMAND | `background` | `accent_blue` | Informational — you're talking to the editor |
| HISTORY | `background` | `accent_yellow` | Time-travel — browsing past versions (see [HISTORY.md](HISTORY.md)) |
| DAY | `background` | `accent_yellow` | Time-travel — browsing daily activity (see [HISTORY.md](HISTORY.md)) |
| JOURNAL | `background` | `accent_yellow` | Time-travel — browsing journal days (see [JOURNAL.md](JOURNAL.md)) |

The temporal modes (`HISTORY`, `DAY`, `JOURNAL`) share `accent_yellow` to form a consistent visual family. They are active when the context strip or calendar grid from [HISTORY.md](HISTORY.md) or [JOURNAL.md](JOURNAL.md) is open.

### Temporal Mode Status Bar Layout

When a temporal mode is active, the status bar repurposes the right section — cursor position and thread indicators are hidden (both irrelevant during temporal browsing), replaced by **key hints** and **position**:

```
│ HIST │ Text Editor Theory           d:diff  r:restore  ↵:list 3/12│
  ├─1──┘   ├──────2──────────┘        ├────────────5────────────┘├─6─┘
```

| # | Element | Description |
|---|---------|-------------|
| 1 | **Mode** | `HIST` / `DAY` / `JRNL` |
| 2 | **Context title** | Page title (HIST), day name (DAY/JRNL), month name (calendar) |
| 5 | **Key hints** | Available actions — replaces pending keys and thread indicators |
| 6 | **Position** | Version position `3/12` (HIST), active day index `◆3` (DAY), selected day `[8]` (CAL) |

Elements 3 (dirty marker) and 4 (macro recording) are hidden — not applicable during temporal browsing.

Key hints shown per mode:

| Mode | Status bar right section |
|------|------------------------|
| HIST (strip) | `d:diff  r:restore  ↵:list  3/12` |
| HIST (expanded) | `j/k:nav  d:diff  r:restore  3/12` |
| DAY | `e:detail  ↵:calendar  [d ]d  ◆3` |
| JRNL | `↵:calendar  SPC j p/n` |

### Background Thread Indicators

The right section of the active pane's status bar shows icons for background threads. Icons are **hidden when idle** (except MCP, which shows a persistent icon when enabled). Icons **animate when active** — the spinner tells the user "something is happening" without being intrusive.

```
│ NORMAL │ Text Editor Theory [+]       ⟳  ⏍  ⚡    12:1 │
                                        ↑   ↑   ↑
                                   indexer  disk  MCP
```

| Thread | Icon | When shown | When animating |
|--------|------|------------|----------------|
| **Indexer** | `⟳` | During index rebuild (startup or `:rebuild-index`) | Spinner: `⟳◐◑◒◓` while scanning/parsing/writing |
| **Disk Writer** | `⏍` | During file write | Brief flash on each atomic write (debounced 300ms) |
| **File Watcher** | `◉` | Processing external file change | Pulse while reloading changed buffer |
| **MCP Server** | `⚡` | Whenever MCP is enabled (opt-in) | Spinner: `⚡◐◑◒◓` when LLM is editing the active buffer |

**Animation:** Active indicators cycle through a spinner sequence at the tick rate (~100ms). The animation plays while the thread is working and returns to idle (or hidden) when work completes. Each thread sends heartbeat messages via its channel; the `tick()` method advances the animation frame.

**Layout order:** Indicators appear between pending keys and cursor position, left-to-right: indexer → disk → watcher → MCP. Only visible indicators take space — the bar doesn't reserve room for hidden icons.

**Styling:**
- Idle/static icons: `faded` — background service, not demanding attention
- Animating icons: `salient` — "something is happening right now"
- All icons share the status bar background

### Separator

The `│` between mode and title is a thin Unicode box-drawing character, styled in `faded` — it separates sections without drawing the eye.

### Truncation

When the pane is narrow (e.g., in a vertical split):
- Title truncates first, with `…`: `Text Edi…`
- If still too narrow, pending keys are hidden
- Mode and position are never truncated — they're the minimum viable status bar

Minimum status bar: `│ NOR  12:1 │` (~12 chars). Below that, the pane is too small to split (enforced by the 20-column minimum).

---

## Navigation Semantics (Nearest Spatial Neighbor)

Navigation (`SPC w h/j/k/l`) currently uses pane-center heuristics. The implementation accepts `cursor_line`, but today it approximates the cursor's position with the active pane center.

### Example: 3-pane layout

```
┌──────────────────┬──────────────────┐
│                  │        B         │
│        A         ├──────────────────┤
│     (cursor ●)   │        C         │
└──────────────────┴──────────────────┘
```

**From A:**
- `SPC w l` (right) → whichever of **B** or **C** has its center closest to A's vertical midpoint. In the symmetric split above, they are equally close.
- `SPC w j` (down) → no pane below A → no-op

**From B:**
- `SPC w h` (left) → **A** (only pane to the left)
- `SPC w j` (down) → **C** (directly below)

**From C:**
- `SPC w h` (left) → **A** (nearest to the left)
- `SPC w k` (up) → **B** (directly above)

### Algorithm

```
1. Compute the center point of each pane (mid-x, mid-y).
2. Use the active pane center as the navigation origin.
3. Filter to panes that are in the correct direction (e.g., "right" = pane center x > active pane right edge).
4. Of those, pick the pane with the minimum perpendicular distance to the active pane center.
5. If no pane exists in that direction, do nothing (no wrapping).
```

This is an approximation of the intended cursor-aware behavior; the current code prefers a simple pane-center heuristic.

---

## Resize Behavior

| Action | Effect |
|--------|--------|
| `SPC w >` | Active pane grows wider by ~5 columns. The adjacent pane shrinks. |
| `SPC w <` | Active pane shrinks by ~5 columns. The adjacent pane grows. |
| `SPC w +` | Active pane grows taller by ~3 lines. The adjacent pane shrinks. |
| `SPC w -` | Active pane shrinks by ~3 lines. The adjacent pane grows. |
| `SPC w =` | All panes balanced to equal ratios (resets all split ratios to 0.5). |

Resize operates on the **nearest split border** to the active pane. If the active pane has borders on multiple sides, the resize direction determines which border moves.

**Minimum size enforcement:** A pane cannot be resized below 20 columns or 5 lines. Resize commands that would violate this are no-ops.

---

## Swap and Rotate

| Action | Effect |
|--------|--------|
| `SPC w x` | Swap the active pane's buffer with the next pane's buffer (clockwise order). Pane sizes don't change — only the content swaps. |
| `SPC w R` | Rotate the layout: vertical split ↔ horizontal split for the parent node. |

### Swap example:

Before `SPC w x` (focus on A):
```
┌──── A ─────┬──── B ─────┐
│  Editor 1  │  Editor 2  │
└────────────┴────────────┘
```

After `SPC w x`:
```
┌──── A ─────┬──── B ─────┐
│  Editor 2  │  Editor 1  │  (content swapped, pane sizes unchanged)
└────────────┴────────────┘
```

### Rotate example:

Before `SPC w R`:
```
┌──── A ─────┬──── B ─────┐
│  Editor 1  │  Editor 2  │
└────────────┴────────────┘
```

After `SPC w R`:
```
┌──── A ──────────────────┐
│  Editor 1               │
├──── B ──────────────────┤
│  Editor 2               │
└─────────────────────────┘
```

---

## Move Buffer (`SPC w H/J/K/L`)

Moves the active buffer to the pane in the given direction. The current pane either closes (if it was the only content) or shows the previously displayed buffer.

Before `SPC w L` (focus on A):
```
┌──── A ─────┬──── B ─────┐
│  Editor 1  │  Editor 2  │
└────────────┴────────────┘
```

After `SPC w L`:
```
┌──── A ─────┬──── B ─────┐
│  (empty)   │  Editor 1  │  (Editor 2 pushed to history, Editor 1 moved right)
└────────────┴────────────┘
```

If pane A has no other buffer in its history, it shows an empty state or closes.

---

## Special Pane Types

Special views always open in a split (creating one if needed), never replace the current editor pane — except the agenda, which is a full-screen takeover.

| View | Default split | Trigger |
|------|--------------|---------|
| Timeline | Vertical, right side | `SPC l t` |
| Agenda | Full-screen takeover | `SPC a a` |
| Undo tree | Vertical, right side | `SPC u u` |
| Backlinks panel | Vertical, right side | `SPC l b` |

**Repeated trigger closes the view.** Pressing `SPC l t` while the timeline is open closes it. Same for `SPC u u` and undo tree. `q` or `Esc` closes the agenda.

### Agenda View

Full-screen takeover with a task list (top ~60%) and source preview (bottom ~40%), separated by a horizontal rule.

```
┌─ Agenda ──────────────────────────────────────────────────────────────────┐
│                                                                           │
│  Overdue                                                                  │
│ ▸- [ ] Read Xi Editor retrospective #rust     Rust Notes          Feb 25  │
│  - [ ] Set up CI pipeline for Bloom           Bloom Dev           Feb 22  │
│                                                                           │
│  Today · Mar 5                                                            │
│  - [ ] Review PR for auth module              Work Log            Today   │
│  - [ ] Buy groceries                          Mar 5               Today   │
│                                                                           │
│  Upcoming                                                                 │
│  - [ ] Prepare Monday presentation            Work Log            Mar 10  │
│  - [ ] Write blog post draft #writing         Blog Ideas          Mar 12  │
│                                                                           │
│  ▸ 1/6 tasks   4 pages   [x]toggle [Enter]jump [q]close                 │
├───────────────────────────────────────────────────────────────────────────┤
│  ## Resources to Review                                                   │
│                                                                           │
│  Some earlier context paragraph.                                          │
│                                                                           │
│  - [ ] Read Xi Editor retrospective @due(2026-02-25)     ← highlighted   │
│                                                                           │
│  More notes below the task in the source file.                            │
│                                                                           │
│  Rust Notes · line 14                                                     │
└───────────────────────────────────────────────────────────────────────────┘
```

**Column layout (task list area):**

| Column | Content | Width | Style |
|--------|---------|-------|-------|
| Marker | `▸` or blank | 3 chars | — |
| Checkbox | `[ ]` or `[x]` | 3 chars | `accent_yellow` / `accent_green` |
| Task text + inline tags | Task description, tags appended naturally | flexible fill | varies by bucket (see below) |
| Source page | Page the task lives in | right-aligned, max 20 chars | `faded` |
| Date | Due date or "Today" | right-aligned, max 8 chars | `faded` (or `critical` for overdue) |

Tags appear inline after the task text (e.g. `Read Xi Editor retrospective #rust`), not in a separate column. This matches how tags render in the editor and the auto-alignment engine.

**Section headers:**

- `"  Overdue"`, `"  Today · Mar 5"`, `"  Upcoming"` — `salient` + bold.
- Section headers are not selectable — arrow keys skip them.
- Empty sections are hidden entirely.

**Styling per time bucket:**

| Bucket | Task text | Date | Rationale |
|--------|-----------|------|-----------|
| Overdue | `critical` (red) | `critical` (red) | Demands attention |
| Today | `foreground` | `faded` | Actionable now, normal weight |
| Upcoming | `faded` | `faded` | Not yet relevant, recedes |

Selected row gets `mild` background across the full width (same as picker selection).

**Preview pane (below separator):**

Shows ~5 lines of context around the selected task in its source page. The task line itself is highlighted with `SearchMatch` background. Footer line shows `"Source Page Title · line N"` in `faded`. Preview updates as you navigate.

**Footer (bottom of task list, above separator):**

```
  ▸ 1/6 tasks   4 pages   [x]toggle [Enter]jump [q]close
```

Selection position updates live (same pattern as the picker footer).

**Keyboard:**

| Key | Action |
|-----|--------|
| `j` / `↓` / `Ctrl+n` | Next task (skip section headers) |
| `k` / `↑` / `Ctrl+p` | Previous task |
| `Enter` | Jump to task in source page (closes agenda) |
| `x` | Toggle task done/undone |
| `q` / `Esc` | Close agenda, return to previous view |

---

## Window Border Rendering

```
Active pane indicator (status bar styling, not borders):

┌─ Page Title ─────────────────┬─ Page Title ──────────────────┐
│                               │                               │
│  (content)                    │  (content)                    │
│                               │                               │
├───────────────────────────────┼───────────────────────────────┤
│ NORMAL │ Page Title [+]    12:1 │ Page Title          (compact) │
└───────────────────────────────┴───────────────────────────────┘
 ↑ active: full status bar       ↑ inactive: title only, dim bg
```

| Border element | Character | Style |
|----------------|-----------|-------|
| Top border | `─` with `┌` `┬` `┐` corners | `faded` |
| Vertical separator | `│` | `faded` |
| Horizontal separator | `─` with `├` `┼` `┤` joints | `faded` |
| Bottom border | `─` with `└` `┴` `┘` corners | `faded` |
| Pane title in top border | Page title text | `faded` |
| Split intersection | `┼` | `faded` |

All borders use `faded` colour — they recede visually, letting the content and active status bar draw the eye.

---

## Related Documents

| Document | Contents |
|----------|----------|
| [GOALS.md G11](GOALS.md) | Window management goal and keybindings |
| [KEYBINDINGS.md](KEYBINDINGS.md) | Full window keybinding reference |
| [USE_CASES.md](test/USE_CASES.md) | UC-52 through UC-57 |
| [API_SURFACES.md](API_SURFACES.md) | `WindowManager` and `LayoutTree` types |
