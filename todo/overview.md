# Dibs Query DSL - TODO

## High Priority

| # | Title | Notes |
|---|-------|-------|
| ~~001~~ | ~~Relation-level ORDER BY~~ | ✓ Done (uses LATERAL for first:true) |
| ~~002~~ | ~~Nested relations~~ | ✓ Done (recursive planner + HashSet dedup) |

## Medium Priority

| # | Title | Notes |
|---|-------|-------|
| ~~003~~ | ~~Timestamp (jiff) support~~ | ✓ Done (facet-tokio-postgres) |
| 004 | JSONB operators | `->`, `->>`, `@>`, `?` |
| ~~005~~ | ~~More filter operators~~ | ✓ Done (`@ne`, `@gte`, `@lte`, `@in`, `@not_null`) |
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
- **Relation-level ORDER BY** ✓ (uses LATERAL for `first: true`)
- **Nested relations** ✓ (product → variants → prices)
- Filter operators: `@null`, `@not-null`, `@ilike`, `@like`, `@gt`, `@lt`, `@gte`, `@lte`, `@ne`, `@in`, bare equality
- Raw SQL escape hatch: `sql <<SQL ... SQL`
- LSP: completions, hover, diagnostics, inlay hints
