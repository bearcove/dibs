# 002: Query Linting & Validation

**Priority:** Medium

## Problem

Queries can have subtle issues that would fail at runtime or produce unexpected results. We should catch these at compile time (LSP squiggles + build errors/warnings).

## Implemented ✓

- [x] **OFFSET without LIMIT** - warning
- [x] **LIMIT without ORDER BY** - warning  
- [x] **first without ORDER BY** (queries) - warning
- [x] **first without ORDER BY** (relations) - warning
- [x] **UPDATE/DELETE without WHERE** - error
- [x] **Unused param** - warning
- [x] **Missing deleted_at filter** - warning (soft-delete tables)
- [x] **Hard delete on soft-delete table** - warning
- [x] **Param type vs column type mismatch** - error
- [x] **Literal type vs column type mismatch** - error
- [x] **No FK relationship in @rel** - error

## To Implement

### Query Performance

- [ ] **Large OFFSET warning** - `offset 10000` is a performance anti-pattern (cursor pagination better)
- [ ] **ORDER BY on non-indexed column** - warn if ordering by unindexed column
- [ ] **WHERE on non-indexed column** - hint that adding index might help (low severity)

### Data Integrity

- [ ] **Nullable column comparison** - `where {email $email}` on nullable column might miss NULL rows
- [ ] **NULL = NULL gotcha** - comparing two nullable columns (NULL = NULL is false!)

### Code Quality

- [ ] **Duplicate column in select** - selecting the same column twice
- [ ] **Conflicting WHERE conditions** - `where {status "active", status "draft"}` (always empty)
- [ ] **Tautology detection** - `where {id id}` (comparing column to itself)
- [ ] **Empty select block** - `select {}` with no columns

### Schema-Aware

- [ ] **Enum value validation** - validate literals match column's enum_variants
- [ ] **Upsert on-conflict not unique** - target must be unique constraint

### Relations

- [ ] **Deep nesting warning** - relations nested 4+ levels may cause N+1

## Implementation Notes

### Schema Requirements

Some validations need schema metadata:
- Index information (for ORDER BY / WHERE index hints)
- Unique constraints (for upsert validation)
- Column nullability (for NULL comparison warnings)

`SchemaInfo` already has `indices: Vec<IndexInfo>` and columns have nullable info.
