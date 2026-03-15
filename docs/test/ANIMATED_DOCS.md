# Animated Documentation Pipeline

> Auto-generated GIFs from real editor sessions — theme-agnostic wireframe animations
> that stay accurate as features evolve.

---

## The Idea

Every feature in the user docs gets an animated wireframe GIF showing the behavior.
Not a screenshot (stale after every theme change). Not a screen recording (OS-specific,
hard to reproduce). A **generated animation** from the test harness — real editor output
rendered as clean monospace wireframes.

```
┌─ Rust Project ─────────────────┐ ┌─ This Week ────────────────────┐
│ ## Tasks                       │ │ ## Priority Tasks              │
│                                │ │                                │
│ - [ ] Review ropey + petgraph  │ │ - [ ] Review ropey + petgraph  │
│        ▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂    │ │        ▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂  │
│   @due(2026-03-16) ^=mir01     │ │   @due(2026-03-16) ^=mir01    │
│ - [ ] Benchmark insert/delete  │ │ - [ ] Process inbox            │
│                                │ │                                │
├────────────────────────────────┤ ├────────────────────────────────┤
│ NORMAL  rust-project.md  14:20 │ │ NORMAL  this-week.md     12:1 │
└────────────────────────────────┘ └────────────────────────────────┘
  Edit in one pane → mirror updates in the other
```

## Architecture

### Three layers

```
Test Harness (SimInput + RenderFrame)
    │
    │  key sequences → frame snapshots (JSON)
    ▼
Frame Recorder (Rust, in bloom-test-harness)
    │
    │  RenderFrame[] → frames.json
    ▼
SVG/GIF Renderer (Node.js script)
    │
    │  frames.json → SVG frames → animated GIF
    ▼
docs/static/animations/feature-name.gif
```

### Layer 1: Frame Recorder

Extends the existing test harness. A `FrameRecorder` wraps `SimInput` and captures
a `RenderFrame` after each step.

```rust
let mut rec = FrameRecorder::new(SimInput::with_vault(vault));

rec.step("SPC p p");             // open pages picker
rec.step_type("Rust");           // type search
rec.step("Enter");               // select
rec.pause(500);                  // visual pause in GIF
rec.step("14gg");                // go to line 14
rec.step("f+");                  // move to "+"
rec.step("ciw");                 // change inner word
rec.step_type("petgraph");       // type replacement
rec.step("<Esc>");               // exit insert
rec.caption("Mirror propagates to This Week");

rec.save("mirror-propagation");  // → target/animations/mirror-propagation.json
```

Each step records:
- The `RenderFrame` (panes, cursor, status bar, visible lines)
- A timestamp (for animation timing)
- An optional caption (rendered below the wireframe)

Output: `target/animations/{name}.json` — array of frames with timing.

### Layer 2: SVG Renderer

A Node.js script (or Rust binary) that reads `frames.json` and renders each frame
as an SVG image:

**Visual style:**
- Background: `#1a1a2e` (dark blue-grey)
- Text: `#e0e0e0` (light grey)
- Cursor: `#6c63ff` (accent purple) — block cursor, one character
- Cursor line: `#252540` (subtle highlight)
- Status bar: `#2a2a4a` background
- Pane border: `#3a3a5a` (thin line)
- Mirror highlight: brief flash on updated line
- Caption: `#a0a0a0` below the frame, smaller font

**Font:** Monospace (rendered as SVG `<text>` with `font-family: monospace`).
No actual font file needed — SVG text works everywhere.

**One accent color only.** The wireframe is intentionally minimal — structure over
decoration. Color is used for: cursor, cursor line, brief highlight on change.
Everything else is grey-on-dark.

**Frame size:** 80×24 characters (standard terminal). Each character cell: 8×16px.
Frame: 640×384px + status bar + caption. Final GIF: ~700×450px.

### Layer 3: GIF Assembly

SVG frames → PNG (via `resvg` or Puppeteer) → GIF (via `gifski` for high quality
or `ImageMagick convert`).

**Timing:**
- Normal step: 100ms between frames (fast, shows responsiveness)
- Typing: 60ms per character (natural typing speed)
- Pause: configurable (500ms default, for "look at this" moments)
- Caption change: 1500ms hold (readable)
- Loop: GIF loops with 2000ms pause at end before restart

### Alternative: APNG or WebP

GIF is universally supported but limited to 256 colors. For the wireframe style
(~5 colors), this is fine. If we ever need more, APNG (animated PNG) or WebP
offer full color and better compression. Docusaurus/MDX can embed any image format.

---

## Integration with Docs Site

### Docusaurus setup

```
docs-site/
  docusaurus.config.js
  docs/
    getting-started.md
    features/
      mirroring.md        ← references /animations/mirror-propagation.gif
      journal.md
      views.md
  static/
    animations/
      mirror-propagation.gif
      journal-day-hopping.gif
      agenda-toggle.gif
```

In MDX:

```mdx
## Block Mirroring

Edit a mirrored block in one pane — all copies update instantly.

![Mirror propagation](/animations/mirror-propagation.gif)
```

### CI integration

```yaml
# .github/workflows/docs.yml
- name: Generate animations
  run: cargo test -p bloom-test-harness --test animations
- name: Render GIFs
  run: node scripts/render-animations.js
- name: Build docs
  run: cd docs-site && npm run build
- name: Deploy to GitHub Pages
  uses: peaceiris/actions-gh-pages@v3
```

Animations are regenerated on every release. If a feature changes behavior,
the GIF updates automatically (the test harness produces different frames).

---

## Planned Animations

| Feature | Scenario | Key moments |
|---------|----------|-------------|
| **Mirror propagation** | Edit task in pane A → pane B updates | Two panes, type in one, highlight flash in other |
| **Mirror sever** | SPC m s on mirrored line | Status bar shows mirror hint, sever notification |
| **Journal day-hopping** | `[d` / `]d` skip empty days | Context strip updates, page changes |
| **Agenda toggle** | `x` on task in Agenda view | Checkbox flips, view re-renders |
| **Theme switching** | SPC T t, arrow through themes | Live preview, colors change per selection |
| **BQL query** | SPC v v, type query, see results | Query prompt → results appear |
| **Block ID assignment** | Open a new page, save | IDs appear at end of lines (faded) |
| **Undo tree** | Edit → undo → edit (branch) | Visual undo tree shows branching |
| **Link following** | Cursor on `[[link]]`, Enter | Page opens, cursor at target |
| **Quick capture** | SPC x a, type task, Enter | Task appended to journal |

---

## Design Principles

1. **Theme-agnostic.** Wireframe style with one accent color. No Bloom theme applied.
   The GIF shows *behavior*, not *appearance*. Users choose their own theme.

2. **Always accurate.** Generated from the test harness, not hand-drawn. If the
   feature changes, the GIF changes. Stale docs are impossible.

3. **Fast and small.** ~5 colors, 80×24 grid, 10-30 frames per GIF. Target: <200KB
   per animation. Total animations budget: <5MB for the entire docs site.

4. **Accessible.** Each GIF has a caption below describing what's happening. The
   docs text explains the feature; the GIF demonstrates it. Content is never
   GIF-only — screen readers get the text.

5. **CI-generated.** `cargo test` produces frame data. A script renders GIFs.
   GitHub Actions deploys to Pages. No manual steps.

---

## Implementation Plan

1. **`FrameRecorder`** in `bloom-test-harness` — wraps SimInput, captures RenderFrame per step
2. **Animation test file** — `tests/animations.rs` with one test per planned GIF
3. **SVG renderer** — `scripts/render-animations.js` (Node.js, reads JSON, outputs SVG)
4. **GIF assembly** — `scripts/assemble-gifs.sh` (gifski or ImageMagick)
5. **Docusaurus scaffold** — `docs-site/` with feature pages and GIF references
6. **CI workflow** — generate + render + build + deploy

Steps 1-2 are Rust (in the repo). Steps 3-5 are the docs toolchain. Step 6 ties them together.
