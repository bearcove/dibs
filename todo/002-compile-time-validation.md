# 002: Foreign Key Validation

**Priority:** Low

## Problem

Foreign key relationships in queries are not validated for type compatibility. You can define a relation that joins on columns with mismatched types, which would fail at runtime or produce unexpected results.

## Goal

```
error: foreign key type mismatch in relation 'variants'
  --> queries.styx:12:5
   |
12 |     variants { fk product_id }
   |                ^^^^^^^^^^^^^
   |
   = note: product_id is INTEGER but product.id is BIGINT
```

## Implementation

When processing relations with `fk` declarations:

1. Look up the FK column type in the related table
2. Look up the PK/target column type in the parent table  
3. Compare types and emit diagnostic if mismatched

This validation should happen in:
- LSP extension (for editor squiggles)
- Codegen (to fail the build)
