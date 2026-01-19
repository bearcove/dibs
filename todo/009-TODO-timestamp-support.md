# 009: Timestamp (jiff) Support

**Status:** TODO
**Priority:** Medium

## Problem

`jiff::Timestamp` fields cause runtime deserialization errors because `facet-tokio-postgres` doesn't have jiff support.

### Current Workaround

Timestamp columns are excluded from SELECT:
```styx
// created_at removed because jiff::Timestamp deserialization not yet supported
ProductByHandle @query{
  select{ id, handle, status }  // no created_at
}
```

### Error

When trying to deserialize a TIMESTAMPTZ column to jiff::Timestamp:
```
Error: column type mismatch
```

## Solution Options

### Option A: Add jiff support to facet-tokio-postgres

Implement `FromSql` for jiff types in the facet ecosystem.

**Location:** https://github.com/facet-rs/facet (facet-tokio-postgres crate)

**Types needed:**
- `jiff::Timestamp` <-> `TIMESTAMPTZ`
- `jiff::civil::Date` <-> `DATE`
- `jiff::civil::Time` <-> `TIME`

### Option B: Use chrono types in query results

Map TIMESTAMPTZ to chrono types which are already supported, then convert to jiff in application code.

**Pros:** Works now
**Cons:** Extra conversion, inconsistent types

### Option C: Manual row extraction for timestamps

Don't use `from_row()` for queries with timestamps, instead generate manual extraction code that handles the conversion.

## Recommendation

**Option A** is the right long-term solution. This is a contribution to facet-rs.

## Implementation (for Option A)

In `facet-tokio-postgres/src/lib.rs`:

```rust
#[cfg(feature = "jiff")]
impl FromSql for jiff::Timestamp {
    fn from_sql(ty: &Type, raw: &[u8]) -> Result<Self, Box<dyn Error + Sync + Send>> {
        // TIMESTAMPTZ is stored as microseconds since 2000-01-01
        let pg_epoch = jiff::Timestamp::from_second(946684800)?; // 2000-01-01 00:00:00 UTC
        let micros: i64 = postgres_types::private::read_be_i64(raw);
        Ok(pg_epoch + jiff::Span::new().microseconds(micros))
    }

    fn accepts(ty: &Type) -> bool {
        matches!(*ty, Type::TIMESTAMPTZ | Type::TIMESTAMP)
    }
}
```

## Files to Modify

- External: `facet-tokio-postgres` crate
- Then: Update queries to include timestamp columns

## Testing

- Query with `created_at` column
- Verify correct timezone handling
- Test NULL timestamps (Option<Timestamp>)
