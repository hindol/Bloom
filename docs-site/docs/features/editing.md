---
sidebar_position: 1
---

# Editing

Bloom supports full Vim grammar — motions, operators, text objects, and undo.

## Basic Editing

Navigate with `h j k l`, delete with `d`, change with `c`, yank with `y`.

![Basic editing](/animations/basic-editing.gif)

## Undo & Redo

Bloom maintains a full undo **tree** (not a linear stack). Branch when you undo
and make a different edit. `u` to undo, `C-r` to redo.

## Join Lines

`J` joins the current line with the next, collapsing whitespace.
