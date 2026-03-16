---
slug: /
sidebar_position: 1
title: Bloom
hide_title: true
---

<div style={{textAlign: 'center', marginBottom: '2rem'}}>

# Bloom 🌱

**Vim-modal notes. Local-first. Built in Rust.**

Your notes are Markdown files on disk — no cloud, no lock-in. Bloom adds modal editing, `[[links]]`, block-level history, and a query engine on top.

</div>

---

## Full Vim Grammar

Motions, operators, text objects, registers, macros — the real thing, not a subset.

![Vim editing in Bloom](/animations/basic-editing.gif)

<details>
<summary>What you just saw</summary>

Navigate with `j`/`k`/`w`, change a word with `ciw`, undo with `u`. Every edit is an undo tree node — branches, not a linear stack. [Learn more →](features/editing)

</details>

---

## Search Everything

`/` for in-buffer. `SPC s s` for vault-wide. Live highlighting, fuzzy matching, jump between results.

![Search in Bloom](/animations/search.gif)

<details>
<summary>What you just saw</summary>

Type `/` and a query — matches highlight instantly. `n`/`N` to jump. `SPC *` searches the word under the cursor across all files. [Learn more →](features/search)

</details>

---

## Block-Level History

Every block has an ID. Press `SPC H b` to scrub through its versions — undo tree for recent, git commits for older. Inline word diffs.

![Block history in Bloom](/animations/block-history.gif)

<details>
<summary>What you just saw</summary>

Cursor on a block, `SPC H b` opens a timeline strip. `h`/`l` scrubs through versions. Changed words are highlighted red/green. `r` restores any version. [Learn more →](features/block-history)

</details>

---

## Get Started

```bash
# macOS / Linux
curl -fsSL https://bloom-editor.github.io/install.sh | sh

# Windows
irm https://bloom-editor.github.io/install.ps1 | iex

# Then
bloom ~/notes
```

<div style={{textAlign: 'center', marginTop: '1.5rem', marginBottom: '1rem'}}>

[API Docs →](/api/bloom_core) · [GitHub →](https://github.com/hindol/Bloom)

</div>
