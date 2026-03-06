# Bloom 🌱 — Adaptive Layout

> Screen-size-aware layout rules for pickers, previews, and result density.
> See [PICKER_SURFACES.md](PICKER_SURFACES.md) for per-picker data definitions,
> [WINDOW_LAYOUTS.md](WINDOW_LAYOUTS.md) for editor pane splits,
> [THEMING.md](THEMING.md) for colour assignments.

---

## Design Principle

Design for large monitors (≥160 cols, ≥50 rows) with graceful degradation to smaller terminals. Every adaptive rule has exactly two states: a **wide/tall** layout and a **compact** fallback. No intermediate breakpoints — simplicity over precision.

The terminal size is known on every frame (`f.area()` flows into `render()`). Layout decisions are pure functions of `(width, height)` — no stored "layout mode" state to drift.

---

## Screen Size Tiers

| Tier | Width | Height | Notes |
|------|-------|--------|-------|
| **Compact** | < 100 cols | < 30 rows | Minimum usable size. Overlays only. |
| **Standard** | 100–159 cols | 30–49 rows | Current default design target. |
| **Wide** | ≥ 160 cols | — | Side-by-side preview, wider picker, richer columns. |
| **Tall** | — | ≥ 50 rows | More result rows, larger preview panes. |

Width and height tiers are independent. A 200×35 terminal is Wide + Standard-height. A 120×60 terminal is Standard-width + Tall.

---

## 1. Side-by-Side Picker Preview

**Trigger:** Terminal width ≥ 160 columns.

On wide screens, the picker preview pane moves from below the results to the right side, creating a side-by-side layout. This uses the extra horizontal space instead of compressing the result list vertically.

### Wide layout (≥ 160 cols)

```
┌─ Find Page ───────────────────────────────────────────────────────────────┐
│ > rust_                                                                   │
│                                                                           │
│ ▸ Text Editor Theory      #rust #editors      Mar 03 │ ## Rope Data      │
│   Rust Programming        #rust               Feb 28 │ Structure         │
│   CRDT Notes              #rust #crdt         Feb 15 │                   │
│   Async Patterns          #rust #async        Feb 10 │ Ropes are O(log n)│
│                                                      │ for inserts. They │
│   4 of 960 pages                                     │ use balanced      │
│                                                      │ binary trees.     │
│                                                      │                   │
│                                                      │ - [ ] Review the  │
│                                                      │   ropey crate API │
└──────────────────────────────────────────────────────┴────────────────────┘
```

- Results pane: ~60% of picker width
- Preview pane: ~40% of picker width
- Vertical separator: `│` in `faded`
- Preview updates on highlight change (lazy load)

### Compact fallback (< 160 cols)

Preview stays below the results, separated by a horizontal rule — the current layout from [PICKER_SURFACES.md](PICKER_SURFACES.md).

```
┌─ Find Page ───────────────────────────────────────────┐
│ > rust_                                               │
│                                                       │
│ ▸ Text Editor Theory    #rust #editors      Mar 03    │
│   Rust Programming      #rust               Feb 28    │
│   4 of 960 pages                                      │
├───────────────────────────────────────────────────────┤
│ ## Rope Data Structure                                │
│ Ropes are O(log n) for inserts.                       │
└───────────────────────────────────────────────────────┘
```

### Implementation

```rust
let side_preview = picker_area.width >= 80 && terminal_width >= 160;
if side_preview {
    // Split picker_area horizontally: 60% results | 40% preview
} else {
    // Split vertically: results on top | preview on bottom (existing)
}
```

---

## 2. Wider Picker with Rich Columns

**Trigger:** Terminal width ≥ 180 columns.

On very wide screens, the picker expands from 60% to 75% of terminal width and shows additional metadata columns that are hidden on narrower screens.

### Column visibility rules

| Column | Always shown | Wide (≥ 180) | Compact (< 100) |
|--------|-------------|--------------|------------------|
| Label (title) | ✓ | ✓ | ✓ |
| Tags | ✓ | ✓ | Hidden |
| Date | ✓ | ✓ | Hidden |
| First content line | — | ✓ (new) | — |

### Wide layout (≥ 180 cols)

```
│ ▸ Text Editor Theory      #rust #editors      Mar 03   Ropes are O(log n) for inserts │
│   Rust Programming        #rust               Feb 28   The borrow checker ensures memo │
│   CRDT Notes              #rust #crdt         Feb 15   Operational Transform preceded  │
```

The "first content line" column shows the first non-frontmatter, non-heading line of the page — a content glimpse without opening the preview. Truncated to available width with `…`.

### Standard layout (100–179 cols)

```
│ ▸ Text Editor Theory      #rust #editors      Mar 03 │
│   Rust Programming        #rust               Feb 28 │
```

### Compact layout (< 100 cols)

```
│ ▸ Text Editor Theory              Mar 03 │
│   Rust Programming                Feb 28 │
```

Tags are hidden; only title and date remain.

### Picker width scaling

| Terminal width | Picker width % | Effective cols |
|---------------|----------------|----------------|
| < 80 | 90% | 72 |
| 80–139 | 60% (current) | 48–83 |
| 140–179 | 65% | 91–116 |
| ≥ 180 | 75% | 135+ |

---

## 3. Adaptive Result Density

**Trigger:** Terminal height ≥ 50 rows.

On tall screens, pickers show more result rows. On compact screens, the result list is tighter to leave room for the status bar and preview.

### Result row counts

| Terminal height | Result rows | Preview rows | Total picker height |
|----------------|-------------|-------------|-------------------|
| < 30 | 5 | 3 | 60% of height |
| 30–49 (current) | 8–10 | 5 | 70% of height |
| ≥ 50 | 15–20 | 8 | 75% of height |

### Implementation

The picker height percentage scales with terminal height:

```rust
let picker_height_pct = if height >= 50 { 75 } else if height >= 30 { 70 } else { 60 };
let h = (area.height * picker_height_pct / 100).max(10).min(area.height);
```

This means on a 60-row terminal, the picker is 45 rows tall — enough for ~20 results plus a generous preview. On a 24-row terminal, it's 14 rows — tight but usable.

---

## 4. Multi-Column Picker Results

**Trigger:** Terminal width ≥ 140 columns AND all visible items are "short" (label + right ≤ 30 chars).

For pickers with many short items (Tags, All Commands, Templates), results flow into a newspaper-style multi-column grid. This shows 3× more items in the same vertical space.

### Multi-column layout (≥ 140 cols, short items)

```
┌─ Tags ────────────────────────────────────────────────────────┐
│ > _                                                           │
│                                                               │
│ ▸ #rust             23  │  #editors           12  │  #crdt  8 │
│   #async            18  │  #vim                9  │  #undo  7 │
│   #data-structures  15  │  #performance        9  │  #tui   6 │
│   #concurrency      14  │  #testing            8  │  #wasm  4 │
│                                                               │
│   42 of 42 tags                                               │
└───────────────────────────────────────────────────────────────┘
```

### Navigation

- **↑/↓** moves within a column
- **←/→** jumps between columns (wraps at edges)
- **Ctrl+N/P** moves linearly through all items (left-to-right, top-to-bottom)
- Selection highlight wraps: moving down from the last row in column 1 goes to the first row in column 2

### Column count calculation

```
col_width = max(item_width) + 4  // padding + separator
col_count = (picker_inner_width / col_width).max(1).min(4)
```

Maximum 4 columns — more than that is visually noisy. If items are too wide for 2 columns, falls back to single column.

### Which pickers use multi-column

| Picker | Multi-column eligible | Rationale |
|--------|-----------------------|-----------|
| Tags | ✓ | Short labels, many items, no preview needed |
| All Commands | ✓ | Short labels with keybinding, many items |
| Templates | ✓ | Short names, typically < 20 items |
| Find Page | ✗ | Long titles + tags + date need full width |
| Search | ✗ | Full content lines need full width |
| Journal | ✗ | Dates are uniform length but preview is important |

### Single-column fallback

When the terminal is < 140 cols or items are too wide, the picker uses the standard single-column list. No special handling needed — the column count formula naturally produces 1.

---

## Breakpoint Summary

| Feature | Breakpoint | Effect |
|---------|-----------|--------|
| Side-by-side preview | width ≥ 160 | Preview moves to the right of results |
| Wider picker | width ≥ 180 | 75% width, extra "first line" column |
| Tag/date columns hidden | width < 100 | Only title + date shown |
| More result rows | height ≥ 50 | 15–20 rows instead of 8–10 |
| Multi-column results | width ≥ 140 + short items | 2–4 column grid for Tags/Commands |
| Picker height increase | height ≥ 50 | 75% of terminal height |

All breakpoints are tested against `f.area()` on every frame — resizing the terminal immediately adapts the layout.

---

## Related Documents

| Document | Contents |
|----------|----------|
| [PICKER_SURFACES.md](PICKER_SURFACES.md) | Per-picker data, columns, ranking, wireframes |
| [WINDOW_LAYOUTS.md](WINDOW_LAYOUTS.md) | Editor pane split layouts, navigation, status bar |
| [THEMING.md](THEMING.md) | Colour assignments for picker surfaces |
| [ARCHITECTURE.md](ARCHITECTURE.md) | RenderFrame abstraction, rendering model |
