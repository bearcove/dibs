# Phase 001: Schema Definition

Define database schemas using Rust structs with facet attributes.

## Decided

From the docs, the syntax is:

```rust
use dibs::prelude::*;
use facet::Facet;

#[derive(Facet)]
#[facet(dibs::table = "users")]
pub struct User {
    #[facet(dibs::pk)]
    pub id: i64,
    
    #[facet(dibs::unique)]
    pub email: String,
    
    pub name: String,
    
    #[facet(dibs::fkey = tenants::id)]
    pub tenant_id: i64,
}
```

## Tasks

### 1. Define facet attributes

In `dibs/src/attrs.rs` or similar:
- `dibs::table = "name"` — table name
- `dibs::pk` — primary key
- `dibs::unique` — unique constraint
- `dibs::fkey = table::column` — foreign key reference

### 2. Rust type → Postgres type mapping

| Rust | Postgres |
|------|----------|
| `i64` | `BIGINT` |
| `i32` | `INTEGER` |
| `String` | `TEXT` |
| `bool` | `BOOLEAN` |
| `Option<T>` | nullable `T` |
| `Vec<u8>` | `BYTEA` |

(Extend as needed)

### 3. In-memory schema representation

```rust
pub struct Schema {
    pub tables: Vec<Table>,
}

pub struct Table {
    pub name: String,
    pub columns: Vec<Column>,
    pub primary_key: Option<Vec<String>>,
    pub unique_constraints: Vec<Vec<String>>,
    pub foreign_keys: Vec<ForeignKey>,
}

pub struct Column {
    pub name: String,
    pub pg_type: PgType,
    pub nullable: bool,
}

pub struct ForeignKey {
    pub columns: Vec<String>,
    pub references_table: String,
    pub references_columns: Vec<String>,
}
```

### 4. Schema collection via facet reflection

Use facet to inspect struct shapes and extract `dibs::*` attributes. Register tables with `inventory`.

```rust
inventory::submit! { TableDef::new::<User>() }

let schema = Schema::collect();
```

### 5. Wire up `dibs schema` CLI

Print the collected schema in a readable format.

## Acceptance criteria

- [ ] `#[derive(Facet)]` + `#[facet(dibs::table)]` defines a table
- [ ] `dibs::pk`, `dibs::unique`, `dibs::fkey` attributes work
- [ ] `Schema::collect()` gathers all tables
- [ ] `dibs schema` prints the schema
