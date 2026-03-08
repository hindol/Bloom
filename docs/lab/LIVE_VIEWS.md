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

1. **Learnable in 10 minutes.** If it takes longer, it's failed. No programming background required. The entire language should fit on one screen.

2. **Reads like English.** `tasks where due before today` not `SELECT * FROM tasks WHERE due_date < date('now')`. The syntax should be guessable.

3. **Composable by piping.** Small operations chained together, Unix-style. Each step transforms the result set. `tasks | where tag = "work" | where due before friday | sort due | group by page`

4. **Zero boilerplate.** A query with no clauses returns everything. Every clause is optional. `tasks` alone is a valid query that shows all tasks.

5. **Live by default.** Query results update in real-time as notes change. There's no "run" button — it's a view, not a report.

6. **Embeddable in notes.** A query inside a fenced code block renders as a live view when the page is displayed. The Markdown source stays portable — other editors see the query as a code block.

---

## The Language: Bloom Query Language (BQL)

### Sources

A query starts with a **source** — what kind of thing you're looking at.

| Source | Returns | Fields available |
|--------|---------|-----------------|
| `pages` | All pages in the vault | `title`, `created`, `tags`, `path`, `links`, `backlinks` |
| `tasks` | All tasks (checkbox items) | `text`, `done`, `due`, `start`, `page`, `tags`, `line` |
| `journal` | Journal pages only | `date`, `title`, `tags` |
| `blocks` | All blocks/paragraphs | `text`, `page`, `line`, `tags`, `links` |
| `tags` | All unique tags | `name`, `count` |
| `links` | All links in the vault | `from`, `to`, `display`, `section` |

The source name alone is a valid query: `tasks` returns all tasks, `pages` returns all pages.

### Pipes

Clauses chain with `|` (pipe). Each pipe transforms the result set.

```
tasks | where due before today | sort due
```

### Clauses

#### `where` — filter

```
where <field> <operator> <value>

Operators:
  =, !=           exact match (strings are case-insensitive)
  <, >, <=, >=    comparison (dates, numbers)
  contains        substring match
  matches         fuzzy match (same as picker)
  before, after   date comparison (sugar for < and >)
  has             set membership: `where tags has "rust"`
  not             negation prefix: `where not done`
```

Multiple `where` clauses AND together. For OR, use `any`:

```
tasks | where any(tag = "work", tag = "urgent")
```

**Date literals** are human-friendly:

```
today, yesterday, tomorrow
monday, tuesday, ... (next occurrence)
last week, this week, next week
last month, this month, next month
2026-03-08 (ISO date)
3 days ago, in 2 weeks
```

#### `sort` — order results

```
sort <field> [asc|desc]
```

Default is ascending. Multiple sort fields separate with `,`:

```
pages | sort created desc, title
```

#### `group` — group results with headers

```
group <field>
group <expression>
```

Groups produce visual section headers in the rendered view.

```
tasks | where not done | group due category
```

`due category` is a built-in grouping that produces Overdue / Today / Upcoming / Undated — exactly what the current agenda does.

Custom grouping expressions:

```
tasks | group page           -- group by source page
pages | group created month  -- group by month created
blocks | group tags first    -- group by first tag
```

#### `select` — choose which fields to display

```
tasks | select text, due, page
```

If omitted, a sensible default per source is used.

#### `limit` — cap result count

```
pages | sort created desc | limit 10
```

#### `count` — aggregate to a number

```
tasks | where not done | count
```

Returns a single number instead of a result set. Useful in embedded views for dashboards.

### Complete Examples

**The current agenda, as a query:**

```
tasks | where not done | sort due | group due category
```

That's it. The entire hard-coded agenda view in one line.

**Tasks due this week tagged #work:**

```
tasks | where not done | where due this week | where tags has "work" | sort due
```

**Recently created pages:**

```
pages | sort created desc | limit 20
```

**All pages about Rust, most linked first:**

```
pages | where tags has "rust" | sort backlinks count desc
```

**Orphan pages (no incoming links):**

```
pages | where backlinks count = 0
```

**Journal entries from February:**

```
journal | where date after 2026-02-01 | where date before 2026-03-01
```

**Open questions (tasks I wrote as questions):**

```
tasks | where not done | where text contains "?"
```

**Dashboard: how much did I write this week?**

```
blocks | where created this week | group page | count
```

---

## Embedding Queries in Notes

A fenced code block with the `bloom` language tag becomes a live view:

````markdown
## My Work Dashboard

Open tasks for this week:

```bloom
tasks | where not done | where tags has "work" | where due this week | sort due
```

Recently modified pages:

```bloom
pages | sort modified desc | limit 5
```

Total open tasks: `bloom: tasks | where not done | count`
````

**Rendering:**
- In Bloom: the code block is replaced with a live, interactive result set. Results update as notes change. You can act on results (toggle tasks, jump to source) just like in the agenda.
- In other editors / GitHub: it's a readable code block. Portable Markdown. No lock-in.
- Inline queries (`` `bloom: ...` ``) render as inline values — numbers, short lists.

**Code-block safety:** Bloom query blocks are NOT evaluated inside regular fenced code blocks or nested code blocks. Same rule as existing Bloom extensions.

---

## Interaction Model

### As a picker/view (`SPC v v`)

`SPC v v` opens a **live view** prompt. Type a query, see results in real-time (same as the picker, but driven by BQL instead of fuzzy match). Results are interactive — you can jump to source, toggle tasks, preview content.

### As embedded views in notes

Write a `bloom` code block in any note. The results render live in the editor pane.

### As saved views

`:save-view <name>` saves the current query. `SPC v s` opens saved views. Saved views are stored in `config.toml` or a `views.toml` file.

### Built-in views (replace hard-coded features)

| Current feature | BQL equivalent | Keybinding (unchanged) |
|----------------|----------------|----------------------|
| Agenda | `tasks \| where not done \| sort due \| group due category` | `SPC a a` |
| Backlinks | `links \| where to = $current \| sort from` | `SPC s l` |
| Tag browse | `tags \| sort count desc` | `SPC s t` |
| Journal search | `journal \| sort date desc` | `SPC s j` |

The keybindings still work exactly as today — they just invoke a BQL query under the hood. Users who never learn BQL see zero difference. Users who do can customise or create their own.

`$current` is a context variable — the current page's ID. Other context variables: `$today`, `$yesterday`, `$tomorrow`.

---

## Implementation Architecture

### Parser

A simple hand-written recursive descent parser. The grammar is small enough that a parser generator is overkill. Parsing a query should take microseconds.

```
Query     = Source ("|" Clause)*
Source    = "pages" | "tasks" | "journal" | "blocks" | "tags" | "links"
Clause    = Where | Sort | Group | Select | Limit | Count
Where     = "where" Predicate
Sort      = "sort" FieldOrder ("," FieldOrder)*
Group     = "group" Field GroupModifier?
Select    = "select" Field ("," Field)*
Limit     = "limit" Number
Count     = "count"
Predicate = Field Operator Value | "not" Predicate | "any(" Predicate ("," Predicate)* ")"
```

### Execution

Queries execute against the SQLite index. Each clause maps to SQL operations:

```
tasks | where not done | where tags has "work" | sort due
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

Execution always goes through the existing read-only index connection. No new database access patterns.

### Live Updates

Embedded queries re-execute when the index changes (after the indexer sends `IndexComplete`). The UI thread receives the notification, re-runs any visible queries, and re-renders if results changed. This is the same pattern as the current backlinks/agenda refresh.

Cost: a few extra SQLite queries per index update. At <1 ms per query, this is negligible even with dozens of embedded views.

### New Modules

| Module | Responsibility |
|--------|---------------|
| `query/parse.rs` | BQL tokeniser + parser → AST |
| `query/compile.rs` | AST → SQL query string |
| `query/execute.rs` | Run compiled query against index, return typed result sets |
| `query/builtins.rs` | Built-in queries (agenda, backlinks, etc.) as BQL constants |
| `query/mod.rs` | Public API: `parse()`, `execute()`, `QueryResult` types |

### Render Integration

Query results produce a `QueryResultFrame` (new variant in `RenderFrame`) containing:
- Column headers (derived from `select` or source defaults)
- Typed rows (strings, dates, booleans, numbers)
- Group headers (from `group`)
- Source locations (page + line, for jump-to-source)
- Actions available per row (toggle task, open page, etc.)

The TUI renders this as a table/list — same visual language as the existing agenda and picker results.

---

## Migration: Agenda as a Query

The current `Agenda` struct, `AgendaView`, `AgendaFrame`, and the dedicated agenda module become **thin wrappers** around a BQL query:

```rust
pub const AGENDA_QUERY: &str =
    "tasks | where not done | sort due | group due category";
```

The `SPC a a` keybinding executes this query and renders the result. The agenda module shrinks to a constant + any agenda-specific actions (toggle, reschedule).

This is a **backwards-compatible** change — users see the exact same agenda. But now they can also write `tasks | where not done | where tags has "work" | sort due | group page` and get a work-specific agenda with zero core code changes.

---

## Open Questions

1. **Error reporting.** What happens when a query has a syntax error in an embedded block? Show the error inline (like a broken link indicator)? Show the raw query text with a diagnostic underline?

2. **Performance guardrails.** Should we prevent `blocks` queries on huge vaults (100K+ blocks)? Auto-add `limit 1000`? Show a warning?

3. **Extensibility.** Can users define custom sources? (e.g., a source backed by a shell command or external data.) Probably out of scope for v1 but the architecture should allow it.

4. **Aggregations beyond count.** Do we need `sum`, `avg`, `min`, `max`? Probably not for v1. `count` covers 90% of dashboard use cases.

5. **Field arithmetic.** `where backlinks count > links count` — do we support computed fields? Adds complexity. Probably defer.

6. **Result caching.** If the same query appears in 5 different notes, do we cache the result? The index connection is read-only and queries are fast (<1 ms), so maybe not worth the complexity.

7. **Cross-referencing queries.** Can one query reference another? `saved:my-work-tasks | count` — referring to a saved view by name. Useful for dashboards but adds a dependency graph.

---

## Non-Goals

- **Turing completeness.** BQL is a query language, not a programming language. No variables, no loops, no conditionals, no functions. If you need that, use the MCP server.
- **Write operations.** Queries are read-only projections. You can *act on* results (toggle a task), but the query itself never mutates data.
- **Cross-vault queries.** Single vault only (consistent with Bloom's v1 scope).
- **Real-time typing queries.** Queries in embedded blocks re-execute on index change, not on every keystroke in the query text. The `SPC v v` interactive prompt does update live as you type, same as the picker.

---

## References

- [Dataview (Obsidian)](https://blacksmithgu.github.io/obsidian-dataview/) — the closest existing thing. JavaScript-based, Obsidian-only, much heavier syntax. BQL aims to be simpler.
- [Org-mode agenda](https://orgmode.org/manual/Agenda-Views.html) — the inspiration for Bloom's agenda. Powerful but complex custom agenda commands.
- [jq](https://stedolan.github.io/jq/) — pipe-based composition for JSON. BQL borrows the `|` chaining philosophy.
- [Logseq Advanced Queries](https://docs.logseq.com/#/page/advanced%20queries) — Datalog-based. Powerful but steep learning curve. BQL explicitly avoids this complexity.
