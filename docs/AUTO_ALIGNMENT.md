# Bloom 🌱 — Auto-Alignment

> Automatic vertical alignment of structured elements on mode transition (Insert → Normal).
> Inspired by Org-mode's Tab-to-align behavior for tables.

---

## Design Principles

1. **Real whitespace.** Alignment inserts actual spaces into the buffer, not virtual rendering. Files look aligned in any editor, round-trip through git cleanly.
2. **Undo-able in one step.** The entire alignment operation is a single edit group — one `u` reverts all changes across the page.
3. **Triggered on Esc.** When the user leaves Insert mode, Bloom aligns the page (or the cursor's block, depending on config). Fast typists are never interrupted.
4. **Block-scoped.** Each alignment type operates within contiguous blocks of the same kind. A blank line or different construct breaks the block.
5. **Presentation-agnostic.** Alignment operates on buffer content only. Word wrapping and viewport width are frontend concerns — core never caps or adjusts for display width.
6. **Configurable.** Users can choose the scope of alignment or disable it entirely.

---

## Configuration

```toml
# config.toml
auto_align = "page"     # default
```

| Value | Behaviour |
|-------|-----------|
| `"page"` | On Esc, scan the entire buffer and align every block found. Ensures consistency across the whole file. |
| `"block"` | On Esc, scan up/down from the cursor line and align only the block the cursor is in. Lighter touch — only what you just edited. |
| `"none"` | Disabled. No alignment on Esc. |

---

## 1. Timestamp Alignment in Task Blocks

### Trigger

Contiguous lines starting with `- [ ] ` or `- [x] ` that contain at least one `@due()`, `@start()`, or `@at()`.

### Rule

- Align the first `@` on each line to a common column.
- The column is `max_text_width_in_block + 1` (the longest text-before-`@` sets the column).
- Lines without any `@` keyword are left untouched.
- If tags appear after a timestamp (e.g. `@due(2026-03-05) #rust`), relocate them before the first `@`.

### Canonical form

```
text [tags] @timestamps
```

Tags that are mid-sentence (e.g. `Review the #rust API`) stay in place — only tags found after `@keyword(...)` patterns are relocated.

### Before

```
- [ ] Review ropey API #rust @due(2026-03-05)
- [ ] Fix parser @due(2026-03-10) #rust
- [ ] Read DDIA @start(2026-03-02) @due(2026-03-15)
- [x] Set up vault #devops @due(2026-03-04)
- [ ] Write tests #testing
```

### After Esc

```
- [ ] Review ropey API #rust           @due(2026-03-05)
- [ ] Fix parser #rust                 @due(2026-03-10)
- [ ] Read DDIA                        @start(2026-03-02) @due(2026-03-15)
- [x] Set up vault #devops             @due(2026-03-04)
- [ ] Write tests #testing
```

### Long lines

The alignment column is set by the longest text-before-`@` in the block. All other lines pad to match. If one line is much longer, the others get more padding — this is correct. Word wrapping of long lines is a frontend concern.

```
- [ ] Fix bug                                                   @due(2026-03-10)
- [ ] Refactor the index module to support incremental updates   @due(2026-03-20)
- [x] Set up vault                                              @due(2026-03-04)
```

Line 2 is the longest and sets the column. Lines 1 and 3 pad to match. If the window is narrow, lines 1 and 2 may word-wrap — that's the GUI's problem, not the alignment logic's.

---

## 2. Markdown Table Pipe Alignment

### Trigger

Contiguous lines that start and end with `|`.

### Rule

- Pad each cell to the width of the widest content in that column.
- Rebuild the alignment row (`|---|`) to match.
- Cells are left-aligned by default. `:---:` and `---:` hints are preserved.

### Before

```
| Key | Action |
|---|---|
| `w` | Next word start |
| `b` | Previous word start |
| `e` | Next word end |
```

### After Esc

```
| Key | Action              |
|-----|---------------------|
| `w` | Next word start     |
| `b` | Previous word start |
| `e` | Next word end       |
```

---

## 3. YAML Frontmatter Value Alignment

### Trigger

Lines between `---` delimiters (frontmatter block).

### Rule

- Align the value portion of `key: value` lines to a common column.
- The column is `max_key_width + 2` (key + `: `).

### Before

```
---
id: 7e8f9a0b
title: "Bloom Architecture Notes"
created: 2026-02-10
tags: [bloom, architecture, rust]
---
```

### After Esc

```
---
id:      7e8f9a0b
title:   "Bloom Architecture Notes"
created: 2026-02-10
tags:    [bloom, architecture, rust]
---
```

---

## 4. Block ID End-of-Line Alignment

### Trigger

Contiguous lines that end with `^block-id` (preceded by a space).

### Rule

- Align the `^` to a common column across the block, same as timestamp alignment.

### Before

```
## Design Principles ^principles
## Architecture Overview ^architecture
## Sync Strategy ^sync
```

### After Esc

```
## Design Principles       ^principles
## Architecture Overview    ^architecture
## Sync Strategy            ^sync
```

---

## Constructs NOT Auto-Aligned

| Construct | Reason |
|-----------|--------|
| **Nested list indentation** | Alignment would break visual hierarchy — indentation IS the structure |
| **Mid-sentence tags** | Tags in `I love #rust and #editors` are prose, not metadata — don't move them |
| **Link display text** | Rare pattern, low benefit, risk of breaking link syntax |
| **Heading levels** | `#` count is semantic — auto-formatting would change meaning |

---

## Implementation Notes

- **Page mode:** single pass over all lines, top to bottom. Track current block type — when a block ends (blank line, different construct), flush alignment for that block. O(n) where n is line count.
- **Block mode:** scan up/down from cursor line to find block boundaries, align that block only.
- All padding changes across the entire operation are wrapped in `begin_edit_group()` / `end_edit_group()` for single-step undo.
- The alignment runs after the Insert→Normal mode change is applied but before the render frame is computed.
- Alignment is idempotent — running it on an already-aligned block produces no edits and does not dirty the undo tree.
- Performance: a 200-line file is <1ms. The highlighter (which runs on every render) is heavier.

---

## Related Documents

- [KEYBINDINGS.md](KEYBINDINGS.md) — Esc triggers alignment
- [THEMING.md](THEMING.md) — TimestampKeyword, Tag styles for aligned elements
- [USE_CASES.md](USE_CASES.md) — UC-41 (create task), UC-42 (toggle task)
