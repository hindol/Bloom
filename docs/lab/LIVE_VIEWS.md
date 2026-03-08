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

6. **Embeddable in notes.** A `` ```bloom `` code block renders as a live view. Other editors see readable code.

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

## Embedding Queries in Notes

A fenced code block with the `bloom` language tag becomes a live view:

````markdown
## My Work Dashboard

Open tasks for this week:

```bloom
tasks | where not done and due this week and tags has #work | sort due
```

Recently created pages:

```bloom
pages | sort created desc | limit 5
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
| Agenda | `tasks \| where not done \| sort due \| group due.category` | `SPC a a` |
| Backlinks | `links \| where to = $page` | `SPC s l` |
| Tag browse | `tags \| sort count desc` | `SPC s t` |
| Journal search | `journal \| sort date desc` | `SPC s j` |

The keybindings still work exactly as today — they just invoke a BQL query under the hood. Users who never learn BQL see zero difference. Users who do can customise or create their own.

`$page` is a context variable — the current page's ID. `$today` resolves to today's date.

---

## Implementation Architecture

### Parser

A hand-written recursive descent parser. The grammar is small enough that a parser generator is overkill. Parsing takes microseconds. Errors include position info for the live feedback UI.

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

### Live Feedback (`SPC v v`)

The interactive query prompt debounces at 150ms. On each pause:

1. **Tokenise + parse** → AST or error with position
2. **Compile** → SQL or semantic error (unknown field, type mismatch)
3. **Execute** → result set
4. **Render** → results below the input, or error inline with caret

This reuses the picker infrastructure: the query input is the picker input, the results are the picker results. BQL replaces the fuzzy matcher with parse → compile → execute.

### Embedded Views

Embedded queries (`` ```bloom ``) re-execute when the index changes (after `IndexComplete`). The UI thread re-runs visible queries and re-renders if results changed. Same pattern as backlinks/agenda refresh. Cost: <1ms per query.

### New Modules

| Module | Responsibility |
|--------|---------------|
| `query/parse.rs` | BQL tokeniser + parser → AST, with position-aware error diagnostics |
| `query/compile.rs` | AST → SQL query string, field/type validation |
| `query/execute.rs` | Run compiled query against index, return typed result sets |
| `query/builtins.rs` | Built-in queries (agenda, backlinks, etc.) as BQL constants |
| `query/mod.rs` | Public API: `parse()`, `execute()`, `QueryResult` types |

### Render Integration

Query results produce a `QueryResultFrame` (new variant in `RenderFrame`) containing:
- Column headers (derived from source defaults)
- Typed rows (strings, dates, booleans, numbers)
- Group headers (from `group`)
- Source locations (page + line, for jump-to-source via block ID)
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

1. **Error rendering in embedded blocks.** Show error inline with caret position? Or show the raw query text with a diagnostic underline? Leaning towards: red-tinted code block with error message below the query text.

2. **Performance guardrails.** Should we prevent `blocks` queries on huge vaults (100K+ blocks)? Auto-add `limit 1000`? Show a warning? Leaning towards: no implicit limit, but show result count and warn if > 1000.

3. **Week start configuration.** `this week` range depends on week start day. Config: `[calendar] week_starts = "monday"`. Default: Monday (ISO 8601).

4. **`contains` operator.** Useful for `tasks | where text contains "?"` (find questions). Not in the grammar yet. Add as a string operator alongside `=` and `!=`? Leaning towards: yes, add it — it's the only substring predicate users will need.

5. **`select` clause.** The original design had `select` to choose display columns. Removed for simplicity. Revisit if users request it.

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
