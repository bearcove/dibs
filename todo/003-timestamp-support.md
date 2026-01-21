# 004: Timestamp (jiff) Support

**Priority:** Medium

## Problem

`jiff::Timestamp` fields cause runtime errors. `facet-tokio-postgres` lacks jiff support.

Current workaround: exclude timestamp columns from SELECT.

## Fix

Add jiff support to `facet-tokio-postgres`:

```rust
#[cfg(feature = "jiff02")]
impl FromSql for jiff::Timestamp {
    fn from_sql(ty: &Type, raw: &[u8]) -> Result<Self, Box<dyn Error + Sync + Send>> {
        // TIMESTAMPTZ = microseconds since 2000-01-01
        let pg_epoch = jiff::Timestamp::from_second(946684800)?;
        let micros: i64 = read_be_i64(raw);
        Ok(pg_epoch + jiff::Span::new().microseconds(micros))
    }

    fn accepts(ty: &Type) -> bool {
        matches!(*ty, Type::TIMESTAMPTZ | Type::TIMESTAMP)
    }
}
```

Types needed:
- `jiff::Timestamp` ↔ `TIMESTAMPTZ`
- `jiff::civil::Date` ↔ `DATE`
- `jiff::civil::Time` ↔ `TIME`

This is upstream work in facet-rs.
