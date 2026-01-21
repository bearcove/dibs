# Dibs Query DSL - TODO

## High Priority

| # | Title | Notes |
|---|-------|-------|
| 001 | Relation-level ORDER BY | Parsed but ignored |
| 002 | Nested relations | Relation within relation |

## Medium Priority

| # | Title | Notes |
|---|-------|-------|
| 003 | Timestamp (jiff) support | Needs facet-tokio-postgres work |
| 004 | JSONB operators | `->`, `->>`, `@>`, `?` |
| 005 | More filter operators | `@ne`, `@gte`, `@lte`, `@in`, `@between` |
| 006 | DISTINCT | `distinct true`, `distinct_on` |
| 007 | GROUP BY / HAVING | Aggregates beyond COUNT |
| 008 | Compile-time validation | Warn on unsupported features |

## LSP

| # | Title | Notes |
|---|-------|-------|
| 009 | LSP line numbers | Use host's `offset_to_position()` |
| 010 | LSP code actions | Currently empty |
| 011 | LSP go-to-definition | Blocked on styx-lsp-ext |

## What's Done

- Basic query parsing and SQL generation
- Parameter binding (`$param`)
- LIMIT/OFFSET pagination
- Single-level JOINs (`first: true` → `Option<T>`)
- Vec relation grouping (`first: false` → `Vec<T>`)
- COUNT aggregates via `@count(table)`
- **Relation-level WHERE clauses** ✓
- Filter operators: `@null`, `@ilike`, `@like`, `@gt`, `@lt`, bare equality
- Raw SQL escape hatch: `sql <<SQL ... SQL`
- LSP: completions, hover, diagnostics, inlay hints
