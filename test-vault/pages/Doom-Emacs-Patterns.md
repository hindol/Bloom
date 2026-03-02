---
id: f3a4b5c6
title: "Doom Emacs Patterns"
created: 2026-02-22T16:00:00Z
tags: [editors, emacs, ux]
---

# Doom Emacs Patterns

Doom Emacs is the primary UX inspiration for Bloom's keybinding system.

## Which-Key

Press `SPC` and wait 300ms → a popup shows available key groups:

```
SPC f → file operations
SPC b → buffer operations
SPC s → search
SPC j → journal
SPC w → window management
```

This is **discoverability without memorization**. New users explore; power users never see the popup because they type fast enough.

## Window Management

- `SPC w v` — vertical split
- `SPC w s` — horizontal split
- `SPC w h/j/k/l` — navigate between splits
- `SPC w d` — close current split
- `SPC w m` — maximize toggle

## Fuzzy Finding

Everything goes through a **fuzzy picker** — files, commands, tags, links.
Orderless matching: `edt thry` matches "**Ed**i**t**or **Th**eo**ry**" #ux

The picker in Bloom uses `nucleo` for scoring [[a1b2c3d4|Text Editor Theory]].
