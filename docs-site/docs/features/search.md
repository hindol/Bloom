---
sidebar_position: 2
---

# Search

Find text in the current buffer or across your entire vault.

## In-Buffer Search

Press `/` to search forward, `?` to search backward. Matches highlight live
as you type.

![Search demo](/animations/search.gif)

`n` jumps to the next match, `N` to the previous. Search wraps around the buffer.

## Vault-Wide Search

`SPC *` searches the word under the cursor across all pages in the vault.
Results appear in the picker — press Enter to jump to a match.

`SPC s s` opens a free-text search prompt for the vault.
