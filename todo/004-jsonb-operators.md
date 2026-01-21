# 005: JSONB Operators

**Priority:** Medium

## Goal

Support PostgreSQL JSONB operators for flexible schema patterns.

## Syntax Ideas

**Path access in SELECT:**
```styx
select{
  id
  brand @json{ path "metadata.brand" }
}
```
→ `metadata->'brand' as brand`

**Filtering:**
```styx
where{
  metadata @json_path{ path "$.brand", eq $brand }
}
```
→ `metadata->>'brand' = $1`

**Containment:**
```styx
where{
  metadata @json_contains{ "premium": true }
}
```
→ `metadata @> '{"premium":true}'`

**Key existence:**
```styx
where{
  metadata @json_has_key{ "premium" }
}
```
→ `metadata ? 'premium'`

## PostgreSQL Operators

| Op | Description |
|----|-------------|
| `->` | Get field as JSON |
| `->>` | Get field as text |
| `@>` | Contains |
| `?` | Key exists |
| `?|` | Any key exists |
| `?&` | All keys exist |

## Files

- `ast.rs` - Add JSON field/filter types
- `parse.rs` - Parse JSON syntax
- `sql.rs` - Generate JSONB SQL
