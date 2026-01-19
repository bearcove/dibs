# 010: JSONB Operators

**Status:** TODO
**Priority:** Medium

## Goal

Support PostgreSQL JSONB operators in queries for flexible schema patterns.

## Desired Syntax

### JSON path access in SELECT

```styx
ProductMetadata @query{
  from product
  select{
    id
    handle
    brand @json{ path "metadata.brand" }
    tags @json{ path "metadata.tags", type "string[]" }
  }
}
```

Generates:
```sql
SELECT id, handle,
       metadata->'brand' as brand,
       metadata->'tags' as tags
FROM product
```

### JSON filtering in WHERE

```styx
ProductsByBrand @query{
  params{ brand @string }
  from product
  where{
    metadata @json_contains{ "brand": $brand }
  }
}
```

Generates:
```sql
SELECT ... FROM product
WHERE metadata @> jsonb_build_object('brand', $1)
```

Or with path operator:
```styx
where{
  metadata @json_path{ path "$.brand", eq $brand }
}
```

Generates:
```sql
WHERE metadata->>'brand' = $1
```

### JSON existence check

```styx
where{
  metadata @json_has_key{ "premium" }
}
```

Generates:
```sql
WHERE metadata ? 'premium'
```

## PostgreSQL JSONB Operators

| Operator | Description | Example |
|----------|-------------|---------|
| `->` | Get JSON object field (as JSON) | `data->'key'` |
| `->>` | Get JSON object field (as text) | `data->>'key'` |
| `#>` | Get JSON object at path (as JSON) | `data#>'{a,b}'` |
| `#>>` | Get JSON object at path (as text) | `data#>>'{a,b}'` |
| `@>` | Contains | `data @> '{"a":1}'` |
| `<@` | Contained by | `data <@ '{"a":1}'` |
| `?` | Key exists | `data ? 'key'` |
| `?|` | Any key exists | `data ?| array['a','b']` |
| `?&` | All keys exist | `data ?& array['a','b']` |
| `||` | Concatenate | `data || '{"b":2}'` |
| `-` | Delete key | `data - 'key'` |
| `jsonb_path_query` | SQL/JSON path | `jsonb_path_query(data, '$.a')` |

## Implementation Steps

1. Add new AST types for JSON operations
2. Parse JSON path syntax in styx
3. Generate appropriate SQL operators
4. Handle type coercion (JSON -> Rust types)

## Files to Modify

- `crates/dibs-query-gen/src/ast.rs` - Add JSON field/filter types
- `crates/dibs-query-gen/src/parse.rs` - Parse JSON syntax
- `crates/dibs-query-gen/src/sql.rs` - Generate JSONB SQL

## Considerations

- Type safety: JSON paths can return any type
- NULL handling: Missing paths return NULL
- Performance: Consider GIN indexes for `@>` queries
