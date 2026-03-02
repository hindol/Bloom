---
id: a1b2c3d4
title: "Text Editor Theory"
created: 2026-02-15T10:00:00Z
tags: [research, editors]
---

# Text Editor Theory

A collection of notes on how text editors work internally.

## Data Structures

The two main approaches are **gap buffers** and **rope trees** [[e5f6a7b8|Rope Buffers]].
Gap buffers are simpler but O(n) for distant inserts. Ropes give O(log n) for all operations.

See also [[c9d0e1f2|Vim Modal Editing]] for the editing model built on top.

## Key Papers

- "Data Structures for Text Sequences" by Charles Crowley #research
- "Piece Tables" from the Bravo editor at Xerox PARC #history
- The Xi editor architecture overview @at(2026-02-20)

## Open Questions

- [ ] How do modern editors handle Unicode normalization? #unicode @due(2026-03-15)
- [ ] What's the performance ceiling for rope-based undo trees? @due(2026-04-01)
- [x] Read about piece tables vs ropes — ropes win for collaborative editing ^q1-done
