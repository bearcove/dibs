# Migration Solver

## Problem

When generating migration SQL from a schema diff, the order of operations matters.

Example failure case:
```
Current DB: posts, comments (with FK to posts.id)
Desired:    post, comment (with FK to post.id)

Naive SQL generation:
  ALTER TABLE comments RENAME TO comment;
  ALTER TABLE comment ADD CONSTRAINT ... REFERENCES post(id);  -- FAILS: "post" doesn't exist yet
  ALTER TABLE posts RENAME TO post;
```

The FK references `post.id` before the table has been renamed from `posts` to `post`.

## Requirements

1. **Start state**: Current database schema (introspected)
2. **End state**: Desired schema (from Rust definitions)
3. **Solver**: Determine valid ordering of operations
4. **Verifiable**: Each step should be checkable against a virtual schema state

## Approach: Virtual Schema + Dependency Graph

### Phase 1: Model Operations with Dependencies

Each `Change` operation has:
- **Preconditions**: What must exist before this can run
- **Effects**: What this creates/removes/modifies

```rust
enum Dependency {
    TableExists(String),
    TableNotExists(String),
    ColumnExists { table: String, column: String },
    ConstraintNotExists { table: String, name: String },
}

impl Change {
    fn preconditions(&self) -> Vec<Dependency>;
    fn effects(&self) -> Vec<Effect>;
}
```

### Phase 2: Topological Sort

1. Build dependency graph from all changes
2. Topological sort to find valid ordering
3. Detect cycles (impossible migrations)

### Phase 3: Virtual Schema Validation

```rust
struct VirtualSchema {
    tables: HashMap<String, VirtualTable>,
}

impl VirtualSchema {
    fn from_introspected(schema: &Schema) -> Self;
    fn apply(&mut self, change: &Change) -> Result<(), ValidationError>;
    fn satisfies(&self, dep: &Dependency) -> bool;
}
```

For each change in the sorted order:
1. Check all preconditions against virtual schema
2. Apply the change to virtual schema
3. Verify virtual schema matches expected intermediate state

### Phase 4: SQL Generation

Generate SQL in the validated order, with comments explaining dependencies.

## Edge Cases

- **Circular renames**: `A -> B, B -> A` (needs temp name)
- **FK to renamed table**: Drop FK, rename table, recreate FK
- **Column rename with type change**: Order matters
- **Self-referential FK on renamed table**: Complex dependency

## Testing

Integration tests with actual Postgres:
1. Create initial schema
2. Generate migration for schema change
3. Run migration
4. Verify final schema matches desired

Test cases:
- [ ] Simple table rename (`users` -> `user`)
- [ ] Multiple table renames with FK dependencies
- [ ] Column rename
- [ ] Table rename + column changes
- [ ] Self-referential FK table rename
- [ ] Circular rename detection

## Open Questions

- Should we use an existing constraint solver library?
- How to handle data migrations mixed with schema changes?
- Should we generate multiple migration files for complex changes?
