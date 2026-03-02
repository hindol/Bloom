# Bloom Test Vault

This is a curated test vault for manual smoke testing.

## Quick Start

From the project root:
    cargo run -p bloom-tui -- --vault test-vault/

## What to Test

### Basic Editing
- hjkl navigation
- i to enter insert mode, type text, Esc to return
- dd to delete a line, u to undo
- :w to save, :q to quit

### Leader Commands (SPC)
- SPC f f → Find page picker (search for "Rope" or "Vim")
- SPC j j → Open today's journal
- SPC j a → Quick append to journal
- SPC s s → Full-text search (try "ropey" or "Phase 0")
- SPC w v → Vertical split
- SPC w h/l → Navigate between splits
- SPC w d → Close split
- SPC u u → Undo tree visualizer

### Inline Pickers
- In insert mode, type [[ → link picker appears
- In insert mode, type ![[ → embed picker appears

### Vault Content (6 pages, 3 journal entries)
- Text Editor Theory (a1b2c3d4) — links to Rope Buffers, Vim Modal Editing
- Rope Buffers (e5f6a7b8) — links back to Text Editor Theory
- Vim Modal Editing (c9d0e1f2) — links to Text Editor Theory, Doom Emacs
- Doom Emacs Patterns (f3a4b5c6) — code blocks, UX patterns
- Rust for Editors (17a8b9c0) — crate ecosystem
- Project Bloom Roadmap (d2e3f4a5) — block IDs, timestamps, all cross-links
