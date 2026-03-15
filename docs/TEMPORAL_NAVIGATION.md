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
├── older ────────── STRIP ──────────── newer ────────┤
│                     ▲                               │
│                  selected                           │
├─────────────────────────────────────────────────────┤
│ MODE  title                            hints        │
└─────────────────────────────────────────────────────┘
```

The strip is a horizontal timeline. Moving `h`/`l` (or `←`/`→`) selects a point in time. The preview pane above updates to show what that moment looks like. The status bar shows the mode and context-specific hints.

Same component, different data sources:

| Context | Trigger | Strip items | Preview pane | Mode |
|---------|---------|-------------|-------------|------|
| **Journal** | `SPC j t`, `[d`/`]d` | Calendar days with journal files | Journal page content | JRNL |
| **Page history** | `SPC H h` | Undo nodes (●) + git commits (○) | Page diff vs current | HIST |
| **Block history** | `SPC H H` | Same, filtered to one block ID | Line diff vs current | HIST |
| **Day activity** | `SPC H c` | Days with vault activity (◆) | Activity summary | DAY |

---

## Shared Interactions

| Key | Action |
|-----|--------|
| `h` / `←` | Older |
| `l` / `→` | Newer |
| `e` | Toggle compact ↔ rich strip (show descriptions) |
| `d` | Toggle diff highlights (history contexts) |
| `r` | Restore to selected version (history contexts) |
| `Enter` | Context action (expand list / open page / jump to source) |
| `Esc` / `q` | Dismiss, return to normal editing |

---

## Strip Modes

### Compact (default)

Single line. Labels only. Good for quick scrubbing.

```
├── ● 2m ─── ● 5m ─── ● 8m ─── ● 15m ── ○ 1h ── ○ 3h ── ○ yday ──┤
                        ▲
```

### Rich (toggle with `e`)

Two lines. Labels + descriptions. More context at a glance.

```
├──────────────────────────────────────────────────────────────────┤
│ ● 2 min      ● 5 min      ● 8 min      ● 15 min     ○ 1 hr    │
│ "insert"     "delete"     "insert"      auto-save     save      │
│                             ▲                                    │
├──────────────────────────────────────────────────────────────────┤
```

For day activity, descriptions are summary stats:

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

## Block History — `SPC H H`

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

## Day Activity — `SPC H c`

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
| `SPC H H` | Block | Open block history strip (cursor's block) |
| `SPC H c` | Vault | Open day activity strip |
