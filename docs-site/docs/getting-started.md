---
slug: /
sidebar_position: 1
---

# Getting Started

Bloom is a local-first, Vim-modal note-taking app built in Rust. Your notes live
as plain Markdown files on disk — no cloud, no lock-in.

## Quick Start

```bash
# Install (macOS / Linux)
curl -fsSL https://bloom-editor.github.io/install.sh | sh

# Open a vault
bloom ~/my-notes

# Or create a new one
bloom init ~/my-notes
```

## Core Concepts

- **Vault** — a folder of Markdown files. Bloom indexes them for search, links, and views.
- **Pages** — each `.md` file is a page. Frontmatter provides metadata (id, title, tags).
- **Block IDs** — `^xxxxx` suffixes on lines. Used for mirroring, history, and cross-references.
- **Journal** — date-named pages (`2026-03-16.md`) with `SPC j t` to open today.

## Vim Modal Editing

Bloom uses full Vim grammar: motions, operators, text objects, registers, macros.

| Mode | Badge | How to enter |
|------|-------|-------------|
| Normal | `NORMAL` | `Escape` |
| Insert | `INSERT` | `i`, `a`, `o`, `c`, etc. |
| Visual | `VISUAL` | `v`, `V` |
| Command | `COMMAND` | `:` |
