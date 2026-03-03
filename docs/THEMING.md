# Bloom 🌱 — Theme Design

> Colour philosophy, palette structure, and concrete colour values for Bloom's built-in themes.
> Informed by Nicolas Rougier's [nano-emacs](https://github.com/rougier/nano-theme) semantic face system and Colin McLear's [Lambda-themes](https://github.com/Lambda-Emacs/lambda-themes) palette.
> See Rougier, ["On the Design of Text Editors"](https://arxiv.org/abs/2008.06030) for the theoretical foundation.

---

## Design Principles

1. **Semantic, not syntactic.** Colours map to *meaning* (structural, important, secondary, urgent), not to syntax categories (keyword, string, type). A single semantic role covers many syntax nodes.
2. **Typographic variation over color.** Following Lambda's non-vibrant mode: most text uses the foreground color with weight/style differences (bold, italic, light). Vivid colour is reserved sparingly for structural accents (headings, crucial info). This is what makes Lambda look calm.
3. **Minimal palette.** A theme is defined by exactly **6 semantic roles + 4 surface colours + 2 mid-tones + 4 accent colours** = 16 named slots. Every UI element maps to one of these slots.
4. **Light and dark variants are peers.** Each theme provides both. The same semantic roles exist in both variants; only the concrete hex values differ.
5. **Medium contrast.** Neither washed-out nor neon. Lambda's "decent compromise between aesthetics and readability" is the target.
6. **Terminal-friendly.** All colours must work on 256-colour terminals. The TUI is the primary frontend.
7. **Monospace throughout.** Both TUI and GUI use monospace fonts. Typography is achieved through bold/italic/dim/size — not font family variation. This keeps Vim column operations correct and simplifies the rendering pipeline. The GUI may use font size variation for headings (larger monospace), but each line remains a uniform monospace grid.

---

## Typography

### Font Strategy

| Frontend | Font | Size Variation |
|----------|------|---------------|
| TUI | Terminal's monospace font | None — fixed grid, all characters same size |
| GUI | Configurable monospace (default: system monospace) | Headings rendered at larger sizes (see below) |

### GUI Font Sizes

| Element | Size | Notes |
|---------|------|-------|
| H1 | 1.5× base | Bold, `strong` colour |
| H2 | 1.3× base | Bold, `salient` colour |
| H3 | 1.1× base | Bold, `foreground` |
| H4–H6 | 1.0× (base) | Bold only |
| Body text | 1.0× (base) | Default: 14px equivalent |
| Code block | 1.0× (base) | `subtle` background wash |
| Frontmatter | 0.9× base | `faded`, italic |
| Status bar, picker, UI chrome | 1.0× (base) | Fixed size |

Base font size is user-configurable in `config.toml`:

```toml
[font]
family = "JetBrains Mono"   # any installed monospace font
size = 14                    # base size in points (GUI only)
line_height = 1.6            # line-height multiplier (GUI only)
```

### Line Spacing

Generous line spacing is the single biggest factor in making monospace prose readable. Default `line_height = 1.6` (inspired by iA Writer). The TUI uses the terminal's native line spacing.

### Recommended Fonts

Not shipped with Bloom, but the docs/setup wizard can suggest:

| Font | Character | Notes |
|------|-----------|-------|
| JetBrains Mono | Clean, modern | Free, excellent bold/italic, ligatures optional |
| Berkeley Mono | Premium, elegant | Paid, beautiful for prose |
| Input Mono | Customizable | Free for personal use, width/spacing variants |
| iA Writer Mono | Writing-optimized | Free, designed specifically for prose readability |
| Fira Code | Developer-friendly | Free, good ligatures |

---

## Palette Structure

A Bloom theme is a named collection of 16 colour slots, arranged in four groups.

### Group 1: Semantic Roles (6 colours)

These are the heart of the system, directly adapted from Rougier's nano-theme.

| Slot | Purpose | Usage examples |
|------|---------|---------------|
| **`critical`** | Demands immediate action. High contrast, used sparingly. | Error diagnostics, broken links, merge conflict markers |
| **`popout`** | Attracts attention via hue contrast (pop-out effect). | Visual selection accent, cursor in visual mode |
| **`strong`** | Structural importance. Same hue as default, bolder weight. | Headings, bold text, links, directory names |
| **`salient`** | Important but not urgent. Different hue, similar intensity. | H2 headings, active picker accent, which-key group names, status bar |
| **`faded`** | De-emphasised secondary info. Same hue, lower intensity. | Comments, tags, timestamps, block IDs, marginalia, line numbers, `~` beyond EOF |
| **`subtle`** | Suggests a physical area. Barely perceptible background. | Code block bg, picker surface bg, frontmatter bg |

### Group 2: Surface Colours (4 colours)

These define the "canvas" — backgrounds and chrome.

| Slot | Purpose |
|------|---------|
| **`foreground`** | Default text colour. |
| **`background`** | Default editor background. |
| **`modeline`** | Status bar / mode-line background. Also used as link bg wash. |
| **`highlight`** | Current line highlight, hover background. |

### Group 3: Mid-Tones (2 colours)

Lambda defines these for situations between "subtle background" and "visible UI element".

| Slot | Purpose |
|------|---------|
| **`mild`** | Visual selection background (region). Clearly visible but not loud. |
| **`ultralight`** | Show-paren match bg, secondary selection. Brighter than highlight. |

### Group 4: Accent Colours (4 colours)

Sparingly-used hue accents for situations where the 6 semantic roles aren't sufficient.

| Slot | Purpose |
|------|---------|
| **`accent_red`** | Deletions, removed lines in diffs, overdue tasks. |
| **`accent_green`** | Insertions, added lines, completed tasks `[x]`. |
| **`accent_blue`** | Informational highlights, status bar (command mode). |
| **`accent_yellow`** | Warnings, TODO items, pending states, unchecked tasks. |

---

## Face Mapping (Lambda Non-Vibrant Style)

The key insight from Lambda: **most text renders in `foreground` with typographic variation only.** Colour is an exception, not the norm.

| `Style` variant | Foreground | Background | Decoration | Rationale |
|----------------|------------|------------|------------|-----------|
| `Normal` | `foreground` | — | — | Default text |
| `Heading { 1 }` | `strong` | — | **bold** | Structural accent |
| `Heading { 2 }` | `salient` | — | **bold** | Secondary structural |
| `Heading { 3+ }` | `foreground` | — | **bold** | Weight only, no color |
| `Bold` | — | — | **bold** | Typographic, not chromatic |
| `Italic` | — | — | *italic* | Typographic, not chromatic |
| `Code` | `foreground` | `subtle` | — | Faint bg wash, like Lambda string-face |
| `CodeBlock` | `foreground` | `subtle` | — | Same faint wash |
| `Link` | `strong` | `modeline` | underline | Lambda: strong + lowlight bg + underline |
| `Tag` | `faded` | — | — | Quiet, like comments |
| `Timestamp` | `faded` | — | — | Secondary info |
| `BlockId` | `faded` | — | dim | Even quieter |
| `ListMarker` | `foreground` | — | — | Structural — conveys document hierarchy |
| `CheckboxUnchecked` | `accent_yellow` | — | — | Pending state |
| `CheckboxChecked` | `accent_green` | — | ~~strikethrough~~ | Completed state |
| `Frontmatter` | `faded` | — | *italic* | Like Lambda comment-face |
| `BrokenLink` | `critical` | — | ~~strikethrough~~ | Demands attention |
| `SyntaxNoise` | `faded` | — | dim | Pure syntax markers — see Syntax Semantic Weight below |

---

## Syntax Semantic Weight

Bloom renders Markdown as a styled document, not a code file. This means the `highlight.rs` scanner splits every construct into **marker spans** and **content spans**, each receiving an independent style. Markers are classified into three tiers based on their semantic weight — how much meaning the reader would lose if the marker were invisible.

### Tier 1 — Structural (Full Visibility)

The marker **is** the meaning. Removing it changes what the reader understands.

| Construct | Marker characters | Marker style | Rationale |
|-----------|-------------------|-------------|-----------|
| List item | `-` or `*` or `+` | `ListMarker` (`foreground`) | Conveys document structure. Without it, items become ambiguous paragraphs. |
| Ordered list | `1.`, `2.`, etc. | `ListMarker` (`foreground`) | Conveys ordering. Without it, sequence is lost. |
| Checkbox (unchecked) | `- [ ]` | `CheckboxUnchecked` (`accent_yellow`) | Signals "this needs doing." The `[ ]` IS the status. |
| Checkbox (checked) | `- [x]` | `CheckboxChecked` (`accent_green`, ~~strikethrough~~) | Signals completion. The `[x]` IS the status. |
| Tag | `#` in `#rust` | Same style as tag text (`faded`) | The `#` is part of the tag's identity. Dimming it makes `rust` look like prose. |
| Blockquote | `>` | `faded` | Conveys attribution/quoting. Without it, quoted text blends into the author's voice. |
| Timestamp keyword | `@due`, `@start`, `@at` | `faded` (same as timestamp) | Tells you the *type* of date — deadline vs start vs event. The keyword is the meaning. |

### Tier 2 — Contextual (Present but Subdued)

Carries some information, but context or styling already communicates the construct's role. Rendered visibly but quieter than Tier 1.

| Construct | Marker characters | Marker style | Rationale |
|-----------|-------------------|-------------|-----------|
| Block ID | `^` in `^rope-perf` | `faded` + dim | The `^` signals "this is a reference target." Already very quiet in current theme. |
| Timestamp parens | `(` `)` in `@due(2026-03-05)` | `SyntaxNoise` (`faded` + dim) | Grouping syntax. `@due` already tells you what follows. |
| Frontmatter delimiters | `---` | `faded` + dim | Marks a section boundary, but frontmatter content is already faded/italic — the delimiters can be quieter. |
| Code fence | ` ``` ` and language hint | `faded` + dim | Marks code block boundary. The `subtle` background wash already signals "code zone." |

### Tier 3 — Noise (Dimmed)

Pure parsing syntax. The rendered styling already communicates everything the marker would. These get the `SyntaxNoise` style: `faded` foreground + `dim` decoration.

| Construct | Marker characters | Content style | Rationale |
|-----------|-------------------|--------------|-----------|
| Bold | `**` | `Bold` (foreground, **bold**) | Bold styling is visible. The `**` adds nothing for the reader. |
| Italic | `*` | `Italic` (foreground, *italic*) | Italic styling is visible. The `*` adds nothing. |
| Heading prefix | `#`, `##`, `###`, etc. | Heading style (bold + colour) | Heading styling (bold, size, colour) already signals the level. The `#` markers are line noise. |
| Link brackets | `[[`, `]]`, `\|` | `Link` (strong, underline) | The underline + colour already says "this is a link." |
| Link UUID | UUID portion of `[[uuid\|text]]` | — (not displayed) | Internal ID, meaningless to the reader. Visually suppressed (rendered in `SyntaxNoise` but could be hidden entirely in future). |
| Inline code markers | `` ` `` | `Code` (foreground + subtle bg) | The background wash already signals "code." |

### Rendering Examples

Showing how a document looks with semantic weight applied. Dim markers shown in parenthetical notation for illustration (in the actual UI they'd be faded/dim):

**Raw Markdown:**
```markdown
### Design Decisions

I chose **Rust** for its *memory safety* and performance.
See [[8f3a1b2c|Text Editor Theory]] for background.

- First consideration
- [ ] Review the ropey crate API @due(2026-03-05)
- [x] Read Xi Editor source

#editors #rust
```

**As rendered in Bloom (described):**
```
(###) Design Decisions                    ← (###) dim, "Design Decisions" bold
                                          
I chose (**)Rust(**) for its (*)memory    ← (**) dim, "Rust" bold; (*) dim
safety(*) and performance.                  "memory safety" italic
See ([[)Text Editor Theory(]])            ← ([[ ]]) dim, "Text Editor Theory"
for background.                             teal underline; UUID hidden

- First consideration                     ← "-" normal (structural)
- [ ] Review the ropey crate API          ← "- [ ]" yellow (structural)
      @due(()2026-03-05())                  "@due" faded, "()" dim, date faded
- [x] Read Xi Editor source               ← "- [x]" green strikethrough

#editors #rust                            ← full tag including "#" in faded
```

### UI Chrome Mapping

| Element | Foreground | Background |
|---------|------------|------------|
| Status bar (Normal) | `foreground` | `modeline` |
| Status bar (Insert) | `background` | `accent_green` |
| Status bar (Visual) | `background` | `popout` |
| Status bar (Command) | `background` | `accent_blue` |
| Picker surface | `foreground` | `subtle` |
| Picker selected row | `foreground` | `mild` |
| Picker border | `faded` | — |
| Which-key popup | `foreground` | `subtle` |
| Visual selection (region) | `foreground` | `mild` |
| Current line (hl-line) | — | `highlight` |
| Window border | `faded` | — |
| Tilde `~` beyond EOF | `faded` | — |

---

## Built-In Themes

All colour values from [Lambda-themes](https://github.com/Lambda-Emacs/lambda-themes) by Colin McLear (GPL-3.0). We ship all four Lambda variants.

### Bloom Dark

Lambda `dark` variant — high contrast on a near-black background.

<div style="font-family: monospace; font-size: 13px; line-height: 1.6; border-radius: 8px; overflow: hidden; max-width: 560px; border: 1px solid #37373E;">
<div style="background: #141414; color: #EBE9E7; padding: 12px 16px;">
<span style="color: #A3A3A3">---</span><br>
<span style="color: #A3A3A3; font-style: italic;">id: abc12345</span><br>
<span style="color: #A3A3A3; font-style: italic;">title: "Text Editor Theory"</span><br>
<span style="color: #A3A3A3">---</span><br>
<br>
<span style="color: #F5F2F0; font-weight: bold;">## Rope Data Structure</span><br>
<br>
Ropes are O(log n) for inserts. They use <span style="font-weight: bold;">balanced</span><br>
binary trees to represent text. See <span style="color: #F5F2F0; background: #1A1919; text-decoration: underline;">[[abc123|Piece Tables]]</span><br>
for an alternative approach. <span style="color: #A3A3A3">#editors</span> <span style="color: #A3A3A3">#rust</span><br>
<br>
<span style="color: #A3A3A3">- </span>Each leaf holds a string fragment<br>
<span style="color: #A3A3A3">- </span><span style="color: #F2DA61">[ ]</span> Review the ropey crate API <span style="color: #A3A3A3">@due(2026-03-05)</span><br>
<span style="color: #A3A3A3">- </span><span style="color: #62C554; text-decoration: line-through;">[x] Read Xi Editor source</span><br>
<br>
<span style="background: #37373E; display: inline-block; width: 100%; padding: 2px 4px;">let rope = Rope::from_str("hello");</span><br>
<br>
<span style="color: #A3A3A3">~</span><br>
<span style="color: #A3A3A3">~</span><br>
</div>
<div style="background: #F4BF4F; color: #141414; padding: 3px 12px; font-weight: bold; font-size: 11px; display: flex; justify-content: space-between;">
<span>NORMAL │ Text Editor Theory [+]</span><span>12:1 │ markdown</span>
</div>
</div>

| Slot | Hex | Lambda name |
|------|-----|-------------|
| foreground | `#EBE9E7` | fg |
| background | `#141414` | bg |
| modeline | `#1A1919` | lowlight |
| highlight | `#212228` | highlight |
| critical | `#CF6752` | urgent |
| popout | `#7A9EFF` | focus |
| strong | `#F5F2F0` | strong |
| salient | `#F4BF4F` | crucial |
| faded | `#A3A3A3` | meek |
| subtle | `#37373E` | faint |
| mild | `#474648` | mild |
| ultralight | `#2C2C34` | ultralight |
| accent_red | `#EC6A5E` | red |
| accent_green | `#62C554` | green |
| accent_blue | `#81A1C1` | blue |
| accent_yellow | `#F2DA61` | yellow |

### Bloom Dark Faded

Lambda `dark-faded` variant — softer, Nord-influenced, easier on the eyes for long sessions.

<div style="font-family: monospace; font-size: 13px; line-height: 1.6; border-radius: 8px; overflow: hidden; max-width: 560px; border: 1px solid #333A47;">
<div style="background: #282B35; color: #ECEFF1; padding: 12px 16px;">
<span style="color: #959EB1">---</span><br>
<span style="color: #959EB1; font-style: italic;">id: abc12345</span><br>
<span style="color: #959EB1; font-style: italic;">title: "Text Editor Theory"</span><br>
<span style="color: #959EB1">---</span><br>
<br>
<span style="color: #FFFFFF; font-weight: bold;">## Rope Data Structure</span><br>
<br>
Ropes are O(log n) for inserts. They use <span style="font-weight: bold;">balanced</span><br>
binary trees to represent text. See <span style="color: #FFFFFF; background: #3C4353; text-decoration: underline;">[[abc123|Piece Tables]]</span><br>
for an alternative approach. <span style="color: #959EB1">#editors</span> <span style="color: #959EB1">#rust</span><br>
<br>
<span style="color: #959EB1">- </span>Each leaf holds a string fragment<br>
<span style="color: #959EB1">- </span><span style="color: #E9B85D">[ ]</span> Review the ropey crate API <span style="color: #959EB1">@due(2026-03-05)</span><br>
<span style="color: #959EB1">- </span><span style="color: #8EB89D; text-decoration: line-through;">[x] Read Xi Editor source</span><br>
<br>
<span style="background: #333A47; display: inline-block; width: 100%; padding: 2px 4px;">let rope = Rope::from_str("hello");</span><br>
<br>
<span style="color: #959EB1">~</span><br>
<span style="color: #959EB1">~</span><br>
</div>
<div style="background: #88C0D0; color: #282B35; padding: 3px 12px; font-weight: bold; font-size: 11px; display: flex; justify-content: space-between;">
<span>NORMAL │ Text Editor Theory [+]</span><span>12:1 │ markdown</span>
</div>
</div>

| Slot | Hex | Lambda name |
|------|-----|-------------|
| foreground | `#ECEFF1` | fg |
| background | `#282B35` | bg |
| modeline | `#3C4353` | lowlight |
| highlight | `#444B5C` | highlight |
| critical | `#F46715` | urgent |
| popout | `#BC85FF` | focus |
| strong | `#FFFFFF` | strong |
| salient | `#88C0D0` | crucial |
| faded | `#959EB1` | meek |
| subtle | `#333A47` | faint |
| mild | `#8791A7` | mild |
| ultralight | `#525868` | ultralight |
| accent_red | `#BF616A` | red |
| accent_green | `#8EB89D` | green |
| accent_blue | `#81A1C1` | blue |
| accent_yellow | `#E9B85D` | yellow |

### Bloom Light

Lambda `light` variant — warm near-white background with strong contrast.

<div style="font-family: monospace; font-size: 13px; line-height: 1.6; border-radius: 8px; overflow: hidden; max-width: 560px; border: 1px solid #706F6F;">
<div style="background: #FFFEFD; color: #0C0D0D; padding: 12px 16px;">
<span style="color: #706F6F">---</span><br>
<span style="color: #706F6F; font-style: italic;">id: abc12345</span><br>
<span style="color: #706F6F; font-style: italic;">title: "Text Editor Theory"</span><br>
<span style="color: #706F6F">---</span><br>
<br>
<span style="color: #000000; font-weight: bold;">## Rope Data Structure</span><br>
<br>
Ropes are O(log n) for inserts. They use <span style="font-weight: bold;">balanced</span><br>
binary trees to represent text. See <span style="color: #000000; background: #F8F6F4; text-decoration: underline;">[[abc123|Piece Tables]]</span><br>
for an alternative approach. <span style="color: #706F6F">#editors</span> <span style="color: #706F6F">#rust</span><br>
<br>
<span style="color: #706F6F">- </span>Each leaf holds a string fragment<br>
<span style="color: #706F6F">- </span><span style="color: #E0A500">[ ]</span> Review the ropey crate API <span style="color: #706F6F">@due(2026-03-05)</span><br>
<span style="color: #706F6F">- </span><span style="color: #005A02; text-decoration: line-through;">[x] Read Xi Editor source</span><br>
<br>
<span style="background: #E3E1E0; display: inline-block; width: 100%; padding: 2px 4px;">let rope = Rope::from_str("hello");</span><br>
<br>
<span style="color: #706F6F">~</span><br>
<span style="color: #706F6F">~</span><br>
</div>
<div style="background: #5D00DA; color: #FFFEFD; padding: 3px 12px; font-weight: bold; font-size: 11px; display: flex; justify-content: space-between;">
<span>NORMAL │ Text Editor Theory [+]</span><span>12:1 │ markdown</span>
</div>
</div>

| Slot | Hex | Lambda name |
|------|-----|-------------|
| foreground | `#0C0D0D` | fg |
| background | `#FFFEFD` | bg |
| modeline | `#F8F6F4` | lowlight |
| highlight | `#F5F2F0` | highlight |
| critical | `#B30000` | urgent |
| popout | `#0044CC` | focus |
| strong | `#000000` | strong |
| salient | `#5D00DA` | crucial |
| faded | `#706F6F` | meek |
| subtle | `#E3E1E0` | faint |
| mild | `#C1C1C1` | mild |
| ultralight | `#EBE9E7` | ultralight |
| accent_red | `#EC6A5E` | red |
| accent_green | `#005A02` | green |
| accent_blue | `#4C4CFF` | blue |
| accent_yellow | `#E0A500` | yellow |

### Bloom Light Faded

Lambda `light-faded` variant — softer, muted light with cooler tones.

<div style="font-family: monospace; font-size: 13px; line-height: 1.6; border-radius: 8px; overflow: hidden; max-width: 560px; border: 1px solid #727D97;">
<div style="background: #FCFAF6; color: #282B35; padding: 12px 16px;">
<span style="color: #727D97">---</span><br>
<span style="color: #727D97; font-style: italic;">id: abc12345</span><br>
<span style="color: #727D97; font-style: italic;">title: "Text Editor Theory"</span><br>
<span style="color: #727D97">---</span><br>
<br>
<span style="color: #000000; font-weight: bold;">## Rope Data Structure</span><br>
<br>
Ropes are O(log n) for inserts. They use <span style="font-weight: bold;">balanced</span><br>
binary trees to represent text. See <span style="color: #000000; background: #E3E7EF; text-decoration: underline;">[[abc123|Piece Tables]]</span><br>
for an alternative approach. <span style="color: #727D97">#editors</span> <span style="color: #727D97">#rust</span><br>
<br>
<span style="color: #727D97">- </span>Each leaf holds a string fragment<br>
<span style="color: #727D97">- </span><span style="color: #E0A500">[ ]</span> Review the ropey crate API <span style="color: #727D97">@due(2026-03-05)</span><br>
<span style="color: #727D97">- </span><span style="color: #00796B; text-decoration: line-through;">[x] Read Xi Editor source</span><br>
<br>
<span style="background: #ECEFF1; display: inline-block; width: 100%; padding: 2px 4px;">let rope = Rope::from_str("hello");</span><br>
<br>
<span style="color: #727D97">~</span><br>
<span style="color: #727D97">~</span><br>
</div>
<div style="background: #303DB4; color: #FCFAF6; padding: 3px 12px; font-weight: bold; font-size: 11px; display: flex; justify-content: space-between;">
<span>NORMAL │ Text Editor Theory [+]</span><span>12:1 │ markdown</span>
</div>
</div>

| Slot | Hex | Lambda name |
|------|-----|-------------|
| foreground | `#282B35` | fg |
| background | `#FCFAF6` | bg |
| modeline | `#E3E7EF` | lowlight |
| highlight | `#DBE1EB` | highlight |
| critical | `#F53137` | urgent |
| popout | `#940B96` | focus |
| strong | `#000000` | strong |
| salient | `#303DB4` | crucial |
| faded | `#727D97` | meek |
| subtle | `#ECEFF1` | faint |
| mild | `#C8CDD8` | mild |
| ultralight | `#CFD6E2` | ultralight |
| accent_red | `#960D36` | red |
| accent_green | `#00796B` | green |
| accent_blue | `#30608C` | blue |
| accent_yellow | `#E0A500` | yellow |

---

## Rust Representation

```rust
pub struct ThemePalette {
    // Surface colours
    pub foreground: Rgb,    // e.g. #EBE9E7
    pub background: Rgb,    // e.g. #141414
    pub modeline: Rgb,      // e.g. #1A1919  (lowlight)
    pub highlight: Rgb,     // e.g. #212228

    // Semantic roles (Rougier's 6 faces)
    pub critical: Rgb,      // e.g. #CF6752  (urgent)
    pub popout: Rgb,        // e.g. #7A9EFF  (focus)
    pub strong: Rgb,        // e.g. #F5F2F0
    pub salient: Rgb,       // e.g. #F4BF4F  (crucial)
    pub faded: Rgb,         // e.g. #A3A3A3  (meek)
    pub subtle: Rgb,        // e.g. #37373E  (faint)

    // Mid-tones (Lambda additions)
    pub mild: Rgb,          // e.g. #474648  (region selection)
    pub ultralight: Rgb,    // e.g. #2C2C34  (paren match)

    // Accent colours
    pub accent_red: Rgb,    // e.g. #EC6A5E
    pub accent_green: Rgb,  // e.g. #62C554
    pub accent_blue: Rgb,   // e.g. #81A1C1
    pub accent_yellow: Rgb, // e.g. #F2DA61
}
```

---

## User Customisation

Themes are defined in `config.toml`:

```toml
[theme]
name = "bloom-dark"   # "bloom-light", "bloom-dark", "bloom-dark-faded", "bloom-light-faded", or "custom"

# Override individual slots (optional)
[theme.overrides]
salient = "#5E81AC"
accent_green = "#A3BE8C"
```

`:theme` command cycles through available themes. `:theme <name>` selects a specific theme. Changes take effect immediately.

---

## Design Rationale

| Decision | Why |
|----------|-----|
| 6 semantic roles (not 20+ syntax colours) | Rougier's research shows 6 cognitive categories are sufficient; more adds noise without aiding comprehension. |
| Typographic variation over color | Lambda's non-vibrant default proves that weight/italic/bg-wash differentiation is calmer and more readable than painting every syntax node a different color. Most faces use `foreground`. |
| `mild` and `ultralight` mid-tones | Lambda uses these for `region` (selection) and `show-paren-match`. Without them, selection either blends into `highlight` (too faint) or jumps to an accent (too loud). |
| 4 accent colours beyond semantic roles | Pure nano-theme is intentionally austere. Bloom needs task-status colours (green/red for checkboxes) and diff-awareness. Bespoke-themes proved this expansion works. |
| Links use `strong` + `lowlight` bg | Lambda maps links this way. The underline + subtle bg wash is more readable than a coloured foreground, and keeps the page calm. |
| Tags use `faded` (not `accent_blue`) | Tags are metadata, not content. They should recede like comments, not draw the eye. |
| Frontmatter uses `faded` + italic | Same principle as Lambda's `font-lock-comment-face`: italic + dimmed, no background band. |

---

## References

- Rougier, N. P. (2020). [On the Design of Text Editors](https://arxiv.org/abs/2008.06030). arXiv:2008.06030.
- [nano-theme](https://github.com/rougier/nano-theme) — 1+6 semantic face system for GNU Emacs.
- [Lambda-themes](https://github.com/Lambda-Emacs/lambda-themes) — Bloom's colour source. 4 palettes (dark, dark-faded, light, light-faded) by Colin McLear, evolved from bespoke-themes and nano-emacs. GPL-3.0.
- [bespoke-themes](https://github.com/mclear-tools/bespoke-themes) — 6 core + 5 accent colour expansion of nano's approach.
- [Nord](https://www.nordtheme.com/) — Arctic, north-bluish colour palette (influences Lambda dark-faded).
- [Material Design colour system](https://material.io/design/color/) — Google's colour guidelines (influences Lambda light).
