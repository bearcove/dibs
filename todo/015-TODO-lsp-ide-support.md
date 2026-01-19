# 015: .styx LSP / IDE Support

**Status:** TODO
**Priority:** Future

## Goal

Provide IDE support for .styx query files through a Language Server Protocol (LSP) implementation.

## Features

### Syntax Highlighting

- Keywords: `@query`, `@rel`, `@count`, `from`, `where`, `select`, etc.
- Parameters: `$name` references
- Types: `@string`, `@int`, `@bool`, etc.
- Operators: `@null`, `@ilike`, `@gt`, etc.
- Comments: `//` line comments

### Autocompletion

- Table names from schema
- Column names based on current `from` table
- Parameter names in scope
- Keywords and operators in context

### Diagnostics

- Syntax errors with precise locations
- Semantic errors (unknown table, column, etc.)
- Warnings for unsupported features

### Hover Information

- Column type on hover
- FK relationships
- Query explanation

### Go to Definition

- Jump from `$param` to its declaration
- Jump from relation `from` to table definition

### Code Actions

- "Add missing column to select"
- "Create parameter for literal value"

## Architecture

```
┌─────────────┐     ┌─────────────┐     ┌──────────────┐
│   Editor    │◄───►│  LSP Server │◄───►│ Query Parser │
│ (VS Code)   │     │             │     │   + Schema   │
└─────────────┘     └─────────────┘     └──────────────┘
```

### LSP Server

Use `tower-lsp` crate for Rust LSP implementation:

```rust
use tower_lsp::{LspService, Server};
use tower_lsp::lsp_types::*;

#[derive(Debug)]
struct StyxLanguageServer {
    client: Client,
    schema: Arc<RwLock<Option<PlannerSchema>>>,
}

#[tower_lsp::async_trait]
impl LanguageServer for StyxLanguageServer {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions::default()),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        // Provide completions based on context
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        // Show type info on hover
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        // Re-parse and publish diagnostics
    }
}
```

### VS Code Extension

```json
{
  "name": "styx-query",
  "contributes": {
    "languages": [{
      "id": "styx",
      "extensions": [".styx"],
      "configuration": "./language-configuration.json"
    }],
    "grammars": [{
      "language": "styx",
      "scopeName": "source.styx",
      "path": "./syntaxes/styx.tmLanguage.json"
    }]
  }
}
```

## Implementation Phases

### Phase 1: Basic Syntax Support
- TextMate grammar for syntax highlighting
- Basic language configuration (brackets, comments)

### Phase 2: LSP Diagnostics
- Parse errors with locations
- Basic semantic errors

### Phase 3: Completions
- Keyword completion
- Table/column completion (requires schema)

### Phase 4: Advanced Features
- Hover information
- Go to definition
- Code actions

## Files to Create

- `crates/styx-lsp/` - New crate for LSP server
- `editors/vscode/` - VS Code extension
- `editors/vscode/syntaxes/styx.tmLanguage.json` - TextMate grammar

## Dependencies

- `tower-lsp` - LSP server framework
- `ropey` - Efficient text rope for editing
- Schema access at runtime (may need daemon or file watching)

## Challenges

- Schema availability: LSP needs schema info but it's collected at build time
- Options:
  1. Generate schema JSON file during build
  2. Run LSP as cargo subcommand with access to crate
  3. Watch for schema changes and reload
