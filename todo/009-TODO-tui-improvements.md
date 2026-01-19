# TUI Improvements

## Error Display

### Issue: Migration errors not showing in dialog

When a migration fails, the error should display in a modal dialog with:
- Full error message
- SQL that failed
- Line/position highlighting (using ariadne)
- Postgres hints and details

**Current state**: Error handling code exists (`format_sql_error` uses ariadne) but may not be triggering the modal correctly.

**Investigation needed**:
- [ ] Verify `show_error()` is called with the formatted error
- [ ] Check if multi-line errors trigger modal (`msg.contains('\n') || msg.len() > 60`)
- [ ] Test error path end-to-end

### Issue: Line numbers not showing for SQL errors

The infrastructure exists:
- `Error::from_postgres_with_sql()` captures position
- `SqlError` proto has `position: Option<u32>`
- `format_sql_error()` uses ariadne to render

**Investigation needed**:
- [ ] Verify position is being passed through the entire chain
- [ ] Check if `tokio_postgres::Error` includes position for syntax errors
- [ ] Test with known-bad SQL to see what position info we get

## Usability

### Issue: Diff view shows individual changes, not migration preview

When viewing the diff, user should see:
- [ ] Preview of the SQL that would be generated
- [ ] Validation status (will this migration work?)
- [ ] Warnings for potentially dangerous operations

### Issue: No way to see migration SQL before running

- [ ] Add 'p' key to preview migration SQL
- [ ] Show dependency order once solver is implemented

## Code Quality

### Missing integration tests

Currently only unit tests in `diff.rs`. Need:
- [ ] Integration tests with real Postgres (testcontainers)
- [ ] Test rename detection end-to-end
- [ ] Test migration generation and execution
- [ ] Test error handling paths

See `008-TODO-migration-solver.md` for specific test cases.
