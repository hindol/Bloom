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
5. **Medium contrast.** Neither washed-out nor neon. Lambda's "decent compromise between aesthetics and readability" is the target. Concrete targets:

   | Role | Contrast ratio vs background | Rationale |
   |------|------------------------------|-----------|
   | `foreground` | **11–12 : 1** | Comfortable for extended reading. iA Writer ~11:1, VS Code dark ~12:1. WCAG AA minimum is 4.5:1. |
   | `strong` | **13–15 : 1** | Slightly above foreground — headings and bold text should pop but not dazzle. |
   | `faded` | **5–7 : 1** | Clearly readable but recedes. Tags, timestamps, comments. Stays above WCAG AA. |
   | `subtle` (bg) | **1.2–1.5 : 1** | Barely perceptible wash. Code blocks, frontmatter background. |
   | Accent colours | **≥ 4.5 : 1** | Must pass WCAG AA for the small amount of text they colour (checkboxes, errors). |

   The "faded" variants of each theme may go ~1–2 points lower across the board for an even calmer feel.
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
| Frontmatter | 0.9× base | Per-field styling: title bold italic, keys faded italic, id dim |
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
| `LinkText` | `strong` | `modeline` | underline | Display text — the part the reader cares about |
| `LinkChrome` | `faded` | — | dim | Brackets `[[` `]]` `|` and UUID — syntax noise |
| `Tag` | `faded` | — | — | Quiet, like comments |
| `TimestampKeyword` | `faded` | — | — | `@due`, `@start`, `@at` — tells you the type of date |
| `TimestampDate` | `foreground` | — | — | The actual date value — this is the information |
| `TimestampOverdue` | `accent_red` | — | — | Past-due `@due()` dates — demands attention |
| `TimestampParens` | `faded` | — | dim | Grouping syntax `(` `)` — noise once you see the keyword |
| `BlockId` | `faded` | — | dim | Even quieter |
| `BlockIdCaret` | `faded` | — | dim | The `^` prefix — slightly less meaningful than the ID itself |
| `ListMarker` | `foreground` | — | — | Structural — conveys document hierarchy |
| `CheckboxUnchecked` | `accent_yellow` | — | — | Pending state — `[ ]` IS the status |
| `CheckboxChecked` | `accent_green` | — | ~~strikethrough~~ | Completed marker — `[x]` IS the status |
| `CheckedTaskText` | `faded` | — | ~~strikethrough~~ | Text after `[x]` — visually completes the entire line |
| `Blockquote` | `foreground` | — | *italic* | Distinguishes quoted voice from the author's own text |
| `BlockquoteMarker` | `faded` | — | — | The `>` prefix — structural but repetitive on multi-line quotes |
| `TablePipe` | `faded` | — | — | `|` delimiters — structural but repetitive grid chrome |
| `TableContent` | `foreground` | — | — | Cell text — normal weight, the actual information |
| `TableAlignmentRow` | `faded` | — | dim | `---`, `:---:` etc. — pure layout hint, noise after initial read |
| `Frontmatter` | `faded` | — | *italic* | Base style for frontmatter structural elements |
| `FrontmatterKey` | `faded` | — | *italic* | YAML keys (`id:`, `title:`, etc.) — structure, not content |
| `FrontmatterTitle` | `foreground` | — | **bold** *italic* | The page name — most important metadata |
| `FrontmatterId` | `faded` | — | *italic* + dim | Internal UUID — rarely useful to the reader |
| `FrontmatterDate` | `faded` | — | *italic* | Useful context, not critical |
| `FrontmatterTags` | `faded` | — | — | Same weight as inline tags — meaningful for navigation |
| `BrokenLink` | `critical` | — | ~~strikethrough~~ | Demands attention |
| `SyntaxNoise` | `faded` | — | dim | Pure syntax markers — see Syntax Semantic Weight below |
| `SearchMatch` | `foreground` | `ultralight` | — | Highlighted search match in preview/results |
| `SearchMatchCurrent` | `foreground` | `popout` | — | The currently focused search match |

---

## Syntax Semantic Weight

Bloom renders Markdown as a styled document, not a code file. This means the `highlight.rs` scanner splits every construct into **marker spans** and **content spans**, each receiving an independent style. Markers are classified into three tiers based on their semantic weight — how much meaning the reader would lose if the marker were invisible.

### Tier 1 — Structural (Full Visibility)

The marker **is** the meaning. Removing it changes what the reader understands.

| Construct | Marker characters | Marker style | Rationale |
|-----------|-------------------|-------------|-----------|
| List item | `-` or `*` or `+` | `ListMarker` (`foreground`) | Conveys document structure. Without it, items become ambiguous paragraphs. |
| Ordered list | `1.`, `2.`, etc. | `ListMarker` (`foreground`) | Conveys ordering. Without it, sequence is lost. |
| Checkbox (unchecked) | `[ ]` | `CheckboxUnchecked` (`accent_yellow`) | Signals "this needs doing." The `[ ]` IS the status. The preceding `-` is a `ListMarker`. |
| Checkbox (checked) | `[x]` | `CheckboxChecked` (`accent_green`, ~~strikethrough~~) | Signals completion. The `[x]` IS the status. The preceding `-` is a `ListMarker`. |
| Checked task text | Text after `[x]` | `CheckedTaskText` (`faded`, ~~strikethrough~~) | The entire line reads as "done" — not just the checkbox. |
| Frontmatter title value | `"My Page Title"` | `FrontmatterTitle` (`foreground`, bold italic) | The page's name — most important metadata, deserves full visibility. |
| Tag | `#` in `#rust` | Same style as tag text (`faded`) | The `#` is part of the tag's identity. Dimming it makes `rust` look like prose. |
| Timestamp keyword | `@due`, `@start`, `@at` | `TimestampKeyword` (`faded`) | Tells you the *type* of date — deadline vs start vs event. The keyword is the meaning. |
| Timestamp date value | `2026-03-05` | `TimestampDate` (`foreground`) | The actual information. Deserves normal reading weight — you need to scan dates quickly. |
| Overdue date | Past `@due()` date | `TimestampOverdue` (`accent_red`) | Demands attention. A missed deadline is urgent. |
| Blockquote content | Text after `>` | `Blockquote` (`foreground`, *italic*) | Italic distinguishes quoted voice from the author's own text. |

### Tier 2 — Contextual (Present but Subdued)

Carries some information, but context or styling already communicates the construct's role. Rendered visibly but quieter than Tier 1.

| Construct | Marker characters | Marker style | Rationale |
|-----------|-------------------|-------------|-----------|
| Blockquote marker | `>` | `BlockquoteMarker` (`faded`) | Structural, but on multi-line quotes it's repetitive. The italic content already signals "quote." |
| Block ID | `^rope-perf` | `BlockId` (`faded` + dim) | Reference target text. Already very quiet. |
| Block ID caret | `^` in `^rope-perf` | `BlockIdCaret` (`faded` + dim) | Prefix syntax — slightly less meaningful than the ID itself. |
| Timestamp parens | `(` `)` in `@due(2026-03-05)` | `TimestampParens` (`faded` + dim) | Grouping syntax. `@due` already tells you what follows. |
| Table pipes | `\|` in table rows | `TablePipe` (`faded`) | Grid structure. Repetitive, but still helps the eye track columns. |
| Table alignment row | `\|---\|:---:\|` | `TableAlignmentRow` (`faded` + dim) | Pure layout hint. Only matters when authoring, not reading. |
| Frontmatter delimiters | `---` | `faded` + dim | Marks a section boundary, but frontmatter content is already styled — the delimiters can be quieter. |
| Frontmatter keys | `id:`, `title:`, `created:`, `tags:` | `FrontmatterKey` (`faded`, italic) | YAML structure — the values carry the meaning, not the keys. |
| Code fence | ` ``` ` and language hint | `faded` + dim | Marks code block boundary. The `subtle` background wash already signals "code zone." |

### Tier 3 — Noise (Dimmed)

Pure parsing syntax. The rendered styling already communicates everything the marker would. These get the `SyntaxNoise` style: `faded` foreground + `dim` decoration.

| Construct | Marker characters | Content style | Rationale |
|-----------|-------------------|--------------|-----------|
| Bold | `**` | `Bold` (foreground, **bold**) | Bold styling is visible. The `**` adds nothing for the reader. |
| Italic | `*` | `Italic` (foreground, *italic*) | Italic styling is visible. The `*` adds nothing. |
| Heading prefix | `#`, `##`, `###`, etc. | Heading style (bold + colour) | Heading styling (bold, size, colour) already signals the level. The `#` markers are line noise. |
| Link brackets | `[[`, `]]`, `\|` | `LinkChrome` (`faded`, dim) | The underline + colour on the display text already says "this is a link." |
| Link UUID | UUID portion of `[[uuid\|text]]` | `LinkChrome` (`faded`, dim) | Internal ID, meaningless to the reader. Visually suppressed (could be hidden entirely in future). |
| Inline code markers | `` ` `` | `Code` (foreground + subtle bg) | The background wash already signals "code." |

### Rendering Examples

Showing how a document looks with semantic weight applied. Dim markers shown in parenthetical notation for illustration (in the actual UI they'd be faded/dim):

**Raw Markdown:**
```markdown
---
id: 8f3a1b2c
title: "Design Decisions"
created: 2026-02-28
tags: [editors, rust]
---

### Design Decisions

I chose **Rust** for its *memory safety* and performance.
See [[8f3a1b2c|Text Editor Theory]] for background.

> The best tool is one that disappears in your hand.

- First consideration
- [ ] Review the ropey crate API @due(2026-03-05)
- [x] Read Xi Editor source
- [ ] Fix overdue item @due(2025-01-01)

| Feature   | Status  |
|-----------|---------|
| Buffer    | Done    |
| Search    | WIP     |

#editors #rust
```

**As rendered in Bloom (described):**
```
---                                       ← faded + dim (delimiter)
id: 8f3a1b2c                              ← "id:" faded italic, value faded italic + dim
title: "Design Decisions"                 ← "title:" faded italic, value foreground bold italic
created: 2026-02-28                       ← "created:" faded italic, value faded italic
tags: [editors, rust]                     ← "tags:" faded italic, values faded (like inline tags)
---                                       ← faded + dim (delimiter)

(###) Design Decisions                    ← (###) dim, "Design Decisions" bold

I chose (**)Rust(**) for its (*)memory    ← (**) dim, "Rust" bold; (*) dim
safety(*) and performance.                  "memory safety" italic
See ([[)Text Editor Theory(]])            ← ([[ | ]]) faded dim, "Text Editor Theory"
for background.                             strong underline; UUID hidden

(>) The best tool is one that             ← ">" faded, content foreground italic
   disappears in your hand.

- First consideration                     ← "-" normal (list marker)
- [ ] Review the ropey crate API          ← "-" normal, "[ ]" yellow (checkbox)
      @due(()2026-03-05())                  "@due" faded, "()" dim, date foreground
- [x] Read Xi Editor source               ← "-" normal, "[x]" green strikethrough, text faded strikethrough
- [ ] Fix overdue item                    ← "-" normal, "[ ]" yellow
      @due(()2025-01-01())                  "@due" faded, "()" dim, date accent_red (overdue!)

| Feature   | Status  |                   ← "|" faded, cell content foreground
|-----------|---------|                   ← alignment row faded + dim
| Buffer    | Done    |
| Search    | WIP     |

#editors #rust                            ← full tag including "#" in faded
```

### UI Chrome Mapping

| Element | Foreground | Background |
|---------|------------|------------|
| Status bar (Normal) | `foreground` | `highlight` |
| Status bar (Insert) | `background` | `accent_green` |
| Status bar (Visual) | `background` | `popout` |
| Status bar (Command) | `background` | `accent_blue` |
| Picker surface | `foreground` | `subtle` |
| Picker selected row | `foreground` | `mild` |
| Picker border | `faded` | — |
| Which-key popup | `foreground` | `subtle` |
| Visual selection (region) | `foreground` | `mild` |
| Search match (current) | `foreground` | `popout` |
| Search match (other) | `foreground` | `ultralight` |
| Current line (hl-line) | — | `highlight` |
| Window border | `faded` | — |
| Tilde `~` beyond EOF | `faded` | — |

### Status Bar Element Weights

Within the active pane's status bar, each element has its own typographic weight. Three visual tiers create a hierarchy without additional colour slots:

| Element | Foreground | Modifier | Tier | Rationale |
|---------|------------|----------|------|-----------|
| Mode badge | mode bg colour | **bold** | Heavy | Most important — tells you what keystrokes do |
| `│` separator | `faded` | — | Noise | Structural, should recede |
| Page title | `foreground` | — | Normal | Primary context — what file you're in |
| Dirty `[+]` | `salient` | — | Signal | Needs attention — unsaved changes |
| Pending keys | `salient` | **bold** | Signal | Transient but important — command is building |
| Macro `@q` | `accent_red` | — | Signal | Recording state — visually distinct from pending |
| Line:col | `faded` | — | Faded | Reference info — glance-at, not stare-at |
| MCP `⚡` (idle) | `faded` | — | Faded | Background service, not active |
| MCP `⚡` (editing) | `salient` | — | Signal | LLM is writing to this buffer right now |
| Indexer `⟳` (active) | `salient` | — | Signal | Index rebuild in progress |
| Disk Writer `⏍` (active) | `salient` | — | Signal | File being written |
| File Watcher `◉` (active) | `salient` | — | Signal | Processing external change |

All thread indicators use the same two-tier styling: `faded` when idle/static, `salient` when animating. Hidden when the thread is off or has no work.

All elements share the status bar background (`highlight` for Normal mode, mode-specific colour for Insert/Visual/Command). The inactive pane bar uses `faded` on `subtle` with just the page title — no mode, position, or pending keys.

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
name = "bloom-dark"   # any built-in: bloom-dark, bloom-light, aurora, frost, ember, solarium, twilight, sakura, verdant, lichen, ink, paper

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
