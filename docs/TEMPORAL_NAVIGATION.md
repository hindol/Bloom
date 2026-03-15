# Temporal Navigation 🕰️

> One component, four contexts. Every time-based view is a horizontal timeline
> strip + a preview pane. Left = older, right = newer.

---

## The Pattern

```
┌─────────────────────────────────────────────────────┐
│                                                     │
│                  PREVIEW PANE                       │
│          (content varies by context)                │
│                                                     │
├─────────────────────────────────────────────────────┤
│ MODE  title                            hints        │  ← status bar (stays anchored)
├── older ────────── STRIP ──────────── newer ────────┤  ← drawer below status bar
│                     ▲                               │
└─────────────────────────────────────────────────────┘
```

The status bar stays at its normal position. The strip opens as a bottom drawer below it (same pattern as the which-key drawer). The content area shrinks to make room. Moving `h`/`l` (or `←`/`→`) selects a point in time. The preview pane updates to show what that moment looks like.

Same component, different data sources:

| Context | Trigger | Strip items | Preview pane | Mode |
|---------|---------|-------------|-------------|------|
| **Journal** | `SPC j t`, `[d`/`]d` | Calendar days with journal files | Journal page content | JRNL |
| **Page history** | `SPC H h` | Undo nodes (●) + git commits (○) | Page diff vs current | HIST |
| **Block history** | `SPC H b` | Same, filtered to one block ID | Line diff vs current | HIST |
| **Day activity** | `SPC H d` | Days with vault activity (◆) | Activity summary | DAY |

---

## Shared Interactions

| Key | Action |
|-----|--------|
| `h` / `←` | Older |
| `l` / `→` | Newer |
| `j` / `k` | At a branch point (`[●]`): switch between branches |
| `e` | Toggle compact ↔ rich (show descriptions) |
| `d` | Toggle diff highlights (history contexts) |
| `r` | Restore to selected version (history contexts) |
| `Enter` | Context action (open page / jump to source) |
| `Esc` / `q` | Dismiss, return to normal editing |

---

## Strip Modes

### Compact (default — 4 lines)

Each node gets ~12 characters. The viewport scrolls horizontally to keep the selected node centered. Nodes off-screen are clipped.

```
├─ Page History ──────────────────────────────┤
│    ● 5 min     [●] 8 min     ● 15 min      │
│                 ▲ insert session             │
├─ h/l:scrub  e:detail  r:restore  q:close ──┤
```

Line 1: title bar (mode + version count + date range)
Line 2: timeline nodes (scrollable, selected centered)
Line 3: selected node's description (follows cursor)
Line 4: key hints

### Rich (toggle with `e` — 6 lines)

Each node gets ~16 characters. Description + diff stat visible for the selected node.

```
├─ Page History ── 12 versions ── Mar 5–now ──┤
│    ● 5 min       [●] 8 min       ● 15 min  │
│                    ▲                         │
│   "delete"    "insert session"   auto-save   │
│                  +3 / -1                     │
├─ h/l:scrub  r:restore  d:diff  q:close ─────┤
```

Line 1: title + stats
Line 2: timeline nodes (scrollable)
Line 3: cursor indicator
Line 4: descriptions for visible nodes
Line 5: diff stat for selected node
Line 6: key hints

### Horizontal scrolling

The timeline is wider than the screen. Moving `h`/`l` scrolls the viewport to keep the selected node roughly centered. ~5-6 nodes visible on 80-column terminals. Edges clip gracefully.

```
At the beginning (selected = first):
│ ▸● 2 min     ● 5 min     ● 8 min     ● 15 min │

After pressing l several times (viewport shifts):
│    ● 8 min     ● 15 min    ○ 1 hour    ○ 3 hr  │

At the end (selected = last):
│  ○ 3 hours    ○ yesterday    ○ Mar 12   ▸○ Mar 5│
```

### Branches (auto-expand at `[●]`)

When the cursor lands on a branch point (`[●]`), the fork auto-expands. `j`/`k` switches between branches. Moving `h`/`l` away from the fork collapses it back to one line.

```
Scrubbing toward a branch:
├── ● 2m ── ● 5m ── [●] 8m ── ● 10m ── ○ 1h ──┤
                      ▲

Cursor arrives at [●] — auto-expands:
├── ● 2m ── ● 5m ──┬── ● 8m "insert" ── ● 10m ── ○ 1h ──┤
                    └── ● 8m "delete" (abandoned)
                    ▲ (on main branch)

k (switch to abandoned branch):
├── ● 2m ── ● 5m ──┬── ● 8m "insert" ── ● 10m ── ○ 1h ──┤
                    └── ● 8m "delete" (abandoned)
                         ▲

l (follow abandoned branch, fork collapses):
├── ● 2m ── ● 5m ── [●] 8m ── ● "delete" ──┤
                                  ▲
```

No keystrokes to discover branches — they appear the moment you reach them. `j`/`k` does nothing on non-branch nodes.

### Branch Rules

**Main branch:** The strip always shows the path from root → current undo node as the top line. Alternate branches hang below. "Current" is defined by the undo tree's `current` pointer.

```
Undo tree: root → A → B → C → D (current)
                       └→ E (abandoned)

Strip shows current path as main line:
├── root ── A ── [●]B ── C ── D ──┤

j at B:
├── root ── A ──┬── C ── D ──┤   ← current path (top)
                └── E             ← abandoned (below)
                ▲
```

**Nested branches:** If an abandoned branch itself has a fork, it renders as an additional line. Capped at 4 visible lines — deeper nesting shows `…`.

```
├── root ── A ──┬── C ── D ──┤
                └── E ──┬── F
                        └── G
                ▲
```

In practice, nested branches are rare (require: edit → undo → edit → undo past fork → edit again).

**Restore from abandoned branch:** `r` on an abandoned node creates a new forward node on the current path. The abandoned branch is preserved — the restored content flows forward, not backward.

```
Before: root → A → B → C (current), B → E (abandoned)
Restore E: root → A → B → C → [new: E's content] (current)
Branch at B still exists in the tree.
```

**Git commits are always linear.** `j`/`k` is a no-op on `○` nodes. Only undo nodes (`●`) can have branches.

**Empty undo tree.** If the page was just opened (no edits this session), the strip shows only `○` git commits. Pure linear. Graceful degradation.

For day activity, descriptions are summary stats (no branching):

```
├──────────────────────────────────────────────────────────────────┤
│ ◆ Mar 5      ◆ Mar 6      ◆ Mar 8      ◆ Mar 12     ◆ Mar 14  │
│ 2pg 1task    3pg          5pg 2tasks    1pg           4pg       │
│                             ▲                                    │
├──────────────────────────────────────────────────────────────────┤
```

---

## Journal — `SPC j t`, `[d`/`]d`

Navigate daily journal files. Already implemented.

```
┌─ 2026-03-14 ───────────────────────────────────────┐
│ ## Friday, March 14                                │
│                                                     │
│ - [x] Implemented mirror markers                   │
│ - [x] Built mirror UX                              │
│ - [ ] Review demo vault                            │
│                                                     │
│ Good progress on block identity.                   │
│                                                     │
├── Mar 12 ── Mar 13 ── Mar 14 ── (empty) ── Mar 16 ─┤
│                         ▲                           │
├─────────────────────────────────────────────────────┤
│ JRNL  2026-03-14          ↵:calendar  [d/]d:hop    │
└─────────────────────────────────────────────────────┘
```

**Preview:** The journal page content, loaded read-only. Navigating `h`/`l` loads the adjacent day's journal. Empty days are skipped (same as `[d`/`]d` behavior).

**Strip items:** Calendar days. Days with journal files shown normally. Days without files skipped during navigation.

---

## Page History — `SPC H h`

Browse all versions of the current page. Undo tree for recent (branching), git commits for older (linear). One seamless timeline.

```
┌─ Rust Project (diff vs current) ───────────────────┐
│  ## Rope Data Structure                             │
│                                                     │
│+ Ropes are O(log n) for inserts.                    │  ← green: in historical
│+ They use balanced binary trees.                    │  ← green: in historical
│- Ropes provide O(log n) insert and delete.          │  ← red: in current
│  See Xi Editor for details.                         │
│                                                     │
├── ● 2m ── ● 5m ── ● 8m ── ● 15m ── ○ 1h ── ○ 3h ── ○ yday ──┤
│                     ▲                                           │
├─────────────────────────────────────────────────────┤
│ HIST  Rust Project           d:diff  r:restore      │
└─────────────────────────────────────────────────────┘
```

**Preview:** Diff view by default. Green = lines present in the historical version but not in current. Red = lines present in current but not in the historical version. Toggle with `d` between diff and raw historical content.

**Strip items:**
- `●` = undo node (recent, per-edit-group, branching)
- `○` = git commit (older, per-save, linear)
- Transition is seamless — no visual break

**Branching:** When the undo tree has branches (undo → edit creates a fork), the strip can show branch points. `j`/`k` switch between branches at a fork point.

**Restore:** `r` replaces the buffer with the selected version. Creates one undo step — undoable. For git versions, creates a new undo branch ("restored from Mar 12").

---

## Block History — `SPC H b`

Same as page history, filtered to the block under the cursor (identified by block ID).

```
┌─ Block ^k7m2x (diff vs current) ──────────────────┐
│                                                     │
│- Review ropey + petgraph API @due(03-16)            │  ← current
│+ Review ropey API @due(03-16)                       │  ← historical
│                                                     │
│  ─── moved: Weekly Review → Rust Project ───        │  ← cross-page
│                                                     │
│+ Review rope libraries @due(03-12)                  │  ← original form
│                                                     │
├── ● 2m ── ● 8m ── ○ 1h ── ○ yday ── ○ Mar 10 ─────┤
│             ▲                                        │
├─────────────────────────────────────────────────────┤
│ HIST  ^k7m2x                 d:diff  r:restore      │
└─────────────────────────────────────────────────────┘
```

**Preview:** Inline diff of the block's line at the selected point vs current.

**Strip items:** Only versions where this block changed. Undo nodes that didn't touch this block are skipped.

**Cross-page moves:** If the block ID moved between pages between two versions, shown as a "moved" separator in the preview.

**Restore:** Replaces only the block's line in the current buffer. Rest of the page untouched.

---

## Day Activity — `SPC H d`

Vault-wide summary of what happened on any given day. Derived from git diffs.

```
┌─ Saturday, March 8 ────────────────────────────────┐
│                                                     │
│  ✏️ Edited                                          │
│  Text Editor Theory                       +12 lines │
│  Rust Programming                          +3 lines │
│                                                     │
│  🌱 Created                                         │
│  Gap Buffer Tradeoffs  #data-structures             │
│                                                     │
│  ✅ Completed                                       │
│  [x] Compare with PieceTable       Text Editor Theory│
│  [x] Read Neovim buffer internals   Rust Programming│
│                                                     │
├── ◆ Mar 5 ── ◆ Mar 6 ── ◆ Mar 8 ── ◆ Mar 12 ── ◆ Mar 14 ──┤
│                           ▲                                   │
├─────────────────────────────────────────────────────┤
│ DAY  March 8                  Enter:page  [d/]d:hop │
└─────────────────────────────────────────────────────┘
```

**Preview:** Activity summary rendered as a read-only buffer. Three sections: edited pages (with line counts), created pages, completed tasks (identified by block ID toggle in git diff).

**Strip items:** Days with git activity (◆). Days without activity are skipped during `h`/`l` navigation.

**Actions on items:** `Enter` on an edited page opens it. `Enter` on a completed task jumps to the source page at the task's line.

---

## Component Architecture

```rust
/// Generic temporal strip — same struct, different data.
struct TemporalStrip<T: StripItem> {
    items: Vec<T>,
    selected: usize,
    compact: bool,          // single-line vs rich (2-line)
}

trait StripItem {
    fn label(&self) -> &str;          // "2 min", "Mar 8"
    fn detail(&self) -> Option<&str>; // "insert session", "3pg 2tasks"
    fn marker(&self) -> char;         // ●, ○, ◆
}

// Preview is NOT owned by the strip — the caller renders it.
// Journal: loads page content
// History: computes diff
// Day activity: builds summary buffer
```

The strip component handles: `h`/`l` navigation, boundary clamping, `e` compact/rich toggle, rendering the strip line(s), managing `selected` index.

The caller handles: what to show in the preview pane, what `Enter`/`r`/`d` do.

---

## Keybinding Summary

| Key | Context | Action |
|-----|---------|--------|
| `SPC j t` | Journal | Open today's journal with scrubber |
| `[d` / `]d` | Journal | Hop to prev/next day (skips empty) |
| `SPC j c` | Journal | Open calendar overlay |
| `SPC H h` | Page | Open page history strip |
| `SPC H b` | Block | Open block history strip (cursor's block) |
| `SPC H d` | Vault | Open day activity strip |
