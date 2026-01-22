# 005: SQL Function Calls

**Priority:** Medium

## Goal

Replace hardcoded `@now` with a general mechanism for calling SQL functions.

## Syntax

Any `@tag(args)` becomes a SQL function call:

```styx
values {
  created_at @now
  name @coalesce($name unnamed)
  slug @lower($handle)
  email @concat($subdomain ".example.com")
  full_name @concat($first " " $last)
}
```

Generates:

```sql
INSERT INTO ... VALUES (
  NOW(),
  COALESCE($1, 'unnamed'),
  LOWER($2),
  CONCAT($3, '.example.com'),
  CONCAT($4, ' ', $5)
)
```

## Rules

- `@funcname` with no payload = nullary function: `@now` → `NOW()`
- `@funcname(args...)` = function with args, space-separated
- Args can be: params (`$name`), literals (`unnamed`, `".example.com"`), or nested functions (`@lower($x)`)
- Strings with special chars need quotes: `".example.com"`, `"hello world"`

## Nesting

Functions can nest:

```styx
values {
  email @coalesce($email @concat($username "@example.com"))
  slug @lower(@concat($prefix "-" $handle))
}
```

## In WHERE clauses

In `where`, top-level tag is still an operator. But operators can take function results:

```styx
where {
  created_at @gt(@now)                    // created_at > NOW()
  name @ilike(@concat("%" $search "%"))   // name ILIKE CONCAT('%', $1, '%')
  handle @lower($input)                   // handle = LOWER($1) (equality with transformed value)
}
```

So:
- `@gt`, `@lt`, `@ilike`, etc. remain comparison operators
- `@now`, `@lower`, `@coalesce`, etc. are functions
- Context determines interpretation (operator position vs value position)

## Special cases

- `@default` stays special - it's the `DEFAULT` keyword, not a function
- `@null` / `@not-null` stay special - they're `IS NULL` / `IS NOT NULL`

## Migration

- `@now` currently hardcoded → becomes general function mechanism
- Backwards compatible (same syntax, just generalized)

## Future

Could add validation that function names are valid PostgreSQL functions, but probably not necessary - invalid functions will fail at query time with clear errors.
