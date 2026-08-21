# dibs Agent Guide

If you are an agent working on a dibs project — either a Postgres schema/query
project built on dibs, or the dibs CLI itself — start from the local source and
the current `dibs` binary.

## Anchor: What dibs Is

dibs is a Postgres toolkit for Rust powered by facet reflection. Typed schemas
are defined in Rust, migrations are generated from Rust types and applied with
the `dibs` CLI, and queries are compile-time-checked against typed SQL builders
instead of hand-written SQL strings. The schema, the PostgreSQL database, and
the query surface all stay in sync through reflection.

Key properties:

- Schemas are Rust types introspected via the `facet` crate — no separate
  schema file to hand-maintain.
- Migrations are generated (`generate`, `generate-from-diff`) and applied
  (`migrate`) through the CLI, with a `status` command to inspect where the
  database stands.
- Queries are checked at compile time by dibs's query compiler; prefer the
  typed builders over raw SQL.
- JSON is serialized through Facet and `facet_json`. Never hand-write JSON in
  dibs code.

## Anchor: Configuration

dibs reads configuration from `.config/dibs.styx` (Styx format, not
TOML/YAML) or from environment variables:

- `DIBS__`-prefixed variables map to config fields.
- `DATABASE_URL` provides the PostgreSQL connection string (also settable as
  `database_url` in `.config/dibs.styx`).

Copy embedded Styx schemas alongside the binary for editor support:

```sh
styx extract $(which dibs)
```

## Anchor: First Commands

Use these before making claims about the project, to ground yourself in the
real CLI surface:

```sh
dibs --help
dibs schema --plain
dibs status
dibs diff
```

Do not invent flags — `dibs --help` is authoritative about what each subcommand
accepts.

## Anchor: Subcommands

The real subcommands, grounded in the dibs `Commands` enum:

- `dibs migrate` — run pending migrations.
- `dibs status` — show migration status.
- `dibs diff` — compare the schema to the database.
- `dibs generate <name>` — generate a migration skeleton (e.g.
  `dibs generate add-users-table`).
- `dibs generate-from-diff <name>` — generate a migration from the current
  schema diff.
- `dibs schema [--plain | --sql]` — browse the current schema. With no flags
  it opens an interactive TUI on a TTY; `--plain` prints it as text (the
  default when not a TTY); `--sql` prints `CREATE TABLE` statements.
- `dibs agent` — print this guide; `dibs agent install` writes the thin
  delegator skill.

Most schema/query work starts with `dibs schema` to see what types are
registered before touching queries or migrations.

For traceability on the CLI itself, the dibs CLI uses `tracing` (via
`RUST_LOG`) for internal logging — routed through the tracing layer, not
`println!`/`eprintln!`, except for direct user-facing command output.

## Anchor: Edit Discipline

- Read `.config/dibs.styx` before changing config, connection, or query
  settings.
- When editing dibs itself, start with `cargo check -p dibs-cli` or
  `cargo build -p dibs-cli`.
- Prefer schema introspection (`dibs schema`) over guessing at table shapes.