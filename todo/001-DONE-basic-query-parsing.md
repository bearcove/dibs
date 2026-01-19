# 001: Basic Query Parsing and SQL Generation

**Status:** DONE

## Summary

Parse .styx query files and generate SQL + Rust code.

## What Was Implemented

- KDL-based query syntax via styx parser
- AST types: `Query`, `Field`, `Filter`, `OrderBy`, `Expr`
- SQL generation for SELECT, FROM, WHERE, ORDER BY, LIMIT
- Filter operators: `=`, `!=`, `<`, `>`, `<=`, `>=`, `LIKE`, `ILIKE`, `IN`, `IS NULL`, `IS NOT NULL`
- Rust codegen: result structs with Facet derive, async query functions

## Files

- `crates/dibs-query-gen/src/ast.rs` - AST types
- `crates/dibs-query-gen/src/parse.rs` - Parser
- `crates/dibs-query-gen/src/sql.rs` - SQL generation
- `crates/dibs-query-gen/src/codegen.rs` - Rust codegen

## Example

```styx
AllProducts @query{
  from product
  where{ status "published", deleted_at @null }
  order_by{ created_at desc }
  limit 20
  select{ id, handle, status }
}
```

Generates:
```sql
SELECT "id", "handle", "status" FROM "product"
WHERE "status" = 'published' AND "deleted_at" IS NULL
ORDER BY "created_at" DESC LIMIT 20
```
