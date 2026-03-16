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
Think freely. Every thought is safe.
</p>

<p style={{color: 'var(--ifm-color-emphasis-700)', maxWidth: '560px', margin: '0 auto', lineHeight: 1.7}}>
Bloom is a local-first note editor built in Rust. Your notes are plain Markdown files on disk — no cloud, no sync, no lock-in. Bloom adds Vim-modal editing, <code>[[links]]</code>, block-level version history, a daily journal, and a query engine that turns your notes into a personal knowledge base.
</p>

</div>

---

## Every thought is safe

Most note apps give you undo and redo — a single line you can walk back and forth. Bloom gives you an **undo tree**. Every edit creates a node. Undo, then make a different edit — that's a branch, not a lost history. Both paths are preserved, navigable, and restorable.

But the safety net goes deeper than one session. Every save triggers an automatic git commit in the background. Your entire vault has a complete version history — per page, per block, across days and weeks. Press `SPC H b` on any block and scrub through its evolution: when it was written, when it changed, when it moved between pages. All without you ever thinking about version control.

The undo tree handles the last few minutes. Git handles everything before that. You see one seamless timeline.

![Block history in Bloom](/animations/block-history.gif)

<details>
<summary>What you're seeing</summary>

The temporal strip at the bottom shows every version of a single block. `h`/`l` scrubs through time — undo nodes for recent edits, git commits for older ones. Changed words are highlighted inline. `r` restores any version. [Learn more →](features/block-history)

</details>

---

## Keeps up with you

Bloom is built in Rust with a strict rule: **the UI thread never blocks.** Rope-based text buffers give you O(log n) inserts regardless of file size. Auto-save, index rebuilds, git commits, and file watching all happen on dedicated background threads, communicating through lock-free channels. The editor stays responsive while the system works.

This isn't theoretical — it's the architecture. The render loop processes your keystrokes, computes the frame, and flushes to the terminal in under 3ms. A 10,000-page vault indexes in the background while you type. Pasting 10,000 characters costs the same as pressing a single key.

The result: Bloom keeps up with your speed of thought. No spinners, no "please wait", no moment where the editor hesitates.

![Editing in Bloom](/animations/basic-editing.gif)

<details>
<summary>What you're seeing</summary>

Full Vim grammar — motions, operators, text objects, registers, macros. Navigate with `j`/`k`/`w`, change a word with `ciw`, undo with `u`. Every operation is instant. [Learn more →](features/editing)

</details>

---

## Block mirroring

Copy a block — a task, a paragraph, a list item — into another page. Bloom detects the duplicate and keeps both copies in sync. Edit one, and all mirrors update. This isn't transclusion (read-only embedding) — it's **bidirectional mirroring** where every copy is a first-class citizen.

Each block in Bloom has a unique 5-character ID (`^k7m2x`). When the same ID appears in multiple files, Bloom marks them as mirrors (`^=k7m2x`) and propagates edits on mode transition. The marker lives in the file content, not a database — delete the index, rebuild from files, and all mirror relationships are preserved.

Need independence? `SPC m s` severs a mirror — the block gets a new ID and stops syncing. The other copies continue mirroring among themselves. No data loss, no confusion about which copy is "real" — they're all real.

<!-- GIF coming: mirror-propagation.gif — edit in one pane, watch the other update -->

---

## Just write

You shouldn't have to decide where a note belongs before you've finished thinking it. Bloom's journal is your default writing surface — `SPC j t` opens today's page, `SPC j a` appends a thought without leaving your current buffer. No filing, no categorization, no friction.

When you need something back, Bloom Query Language (BQL) pulls it to the surface. `tasks | where not done and due this week` shows your open tasks. `blocks | where tags has #rust | sort modified desc` finds everything you've tagged. Queries compose with pipes, filter by tag, date, page, or task status, and render as live views that update as your vault changes.

The journal captures your stream of consciousness. Links and tags add lightweight structure over time. BQL queries turn it into a searchable, queryable knowledge base — without you ever having to reorganize your files.

<!-- GIF coming: journal-workflow.gif — SPC j t, quick capture, then BQL view -->

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
