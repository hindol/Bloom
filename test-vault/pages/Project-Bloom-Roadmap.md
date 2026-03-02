---
id: d2e3f4a5
title: "Project Bloom Roadmap"
created: 2026-02-10T08:00:00Z
tags: [bloom, planning]
---

# Project Bloom Roadmap

Tracking the high-level milestones for Bloom 🌱.

## Phase 0 — Foundation ✅

- [x] Document model + frontmatter ^p0-doc
- [x] Rope buffer + undo tree ^p0-buffer
- [x] File store + atomic writes ^p0-store
- [x] Markdown parser ^p0-parser
- [x] Vim editing engine ^p0-vim
- [x] RenderFrame abstraction ^p0-render
- [x] TUI + GUI frontends ^p0-ui

## Phase 1 — Note-Taking Core ✅

- [x] SQLite index with FTS5 ^p1-index
- [x] Link resolver + backlinks ^p1-resolver
- [x] Picker system (11 surfaces) ^p1-picker
- [x] Journal service ^p1-journal
- [x] Which-key + window management ^p1-ux

## Phase 2 — Power Features @start(2026-03-01)

- [ ] Templates + snippet placeholders @due(2026-03-15)
- [ ] Agenda view (overdue/today/upcoming) @due(2026-03-20)
- [ ] Page split/merge/move-block @due(2026-04-01)
- [ ] Logseq import @due(2026-04-15)
- [ ] Session restore @due(2026-04-30)

## Phase 3 — Interop

- [ ] MCP server for LLM integration @due(2026-05-15)
- [ ] Cross-platform CI (macOS + Windows) @due(2026-05-30)
- [ ] Performance benchmarks @due(2026-06-15)

## Links

- [[a1b2c3d4|Text Editor Theory]] — foundational reading
- [[e5f6a7b8|Rope Buffers]] — buffer implementation
- [[c9d0e1f2|Vim Modal Editing]] — editing model
- [[f3a4b5c6|Doom Emacs Patterns]] — UX inspiration
- [[17a8b9c0|Rust for Editors]] — language choice
