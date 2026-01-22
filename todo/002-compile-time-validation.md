# 002: Query Linting & Validation

**Status:** Mostly Complete ✓

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
- [x] **Large OFFSET warning** (>1000) - warning
- [x] **Empty select block** - warning
- [x] **Enum value validation** - validates literals against column's enum_variants
- [x] **Duplicate columns / Conflicting WHERE** - handled by styx parser (duplicate key errors)

## Remaining Ideas (Low Priority)

### Query Performance
- [ ] **ORDER BY on non-indexed column** - warn if ordering by unindexed column
- [ ] **WHERE on non-indexed column** - hint that adding index might help

### Data Integrity
- [ ] **Nullable column comparison** - `where {email $email}` on nullable column might miss NULL rows

### Schema-Aware
- [ ] **Upsert on-conflict not unique** - target must be unique constraint

### Relations
- [ ] **Deep nesting warning** - relations nested 4+ levels may cause N+1
