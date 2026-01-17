# Phase 002: Schema Introspection

Read the current schema from a live Postgres database.

## Tasks

### 1. Query tables from information_schema

```sql
SELECT table_name
FROM information_schema.tables
WHERE table_schema = 'public'
  AND table_type = 'BASE TABLE';
```

### 2. Query columns

```sql
SELECT column_name, data_type, is_nullable, column_default
FROM information_schema.columns
WHERE table_schema = 'public' AND table_name = $1
ORDER BY ordinal_position;
```

### 3. Query primary keys

```sql
SELECT kcu.column_name
FROM information_schema.table_constraints tc
JOIN information_schema.key_column_usage kcu 
    ON tc.constraint_name = kcu.constraint_name
WHERE tc.constraint_type = 'PRIMARY KEY'
    AND tc.table_name = $1;
```

### 4. Query foreign keys

```sql
SELECT kcu.column_name, ccu.table_name, ccu.column_name
FROM information_schema.table_constraints tc
JOIN information_schema.key_column_usage kcu ON tc.constraint_name = kcu.constraint_name
JOIN information_schema.constraint_column_usage ccu ON tc.constraint_name = ccu.constraint_name
WHERE tc.constraint_type = 'FOREIGN KEY' AND tc.table_name = $1;
```

### 5. Query unique constraints

```sql
SELECT kcu.column_name
FROM information_schema.table_constraints tc
JOIN information_schema.key_column_usage kcu ON tc.constraint_name = kcu.constraint_name
WHERE tc.constraint_type = 'UNIQUE' AND tc.table_name = $1;
```

### 6. Build Schema from queries

```rust
impl Schema {
    pub async fn from_database(client: &Client) -> Result<Self, Error> {
        // ...
    }
}
```

### 7. Wire up `dibs status`

Show database schema summary.

## Acceptance criteria

- [ ] Can connect and read tables
- [ ] Columns read with types and nullability
- [ ] Primary keys identified
- [ ] Foreign keys identified
- [ ] Unique constraints captured
- [ ] `dibs status` shows DB schema
