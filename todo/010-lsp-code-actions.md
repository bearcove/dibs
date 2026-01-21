# 011: LSP Code Actions

**Priority:** Low

## Current State

Returns empty:
```rust
async fn code_actions(&self, _params: CodeActionParams) -> Vec<CodeAction> {
    Vec::new()
}
```

## Ideas

**"Add missing column"** - When cursor is in select block, offer to add columns from table.

**"Create parameter"** - Convert literal to parameter:
```styx
where{ status "published" }
```
→
```styx
params{ status @string }
where{ status $status }
```

**"Add from clause"** - When `@rel` missing `from`, suggest based on field name.

**"Extract to named query"** - Pull out a relation into its own `@query`.

## Implementation

1. Parse document to understand cursor context
2. Generate appropriate `CodeAction` with `TextEdit`
3. Return in `code_actions()` method
