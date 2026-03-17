# Bloom 🌱 — GUI Design System

> Visual design language for the Iced Canvas GUI. Pixel-precise specifications
> for spacing, typography, borders, overlays, and animation.
> See [THEMING.md](THEMING.md) for colour palette, [WINDOW_LAYOUTS.md](WINDOW_LAYOUTS.md) for logical layout.

---

## Design Principles

1. **Content is king.** UI chrome recedes; the writer's text dominates. Every pixel of chrome must earn its place.
2. **Consistent spacing.** A single spacing scale governs all padding, margins, and gaps. No magic numbers.
3. **Layered depth.** Base content → drawers → overlays. Each layer is visually distinct through background, border, and shadow treatment.
4. **Keyboard-first signals.** The UI communicates state through the status bar, mode badges, and cursor — not through mouse-oriented affordances (no hover states, no clickable buttons).
5. **Calm defaults, loud when needed.** Most UI is monochrome foreground on background. Colour appears only for state (mode badges), urgency (errors), and task status (checkboxes).

---

## Spacing Scale

All spacing derives from a base unit. The base unit is `FONT_SIZE / 2` (currently 6.5px at 13px font).

| Token | Value | Usage |
|-------|-------|-------|
| `xs` | 2px | Hairline gaps, border insets |
| `sm` | 4px | Inner padding within compact elements (badges, pills) |
| `md` | 8px | Standard padding (status bar sides, gutter margin, overlay insets) |
| `lg` | 16px | Section gaps within overlays, pane-to-drawer spacing |
| `xl` | 24px | Outer margins for centered overlays |

**Rule:** No raw pixel values in draw code. Use these tokens (defined as constants).

---

## Typography

| Element | Font | Size | Weight | Colour |
|---------|------|------|--------|--------|
| Body text | Monospace (JetBrains Mono bundled) | 13px (`FONT_SIZE`) | Normal | `foreground` |
| Line numbers | Monospace | 13px | Normal | `faded` |
| Status bar text | Monospace | 13px | Normal | `foreground` (active), `faded` (inactive) |
| Mode badge | Monospace | 13px | Normal | `background` on mode colour |
| Heading H1 | Monospace | 19.5px (1.5×) | Bold | `strong` |
| Heading H2 | Monospace | 16.9px (1.3×) | Bold | `salient` |
| Heading H3 | Monospace | 14.3px (1.1×) | Bold | `foreground` |
| Overlay title | Monospace | 13px | Normal | `strong` |
| Picker result | Monospace | 13px | Normal | `foreground` (label), `faded` (marginalia) |
| Notification | Monospace | 13px | Normal | Varies by severity |

**Line height:** `FONT_SIZE × 1.4` = 18.2px. This is the universal row height for editor content.

**Text centering:** All text is vertically centered within its row via `TEXT_Y_OFFSET = (LINE_HEIGHT - FONT_SIZE) / 2`.

---

## Line Height & Row Model

Every visible element sits on a row grid:

```
┌──────────────────────────────────────┐
│ TEXT_Y_OFFSET (2.6px)                │
│ ── text baseline ──────────────────  │  ← FONT_SIZE (13px)
│ TEXT_Y_OFFSET (2.6px)                │
├──────────────────────────────────────┤  ← LINE_HEIGHT (18.2px)
│ next row...                          │
```

The **status bar** is an exception: `STATUS_BAR_HEIGHT = LINE_HEIGHT × 1.5` (27.3px) — taller for visual prominence. Text is still centered within it.

---

## Borders & Separators

| Element | Style | Colour | Width |
|---------|-------|--------|-------|
| Pane split (vertical) | Solid line | `faded` | 1px |
| Pane split (horizontal) | Solid line | `faded` | 1px |
| Status bar top edge | Solid line | `faded` | 1px |
| Overlay panel border | Solid stroke | `faded` | 1px |
| Picker separator (results/preview) | Horizontal line | `faded` | 1px |
| Drawer top edge | Solid line | `faded` | 1px |
| Notification border | Solid stroke | `faded` | 1px |

**No rounded corners** in the current monospace Canvas renderer. All rectangles are sharp. Rounded corners may be added when proportional font support lands (Phase 3).

**No shadows.** The layered Stack compositing provides visual depth without drop shadows. The overlay scrim (85% opacity background wash) creates sufficient separation.

---

## Layers & Compositing

The GUI uses `iced::widget::Stack` with two Canvas layers:

```
Stack [
    BaseCanvas      ← Layer 0: panes, status bars, borders, drawers
    OverlayCanvas   ← Layer 1: picker, dialog, date picker, view, inline menu, notifications
]
```

| Layer | Contents | Background treatment |
|-------|----------|---------------------|
| **Base** | Editor panes with content, gutter, cursor. Bottom drawers (which-key, temporal strip, context strip). Split borders. | Opaque `background` fill per pane rect. |
| **Overlay** | Modal overlays that cover base content. | 85% opacity `background` scrim over entire canvas, then opaque panel. |

**Why two layers:** Iced Canvas `fill_rectangle` blends within a single geometry. Text drawn first cannot be "overwritten" by a later fill in the same Canvas. Separate Canvas widgets in a Stack composite correctly — the overlay layer's opaque regions fully cover the base.

---

## Status Bar

The status bar is the most information-dense UI element. It communicates mode, file state, cursor position, and background thread activity.

### Anatomy

```
┌─[MODE]─────── title [+] ──────────────── pending  12:34 ─┐
│ badge  │ sep │  filename  dirty │           right-aligned  │
└────────────────────────────────────────────────────────────┘
```

### Dimensions

| Property | Value |
|----------|-------|
| Height | `STATUS_BAR_HEIGHT` = 1.5 × LINE_HEIGHT (27.3px) |
| Background | `highlight` (active pane), `subtle` (inactive pane) |
| Top border | 1px `faded` line |
| Side padding | `md` (8px) |
| Text vertical centering | `(STATUS_BAR_HEIGHT - LINE_HEIGHT) / 2` offset |

### Mode Badge

The mode badge is a coloured pill at the left edge of the status bar.

| Mode | Badge foreground | Badge background | Rationale |
|------|-----------------|-----------------|-----------|
| NORMAL | `foreground` | `mild` | Calm default |
| INSERT | `background` | `accent_green` | Green = "go", actively writing |
| VISUAL | `background` | `popout` | Selection happening |
| COMMAND | `background` | `accent_blue` | Talking to the editor |
| HIST / DAY / JRNL | `background` | `accent_yellow` | Temporal browsing family |

Badge format: ` MODE ` (space-padded). Badge fills the full `STATUS_BAR_HEIGHT` vertically.

### Elements

| Element | Colour | Position |
|---------|--------|----------|
| Mode badge | See table above | Left edge, full height |
| Filename | `foreground` | After badge + `md` gap |
| Dirty `[+]` | `salient` | After filename |
| Pending keys | `salient` | Right-aligned |
| Macro `@q` | `accent_red` | Right-aligned |
| Line:col | `faded` | Right-aligned, rightmost |
| Thread indicators | `faded` (idle), `salient` (active) | Right-aligned, before line:col |

### Inactive Pane

Only the filename in `faded` on `subtle` background. No badge, no position, no indicators.

---

## Overlays

### Scrim

When an overlay opens, the entire base canvas is dimmed with an 85% opacity `background` wash. This:
- Visually separates the overlay from base content
- Signals "modal — base content is not interactive"
- Maintains readability of the overlay panel

### Panel

All overlays use a centered panel:

| Property | Value |
|----------|-------|
| Background | `background` (opaque) |
| Border | 1px `faded` stroke |
| Inset padding | `md` (8px) on all sides |
| Width | 60% of window (72% for wide pickers like search) |
| Height | 70% of window |
| Min size | 40 chars × 12 rows |

### Picker

```
┌─ Title ──────────────────────────────────────────────┐
│ [md] > query text█                        [filters]  │
│ [md]                                                 │
│ [md] ▸ selected row          middle       right      │  ← mild bg
│ [md]   result row            middle       right      │
│ [md]   result row            middle       right      │
│ [md]                                                 │
│ [md]   N of M noun                                   │  ← faded
├──────────────────────────────────────────────────────┤  ← 1px faded
│ [md]                                                 │
│ [md]   preview content                               │  ← faded text
│ [md]                                                 │
└──────────────────────────────────────────────────────┘
```

- Selected row: `mild` background, full width
- Result columns: label (left), middle (after gap), right (right-aligned)
- Preview: below separator, same panel, `faded` text

### Dialog

Compact centered panel. Message text + choice buttons.

| Property | Value |
|----------|-------|
| Width | Auto (fits content + `xl` padding) |
| Height | Auto (message + choices + `lg` spacing) |
| Selected choice | `mild` background |

### Inline Menu

Anchored to cursor position or command line. No scrim (not modal).

| Property | Value |
|----------|-------|
| Background | `background` (opaque) |
| Border | 1px `faded` stroke |
| Width | max(item width) + `md` padding, capped at 40 chars |
| Max visible items | 8 |
| Selected row | `mild` background |

---

## Drawers

Bottom-anchored panels that push content up. No scrim — they coexist with the editor.

### Which-Key

| Property | Value |
|----------|-------|
| Background | `subtle` (opaque) |
| Top border | 1px `faded` line |
| Grid columns | 24 chars each, up to 4 columns |
| Key | `strong` colour |
| Label | `foreground` |
| Prefix header | `faded` |

### Temporal Strip

| Property | Value |
|----------|-------|
| Background | `highlight` (opaque) |
| Top border | 1px `faded` line |
| Compact height | 4 × LINE_HEIGHT |
| Rich height | 6 × LINE_HEIGHT |
| Selected node | `strong` colour, `▸` prefix |
| Cursor indicator | `▲` in `accent_yellow` |
| Diff preview | Replaces editor content in base layer (not overlaid) |

### Context Strip (Journal)

| Property | Value |
|----------|-------|
| Background | `background` (opaque) |
| Top border | 1px `faded` line |
| Height | 3 × LINE_HEIGHT |
| Three columns | prev (faded), current (foreground, bold label), next (faded) |

---

## Cursor

| Mode | Shape | Colour | Animation |
|------|-------|--------|-----------|
| Normal | Block (full character cell) | `foreground` at 45% opacity | Smooth vertical slide (50ms ease-out) |
| Insert | Bar (2px wide, left edge) | `foreground` | Smooth vertical slide |
| Visual | Block (same as Normal) | `foreground` at 45% opacity | Smooth vertical slide |
| Command | Bar (in command line) | `foreground` | Instant (no animation in command line) |

### Current Line Highlight

Full-width `highlight` background on the cursor's row. Slides with the cursor animation.

### Animation Parameters

| Parameter | Value |
|-----------|-------|
| Lerp factor | 0.6 per frame (60% of remaining distance) |
| Snap threshold | 0.5px (prevents sub-pixel jitter) |
| Convergence time | ~50ms (3 frames at 120Hz) |
| Horizontal motion | Instant (no animation) |

---

## Notifications

Bottom-right stack, above the status bar.

| Property | Value |
|----------|-------|
| Position | Right-aligned, `md` from right edge, `sm` above status bar |
| Max visible | 3 (oldest auto-expiring evicted on 4th) |
| Stack direction | Upward (newest at bottom) |
| Gap between | 4px |
| Background | `subtle` (info), `accent_yellow` (warning), `critical` (error) |
| Border | 1px `faded` stroke |
| Text | `foreground` (info), `background` (warning/error) |
| Prefix | `✓` (info), `⚠` (warning), `✗` (error) |
| Max width | 48 chars |
| Auto-expire | 4s (info), 8s (warning), never (error — manual dismiss) |

---

## Refresh Rate

| State | Tick interval | Subscriptions active |
|-------|--------------|---------------------|
| Idle | None (0 Hz) | `keyboard::listen()` only |
| Active (after keystroke) | 8ms (~120 Hz) | + `AnimTick` |
| Animating (cursor moving) | 8ms (~120 Hz) | + `AnimTick` |
| Settled | Drops to idle | `AnimTick` removed |

Zero CPU when idle. The animation subscription only exists while `animating == true`.

---

## Constants Reference

| Constant | Value | Derivation |
|----------|-------|-----------|
| `FONT_SIZE` | 13.0 | Base font size |
| `LINE_HEIGHT` | 18.2 | `FONT_SIZE × 1.4` |
| `TEXT_Y_OFFSET` | 2.6 | `(LINE_HEIGHT - FONT_SIZE) / 2` |
| `STATUS_BAR_HEIGHT` | 27.3 | `LINE_HEIGHT × 1.5` |
| `CHAR_WIDTH` | 7.8 | `FONT_SIZE × 0.6` (monospace approximation) |
| `GUTTER_CHARS` | 5 | Max 4-digit line number + 1 space |
| `GUTTER_WIDTH` | 39.0 | `GUTTER_CHARS × CHAR_WIDTH` |

These will be recalculated from actual font metrics when proportional font support lands.

---

## Related Documents

| Document | Contents |
|----------|----------|
| [THEMING.md](THEMING.md) | Colour palette, semantic roles, face mapping |
| [WINDOW_LAYOUTS.md](WINDOW_LAYOUTS.md) | Logical layout: split tree, pane navigation, status bar anatomy (TUI wireframes) |
| [ADAPTIVE_LAYOUT.md](ADAPTIVE_LAYOUT.md) | Responsive breakpoints for picker width, column visibility |
| [GUI.md](GUI.md) | GUI architecture: Iced Canvas, IPC bridge, DOM rendering (historical — Tauri era) |
