# Dibs Query DSL - TODO

## Query Features

| # | Title | Notes |
|---|-------|-------|
| 001 | GROUP BY / HAVING | Aggregates beyond COUNT |
| 002 | Compile-time validation | Warn on unsupported features |

## LSP

| # | Title | Notes |
|---|-------|-------|
| 003 | LSP code actions | Currently empty |

## What's Done

- Basic query parsing and SQL generation
- Parameter binding (`$param`)
- LIMIT/OFFSET pagination
- Single-level JOINs (`first: true` → `Option<T>`)
- Vec relation grouping (`first: false` → `Vec<T>`)
- COUNT aggregates via `@count(table)`
- Relation-level WHERE clauses
- Relation-level ORDER BY (uses LATERAL for `first: true`)
- Nested relations (product → variants → prices)
- Filter operators: `@null`, `@not-null`, `@ilike`, `@like`, `@gt`, `@lt`, `@gte`, `@lte`, `@ne`, `@in`, bare equality
- JSONB operators (`@json-get`, `@json-get-text`, `@contains`, `@key-exists`)
- DISTINCT and DISTINCT ON
- Raw SQL escape hatch: `sql <<SQL ... SQL`
- LSP: completions, hover, diagnostics, inlay hints (with proper line numbers)
- LSP: go-to-definition ($param → declaration)
- Codegen refactoring (Block-based generation)
