# Codegen Refactoring - Use codegen crate AST API

## Problem

The current `dibs-query-gen` codegen implementation uses manual string manipulation (`push_str`, `format!`, etc.) to generate Rust code instead of leveraging the `codegen` crate's AST-based API.

### Example of Current Approach (String Manipulation)

```rust
fn generate_join_query_body(ctx: &CodegenContext, query: &Query, struct_name: &str) -> String {
    let mut body = String::new();
    body.push_str(&format!("const SQL: &str = r#\"{}\"#;\n\n", generated.sql));
    
    if params.is_empty() {
        body.push_str("let rows = client.query(SQL, &[]).await?;\n\n");
    } else {
        body.push_str("let rows = client.query(SQL, &[");
        for (i, param_name) in params.iter().enumerate() {
            if i > 0 {
                body.push_str(", ");
            }
            body.push_str(param_name);
        }
        body.push_str("]).await?;\n\n");
    }
    // ... more manual string building
}
```

### Expected Approach (codegen AST API)

```rust
fn generate_join_query_body(ctx: &CodegenContext, query: &Query, struct_name: &str) -> Block {
    let mut block = Block::new();
    
    // SQL constant
    block.push_stmt(Stmt::Local(Local {
        name: Ident::new("SQL"),
        init: Some(Expr::StringLiteral(generated.sql)),
        ..
    }));
    
    // Parameterized query
    let args = params.iter()
        .map(|name| Expr::Path(Ident::new(name)))
        .collect();
    block.push_stmt(Stmt::Local(Local {
        name: Ident::new("rows"),
        init: Some(Expr::MethodCall(
            Box::new(Expr::Path(Ident::new("client"))),
            "query",
            vec![
                Expr::Path(Ident::new("SQL")),
                Expr::ArrayLiteral(args),
            ],
        )),
        ..
    }));
    
    block
}
```

## Why This Matters

1. **Type Safety**: AST-based codegen catches errors at compile time, not runtime
2. **Maintainability**: Adding new features doesn't require scattered string manipulation
3. **Correctness**: AST ensures valid Rust syntax (matching braces, semicolons, etc.)
4. **Consistency**: Leverages battle-tested codegen crate instead of reinventing
5. **Debugging**: Generated code is structured, not opaque strings

## Scope

This refactoring should cover:

- **`generate_query_function`**: Convert to use `Function` and `Block` from codegen
- **`generate_join_query_body`**: Use AST for complex JOIN assembly logic
- **`generate_vec_relation_assembly`**: AST-based nested struct building
- **`generate_option_relation_assembly`**: AST-based optional relation handling
- **`generate_mutation_body`**: Unified AST approach for INSERT/UPDATE/DELETE/UPSERT
- **`generate_result_struct`**: Use `Struct` from codegen crate

## Notes

- The current approach technically works but is brittle and error-prone
- This is technical debt that should be addressed before adding more complex features
- Refactoring can be done incrementally, one function at a time
- Consider adding integration tests that compile generated code to catch regressions

## Success Criteria

1. No manual `push_str` / `format!` for Rust code generation
2. All generated code passes `cargo check` and clippy
3. No functional regressions (all existing tests pass)
4. Generated code is readable and properly formatted
5. Adding new operators/features doesn't require touching multiple string concatenation sites

## Related Issues

- Todo 004 (JSONB operators) - Works despite codegen debt
- Todo 006 (DISTINCT) - Should be easier with proper codegen
- Todo 007 (GROUP BY / HAVING) - Complex case that needs solid codegen