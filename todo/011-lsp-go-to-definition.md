# 012: LSP Go-to-Definition

**Priority:** Low

## Status

`Capability::Definition` exists in styx-lsp-ext but there's no `definition()` method in `StyxLspExtension` trait yet.

This is blocked on styx-lsp-ext adding the trait method.

## When Available

Jump from reference to definition:

- `$param` → param declaration in same query
- Table name → Rust struct (needs source locations from schema metadata)
