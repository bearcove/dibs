# 010: LSP Line Numbers

**Priority:** Medium

## Problem

`span_to_range()` uses character offsets as line 0:

```rust
fn span_to_range(span: &styx_tree::Span) -> Range {
    Range {
        start: Position { line: 0, character: span.start },
        end: Position { line: 0, character: span.end },
    }
}
```

## Solution Available

The LSP host provides `offset_to_position()`:

```rust
#[roam::service]
pub trait StyxLspHost {
    async fn offset_to_position(&self, params: OffsetToPositionParams) -> Option<Position>;
}
```

Just need to call it instead of hardcoding `line: 0`.

## Fix

Store the host client and use it:

```rust
let start = host.offset_to_position(OffsetToPositionParams { offset: span.start }).await?;
let end = host.offset_to_position(OffsetToPositionParams { offset: span.end }).await?;
Range { start, end }
```

This is a small change - the infrastructure exists.
