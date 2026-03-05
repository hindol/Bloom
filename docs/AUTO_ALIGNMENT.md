# Bloom 🌱 — Auto-Alignment

> Automatic vertical alignment of structured elements on mode transition (Insert → Normal).
> Inspired by Org-mode's Tab-to-align behavior for tables.

---

## Design Principles

1. **Real whitespace.** Alignment inserts actual spaces into the buffer, not virtual rendering. Files look aligned in any editor, round-trip through git cleanly.
2. **Undo-able in one step.** The entire alignment operation is a single edit group — one `u` reverts the whole block.
3. **Triggered on Esc.** When the user leaves Insert mode, Bloom detects alignable blocks around the cursor and formats them. Fast typists are never interrupted.
4. **Conservative.** Only align within contiguous blocks of the same type. A blank line or different construct breaks the block. Never reformats content the user didn't just edit.
5. **Viewport-aware.** Alignment respects a column cap to prevent artificial line wrapping. Lines already longer than the cap are left alone.

---

## 1. Timestamp Alignment in Task Blocks

### Trigger

Contiguous lines starting with `- [ ] ` or `- [x] ` that contain at least one `@due()`, `@start()`, or `@at()`.

### Rule

- Align the first `@` on each line to a common column.
- The column is `min(max_text_width + 1, viewport_width - max_timestamp_length - right_margin)`.
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

### Narrow viewport (cap kicks in)

If the alignment column would push content beyond the viewport width, the cap prevents wrapping. Three scenarios:

### Scenario A: One line exceeds the cap

Viewport is 70 columns. Most lines are short, but one is long:

```
- [ ] Fix bug                                  @due(2026-03-10)
- [ ] Refactor the index module to support incremental updates @due(2026-03-20)
- [x] Set up vault                              @due(2026-03-04)
```

The long line (line 2) can't be padded without wrapping, so it keeps `@` right after its text with 1 space. Lines 1 and 3 align to the **cap column** (`viewport - max_timestamp - margin`), not to line 2's text width. The block is as aligned as possible without causing wraps.

### Scenario B: The longest line IS the alignment target but fits

```
- [ ] Review the ropey crate API documentation   @due(2026-03-05)
- [ ] Fix bug                                    @due(2026-03-10)
- [x] Set up vault                               @due(2026-03-04)
```

Line 1 sets the column. Lines 2 and 3 pad to match. No cap issue — everything fits within viewport.

### Scenario C: Multiple lines exceed the cap

```
- [ ] Refactor the index module to support incremental updates @due(2026-03-20)
- [ ] Implement background re-indexing with progress reporting @due(2026-03-25)
- [ ] Fix bug @due(2026-03-10)
```

Lines 1 and 2 both exceed the cap — they each keep `@` with 1 space after their text. Line 3 pads to the cap column. The result:

```
- [ ] Refactor the index module to support incremental updates @due(2026-03-20)
- [ ] Implement background re-indexing with progress reporting @due(2026-03-25)
- [ ] Fix bug                                @due(2026-03-10)
```

Lines that can align, do. Lines that can't, don't. No wrapping ever.

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

- Align the `^` to a common column across the block.
- Same cap logic as timestamps — don't push beyond viewport.

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

- Each alignment type detects its block by scanning up/down from the cursor line until it hits a non-matching line or blank line.
- All padding changes in a block are wrapped in `begin_edit_group()` / `end_edit_group()` for single-step undo.
- The alignment runs after the Insert→Normal mode change is applied but before the render frame is computed.
- Alignment is idempotent — running it on an already-aligned block produces no changes.

---

## Related Documents

- [KEYBINDINGS.md](KEYBINDINGS.md) — Esc triggers alignment
- [THEMING.md](THEMING.md) — TimestampKeyword, Tag styles for aligned elements
- [USE_CASES.md](USE_CASES.md) — UC-41 (create task), UC-42 (toggle task)
