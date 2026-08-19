---
name: dibs
description: Use when working on a dibs Postgres schema/query project or the dibs CLI. This skill delegates to the installed dibs binary so guidance stays current.
compatibility: Compatible with the open Agent Skills standard. Requires dibs on PATH.
metadata:
  source: dibs-cli
---

# dibs

This installed skill is intentionally small. dibs's agent guidance is bundled
inside the `dibs` binary so the CLI can be updated without relying on a stale
copied skill.

Before making claims or edits in a dibs project, run:

```sh
dibs agent
```

Then follow the current CLI it points you to. dibs is a Postgres toolkit for
Rust powered by facet reflection: schemas are defined in Rust, migrations are
generated and applied with the `dibs` CLI, and queries are compile-time-checked
against typed SQL builders.

Config comes from `.config/dibs.styx` or environment variables (a `DIBS__`
prefix, plus `DATABASE_URL` for the connection string). dibs code uses Facet
reflection and `facet_json`; do not hand-write JSON.

To refresh this installed skill from the current binary:

```sh
dibs agent install
dibs agent install --dir .agents/skills/dibs
```