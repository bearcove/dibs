# dibs roadmap

Schema-first Postgres toolkit for Rust, powered by facet reflection.

## Done

- Schema definition via facet attributes
- Schema introspection from Postgres
- Schema diffing (Rust code vs live database)
- Table/column rename detection with heuristics
- Migration solver with dependency ordering
- Migration generation and execution with transactions
- TUI schema browser with FK navigation
- Query builder (SELECT, INSERT, UPDATE, DELETE with filters)
- Backoffice service (generic CRUD over roam)
- Admin UI (dibs-admin) with dashboard, list/detail views, inline editing
- Integration tests with testcontainers (rename execution, type changes, column defaults)
- TUI with schema browser, diff visualization, ariadne error display, syntax highlighting

## Next Up

### 1. Query Builder Completion

Current: basic SELECT/INSERT/UPDATE/DELETE with filters and pagination.

Missing:
- JOINs via foreign key relationships
- Aggregations (GROUP BY, HAVING)
- Subqueries
- Type-safe API (in addition to dynamic)

### 2. Hooks System

Business logic callbacks for mutations:

```rust
dibs::hooks! {
    orders => {
        before_create: |ctx, row| { /* validate */ },
        after_create: |ctx, row| { /* send email */ },
    },
}
```

Both storefront code and admin UI would respect these hooks.

## Design Decisions

### CLI-driven migrations

Migrations run via `dibs migrate`, not automatically at startup.
- Avoids race conditions with multiple replicas
- Clear failure point for debugging
- Can review before running

### No down migrations

Only forward migrations. To rollback, write a new forward migration.
- Down migrations are rarely used in production
- Hard to write correctly for data migrations

### Diff against live database

`dibs diff` introspects the actual database, not a local snapshot.
- Works against dev, staging, or prod
- Catches manual schema changes

## Non-Goals

- ORM functionality (dibs is schema + queries, not an ORM)
- Connection pooling (use deadpool, bb8, etc.)
- Non-Postgres databases
