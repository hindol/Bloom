# Journal Redesign 📓

> One file called `journal.md`. Fresh every day. Archive browsed by calendar, not filenames.
> Status: **Draft** — exploratory, not committed.
> See also: [TIME_TRAVEL.md](TIME_TRAVEL.md) for git-backed history, day views, and calendar navigation.

---

## The Problem

Today Bloom creates one file per day in `journal/`: `2026-03-08.md`, `2026-03-09.md`, etc. This has three problems:

1. **Date-named files are meaningless in a picker.** `SPC f f` shows "2026-03-08" alongside "Text Editor Theory" — one evokes an idea, the other evokes nothing. Journal files pollute the page namespace with noise.

2. **Users think about files instead of writing.** "Which daily file was that thought in?" is the wrong question. You should be thinking about *when* or *what*, not *which file*.

3. **The daily boundary is an implementation detail.** You don't care that March 8 is a separate file from March 9. You care about the stream of your thinking over time.

---

## The Design

### One File: `journal.md`

The user sees exactly one journal file, always called `journal.md`, living at the vault root (not in a subdirectory). `SPC j j` opens it. It's always there.

Every day when Bloom starts (or at midnight if running), the current `journal.md` is **auto-rotated**: its contents are moved to the archive, and a fresh `journal.md` appears. If the journal is empty (no edits that day), no archive entry is created.

The user never thinks about this. They open journal, they write, they close. Tomorrow it's fresh.

### The Archive: `.journal/`

Rotated journals live in a hidden directory:

```
~/bloom/
├── journal.md              ← today, always this name
├── .journal/               ← hidden archive
│   ├── 2026-03-07.md
│   ├── 2026-03-06.md
│   ├── 2026-03-04.md       ← March 5 had no entries, no file
│   └── ...
├── pages/
└── ...
```

Archive files are **never shown in `SPC f f`** (the page picker). They are a separate namespace — the journal log, not the knowledge base.

Archive files are **fully indexed** — FTS5 search, tags, links, tasks, and (future) embeddings all cover them. `SPC s s` full-text search finds content in the archive. Queries can target them.

### Quick Capture (unchanged UX)

| Keybinding | Action |
|-----------|--------|
| `SPC j j` | Open today's journal |
| `SPC j a` | Quick-append a line (without leaving current buffer) |
| `SPC j t` | Quick-append a task |

These work exactly as today. The only difference is the file is always `journal.md`, not `journal/2026-03-08.md`.

### Task Carry-Forward

When `journal.md` rotates, any uncompleted tasks (`- [ ]`) are **automatically copied into the new journal**. A faint back-reference is added:

```markdown
- [ ] Review the ropey crate API @due(2026-03-10)  ← carried from Mar 7
```

The original task in the archive is left as-is (not modified). The carried copy is a new block — toggling either one is independent. This ensures open loops never disappear into the archive.

**Completed tasks (`- [x]`) are NOT carried forward.** They stay in the archive where they were completed.

### Orphan Nudge

On rotation, Bloom scans the archived journal for **orphan blocks** — blocks with no `[[links]]`, no `#tags`, and no tasks. These are raw thoughts that will be hard to find later.

If orphans are found, a non-blocking notification appears on next launch:

```
📓 Yesterday's journal has 3 unlinked notes. SPC j r to review.
```

`SPC j r` opens a picker showing just the orphan blocks from the most recent rotation. For each orphan, the user can:

- **Tag it** — add a `#tag` (searchable forever)
- **Link it** — promote to or link to a page
- **Dismiss** — it's fine as-is, don't remind again

This is a gentle nudge, not a gate. Ignoring it is fine — the content is still indexed and searchable.

### Journal + Pages: Separate Namespaces

| Namespace | Contents | Picker | Index |
|-----------|----------|--------|-------|
| **Pages** (`pages/`) | Named ideas with identity | `SPC f f` | Full |
| **Journal** (`journal.md` + `.journal/`) | Daily stream, temporal | `SPC j c` (calendar) | Full |

Pages are things you navigate by *name*. The journal is something you navigate by *time*. Mixing them in the same picker confuses both.

Both namespaces are fully searchable via `SPC s s` (full-text) and BQL queries. The separation is a *navigation* concern, not a *data* concern.

### BQL Integration

The `journal` source in BQL targets the journal namespace (today's file + archive):

```
journal | where date = today                          -- today's journal
journal | where date this week                        -- this week's entries
blocks  | where page in $journal | where tags has "rust"  -- all journal blocks tagged #rust
tasks   | where page in $journal | where not done     -- open tasks from any journal day
```

`$journal` is a context variable representing the journal namespace (today's file + all archive files).

---

## Vault Structure (revised)

```
~/bloom/
├── journal.md              ← today's journal (always this name)
├── pages/                  ← named pages
│   ├── Text Editor Theory.md
│   └── Rust Programming.md
├── .journal/               ← hidden archive (auto-rotated daily journals)
│   ├── 2026-03-07.md
│   ├── 2026-03-06.md
│   └── ...
├── templates/
├── images/
├── .index/
├── .gitignore
└── config.toml
```

---

## Migration from Current Design

For users with an existing `journal/` directory:

1. On first launch after upgrade, Bloom detects the old `journal/` directory.
2. Files are moved to `.journal/` (renamed, not re-created).
3. The most recent day's file becomes the new `journal.md`.
4. A notification explains the change.

This is a one-time migration, non-destructive (files are moved, not deleted).

---

## Open Questions

1. **Midnight rotation while Bloom is running.** If you're writing at 11:59 PM and keep going past midnight, when does the rotation happen? Options: (a) on next launch only, (b) at midnight with a prompt, (c) silently at midnight, content before midnight stays in today's archive, content after goes to new journal. Leaning towards (a) — simplest, no surprises mid-session.

2. **Archive editability.** Should archived journals be read-only? Or editable (for fixing typos, adding tags after the fact)? Editable is more flexible but means the archive is mutable. Leaning towards editable — the orphan nudge workflow requires adding tags to archived content.

3. **Carry-forward limit.** If a task has been carried forward for 30 days straight, it's clearly stuck. Should Bloom flag long-carried tasks differently? Maybe surface them with a different indicator after N days.

4. **Journal frontmatter.** Today's `journal.md` — does it have frontmatter? It doesn't need an ID (it's not linkable by UUID). It might want `created: 2026-03-08` for the indexer. Minimal frontmatter: just the date.

5. **Linking to journal entries.** If a page wants to reference "what I wrote on March 8", how? Today you'd link to the journal page by UUID. In this model, the archive file has a UUID but isn't in the page picker. Maybe: `[[journal:2026-03-08]]` as a special link syntax? Or just use BQL inline queries.

---

## References

- Current design: [GOALS.md G14](../GOALS.md) (Daily Journal)
- [TIME_TRAVEL.md](TIME_TRAVEL.md) — git-backed history, day view design, calendar + day-hopping navigation
- Task carry-forward inspired by: Bullet Journal "migration" concept
