# Bloom 🌱 — Setup Wizard

> Wireframes for the first-launch setup wizard.
> See [GOALS.md G21](GOALS.md) for the feature goal, [USE_CASES.md UC-73](USE_CASES.md) for the use case.

---

## Flow

```
First Launch
    │
    ▼
┌─────────────────────┐
│  Welcome Screen     │  "Welcome to Bloom 🌱"
│  (Step 1)           │  Explain what a vault is.
└────────┬────────────┘
         │
         ▼
┌─────────────────────┐
│  Choose Vault Path  │  Default: ~/bloom/
│  (Step 2)           │  Editable path input.
└────────┬────────────┘
         │
         ▼
┌─────────────────────┐
│  Import from Logseq │  "Do you have a Logseq vault?"
│  (Step 3)           │  Yes → path picker. No → skip.
└────────┬────────────┘
         │ (if Yes)
         ▼
┌─────────────────────┐
│  Import Progress    │  "Importing... 47/132 pages"
│  (Step 3b)          │  Show progress and summary.
└────────┬────────────┘
         │
         ▼
┌─────────────────────┐
│  Complete           │  "Your vault is ready."
│  (Step 4)           │  Press Enter → today's journal.
└─────────────────────┘
```

**Rules:**
- The wizard owns the full screen. No editor panes behind it.
- Navigation: `Tab` / `Shift+Tab` to move between fields, `Enter` to confirm each step, `Esc` to go back one step (cannot exit on Step 1).
- The wizard never appears again after completion. A `config.toml` file in the vault marks it as initialized.

---

## Step 1 — Welcome

```
┌───────────────────────────────────────────────────────────────┐
│                                                               │
│                                                               │
│                                                               │
│                       Bloom 🌱                                │
│                                                               │
│         A local-first, keyboard-driven note-taking app.       │
│                                                               │
│         Your notes are stored as Markdown files in a          │
│         single folder called a vault. No cloud, no sync —     │
│         everything stays on your machine.                     │
│                                                               │
│                                                               │
│                                                               │
│                                                               │
│                                                               │
├───────────────────────────────────────────────────────────────┤
│                                    Press Enter to get started │
└───────────────────────────────────────────────────────────────┘
```

| Element | Style |
|---------|-------|
| Title "Bloom 🌱" | `strong`, bold |
| Description | `foreground` |
| Prompt | `faded` |
| Border | `faded` |

---

## Step 2 — Choose Vault Location

```
┌───────────────────────────────────────────────────────────────┐
│                                                               │
│                                                               │
│         Choose vault location                                 │
│                                                               │
│         This is where your notes, journal, and config         │
│         will live. You can move it later.                     │
│                                                               │
│         Path: ~/bloom/█                                      │
│                                                               │
│                                                               │
│                                                               │
│         Bloom will create:                                    │
│           pages/       — topic pages                          │
│           journal/     — daily journal                        │
│           templates/   — page templates                       │
│           images/      — attachments                          │
│                                                               │
├───────────────────────────────────────────────────────────────┤
│  Esc back                                Enter to confirm     │
└───────────────────────────────────────────────────────────────┘
```

| Element | Style |
|---------|-------|
| Heading "Choose vault location" | `strong`, bold |
| Description | `foreground` |
| Path input | `foreground` on `modeline` bg, cursor visible |
| Directory preview | `faded` |
| Nav hints | `faded` |

**Behavior:**
- Default value is `~/bloom/`. Cursor starts at end of path.
- Standard text editing: `Backspace`, `Ctrl+U` clears, `Ctrl+A` / `Home` go to start.
- `Enter` confirms. If the directory already exists and contains `config.toml`, skip to Step 4 (vault already initialized).
- If the path is invalid or unwritable, show inline error in `critical`: `"Cannot create directory: permission denied"`.
- `~` is expanded to the user's home directory.
- `Tab` auto-completes path segments (filesystem completion).

---

## Step 3 — Import from Logseq

```
┌───────────────────────────────────────────────────────────────┐
│                                                               │
│                                                               │
│         Import from Logseq?                                   │
│                                                               │
│         If you have an existing Logseq vault, Bloom           │
│         can import your pages, journals, and links.           │
│         Your Logseq files will not be modified.               │
│                                                               │
│                                                               │
│           ▸ No, start fresh                                   │
│             Yes, import from Logseq                           │
│                                                               │
│                                                               │
│                                                               │
│                                                               │
│                                                               │
│                                                               │
├───────────────────────────────────────────────────────────────┤
│  Esc back                 ↑↓ select         Enter to confirm  │
└───────────────────────────────────────────────────────────────┘
```

| Element | Style |
|---------|-------|
| Heading | `strong`, bold |
| Description | `foreground` |
| Selected option `▸` | `foreground` on `mild` bg |
| Unselected option | `foreground` |
| Nav hints | `faded` |

**Behavior:**
- `↑` / `↓` or `j` / `k` to toggle selection.
- `Enter` confirms selection.
- Selecting "No" → skip to Step 4.
- Selecting "Yes" → show Logseq path input:

```
┌───────────────────────────────────────────────────────────────┐
│                                                               │
│                                                               │
│         Import from Logseq                                    │
│                                                               │
│         Enter the path to your Logseq vault:                  │
│                                                               │
│         Path: ~/logseq/█                                      │
│                                                               │
│                                                               │
│                                                               │
│                                                               │
│                                                               │
│                                                               │
│                                                               │
│                                                               │
│                                                               │
│                                                               │
├───────────────────────────────────────────────────────────────┤
│  Esc back                                Enter to start import│
└───────────────────────────────────────────────────────────────┘
```

**Validation:** On `Enter`, verify the path contains `pages/` and `journals/` subdirectories (Logseq structure). If not: inline error in `critical`: `"Not a Logseq vault: missing pages/ directory"`.

---

## Step 3b — Import Progress

```
┌───────────────────────────────────────────────────────────────┐
│                                                               │
│                                                               │
│         Importing from Logseq...                              │
│                                                               │
│         ████████████████████████░░░░░░░░░░  72/132 pages      │
│                                                               │
│         ✓ 68 pages imported                                   │
│         ✓ 4 journals imported                                 │
│         ✓ 203 links resolved                                  │
│         ⚠ 3 warnings (unresolved refs)                        │
│                                                               │
│                                                               │
│                                                               │
│                                                               │
│                                                               │
│                                                               │
│                                                               │
├───────────────────────────────────────────────────────────────┤
│                                                               │
└───────────────────────────────────────────────────────────────┘
```

After completion:

```
┌───────────────────────────────────────────────────────────────┐
│                                                               │
│                                                               │
│         Import complete                                       │
│                                                               │
│         ✓ 128 pages imported                                  │
│         ✓ 4 journals imported                                 │
│         ✓ 523 links resolved                                  │
│         ⚠ 3 warnings (unresolved refs)                        │
│         ✗ 1 error (unparseable file)                          │
│                                                               │
│         Warnings:                                             │
│           page "CRDT Notes" — 2 refs could not be resolved    │
│           page "Meeting 2025-12-01" — 1 ref not resolved      │
│                                                               │
│         Errors:                                               │
│           journals/corrupted.md — parse error at line 14      │
│                                                               │
├───────────────────────────────────────────────────────────────┤
│                                    Press Enter to continue    │
└───────────────────────────────────────────────────────────────┘
```

| Element | Style |
|---------|-------|
| Progress bar filled | `salient` bg |
| Progress bar empty | `subtle` bg |
| `✓` lines | `accent_green` |
| `⚠` lines | `accent_yellow` |
| `✗` lines | `critical` |
| Warning/error details | `faded` |

---

## Step 4 — Complete

```
┌───────────────────────────────────────────────────────────────┐
│                                                               │
│                                                               │
│                                                               │
│         Your vault is ready 🌱                                │
│                                                               │
│         Location:  ~/bloom/                                  │
│         Pages:     128                                        │
│         Journal:   4 entries                                  │
│                                                               │
│         Tips:                                                 │
│           SPC j t     open today's journal                    │
│           SPC f f     find a page                             │
│           SPC p n     create a new page                       │
│           SPC ?       all commands                            │
│                                                               │
│                                                               │
│                                                               │
├───────────────────────────────────────────────────────────────┤
│                             Press Enter to open your journal  │
└───────────────────────────────────────────────────────────────┘
```

| Element | Style |
|---------|-------|
| Heading | `strong`, bold |
| Stats labels ("Location:", "Pages:") | `faded` |
| Stats values | `foreground` |
| Key bindings | `salient` (key), `foreground` (description) |
| Prompt | `faded` |

**Behavior:**
- If no import was done, stats show `Pages: 0`, `Journal: 0 entries`.
- `Enter` dismisses the wizard and opens today's journal (normal startup).

---

## Returning User Detection

The wizard does NOT appear if:
1. `~/bloom/config.toml` exists (vault already initialized), OR
2. A `--vault` CLI argument was passed (explicit vault path)

If the vault path exists but is missing expected subdirectories (`pages/`, `journal/`), Bloom creates them silently and proceeds — no wizard needed.

---

## Error States

### Permission denied

```
│         Path: /root/bloom/█                                   │
│                                                               │
│         ✗ Cannot create directory: permission denied           │
```

The error appears inline below the path input in `critical` style. The path input remains editable.

### Disk full / IO error

```
│         ✗ Failed to create vault: No space left on device     │
```

Same inline pattern. User can change path and retry.

---

## Related Documents

| Document | Contents |
|----------|----------|
| [GOALS.md G21](GOALS.md) | Setup wizard goal and vault structure |
| [GOALS.md G13](GOALS.md) | Logseq import specification |
| [USE_CASES.md UC-73](USE_CASES.md) | First launch use case |
| [USE_CASES.md UC-74](USE_CASES.md) | Logseq import use case |
| [THEMING.md](THEMING.md) | Colour slots referenced in style tables |
