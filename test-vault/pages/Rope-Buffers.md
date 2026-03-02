---
id: e5f6a7b8
title: "Rope Buffers"
created: 2026-02-18T14:30:00Z
tags: [editors, data-structures]
---

# Rope Buffers

A rope is a binary tree where leaves hold short strings. This gives O(log n) insert, delete, and index operations.

## Why Ropes?

- Gap buffers are O(n) for inserts far from the gap
- Piece tables are immutable-friendly but complex to implement undo
- Ropes compose naturally with **undo trees** — snapshot a subtree to branch

## Bloom's Choice

We chose `ropey` for Bloom's buffer layer [[a1b2c3d4|Text Editor Theory]].
The undo tree stores diffs against the rope, not full snapshots. ^design-choice

## Implementation Notes

- `ropey::Rope` is the backing store
- Edits are O(log n) ≈ microseconds — safe on the UI thread #performance
- Line indexing uses `byte_to_line` / `line_to_byte` converters
- [ ] Benchmark insert latency at 100K lines @due(2026-03-10)
