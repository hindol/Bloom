---
sidebar_position: 3
---

# Block History

Scrub through the history of a single block — see exactly when and how it changed.

## How It Works

Place your cursor on any line with a block ID (`^xxxxx`) and press `SPC H b`.
A temporal strip appears at the bottom showing all versions of that specific block.

![Block history demo](/animations/block-history.gif)

## Navigation

| Key | Action |
|-----|--------|
| `h` / `←` | Older version (skips unchanged commits) |
| `l` / `→` | Newer version |
| `r` | Restore selected version |
| `e` | Toggle compact/rich mode |
| `q` / `Esc` | Close |

## Inline Word Diff

The block line shows a word-level diff: red for removed words, green for added.
The diff flows through normal word wrap — long lines wrap correctly.

## Unchanged Commit Dimming

Git commits where this block didn't change are automatically dimmed (`·` marker)
and skipped during navigation. Only meaningful changes get stops.

## Page History

`SPC H h` opens full page history — a unified timeline of undo tree nodes and
git commits. The preview pane shows a line-level diff.
