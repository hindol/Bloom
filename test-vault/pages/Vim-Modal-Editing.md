---
id: c9d0e1f2
title: "Vim Modal Editing"
created: 2026-02-20T09:00:00Z
tags: [editors, vim, ux]
---

# Vim Modal Editing

Vim's core insight: separate **navigation** from **editing**. You spend most time reading, not writing.

## The Grammar

Every command follows: `[count][operator][motion]`

- `d2w` = delete 2 words
- `ciw` = change inner word
- `yap` = yank around paragraph

## Modes

| Mode | Purpose |
|------|---------|
| Normal | Navigate, compose operators |
| Insert | Type text |
| Visual | Select regions |
| Command | Execute commands (`:w`, `:q`) |

## Why It Works for Note-Taking

Modal editing shines when you're *restructuring* notes — moving blocks between pages, refactoring link text, rewriting headings. These are operator+motion tasks.

See [[a1b2c3d4|Text Editor Theory]] for the underlying data structures.
See [[f3a4b5c6|Doom Emacs Patterns]] for the which-key layer we borrow.

## Bloom Extensions

- `il` / `al` — inner/around link text objects #bloom
- `ie` / `ae` — inner/around embed
- `i#` / `a#` — inner/around tag
- `i@` / `a@` — inner/around timestamp
