---
slug: /
sidebar_position: 1
sidebar_label: Home
title: Bloom
hide_title: true
---

<div style={{textAlign: 'center', padding: '3rem 0 1rem'}}>

<span style={{fontSize: '3.5rem'}}>🌱</span>

# Bloom

<p style={{fontSize: '1.35rem', fontWeight: 500, marginBottom: '0.5rem'}}>
Vim-modal notes. Local-first. Built in Rust.
</p>

<p style={{color: 'var(--ifm-color-emphasis-700)', maxWidth: '520px', margin: '0 auto'}}>
Your notes are Markdown files on disk — no cloud, no lock-in.<br/>
Bloom adds modal editing, <code>[[links]]</code>, block-level history, and a query engine on top.
</p>

</div>

<div className="bloom-features-intro">

- 📝 **Plain Markdown** — files on disk, version-controllable, portable
- 🔗 **`[[Links]]` + Block IDs** — UUID-based, survive renames and moves
- 📓 **Daily Journal** — `SPC j t` opens today, quick-capture without switching buffers
- ⌨️ **Full Vim Grammar** — motions, operators, text objects, registers, macros

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
curl -fsSL https://raw.githubusercontent.com/hindol/Bloom/main/install.sh | sh

# Windows (PowerShell)
irm https://raw.githubusercontent.com/hindol/Bloom/main/install.ps1 | iex

# Then
bloom ~/notes
```

<div style={{textAlign: 'center', marginTop: '1.5rem', marginBottom: '1rem'}}>

[API Docs →](pathname:///Bloom/api/bloom_core) · [GitHub →](https://github.com/hindol/Bloom) · [Releases →](https://github.com/hindol/Bloom/releases)

</div>
