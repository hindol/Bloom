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
- Navigation uses **nearest spatial neighbor** — `SPC w l` (right) targets the pane whose vertical center is closest to the cursor's position in the current pane.
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
│ NORMAL │ Text Editor Theory [+]              12:1 │ markdown  │
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
│ NORMAL │ Text Editor… 12:1    │ NORMAL │ 2026-03-03    4:1    │
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
│ NORMAL │ Rust Programming [+]                 8:1 │ markdown  │
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
│  Used by Xi Editor and Zed.  │  ☐ Read Xi Editor paper Feb 25 │
│                              │                                │
│  ## Piece Table              │  Today · Mar 3                 │
│                              │  ☐ Review PR for auth          │
│  Used by VS Code.            │  ☐ Buy groceries               │
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
│ NORMAL │ Text Editor Theory [+]              14:1 │ markdown   │
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

## Navigation Semantics (Nearest Spatial Neighbor)

Navigation (`SPC w h/j/k/l`) targets the pane whose center is closest to the cursor's position in the current pane.

### Example: 3-pane layout

```
┌──────────────────┬──────────────────┐
│                  │        B         │
│        A         ├──────────────────┤
│     (cursor ●)   │        C         │
└──────────────────┴──────────────────┘
```

**From A, cursor near the top:**
- `SPC w l` (right) → **B** (B's center is closer to A's cursor)
- `SPC w j` (down) → no pane below A → no-op

**From A, cursor near the bottom:**
- `SPC w l` (right) → **C** (C's center is closer to A's cursor)

**From B:**
- `SPC w h` (left) → **A** (only pane to the left)
- `SPC w j` (down) → **C** (directly below)

**From C:**
- `SPC w h` (left) → **A** (nearest to the left)
- `SPC w k` (up) → **B** (directly above)

### Algorithm

```
1. Compute the center point of each pane (mid-x, mid-y).
2. From the active pane's cursor position, cast a ray in the requested direction.
3. Filter to panes that are in the correct direction (e.g., "right" = pane center x > active pane right edge).
4. Of those, pick the pane with the minimum perpendicular distance to the cursor.
5. If no pane exists in that direction, do nothing (no wrapping).
```

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

Special views always open in a split (creating one if needed), never replace the current editor pane.

| View | Default split | Trigger |
|------|--------------|---------|
| Timeline | Vertical, right side | `SPC l t` |
| Agenda | Full pane (replaces or new split) | `SPC a a` |
| Undo tree | Vertical, right side | `SPC u u` |
| Backlinks panel | Vertical, right side | `SPC l b` |

**Repeated trigger closes the view.** Pressing `SPC l t` while the timeline is open closes it. Same for `SPC u u` and undo tree.

---

## Window Border Rendering

```
Active pane indicator (status bar styling, not borders):

┌─ Page Title ─────────────────┬─ Page Title ──────────────────┐
│                               │                               │
│  (content)                    │  (content)                    │
│                               │                               │
├───────────────────────────────┼───────────────────────────────┤
│ NORMAL │ page [+]  12:1  md  │ page               (compact)  │
└───────────────────────────────┴───────────────────────────────┘
 ↑ active: full status bar       ↑ inactive: compact, dim bg
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
| [USE_CASES.md](USE_CASES.md) | UC-52 through UC-57 |
| [API_SURFACES.md](API_SURFACES.md) | `WindowManager` and `LayoutTree` types |
