# Bloom 🌱 — GUI Design

> Tauri-based GUI frontend. The delta from the TUI — only what's different because it's a GUI.
> See [ARCHITECTURE.md](ARCHITECTURE.md) for the shared core, [THEMING.md](THEMING.md) for palette and typography.

---

## Architecture

### IPC Bridge

```
bloom-core (Rust)                    Frontend (TypeScript)
─────────────────                    ─────────────────────
BloomEditor                          
    │                                
    │ render(w, h) → RenderFrame     
    │                                
    ├──▶ serialize to JSON ──────▶  receive via Tauri IPC
    │                                    │
    │                                    ▼
    │                                render to DOM/canvas
    │                                    │
    │◀── key/mouse events ◀──────── Tauri command invoke
    │                                
    │ handle_key() / handle_mouse()  
```

The Rust backend owns `BloomEditor` and all state. The TypeScript frontend is a pure render target — it receives frames and sends input events. No editor logic in TypeScript.

### Tauri Commands

| Command | Direction | Payload |
|---------|-----------|---------|
| `render` | Rust → JS | `RenderFrame` as JSON |
| `key_event` | JS → Rust | `KeyEvent` (key code, modifiers) |
| `mouse_event` | JS → Rust | `MouseEvent` (position, button, kind) |
| `resize` | JS → Rust | `(width_px, height_px)` |
| `focus` | JS → Rust | Window gained/lost focus |

The backend emits `render` events on state change (key processed, index complete, notification). No polling — the frontend subscribes to a Tauri event channel.

### Crate Structure

```
crates/
├── bloom-core/       ← shared: editor, vim, buffer, parser, index, query, themes
├── bloom-tui/        ← ratatui frontend (existing)
└── bloom-gui/        ← Tauri frontend (new)
    ├── src/
    │   └── main.rs   ← Tauri app setup, BloomEditor, command handlers
    ├── frontend/     ← TypeScript + HTML + CSS
    │   ├── index.html
    │   ├── main.ts
    │   ├── render.ts ← RenderFrame → DOM
    │   ├── input.ts  ← keyboard/mouse → Tauri commands
    │   └── theme.ts  ← ThemePalette → CSS custom properties
    └── tauri.conf.json
```

`bloom-gui/src/main.rs` is structurally identical to `bloom-tui/src/main.rs` — creates `BloomEditor`, runs the event loop, dispatches to core. The difference is output: JSON over IPC instead of terminal escape codes.

---

## Text Rendering Strategy

### Canvas, Not DOM

Editor content renders on a `<canvas>` element. Reasons:

- **Pixel-precise cursor placement.** The cursor must land exactly on character boundaries. DOM layout is async and imprecise for this.
- **Monospace grid control.** Even with variable heading sizes, each line is a monospace row. Canvas gives exact control over glyph positioning.
- **Performance.** A 50-line viewport is 50 `fillText` calls. DOM would be 50 `<div>`s × N `<span>`s with style recalc on every frame.
- **Consistent with terminal model.** The TUI paints cells; the GUI paints pixels. Same mental model, different resolution.

### What Uses DOM

- **Picker overlay** — `<div>` with `<input>` for the query field. Needs native text input, IME, clipboard.
- **Status bar** — simple `<div>`, updates infrequently.
- **Which-key popup** — `<div>` positioned at bottom of viewport.
- **Dialogs / notifications** — `<div>` overlays.
- **Setup wizard** — full-page DOM, no canvas.

The rule: **canvas for editor content, DOM for UI chrome and input fields.**

### Canvas Rendering Loop

```typescript
function renderFrame(frame: RenderFrame) {
    const ctx = canvas.getContext('2d');
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    
    for (const pane of frame.panes) {
        renderPane(ctx, pane);
    }
    renderCursor(ctx, frame);
}

function renderPane(ctx: CanvasRenderingContext2D, pane: PaneFrame) {
    const { x, y, width, content_height } = pane.rect;
    
    for (let row = 0; row < pane.visible_lines.length; row++) {
        const line = pane.visible_lines[row];
        const lineY = y + row * lineHeight;
        
        // Gutter
        renderGutter(ctx, line, lineY, x);
        
        // Styled spans
        let cursorX = x + gutterWidth;
        for (const span of line.spans) {
            const text = line.text.substring(span.range.start, span.range.end);
            const style = resolveStyle(span.style);
            ctx.font = style.font;
            ctx.fillStyle = style.color;
            ctx.fillText(text, cursorX, lineY + baseline);
            cursorX += ctx.measureText(text).width;
        }
    }
}
```

---

## Font & Typography

### Font Stack

```css
--font-mono: 'JetBrains Mono', 'Fira Code', 'SF Mono', 'Cascadia Code', 'Consolas', monospace;
```

User-configurable in `config.toml`:

```toml
[font]
family = "JetBrains Mono"
size = 14
line_height = 1.6
```

### Heading Sizes

The GUI uses font size variation for headings — the TUI can't do this (fixed grid). From [THEMING.md](THEMING.md):

| Element | Size | Font |
|---------|------|------|
| H1 | 1.5× base | Bold, `strong` colour |
| H2 | 1.3× base | Bold, `salient` colour |
| H3 | 1.1× base | Bold, `foreground` |
| H4–H6 | 1.0× base | Bold only |
| Body | 1.0× base (14px default) | Normal weight |
| Code block | 1.0× base | `subtle` background |
| Frontmatter | 0.9× base | Italic, `faded` |

Each line still occupies a monospace grid row — the heading's larger font renders within a taller row. The `MeasureWidth` implementation accounts for this:

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

### Line Height

Default `line_height = 1.6` — generous spacing for readable prose (inspired by iA Writer). Each line row is `font_size × line_height` pixels tall. The gutter, cursor, and current-line highlight all use this height.

---

## Cursor

### Shapes

| Mode | Shape | Rendering |
|------|-------|-----------|
| Normal | Block | Filled rectangle over one character cell, `foreground` colour at ~50% opacity |
| Insert | Bar | 2px vertical line at left edge of character cell |
| Visual | Block | Same as Normal, selection highlight covers the range |
| Command | Bar | Same as Insert |

### Blink

Cursor blinks in Insert mode: 530ms on, 530ms off (macOS default). No blink in Normal mode — the block cursor is always visible. Blink resets on any keypress.

### IME Composition

When an IME is active (CJK input, accent composition), the canvas renders the composition string inline at the cursor position with an underline. The IME candidate window is positioned by Tauri's native IME support — the frontend reports the cursor pixel position via `window.inputMethodEditor.setCandidateWindowRect()`.

---

## Mouse Input

The TUI is keyboard-only. The GUI adds mouse support.

### Click-to-Position

A click in the editor content area maps pixel `(x, y)` to a buffer position:

```
pixel_y → screen_row = (y - pane.rect.y) / line_height
screen_row → (line_idx, wrap_offset) via ScreenMap
pixel_x → column = character offset via font.measureText() bisection
(line_idx, column) → char offset in rope → set_cursor()
```

### Selection

| Gesture | Selection |
|---------|-----------|
| Click | Place cursor |
| Click + drag | Character-level selection (Visual mode) |
| Double-click | Select word (viw equivalent) |
| Triple-click | Select line (V equivalent) |
| Shift + click | Extend selection from cursor to click point |

Mouse selection enters Visual mode in core — same state as pressing `v` and moving with motions. The selection is stored as a Vim Visual range, not a separate GUI concept.

### Scroll

| Input | Behaviour |
|-------|-----------|
| Mouse wheel | Scroll 3 lines per tick (configurable) |
| Trackpad | Smooth pixel-level scroll with momentum |
| Scroll bar | Thin, auto-hiding, right edge. Click-and-drag or click-to-jump. |

Scrolling updates `viewport.first_visible_line`. The core re-renders with the new viewport.

---

## Window Management

### Split Panes

Core computes `PaneRectFrame` with `(x, y, width, content_height)` in cell units. The GUI converts to pixels:

```typescript
const paneX = pane.rect.x * charWidth;
const paneY = pane.rect.y * lineHeight;
const paneW = pane.rect.width * charWidth;
const paneH = pane.rect.total_height * lineHeight;
```

Each pane is a clipped region on the canvas. Borders are drawn as 1px lines at pane boundaries in `faded` colour.

### Resize Handles

Pane borders are draggable. A 4px invisible hit zone around each border detects hover (cursor changes to resize arrow) and drag. Dragging sends a resize event to core, which adjusts the split ratio.

### Title Bar

- **macOS:** Native title bar with traffic lights. Window title shows the active page name.
- **Windows:** Custom title bar (Tauri `decorations: false`) matching the theme. Minimize/maximize/close buttons rendered in the title bar area.

---

## Platform Shortcuts

Checked before Vim layer (same as TUI, but with platform-native keys):

| macOS | Windows | Action |
|-------|---------|--------|
| Cmd+S | Ctrl+S | Save |
| Cmd+Z | Ctrl+Z | Undo (maps to `u` in Normal) |
| Cmd+Shift+Z | Ctrl+Y | Redo (maps to `Ctrl+R` in Normal) |
| Cmd+C | Ctrl+C | Copy (yank to system clipboard) |
| Cmd+V | Ctrl+V | Paste (put from system clipboard) |
| Cmd+A | Ctrl+A | Select all (enters Visual line mode) |
| Cmd+W | Ctrl+W | Close window/pane |
| Cmd+Q | Alt+F4 | Quit |
| Cmd+N | Ctrl+N | New page (maps to `SPC n`) |
| Cmd++ | Ctrl++ | Increase font size |
| Cmd+- | Ctrl+- | Decrease font size |
| Cmd+0 | Ctrl+0 | Reset font size |

These are intercepted in the frontend's key handler and translated to core `KeyEvent`s or direct `BloomEditor` method calls.

---

## Images

The TUI can't render images. The GUI can.

When a line contains `![alt](path)`, the renderer inserts an `<img>` element (DOM, not canvas) positioned over the canvas at the correct line position. The image is loaded from `{vault_root}/images/{path}`.

Image lines consume N screen rows based on the rendered image height divided by `lineHeight`. The `ScreenMap` entry for an image line has `row_count = ceil(img_height / line_height)`.

Images are display-only — clicking on the image area does not place the cursor. The cursor skips over image lines (same pattern as query result blocks in the original design, if we ever had them).

---

## Performance

### Rendering Budget

Target: **16ms per frame** (60fps). The critical path:

```
RenderFrame serialization (Rust → JSON): < 1ms
JSON parse (JS): < 1ms  
Canvas paint: < 5ms (50 lines × styled spans)
Total: < 7ms — well within budget
```

### Virtual Scrolling

Only visible lines are rendered. The core already produces only `viewport.height` lines in `visible_lines`. The frontend paints exactly what it receives — no virtual scrolling logic needed in TypeScript.

### Large Files

Files with 10K+ lines: the core's viewport handles this (only visible lines are rendered). The GUI adds no overhead — it paints what the core sends.

### Debouncing

- Key events: no debounce (immediate dispatch to core).
- Resize events: 50ms debounce (avoid render storm during window drag).
- Scroll events: requestAnimationFrame batching (at most one render per frame).

---

## Packaging

### Binary

Tauri produces a single binary per platform with the web frontend bundled inside. No external runtime (no Electron, no Node.js, no browser dependency).

| Platform | Format | Expected size |
|----------|--------|---------------|
| macOS | `.dmg` with `.app` bundle | ~15-20 MB |
| Windows | `.msi` installer | ~15-20 MB |

Size is dominated by: Rust binary (~10 MB) + Tauri runtime (~3 MB) + web frontend (~1 MB) + system webview (provided by OS).

### Auto-Update

Tauri has built-in auto-update support. Bloom checks for updates on launch (configurable, off by default — local-first principle). Updates are signed and verified.

---

## What's NOT in This Doc

Everything that's the same between TUI and GUI — which is most of Bloom:

- Editor engine, Vim state machine, motions, operators, text objects
- Buffer management, undo tree, auto-save, atomic writes
- Parser, highlighter, Bloom extensions
- Index, search, backlinks, tags, tasks
- BQL query language and named views
- Picker logic, ranking, frecency
- Window manager (binary split tree, layout computation)
- Theme palette (semantic slots, contrast targets)
- Keybinding architecture (leader keys, which-key)
- All of this lives in bloom-core and is shared unchanged.

---

## Implementation Order

1. **Scaffold** — Tauri project, blank window, IPC bridge, key event routing.
2. **Canvas text** — render `RenderedLine` spans with monospace font. Gutter. Background.
3. **Cursor** — block/bar shapes, blink, current-line highlight.
4. **Theme** — `ThemePalette` → CSS custom properties → canvas colours.
5. **Picker** — DOM overlay for picker, connected to `PickerFrame`.
6. **Status bar** — DOM element, connected to `StatusBarFrame`.
7. **Split panes** — multiple canvas clip regions from `PaneRectFrame`.
8. **Mouse** — click-to-position, selection, scroll.
9. **Heading sizes** — variable font size per heading level.
10. **Platform shortcuts** — Cmd/Ctrl mappings.
11. **Which-key, dialogs, notifications** — DOM overlays.
12. **Images** — inline `<img>` elements positioned over canvas.
13. **Packaging** — installers, auto-update, code signing.

Steps 1–6 produce a usable editor. Steps 7–13 reach feature parity with the TUI and add GUI-only features.
