# Bloom 🌱 — Keybinding Reference

> All keybindings follow Doom Emacs conventions. `SPC` is the leader key in Normal mode.
> See [GOALS.md](GOALS.md) for goals and [PICKER_SURFACES.md](PICKER_SURFACES.md) for picker wireframes.

---

## Leader Bindings (`SPC`)

| Binding | Action | Goal |
|---------|--------|------|
| **Files** | | |
| `SPC f f` | Find page (fuzzy picker) | G16 |
| `SPC f r` | Rename current page (edit title) | G3 |
| `SPC f D` | Delete current page (with confirmation) | — |
| **Buffers** | | |
| `SPC b b` | Switch buffer (fuzzy picker) | G16 |
| `SPC b d` | Close current buffer | — |
| **Journal** | | |
| `SPC j j` | Open the journal picker | G14 |
| `SPC j p` / `SPC j n` | Previous / next day's journal | G14 |
| `SPC j d` | Jump to journal by date (date picker) | G14 |
| `SPC j t` | Open today's journal | G14 |
| `SPC j a` | Quick-append to today's journal (inline input) | G14 |
| **Tasks** | | |
| `SPC x a` | Quick-append task to today's journal | G14 |
| **Search** | | |
| `SPC s s` | Full-text search across all notes | G16 |
| `SPC s j` | Search journal entries | G16 |
| `SPC s t` | Search tags (on select: transitions to search filtered by tag) | G16 |
| `SPC s l` | Search backlinks to current page | G16 |
| `SPC s u` | Search unlinked mentions | G16 |
| **Links** | | |
| `SPC l l` | Insert link (alternative to `[[`) | G4 |
| `SPC l y` | Yank link to current page to clipboard | G4 |
| `SPC l Y` | Yank link to current block to clipboard | G4 |
| `SPC l t` | Open timeline view for current page | G6 |
| `SPC l b` | Open backlinks panel for current page | G5 |
| **Tags** | | |
| `SPC t a` | Add tag to current page (picker) | G4 |
| `SPC t r` | Remove tag from current page (picker) | G4 |
| **Agenda** | | |
| `SPC a a` | Open agenda view | G15 |
| **Insert** | | |
| `SPC i d` | Insert `@due()` with date picker | G4 |
| `SPC i s` | Insert `@start()` with date picker | G4 |
| `SPC i a` | Insert `@at()` with date picker | G4 |
| `SPC i y` | Paste from kill ring (fuzzy picker) | — |
| **Notes** | | |
| `SPC p n` | New page (template picker) | G19 |
| **Refactor** | | |
| `SPC r s` | Split page (extract section to new page) | G18 |
| `SPC r m` | Merge pages | G18 |
| `SPC r b` | Move block to another page | G18 |
| **History** | | |
| `SPC H h` | Open page history surface | G9 |
| `SPC H b` | Open block history surface | G9 |
| `SPC H c` | Create explicit checkpoint | G9 |
| **Undo** | | |
| `SPC u u` | Open unified history surface (same as `SPC H h`) | G9 |
| **Windows** | | |
| `SPC w v` | Vertical split | G11 |
| `SPC w s` | Horizontal split | G11 |
| `SPC w h/j/k/l` | Navigate between windows | G11 |
| `SPC w d` | Close window | G11 |
| `SPC w o` | Close all other windows | G11 |
| `SPC w =` | Balance window sizes | G11 |
| `SPC w m` | Maximize / restore current window | G11 |
| `SPC w >` / `SPC w <` | Widen / narrow window | G11 |
| `SPC w +` / `SPC w -` | Taller / shorter window | G11 |
| `SPC w x` | Swap with next window | G11 |
| `SPC w R` | Rotate window layout | G11 |
| `SPC w H/J/K/L` | Move buffer to window in direction | G11 |
| **Toggles** | | |
| `SPC T m` | Toggle MCP server on/off | G17 |
| `SPC T t` | Theme selector (live preview) | — |
| **Help / Meta** | | |
| `SPC SPC` | All commands (M-x equivalent) | G16 |
| `SPC ?` | Fuzzy-searchable command list | G8 |
| `SPC h r` | Rebuild index | G22 |

---

## Insert-Mode Triggers

| Trigger | Action |
|---------|--------|
| `[[` | Inline fuzzy picker → inserts `[[uuid\|title]]` |

---

## Picker Navigation (inside any fuzzy picker)

| Binding | Action |
|---------|--------|
| `Ctrl+N` / `Ctrl+J` / `↓` | Next result |
| `Ctrl+P` / `Ctrl+K` / `↑` | Previous result |
| `Enter` | Select / confirm |
| `Alt+Enter` | Create new page from query text |
| `Escape` / `Ctrl+G` | Cancel / close picker |
| `Tab` | Action menu on highlighted result |
| `Ctrl+U` | Clear search input |
| `Ctrl+D` | Delete highlighted item (with confirmation, where applicable) |
| `Ctrl+←` / `Ctrl+→` | Navigate between filter pills (G12) |
| `Backspace` on pill | Remove focused filter |
| `Ctrl+Backspace` | Clear all filters |

Preview is automatic on highlight. Picker results are scrollable and support vim-style `gg`/`G` for top/bottom.

See [PICKER_SURFACES.md](PICKER_SURFACES.md) for detailed wireframes of each picker surface.

---

## Agenda View Navigation (inside agenda)

| Binding | Action |
|---------|--------|
| `j` / `k` | Next / previous item |
| `Enter` | Jump to source note |
| `o` | Open source in split window |
| `x` | Toggle checkbox |
| `s` | Reschedule (open date picker) |
| `t` | Filter by tag |
| `d` | Filter by date range |
| `v d` / `v w` | Day view / week view |
| `q` | Close agenda |

---

## Timeline View Navigation (inside timeline)

| Binding | Action |
|---------|--------|
| `j` / `k` | Next / previous entry |
| `Enter` | Jump to source note |
| `o` | Open source in split window |
| `e` | Toggle expand / collapse entry |
| `q` | Close timeline |

---

## History Surface (inside page/block history)

| Binding | Action |
|---------|--------|
| `h` / `l` | Select older / newer stop |
| `j` / `k` | Switch undo branches when the selected stop is a fork |
| `d` | Toggle diff/raw preview |
| `r` | Restore selected stop into the current buffer |
| `c` | Create explicit checkpoint |
| `e` | Toggle compact / detail strip mode |
| `Enter` | Context action |
| `q` / `Esc` | Close history |

---

## Orphaned Link Navigation

| Binding | Action |
|---------|--------|
| `]l` | Jump to next broken link |
| `[l` | Jump to previous broken link |

---

## File Info

| Binding | Action |
|---------|--------|
| `Ctrl+G` | Show full file path, line count, and cursor position |

---

## Bloom-Specific Vim Text Objects

| Object | Syntax | Selects |
|--------|--------|---------|
| Inside link | `il` | Content within `[[...]]` |
| Around link | `al` | Entire `[[...]]` including brackets |
| Inside tag | `i#` | Tag name after `#` |
| Around tag | `a#` | `#tag` including the `#` |
| Inside timestamp | `i@` | Date within `@due(...)` etc. |
| Around timestamp | `a@` | Entire `@due(2026-03-05)` |
| Around heading section | `ah` | Heading + all content until next same-level heading |
| Inside heading section | `ih` | Content under heading (excluding the heading line) |

---

## Standard Vim Support

Standard Vim text objects (`iw`, `aw`, `ip`, `ap`, `i"`, `a"`, `i(`, `a(`, etc.), motions (`w`, `b`, `e`, `f`, `t`, `%`, `gg`, `G`, etc.), registers (`"a`-`"z`, `"+` for system clipboard), marks (`ma`, `'a`), `.` repeat, and macros (`qa`...`q`, `@a`) are all supported as per standard Vim.

---

## Clipboard & Kill Ring

Bloom combines Vim registers, system clipboard, and an Emacs-inspired kill ring (Doom Emacs `counsel-yank-pop` model).

### Three layers

| Layer | Access | Scope |
|-------|--------|-------|
| **Vim registers** | `"ay` (yank to `a`), `"ap` (put from `a`) | In-memory, 26 named registers `a`-`z` + unnamed `"` |
| **System clipboard** | `"+y` (yank to clipboard), `"+p` (put from clipboard) | OS-wide, shared with other applications |
| **Kill ring** | `SPC i y` (browse & paste from ring) | In-memory, 32 most recent yanks/deletes |

### How the kill ring works

Every yank (`y`), delete (`d`, `x`, `c`), or change operation pushes the affected text onto the kill ring — a stack of the 32 most recent clipboard entries. This happens automatically, no special keybinding needed.

| Binding | Action |
|---------|--------|
| `p` / `P` | Put from the most recent kill ring entry (same as unnamed register `"`) |
| `SPC i y` | Open kill ring picker — fuzzy search through all 32 entries, `Enter` pastes the selected one |
| `"+y` | Yank to system clipboard (also pushed to kill ring) |
| `"+p` | Paste from system clipboard (bypasses kill ring) |
| `Cmd+C` | `"+y` (macOS shortcut) |
| `Cmd+V` | `"+p` (macOS shortcut) |

### Kill ring picker (`SPC i y`)

A standard Bloom picker showing kill ring entries, most recent first. Each entry shows a truncated preview of the text. Fuzzy search filters entries. `Enter` pastes at the cursor. This is the Doom Emacs `counsel-yank-pop` equivalent — more discoverable than blind `M-y` cycling.
