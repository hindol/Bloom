# Bloom 🌱 — Word Wrap

> Visual line wrapping for long lines. See [ARCHITECTURE.md](ARCHITECTURE.md) for the render pipeline.

---

## Principles

1. **Display-only.** Wrapping never modifies the buffer. No newlines inserted, no undo entries. The buffer is the source of truth; wrapping is a frontend lens.
2. **Frontend-owned.** bloom-core emits unwrapped `RenderedLine`s (one per buffer line). The GUI computes its own wrap map using its own width measurement — pixels with font metrics.
3. **Pluggable measurement.** The wrap algorithm is generic over a `MeasureWidth` trait so the same logic works for monospace columns, proportional fonts, and mixed-width (CJK + Latin) text.

---

## Architecture

```
bloom-core                          Frontend (GUI)
─────────                           ─────────────────────
Buffer (rope)                       
    │                               
    ▼                               
RenderedLine[]                      
  (one per buffer line,             
   with byte-range spans)           
    │                               
    ├──────────────────────────────▶ WrapMap::new(lines, width, &dyn MeasureWidth)
    │                                   │
    │                                   ▼
    │                               WrapMap
    │                                 • screen_row → (line_idx, wrap_offset, byte_start)
    │                                 • (line_idx, column) → screen_row
    │                                   │
    │                                   ▼
    │                               ScreenScroll
    │                                 • scrolloff in screen-row units
    │                                   │
    │                                   ▼
    │                               draw_editor_content()
    │                                 • iterates screen rows via WrapMap
    │                                 • gutter: line number or ↪ indicator
    │                                 • spans: clipped to wrapped byte range
    │                                 • cursor: placed at screen (row, col)
    │                               
CursorState { line, column }        
Viewport { first_visible_line }     ── core-owned buffer-line viewport
```

---

## MeasureWidth Trait

```rust
/// Measure the display width of a text slice.
/// Returns a unit appropriate for the frontend — pixels for GUI, columns for monospace fallback.
pub trait MeasureWidth {
    fn width(&self, text: &str) -> usize;
}
```

**Monospace implementation (bloom-core::render::measure):**
```rust
struct MonospaceWidth;

impl MeasureWidth for MonospaceWidth {
    fn width(&self, text: &str) -> usize {
        unicode_width::UnicodeWidthStr::width(text)
    }
}
```

**Future GUI implementation:**
```rust
struct FontWidth {
    font: Font,
    size: f32,
}

impl MeasureWidth for FontWidth {
    fn width(&self, text: &str) -> usize {
        self.font.measure(text, self.size).width.ceil() as usize
    }
}
```

The trait returns `usize` in both cases — the unit differs (columns vs pixels) but the wrap algorithm only compares widths against a max, so the unit doesn't matter as long as `max_width` uses the same unit.

---

## WrapMap

```rust
pub struct WrapMap {
    entries: Vec<WrapEntry>,
    total_screen_rows: usize,
}

struct WrapEntry {
    screen_row_start: usize,    // first screen row for this line
    row_count: usize,           // number of screen rows (1 = no wrap)
    break_offsets: Vec<usize>,  // byte offsets where each screen row starts
}
```

### Construction

`WrapMap::new(lines: &[RenderedLine], max_width: usize, measure: &dyn MeasureWidth)`

For each `RenderedLine`:
1. Strip trailing `\n`/`\r`
2. Scan forward, accumulating width via `measure.width()`
3. When accumulated width exceeds `max_width`:
   - Search backward for the last whitespace (word boundary)
   - If found: break there, record byte offset
   - If not found (single long word): break at the character boundary that fits
4. Record the byte offset of each break in `break_offsets`

`break_offsets` always starts with `[0]` (the first row starts at byte 0). A line that doesn't wrap has `break_offsets = [0]` and `row_count = 1`.

### Queries

| Method | Input | Output |
|--------|-------|--------|
| `total_screen_rows()` | — | Total screen rows across all lines |
| `cursor_screen_row(line_idx, column)` | Buffer line index + column | Absolute screen row |
| `screen_row_to_line(screen_row)` | Absolute screen row | `(line_idx, wrap_offset, byte_start)` |
| `cursor_col_in_row(line_idx, column)` | Buffer line index + column | Column within the wrapped row |

---

## ScreenScroll

Replaces `Viewport::ensure_visible_with_scrolloff` for the GUI, operating on screen rows:

```rust
pub struct ScreenScroll {
    pub first_screen_row: usize,
}

impl ScreenScroll {
    pub fn ensure_visible(
        &mut self,
        cursor_screen_row: usize,
        visible_height: usize,
        scrolloff: usize,
    ) {
        let top = self.first_screen_row + scrolloff;
        let bottom = self.first_screen_row + visible_height - 1 - scrolloff;
        if cursor_screen_row < top {
            self.first_screen_row = cursor_screen_row.saturating_sub(scrolloff);
        } else if cursor_screen_row > bottom {
            self.first_screen_row = (cursor_screen_row + scrolloff + 1).saturating_sub(visible_height);
        }
    }
}
```

bloom-core's `Viewport` still decides which buffer lines to render. `ScreenScroll` adds the screen-row refinement on top.

---

## Gutter Rendering

| Row type | Gutter content | Style |
|----------|---------------|-------|
| First row of buffer line (`wrap_offset == 0`) | Right-aligned line number: `  42 ` | `faded` (normal), `current_line` (cursor line) |
| Continuation row (`wrap_offset > 0`) | Wrap indicator: `   ↪ ` | `faded` always |
| Beyond EOF | Tilde: `  ~  ` | `faded` |

The wrap indicator character is configurable:

```toml
[editor]
wrap_indicator = "↪"    # default
```

---

## Span Clipping

Syntax highlighting spans are byte ranges on the full buffer line. When rendering a wrapped row that covers bytes `[start..end]`, each span is clipped:

```rust
for span in &rendered_line.spans {
    let s = span.range.start.max(row_byte_start);
    let e = span.range.end.min(row_byte_end);
    if s < e {
        // Render span slice [s..e] with offset (s - row_byte_start)
    }
}
```

This reuses the existing span-rendering loop with a byte offset adjustment. No changes needed to bloom-core's highlighting.

---

## Cursor Placement

The cursor is at buffer position `(line, column)`. The GUI translates:

```rust
let screen_row = wrap_map.cursor_screen_row(cursor_line_idx, cursor_column);
let col_in_row = wrap_map.cursor_col_in_row(cursor_line_idx, cursor_column);

let cy = area.y + (screen_row - scroll.first_screen_row) as u16;
let cx = area.x + gutter_width + col_in_row as u16;
f.set_cursor_position((cx, cy));
```

---

## Vim Motion Behavior

| Motion | Behavior | Notes |
|--------|----------|-------|
| `j` / `k` | Move by buffer lines | Unchanged — standard Vim |
| `gj` / `gk` | Move by screen rows (display lines) | Future addition — requires wrap map query from core |
| `0` / `$` | Start/end of buffer line | Unchanged |
| `g0` / `g$` | Start/end of screen row | Future addition |
| `h` / `l` | Move by character | Unchanged — may cross wrap boundaries |

`gj`/`gk` require the core to know about screen rows. The clean approach: the GUI passes a `screen_row_count_for_line(line_idx) -> usize` callback (or the wrap map itself) to the core when processing `gj`/`gk`, rather than the core owning wrapping.

---

## Configuration

```toml
[editor]
word_wrap = true          # on by default; false = horizontal scroll
wrap_indicator = "↪"      # gutter character for continuation rows
```

No `wrap_column` setting — wrapping always uses the pane width. A fixed-column wrap would be a reflow operation (buffer mutation), not a display concern.

---

## What Does NOT Change

- **Buffer** — no newlines inserted, no mutations
- **`RenderedLine`** — still one per buffer line, unchanged struct
- **Syntax highlighting** — computed per buffer line in bloom-core, unchanged
- **bloom-core `Viewport`** — still emits buffer-line ranges; GUI refines to screen rows
- **`CursorState`** — still `(line, column)` in buffer coordinates
- **Undo/redo** — unaffected, wrapping is stateless display
- **Autosave/file watcher** — unaffected

---

## Cross-References

| Document | Section |
|----------|---------|
| [ARCHITECTURE.md](ARCHITECTURE.md) | Render pipeline — core produces RenderFrame, frontends consume |
| [THEMING.md](THEMING.md) | `faded` color for gutter, `current_line` highlight |
| [GOALS.md](GOALS.md) | G7 — Vim motions (j/k vs gj/gk) |
| [DEBUGGABILITY.md](DEBUGGABILITY.md) | Wrap map computation timing at `trace` level |
