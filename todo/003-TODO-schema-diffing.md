# Phase 003: Schema Diffing

Compare Rust-defined schema against database schema.

## Output format (from docs)

```
$ dibs diff
Changes detected:

  users:
    + email_normalized: TEXT (nullable)
    ~ name: VARCHAR(100) -> TEXT
```

## Tasks

### 1. Diff types

```rust
pub struct SchemaDiff {
    pub changes: Vec<TableDiff>,
}

pub struct TableDiff {
    pub table: String,
    pub changes: Vec<Change>,
}

pub enum Change {
    AddTable(Table),
    DropTable(String),
    AddColumn(Column),
    DropColumn(String),
    AlterColumn { name: String, from: PgType, to: PgType },
    AddPrimaryKey(Vec<String>),
    DropPrimaryKey,
    AddForeignKey(ForeignKey),
    DropForeignKey(String),
    AddUnique(Vec<String>),
    DropUnique(String),
}
```

### 2. Implement diffing

```rust
impl Schema {
    pub fn diff(&self, db: &Schema) -> SchemaDiff {
        // Tables in self but not db → AddTable
        // Tables in db but not self → DropTable  
        // Tables in both → diff columns/constraints
    }
}
```

### 3. Wire up `dibs diff`

Connect to DB, collect Rust schema, diff, print.

## Acceptance criteria

- [ ] Detects added tables
- [ ] Detects dropped tables
- [ ] Detects added columns
- [ ] Detects dropped columns
- [ ] Detects type changes
- [ ] Detects constraint changes
- [ ] `dibs diff` prints readable output
