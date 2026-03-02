---
id: 17a8b9c0
title: "Rust for Editors"
created: 2026-02-25T11:00:00Z
tags: [rust, editors, performance]
---

# Rust for Editors

Why Rust is a good fit for building text editors.

## Memory Safety Without GC

- No GC pauses during editing — predictable latency
- Ownership system prevents data races in multi-threaded indexer
- `Send` + `Sync` enforce thread safety at compile time

## The Ecosystem

| Crate | Purpose |
|-------|---------|
| `ropey` | Rope buffer |
| `ratatui` | TUI rendering |
| `tauri` | GUI (webview) |
| `rusqlite` | SQLite index |
| `nucleo` | Fuzzy matching |
| `crossbeam` | Channels |
| `notify` | File watching |

## Challenges

- Compile times are long for large workspaces #pain-point
- Async story is complex — we chose OS threads + crossbeam instead @at(2026-02-25)
- String handling: UTF-8 everywhere, but byte offsets vs char indices trip you up
- [ ] Profile build times after Phase 2 @due(2026-05-01)

Text Editor Theory page has more context: [[a1b2c3d4|Text Editor Theory]]
