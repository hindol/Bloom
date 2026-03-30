# Bloom 🌱 — File Format Reference

> Bloom keeps its core data model legible: Markdown pages on disk, a few carefully chosen extensions, and a versioned `config.toml`.

---

## Page Structure

A Bloom page is a Markdown file with YAML frontmatter.

```markdown
---
id: 8f3a1b2c
title: "Text Editor Theory"
created: 2026-02-28
tags: [rust, editors]
---

Your content here.
```

| Field | Meaning |
|-------|---------|
| `id` | Stable page ID |
| `title` | Page title; Bloom derives filenames from it |
| `created` | ISO date |
| `tags` | Frontmatter tags; inline `#tags` can also appear in the body |

Bloom preserves unknown frontmatter keys rather than stripping them away. That matters for imports and for future-proofing.

---

## Standard Markdown

Bloom keeps ordinary Markdown ordinary: headings, emphasis, lists, checkboxes, blockquotes, fenced code blocks, images, and horizontal rules all behave as expected.

The point of Bloom's format is not to replace Markdown. It is to add just enough structure for linking, navigation, tasks, and time-aware workflows.

---

## Bloom Extensions

| Syntax | Purpose |
|--------|---------|
| `[[page-id\|display]]` | Page link |
| `[[page-id^block-id\|display]]` | Deep link to a block or section |
| `^block-id` | Stable block anchor |
| `^=block-id` | Mirrored block marker |
| `#tag` | Inline tag |
| `@due(YYYY-MM-DD)` | Due date |
| `@start(YYYY-MM-DD)` | Scheduled start |
| `@at(YYYY-MM-DD HH:MM)` | Timestamp or event time |

These extensions stay text-first. You can still open the file in another editor and understand what it says.

---

## Parsing Rules That Matter

**Links.** `[[...]]` opens a Bloom link. The payload can be a page ID alone, a page ID plus display text, or a page ID plus block ID and display text.

**Block IDs.** Bloom appends block IDs to content and manages them automatically. They are not decorative metadata; they are what let Bloom track a block through moves, mirrors, history, and deep links.

**Tags.** Inline tags begin with `#` and are recognized only in text contexts where they are meant to be tags, not as part of a word.

**Timestamps.** `@due`, `@start`, and `@at` are parsed as structured time metadata and power task and journal workflows.

**Code safety.** Bloom ignores links, IDs, tags, and timestamps inside inline code, fenced code blocks, and frontmatter values where those markers should remain literal text.

---

## Templates

Templates use snippet-style placeholders such as `${1:description}`.

| Placeholder | Meaning |
|-------------|---------|
| `${1:...}`, `${2:...}` | Numbered tab stops |
| `${AUTO}` | Generated page ID |
| `${DATE}` | Today's ISO date |
| `${TITLE}` | The title entered during creation |
| `$0` | Final cursor position |

A template can also declare metadata in an HTML comment:

```markdown
<!-- template: Meeting Notes | Notes for recurring meetings with attendees, agenda, and action items -->
```

Bloom strips that comment from the expanded output. The name and description feed the picker; the page content stays clean.

### Example

```markdown
<!-- template: Meeting Notes | Notes for recurring meetings with attendees, agenda, and action items -->
---
id: ${AUTO}
title: "${TITLE}"
created: ${DATE}
tags: [meeting]
---

## Attendees
${1:Names}

## Agenda
${2:Topics}

## Notes
$0

## Action Items
- [ ] ${3:First action item}
```

Bloom walks the numbered tab stops in order. Repeated placeholders mirror each other, so filling one occurrence fills the rest. That keeps templates expressive without inventing a separate mini language.

---

## Configuration (`config.toml`)

Bloom ships a versioned config template and migrates older configs forward. The config file is not just parsed; it is regenerated from the template when the schema version changes so new keys appear in a readable, documented form.

```toml
# Bloom Configuration
config_version = 1

[startup]
# "journal", "restore", or "blank"
# startup.mode = "journal"

# autosave_debounce_ms = 300
# which_key_timeout_ms = 500
# scrolloff = 3
# word_wrap = true
# wrap_indicator = "↪"
# auto_align = "page"
# max_results = 100

[theme]
# name = "bloom-dark"

[font]
# family = "JetBrains Mono"
# size = 14
# line_height = 1.6
```

### What Bloom Configures Today

| Section | Current keys |
|---------|--------------|
| `startup` | `mode` |
| top-level editor settings | `autosave_debounce_ms`, `which_key_timeout_ms`, `scrolloff`, `word_wrap`, `wrap_indicator`, `auto_align`, `max_results` |
| `theme` | `name`, `overrides` |
| `font` | `family`, `size`, `line_height` |
| `history` | `auto_commit_idle_minutes`, `max_commit_interval_minutes` |
| `mcp` | `enabled`, `mode`, `exclude_paths` |
| `views` | named BQL views with optional keybindings |
| `calendar` | `week_starts` |

### Migration Model

Every config file carries `config_version`. When Bloom loads an older version, it rebuilds the file from the current template and preserves the user's non-default values. That keeps new settings discoverable without making users hand-merge config snippets from release notes.

The broader format story is simple on purpose: Markdown for content, a small set of editor-native extensions, and a config file that stays readable as Bloom grows.
