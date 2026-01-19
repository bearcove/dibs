# dibs roadmap

Schema-first Postgres toolkit for Rust, powered by facet reflection.

## Done

- Schema definition via facet attributes
- Schema introspection from Postgres
- Schema diffing (Rust vs database)
- Migration generation (`dibs generate`)
- Migration execution with transactions (`dibs migrate`, `dibs status`)
- TUI schema browser with FK navigation

## In Progress

- Table rename detection in diff (implemented, needs solver for correct ordering)

## TODO

| File | Description |
|------|-------------|
| [008-TODO-migration-solver.md](./008-TODO-migration-solver.md) | **HIGH** - Correct ordering of migration operations |
| [009-TODO-tui-improvements.md](./009-TODO-tui-improvements.md) | TUI error display and usability |
| [010-TODO-integration-tests.md](./010-TODO-integration-tests.md) | Integration tests with real Postgres |
| [006-TODO-query-building.md](./006-TODO-query-building.md) | Type-safe queries (stretch goal) |

## Design decisions

### CLI-driven migrations (not automatic at startup)

Migrations run via `dibs migrate`, not automatically when the app starts. Reasons:
- Avoids race conditions with multiple replicas
- Clear failure point for debugging
- Can review/inspect before running
- Doesn't block health checks during slow migrations

### No `down` migrations

Only forward migrations. To rollback, write a new forward migration that undoes the change.
- Down migrations are rarely used in production
- Hard to write correctly for data migrations
- Adds cognitive overhead

### Diff against live database

`dibs diff` introspects the actual database via `information_schema`, not a local snapshot.
- Works against dev DB (for generating migrations)
- Works against staging/prod (for verification)
- Catches manual schema changes
