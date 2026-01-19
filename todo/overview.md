# Query DSL Roadmap

## Status Overview

| # | Status | Title | Priority |
|---|--------|-------|----------|
| 001 | DONE | Basic query parsing and SQL generation | - |
| 002 | DONE | Parameter binding | - |
| 003 | DONE | LIMIT/OFFSET pagination | - |
| 004 | DONE | Single-level JOINs (first: true) | - |
| 005 | TODO | Fix Vec<T> relation grouping | Critical |
| 006 | TODO | Relation-level WHERE clauses | High |
| 007 | TODO | Relation-level ORDER BY | High |
| 008 | TODO | COUNT aggregates | Medium |
| 009 | TODO | Timestamp (jiff) support | Medium |
| 010 | TODO | JSONB operators | Medium |
| 011 | TODO | Nested relations | Medium |
| 012 | TODO | DISTINCT keyword | Low |
| 013 | TODO | GROUP BY / HAVING | Low |
| 014 | TODO | Compile-time validation | Low |
| 015 | TODO | .styx LSP / IDE support | Future |

## Current State

The query DSL is **usable for simple queries** but has significant gaps for production use with relations.

### What Works
- Single-table queries with filters, ordering, pagination
- Parameter binding (SQL injection safe)
- `first: true` relations (belongs-to / Option<T>)

### What's Broken
- `first: false` relations return wrong results (doesn't group by parent)
- Relation-level `where` and `order_by` are silently ignored

### Recommended Before Production
1. Fix #005 (Vec<T> grouping) - currently returns incorrect data
2. Fix #006 (relation WHERE) - currently silently ignored
3. Add #014 (validation) - warn about unsupported features
