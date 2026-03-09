# Live Views 🔭

> An embedded query language for composable, real-time views over your knowledge.
> Status: **Draft** — exploratory, not committed.

---

## The Problem

Today Bloom has several hard-coded views: the agenda shows tasks, the timeline shows backlinks, the picker searches text. Each is a separate feature, separately implemented, with its own keybinding, its own renderer, its own data path.

But they're all the same thing: **a filtered, grouped, sorted projection of the vault.**

The agenda is `tasks where status = open, grouped by due date`. The timeline is `chunks where links to X, sorted by date`. Backlinks is `pages where links to X`. Tag browse is `pages where tag = Y`. These are all queries. We just hard-coded them.

**The cost of hard-coding:**
- Users can't ask questions we didn't anticipate ("show me all tasks tagged #work due this week that mention 'budget'")
- Every new view requires core code changes
- The agenda can't be customised — its grouping and filtering are frozen
- There's no way to save a useful query for reuse
- There's no way to embed a live result set inside a note

**The opportunity:** A small query language that unifies all of these into one composable system. The agenda becomes a built-in query. The user can write their own.

---

## Design Principles

1. **Learnable in 5 minutes.** The entire language fits on one screen. No programming background required.

2. **Reads like English.** `tasks | where not done and due < today` not `SELECT * FROM tasks WHERE done = 0 AND due_date < date('now')`.

3. **Composable by piping.** Small operations chained with `|`. Each step transforms the result set.

4. **Zero boilerplate.** A query with no clauses returns everything. Every clause is optional. `tasks` alone is valid.

5. **Live feedback.** The interactive query prompt (`SPC v v`) parses and executes on every pause (150ms debounce), showing results or a clear error with position info.

6. **Embeddable in notes.** A `{{...}}` block renders as a live view. Other editors see readable text.

---

## The Language: Bloom Query Language (BQL)

### Grammar

```
QUERY  = SOURCE ("|" CLAUSE)*

SOURCE = "pages" | "tasks" | "journal" | "blocks" | "tags" | "links"

CLAUSE = WHERE | SORT | GROUP | LIMIT | COUNT

WHERE  = "where" EXPR
EXPR   = PRED (("and" | "or") PRED)*
PRED   = "not" ATOM | ATOM
ATOM   = "(" EXPR ")"
       | FIELD OP VALUE
       | FIELD "has" TAG
       | FIELD RANGE

OP     = "=" | "!=" | "<" | ">" | "<=" | ">="

RANGE  = "this week" | "last week" | "next week"
       | "this month" | "last month" | "next month"

TAG    = "#" identifier

FIELD  = identifier ("." identifier)*
VALUE  = STRING | DATE | NUMBER | BOOL | VAR | "none"
STRING = '"..."' | "'...'"
DATE   = YYYY-MM-DD | "today" | "yesterday" | "tomorrow"
NUMBER = digits
BOOL   = "true" | "false"
VAR    = "$page" | "$today"

SORT   = "sort" FIELD ["asc"|"desc"] ("," FIELD ["asc"|"desc"])*
GROUP  = "group" FIELD
LIMIT  = "limit" NUMBER
COUNT  = "count"
```

### Rules

- Strings **must** be quoted. `"hello"` or `'hello'`.
- `and`/`or` combine predicates within a single `where`. Parentheses control precedence.
- `not` can only precede an atom (no double negation).
- `count` must be the last clause. With `group`, it returns per-group counts. Without `group`, a single total.
- Sort only works on scalar fields (not lists like `tags`).
- `$page` resolves to the current page's ID. Returns empty results if no page is open.
- `none` represents a missing value: `where due = none` (no due date), `where due != none` (has due date).
- Range predicates (`this week`, etc.) only apply to date fields. Week start day is configured in `config.toml`.
- `has` only applies to list fields (`tags`). Uses Bloom's tag syntax: `tags has #rust`.
- Unknown fields or type mismatches produce compile-time errors with clear messages.

### Sources

| Source | Returns | Fields |
|--------|---------|--------|
| `pages` | All pages | `title`, `created`, `tags`, `path`, `backlinks.count` |
| `tasks` | All tasks (checkbox items) | `text`, `done`, `due`, `start`, `page`, `tags`, `line` |
| `journal` | Journal entries only | `date`, `title`, `tags` |
| `blocks` | All content blocks | `text`, `page`, `line`, `tags`, `modified` |
| `tags` | All unique tags | `name`, `count` |
| `links` | All links | `from`, `to`, `display` |

### Examples

```
tasks                                          -- all tasks
tasks | where not done                         -- open tasks
tasks | where not done and due < today         -- overdue
tasks | where not done and due this week       -- due this week
  and tags has #work
tasks | where tags has #work                   -- work tasks
  or tags has #urgent
tasks | where not done and due = none          -- tasks with no due date
tasks | where not done                         -- agenda
  | sort due | group due.category
tasks | where not done | count                 -- total open tasks
tasks | where not done                         -- per-page open task counts
  | group page | count

pages | sort created desc | limit 20           -- recently created
pages | where tags has #rust | sort title      -- all Rust pages
pages | where backlinks.count = 0              -- orphan pages

links | where to = $page                       -- what links here

tags | sort count desc                         -- all tags by frequency

blocks | where modified = today                -- everything touched today
```

---

## Queries Live in Pages

There is no separate "view" mode. A query is a `{{...}}` block in a regular page. The page is the view.

### Example: A User-Created Agenda Page

The user creates a page called "Agenda" (`pages/Agenda.md`):

```markdown
---
id: a1b2c3d4
title: "Agenda"
created: 2026-03-08
tags: []
---

## Overdue

{{tasks | where not done and due < today | sort due}}

## This Week

{{tasks | where not done and due this week | sort due}}

## No Due Date

{{tasks | where not done and due = none | sort page}}
```

This is a normal page. It shows up in `SPC f f`. It can have prose, headings, links, tags — anything. The `{{...}}` blocks render as live, interactive result sets inside the page content.

### Rendering

- **In Bloom:** Each `{{...}}` block is replaced inline with a result table. The query text is shown dimmed above the results. Results update on `IndexComplete`.
- **In other editors / GitHub:** It's readable text with a clear `{{...}}` marker. Portable Markdown. No lock-in.
- **Inline queries:** `{{tasks | where not done | count}}` renders as an inline number (e.g., "12").

**Code-block safety:** `{{...}}` blocks inside fenced code blocks are NOT evaluated. Same rule as all Bloom extensions.

### Interaction Within a Query Block

When the cursor enters a query result block:

| Key | Action |
|-----|--------|
| `j` / `k` | Navigate between result rows |
| `x` | Toggle task (writes to source file via block ID, undo-able) |
| `Enter` | Jump to source page at the block |
| `Ctrl-o` | Jump back to the query page (same row) |
| `u` | Undo last toggle (task reappears if it still matches the query) |
| `q` | Move cursor out of the result block |

The query block text itself is editable — you can modify the query, and after 150ms debounce the results re-render.

### Built-in Query Pages

Bloom ships (or auto-creates on first launch) template query pages:

| Keybinding | Opens page | Default query |
|-----------|-----------|---------------|
| `SPC a a` | "Agenda" | `tasks \| where not done \| sort due \| group due.category` |

The user can edit, rename, or delete this page. `SPC a a` simply opens the page titled "Agenda" — if deleted, the keybinding does nothing (or offers to re-create it).

Other built-in features don't need dedicated pages — they already work as picker surfaces:

| Feature | Mechanism |
|---------|-----------|
| Backlinks (`SPC s l`) | Picker (existing) |
| Tag browse (`SPC s t`) | Picker (existing) |
| Journal search (`SPC s j`) | Picker (existing) |

`$page` is a context variable — the current page's ID. `$today` resolves to today's date. These resolve based on the page you were viewing *before* opening the query page.

---

## Implementation Architecture

### Parser

A hand-written recursive descent parser. The grammar is small enough that a parser generator is overkill. Parsing takes microseconds. Errors include position info for inline display.

### Execution

Queries compile to SQL and execute against the SQLite index:

```
tasks | where not done and tags has #work | sort due
       ↓
SELECT t.*, GROUP_CONCAT(tg.tag) as tags
FROM tasks t
LEFT JOIN tags tg ON t.page_id = tg.page_id
WHERE t.done = 0
AND t.page_id IN (SELECT page_id FROM tags WHERE tag = 'work')
ORDER BY t.due_date
```

The BQL-to-SQL compiler is straightforward because:
- Sources map to tables (already exist in the index)
- `where` maps to `WHERE`
- `sort` maps to `ORDER BY`
- `group` maps to result-set post-processing (grouping is a display concern, not a SQL concern)
- `limit` maps to `LIMIT`
- `count` maps to `COUNT(*)`, with `GROUP BY` when preceded by `group`

Execution always goes through the existing read-only index connection. No new database access patterns.

### Live Updates

Query blocks re-execute when the index changes (after `IndexComplete`). The editor re-runs queries for the visible page and re-renders if results changed. Same pattern as backlinks/agenda refresh. Cost: <1ms per query.

When the user edits the query text in a `{{...}}` block, a 150ms debounce triggers re-parse + re-execute + re-render.

### New Modules

| Module | Responsibility |
|--------|---------------|
| `query/parse.rs` | BQL tokeniser + parser → AST, with position-aware error diagnostics |
| `query/compile.rs` | AST → SQL query string, field/type validation |
| `query/execute.rs` | Run compiled query against index, return typed result sets |
| `query/mod.rs` | Public API: `parse()`, `execute()`, `QueryResult` types |

### Render Integration

The editor's content renderer detects `{{...}}` blocks during `render()`. For each:

1. Parse the query text
2. If valid: compile → execute → produce `QueryResultBlock` (rows, columns, group headers, actions)
3. If error: produce `QueryErrorBlock` (error message with position)
4. The TUI renders result blocks inline where the code block would be, with the same visual language as agenda/picker results

No new `RenderFrame` variant needed — query results are part of the page's `RenderedLine` stream, rendered inline with the rest of the page content.

---

## Migration: Agenda as a Page

The current `Agenda` struct, `AgendaView`, `AgendaFrame`, and the dedicated agenda overlay become unnecessary. The agenda is a page with query blocks.

On first launch (or upgrade), Bloom creates `pages/Agenda.md` with default queries. `SPC a a` opens this page. The dedicated agenda rendering code is replaced by the general query block renderer.

This is **backwards-compatible** — users see the same tasks, same grouping, same actions. But now they can edit the queries, add sections, combine with prose. The agenda is theirs.

---

## Decisions (from design review)

1. **Error rendering:** Dimmed query text + error message in `critical` colour below the block.
2. **Performance guardrails:** No implicit limit. Show result count. Warn if >1000.
3. **Week start:** `[calendar] week_starts = "monday"` in config. Default Monday (ISO 8601).
4. **`$page` resolution:** Resolves to the current page's frontmatter ID. Always available, no tracking.
5. **Cursor at block boundaries:** `j`/`k` enters and exits query blocks automatically — no mode switch. The block behaves like a tall line. `▸` marks the selected row; action hints appear in footer.

## Open Questions

1. **`contains` operator.** Useful for `tasks | where text contains "?"`. Not in the grammar yet. Leaning towards: add it — the only substring predicate needed.

---

## Non-Goals

- **Turing completeness.** BQL is a query language, not a programming language. No variables, no loops, no conditionals, no user-defined functions.
- **Write operations.** Queries are read-only projections. You can *act on* results (toggle a task), but the query itself never mutates data.
- **Cross-vault queries.** Single vault only (consistent with Bloom's v1 scope).
- **Source OR-ing.** `pages or journal | ...` is not supported. Use `blocks` for cross-source queries.
- **Double negation.** `not not done` is a parse error. Keep the grammar simple.

---

## References

- [Dataview (Obsidian)](https://blacksmithgu.github.io/obsidian-dataview/) — the closest existing thing. JavaScript-based, Obsidian-only, much heavier syntax. BQL aims to be simpler.
- [Org-mode agenda](https://orgmode.org/manual/Agenda-Views.html) — the inspiration for Bloom's agenda. Powerful but complex custom agenda commands.
- [jq](https://stedolan.github.io/jq/) — pipe-based composition for JSON. BQL borrows the `|` chaining philosophy.
- [Logseq Advanced Queries](https://docs.logseq.com/#/page/advanced%20queries) — Datalog-based. Powerful but steep learning curve. BQL explicitly avoids this complexity.
