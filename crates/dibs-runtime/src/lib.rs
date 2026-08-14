//! Runtime types for dibs-generated query code.
//!
//! Generated functions capture an [`std::time::Instant`] immediately before
//! calling PostgreSQL, convert execution and decode errors with
//! [`WithQueryContext`], pass complete decoded rows or the affected count to
//! [`many`], [`optional`], [`one`], or [`exec`], then apply [`TraceCompletion`]
//! and [`TraceErr`]. The helpers never construct SQL or truncate result sets.
//!
//! No helper accepts SQL or bind values.

// Re-export tokio-postgres for query execution.
pub use tokio_postgres;

// Re-export facet for deriving.
pub use facet;

// Re-export facet-tokio-postgres for row deserialization.
pub use facet_tokio_postgres;

// Re-export common types used in generated structs.
pub mod types {
    pub use dibs_jsonb::Jsonb;
    pub use facet_value;
    pub use jiff::{Timestamp, civil::Date, civil::Time};
    pub use rust_decimal::Decimal;
    pub use uuid::Uuid;
}

/// Minimal identity attached to one generated query.
///
/// `identity` is deliberately opaque to the runtime. The compiler backend may
/// place its minimal stable identifier here without coupling this crate to the
/// compiler artifact that produced it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryContext {
    /// Public query declaration name.
    pub name: std::borrow::Cow<'static, str>,
    /// Opaque minimal query identity.
    pub identity: std::borrow::Cow<'static, str>,
}

impl QueryContext {
    /// Construct query context for generated code or dynamic runtime callers.
    pub fn new(
        name: impl Into<std::borrow::Cow<'static, str>>,
        identity: impl Into<std::borrow::Cow<'static, str>>,
    ) -> Self {
        Self {
            name: name.into(),
            identity: identity.into(),
        }
    }

    /// Construct allocation-free context for generated static constants.
    pub const fn from_static(name: &'static str, identity: &'static str) -> Self {
        Self {
            name: std::borrow::Cow::Borrowed(name),
            identity: std::borrow::Cow::Borrowed(identity),
        }
    }

    fn from_name(name: &'static str) -> Self {
        Self::from_static(name, name)
    }
}

/// Declared cardinality enforced by a generated query helper.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RowCountExpectation {
    /// Zero or one row is valid.
    AtMostOne,
    /// Exactly one row is valid.
    ExactlyOne,
}

/// Row-count contract that a query result violated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnexpectedRowCount {
    /// Declared row-count contract.
    pub expected: RowCountExpectation,
    /// Number of rows returned by PostgreSQL.
    pub actual: usize,
}

impl UnexpectedRowCount {
    /// Construct an `at most one row` violation.
    pub const fn at_most_one(actual: usize) -> Self {
        Self {
            expected: RowCountExpectation::AtMostOne,
            actual,
        }
    }

    /// Construct an `exactly one row` violation.
    pub const fn exactly_one(actual: usize) -> Self {
        Self {
            expected: RowCountExpectation::ExactlyOne,
            actual,
        }
    }

    /// Declared row-count contract that was violated.
    pub const fn expected(&self) -> RowCountExpectation {
        self.expected
    }

    /// Number of rows returned by PostgreSQL.
    pub const fn actual(&self) -> usize {
        self.actual
    }

    /// Whether the violated contract allowed zero rows.
    pub const fn allowed_zero(&self) -> bool {
        matches!(self.expected, RowCountExpectation::AtMostOne)
    }
}

impl std::fmt::Display for UnexpectedRowCount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let expected = match self.expected {
            RowCountExpectation::AtMostOne => "at most one row",
            RowCountExpectation::ExactlyOne => "exactly one row",
        };
        write!(f, "expected {expected}, got {}", self.actual)
    }
}

impl std::error::Error for UnexpectedRowCount {}

/// Typed category for a generated-query failure.
#[derive(Debug)]
pub enum QueryErrorKind {
    /// PostgreSQL query execution failed. The original error is retained.
    Database(Box<tokio_postgres::Error>),
    /// `facet-tokio-postgres` could not decode a row.
    Decode(Box<facet_tokio_postgres::Error>),
    /// The decoded result did not satisfy its declared result mode.
    UnexpectedRowCount(UnexpectedRowCount),
    /// A generated query parameter violates its static input contract.
    InvalidLimitParameter {
        /// Public generated parameter name.
        parameter: &'static str,
        /// Rejected signed bigint value.
        value: i64,
    },
}

/// Error type for generated query functions.
#[derive(Debug)]
pub struct QueryError {
    context: Option<QueryContext>,
    kind: QueryErrorKind,
}

impl QueryError {
    /// Preserve a PostgreSQL execution error before query context is attached.
    #[must_use]
    pub fn database(source: tokio_postgres::Error) -> Self {
        Self {
            context: None,
            kind: QueryErrorKind::Database(Box::new(source)),
        }
    }

    /// Preserve a facet row-decoding error before query context is attached.
    #[must_use]
    pub fn decode(source: facet_tokio_postgres::Error) -> Self {
        Self {
            context: None,
            kind: QueryErrorKind::Decode(Box::new(source)),
        }
    }

    fn row_count(context: QueryContext, source: UnexpectedRowCount) -> Self {
        Self {
            context: Some(context),
            kind: QueryErrorKind::UnexpectedRowCount(source),
        }
    }

    fn invalid_limit(context: QueryContext, parameter: &'static str, value: i64) -> Self {
        Self {
            context: Some(context),
            kind: QueryErrorKind::InvalidLimitParameter { parameter, value },
        }
    }

    /// Query identity associated with this failure, once attached.
    pub fn context(&self) -> Option<&QueryContext> {
        self.context.as_ref()
    }

    /// Attach the generated query's context while preserving the source error.
    #[must_use]
    pub fn with_context(mut self, context: QueryContext) -> Self {
        self.context = Some(context);
        self
    }

    /// Typed failure category and preserved source error.
    pub const fn kind(&self) -> &QueryErrorKind {
        &self.kind
    }

    /// Return the row-count violation when this is a cardinality error.
    pub const fn unexpected_row_count(&self) -> Option<&UnexpectedRowCount> {
        match &self.kind {
            QueryErrorKind::UnexpectedRowCount(source) => Some(source),
            QueryErrorKind::Database(_)
            | QueryErrorKind::Decode(_)
            | QueryErrorKind::InvalidLimitParameter { .. } => None,
        }
    }

    /// Preserved PostgreSQL error, when execution failed.
    pub fn database_source(&self) -> Option<&tokio_postgres::Error> {
        match &self.kind {
            QueryErrorKind::Database(source) => Some(source.as_ref()),
            QueryErrorKind::Decode(_)
            | QueryErrorKind::UnexpectedRowCount(_)
            | QueryErrorKind::InvalidLimitParameter { .. } => None,
        }
    }

    /// Preserved facet row-decoding error, when decoding failed.
    pub fn decode_source(&self) -> Option<&facet_tokio_postgres::Error> {
        match &self.kind {
            QueryErrorKind::Decode(source) => Some(source.as_ref()),
            QueryErrorKind::Database(_)
            | QueryErrorKind::UnexpectedRowCount(_)
            | QueryErrorKind::InvalidLimitParameter { .. } => None,
        }
    }

    /// PostgreSQL SQLSTATE, when the preserved execution error has one.
    pub fn sqlstate(&self) -> Option<&tokio_postgres::error::SqlState> {
        self.database_source().and_then(|source| source.code())
    }
}

impl std::fmt::Display for QueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(context) = &self.context {
            write!(f, "query {} ({}): ", context.name, context.identity)?;
        }
        match &self.kind {
            QueryErrorKind::Database(source) => write!(f, "database error: {source}"),
            QueryErrorKind::Decode(source) => write!(f, "row decoding error: {source}"),
            QueryErrorKind::UnexpectedRowCount(source) => source.fmt(f),
            QueryErrorKind::InvalidLimitParameter { parameter, .. } => {
                write!(
                    f,
                    "invalid LIMIT parameter {parameter}: expected non-negative bigint"
                )
            }
        }
    }
}

impl std::error::Error for QueryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            QueryErrorKind::Database(source) => Some(source.as_ref()),
            QueryErrorKind::Decode(source) => Some(source.as_ref()),
            QueryErrorKind::UnexpectedRowCount(source) => Some(source),
            QueryErrorKind::InvalidLimitParameter { .. } => None,
        }
    }
}

impl From<tokio_postgres::Error> for QueryError {
    fn from(source: tokio_postgres::Error) -> Self {
        Self::database(source)
    }
}

impl From<facet_tokio_postgres::Error> for QueryError {
    fn from(source: facet_tokio_postgres::Error) -> Self {
        Self::decode(source)
    }
}

/// Result type returned by generated query functions.
pub type QueryResult<T> = Result<T, QueryError>;

/// Enforce PostgreSQL's non-negative dynamic `LIMIT` contract before execution.
pub fn valid_limit(context: &QueryContext, parameter: &'static str, value: i64) -> QueryResult<()> {
    if value < 0 {
        Err(QueryError::invalid_limit(context.clone(), parameter, value))
    } else {
        Ok(())
    }
}

/// Preserve every decoded row for a `many` query.
pub fn many<T>(rows: Vec<T>) -> QueryResult<Vec<T>> {
    Ok(rows)
}

/// Enforce the `optional` result mode over all decoded rows, rejecting more than one.
pub fn optional<T>(context: &QueryContext, rows: Vec<T>) -> QueryResult<Option<T>> {
    let actual = rows.len();
    if actual > 1 {
        Err(QueryError::row_count(
            context.clone(),
            UnexpectedRowCount::at_most_one(actual),
        ))
    } else {
        Ok(rows.into_iter().next())
    }
}

/// Enforce the `one` result mode over all decoded rows, rejecting zero or more than one.
pub fn one<T>(context: &QueryContext, rows: Vec<T>) -> QueryResult<T> {
    match <Vec<T> as TryInto<[T; 1]>>::try_into(rows) {
        Ok([row]) => Ok(row),
        Err(rows) => Err(QueryError::row_count(
            context.clone(),
            UnexpectedRowCount::exactly_one(rows.len()),
        )),
    }
}

/// Return PostgreSQL's affected-row count without narrowing its `u64` type.
pub const fn exec(affected: u64) -> QueryResult<u64> {
    Ok(affected)
}

/// Attach query context while converting a lower-level execution or decode result.
pub trait WithQueryContext<T> {
    /// Preserve the lower-level error as the source of a contextual `QueryError`.
    fn with_query_context(self, context: QueryContext) -> QueryResult<T>;
}

impl<T> WithQueryContext<T> for Result<T, tokio_postgres::Error> {
    fn with_query_context(self, context: QueryContext) -> QueryResult<T> {
        self.map_err(|source| QueryError::database(source).with_context(context.clone()))
    }
}

impl<T> WithQueryContext<T> for Result<T, facet_tokio_postgres::Error> {
    fn with_query_context(self, context: QueryContext) -> QueryResult<T> {
        self.map_err(|source| QueryError::decode(source).with_context(context.clone()))
    }
}

impl<T> WithQueryContext<T> for QueryResult<T> {
    fn with_query_context(self, context: QueryContext) -> QueryResult<T> {
        self.map_err(|error| error.with_context(context))
    }
}

/// Emit successful query completion fields after applying a result helper.
pub trait TraceCompletion: Sized {
    /// Record duration plus a decoded row count.
    fn trace_rows(self, context: &QueryContext, started: std::time::Instant, rows: usize) -> Self;

    /// Record duration plus PostgreSQL's affected-row count.
    fn trace_affected(
        self,
        context: &QueryContext,
        started: std::time::Instant,
        affected: u64,
    ) -> Self;
}

impl<T, E> TraceCompletion for Result<T, E> {
    fn trace_rows(self, context: &QueryContext, started: std::time::Instant, rows: usize) -> Self {
        if self.is_ok() {
            trace_completion(context, started, Completion::Rows(rows));
        }
        self
    }

    fn trace_affected(
        self,
        context: &QueryContext,
        started: std::time::Instant,
        affected: u64,
    ) -> Self {
        if self.is_ok() {
            trace_completion(context, started, Completion::Affected(affected));
        }
        self
    }
}

/// Emit a structured error event and preserve the result.
pub trait TraceErr: Sized {
    /// Trace an error for legacy generated code carrying only a query name.
    fn trace_err(self, query: &'static str) -> Self;

    /// Trace an error with measured duration and the supplied query context.
    fn trace_query_err(self, context: &QueryContext, started: std::time::Instant) -> Self;
}

impl<T> TraceErr for QueryResult<T> {
    fn trace_err(self, query: &'static str) -> Self {
        let fallback = QueryContext::from_name(query);
        if let Err(error) = &self {
            let context = error.context().unwrap_or(&fallback);
            log_query_error(context, None, error);
        }
        self
    }

    fn trace_query_err(self, context: &QueryContext, started: std::time::Instant) -> Self {
        if let Err(error) = &self {
            let context = error.context().unwrap_or(context);
            log_query_error(context, Some(started), error);
        }
        self
    }
}

enum Completion {
    Rows(usize),
    Affected(u64),
}

fn elapsed_us(started: std::time::Instant) -> u64 {
    u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX)
}

fn trace_completion(context: &QueryContext, started: std::time::Instant, completion: Completion) {
    let duration_us = elapsed_us(started);
    match completion {
        Completion::Rows(rows) => tracing::debug!(
            name: "dibs_query_complete",
            query_name = %context.name,
            query_identity = %context.identity,
            duration_us,
            rows = u64::try_from(rows).unwrap_or(u64::MAX),
            "dibs query completed",
        ),
        Completion::Affected(affected) => tracing::debug!(
            name: "dibs_query_complete",
            query_name = %context.name,
            query_identity = %context.identity,
            duration_us,
            affected,
            "dibs query completed",
        ),
    }
}

fn log_query_error(
    context: &QueryContext,
    started: Option<std::time::Instant>,
    error: &QueryError,
) {
    let duration_us = started.map(elapsed_us);
    match error.kind() {
        QueryErrorKind::Database(source) => {
            if let Some(database) = source.as_db_error() {
                tracing::error!(
                    name: "dibs_query_failed",
                    query_name = %context.name,
                    query_identity = %context.identity,
                    duration_us = ?duration_us,
                    kind = "database",
                    sqlstate = database.code().code(),
                    severity = database.severity(),
                    db_message = database.message(),
                    detail = ?database.detail(),
                    hint = ?database.hint(),
                    schema = ?database.schema(),
                    table = ?database.table(),
                    column = ?database.column(),
                    constraint = ?database.constraint(),
                    routine = database.routine(),
                    "dibs query failed",
                );
            } else {
                tracing::error!(
                    name: "dibs_query_failed",
                    query_name = %context.name,
                    query_identity = %context.identity,
                    duration_us = ?duration_us,
                    kind = "database",
                    sqlstate = ?source.code().map(|code| code.code()),
                    error = %source,
                    "dibs query failed",
                );
            }
        }
        QueryErrorKind::Decode(source) => tracing::error!(
            name: "dibs_query_failed",
            query_name = %context.name,
            query_identity = %context.identity,
            duration_us = ?duration_us,
            kind = "decode",
            error = %source,
            "dibs row decoding failed",
        ),
        QueryErrorKind::UnexpectedRowCount(source) => tracing::error!(
            name: "dibs_query_failed",
            query_name = %context.name,
            query_identity = %context.identity,
            duration_us = ?duration_us,
            kind = "unexpected_row_count",
            actual_rows = u64::try_from(source.actual()).unwrap_or(u64::MAX),
            allowed_zero = source.allowed_zero(),
            error = %source,
            "dibs query returned an unexpected number of rows",
        ),
        QueryErrorKind::InvalidLimitParameter { parameter, .. } => tracing::error!(
            name: "dibs_query_failed",
            query_name = %context.name,
            query_identity = %context.identity,
            duration_us = ?duration_us,
            kind = "invalid_limit_parameter",
            parameter,
            "dibs query received an invalid LIMIT parameter",
        ),
    }
}

/// Convenient imports for generated code.
pub mod prelude {
    pub use facet::Facet;
    pub use facet_tokio_postgres::from_row;

    pub use super::types::*;
    pub use super::{
        QueryContext, QueryError, QueryErrorKind, QueryResult, RowCountExpectation,
        TraceCompletion, TraceErr, UnexpectedRowCount, WithQueryContext, exec, many, one, optional,
        valid_limit,
    };
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::*;

    fn context() -> QueryContext {
        QueryContext::new("load_widget", "query:7f83")
    }

    fn started() -> std::time::Instant {
        std::time::Instant::now()
    }

    #[test]
    fn query_context_accepts_owned_minimal_identity() {
        let context = QueryContext::new(String::from("load_widget"), String::from("query:7f83"));

        assert_eq!(context.name, "load_widget");
        assert_eq!(context.identity, "query:7f83");
    }

    #[test]
    fn many_retains_zero_one_and_multiple_rows() {
        assert_eq!(many::<i32>(Vec::new()).unwrap(), Vec::<i32>::new());
        assert_eq!(many(vec![1]).unwrap(), vec![1]);
        assert_eq!(many(vec![1, 2, 3]).unwrap(), vec![1, 2, 3]);
    }

    #[test]
    fn optional_accepts_zero_or_one_row() {
        let context = context();
        assert_eq!(optional::<i32>(&context, Vec::new()).unwrap(), None);
        assert_eq!(optional(&context, vec![7]).unwrap(), Some(7));
    }

    #[test]
    fn optional_rejects_multiple_rows_with_query_context() {
        let context = context();
        let error = optional(&context, vec![7, 8]).unwrap_err();

        assert_eq!(error.context(), Some(&context));
        assert_eq!(
            error.unexpected_row_count(),
            Some(&UnexpectedRowCount::at_most_one(2))
        );
        assert_eq!(
            error.source().unwrap().to_string(),
            "expected at most one row, got 2"
        );
    }

    #[test]
    fn dynamic_limit_accepts_zero_and_positive_values() {
        let context = context();
        assert!(valid_limit(&context, "row_limit", 0).is_ok());
        assert!(valid_limit(&context, "row_limit", i64::MAX).is_ok());
    }

    #[test]
    fn dynamic_limit_rejects_negative_value_with_context() {
        let context = context();
        let error = valid_limit(&context, "row_limit", -1).unwrap_err();
        assert_eq!(error.context(), Some(&context));
        assert!(matches!(
            error.kind(),
            QueryErrorKind::InvalidLimitParameter {
                parameter: "row_limit",
                value: -1,
            }
        ));
    }

    #[test]
    fn one_accepts_exactly_one_row() {
        let context = context();
        assert_eq!(one(&context, vec![7]).unwrap(), 7);
    }

    #[test]
    fn one_rejects_zero_rows_with_query_context() {
        let context = context();
        let error = one::<i32>(&context, Vec::new()).unwrap_err();

        assert_eq!(error.context(), Some(&context));
        assert_eq!(
            error.unexpected_row_count(),
            Some(&UnexpectedRowCount::exactly_one(0))
        );
    }

    #[test]
    fn one_rejects_multiple_rows_without_truncating() {
        let context = context();
        let error = one(&context, vec![7, 8, 9]).unwrap_err();

        assert_eq!(
            error.unexpected_row_count(),
            Some(&UnexpectedRowCount::exactly_one(3))
        );
    }

    #[test]
    fn exec_preserves_u64_affected_count() {
        let affected = u64::MAX - 1;

        assert_eq!(exec(affected).unwrap(), affected);
    }

    #[test]
    fn decode_errors_keep_context_and_source_chain() {
        let context = context();
        let decode = facet_tokio_postgres::Error::MissingColumn {
            column: "widget_id".to_owned(),
        };
        let error = QueryError::decode(decode).with_context(context.clone());

        assert_eq!(error.context(), Some(&context));
        assert!(matches!(error.kind(), QueryErrorKind::Decode(_)));
        assert!(error.decode_source().is_some());
        assert!(error.database_source().is_none());
        assert!(error.sqlstate().is_none());
        assert_eq!(
            error.source().unwrap().to_string(),
            "missing column: widget_id"
        );
    }

    #[test]
    fn database_errors_keep_context_and_postgres_source() {
        let context = context();
        let config_error = "invalid-port"
            .parse::<tokio_postgres::Config>()
            .unwrap_err();
        let postgres_message = config_error.to_string();
        let error = QueryError::database(config_error).with_context(context.clone());

        assert_eq!(error.context(), Some(&context));
        assert!(matches!(error.kind(), QueryErrorKind::Database(_)));
        assert!(error.database_source().is_some());
        assert!(error.decode_source().is_none());
        assert_eq!(error.sqlstate(), error.database_source().unwrap().code());
        assert_eq!(error.source().unwrap().to_string(), postgres_message);
    }

    #[test]
    fn contextual_conversion_preserves_decode_error() {
        let context = context();
        let decode = facet_tokio_postgres::Error::MissingColumn {
            column: "widget_id".to_owned(),
        };
        let error = Result::<(), _>::Err(decode)
            .with_query_context(context.clone())
            .unwrap_err();

        assert_eq!(error.context(), Some(&context));
        assert!(matches!(error.kind(), QueryErrorKind::Decode(_)));
        assert_eq!(
            error.source().unwrap().to_string(),
            "missing column: widget_id"
        );
    }

    #[test]
    fn contextual_conversion_preserves_postgres_error() {
        let context = context();
        let config_error = "invalid-port"
            .parse::<tokio_postgres::Config>()
            .unwrap_err();
        let postgres_message = config_error.to_string();
        let error = Result::<(), _>::Err(config_error)
            .with_query_context(context.clone())
            .unwrap_err();

        assert_eq!(error.context(), Some(&context));
        assert!(matches!(error.kind(), QueryErrorKind::Database(_)));
        assert_eq!(error.source().unwrap().to_string(), postgres_message);
    }

    #[test]
    fn completion_tracing_has_identity_duration_and_rows_without_bind_values() {
        let context = context();
        let events = capture_events(|| {
            let rows = vec![1, 2, 3];
            let row_count = rows.len();
            let _ = many(rows).trace_rows(&context, started(), row_count);
        });

        let event = completion_event(&events);
        assert_eq!(
            event.fields.get("query_name").map(String::as_str),
            Some("load_widget")
        );
        assert_eq!(
            event.fields.get("query_identity").map(String::as_str),
            Some("query:7f83")
        );
        assert_eq!(event.fields.get("rows").map(String::as_str), Some("3"));
        assert!(event.fields.contains_key("duration_us"));
        assert!(!event.fields.keys().any(|field| field.contains("bind")));
    }

    #[test]
    fn exec_tracing_records_affected_instead_of_rows() {
        let context = context();
        let events = capture_events(|| {
            let _ = exec(41).trace_affected(&context, started(), 41);
        });

        let event = completion_event(&events);
        assert_eq!(event.fields.get("affected").map(String::as_str), Some("41"));
        assert!(!event.fields.contains_key("rows"));
    }

    #[test]
    fn row_count_error_tracing_has_identity_and_actual_rows() {
        let context = context();
        let events = capture_events(|| {
            let result = optional(&context, vec![1, 2]);
            let _ = result.trace_query_err(&context, started());
        });

        let event = events
            .iter()
            .find(|event| event.name == "dibs_query_failed")
            .expect("failure event");
        assert_eq!(
            event.fields.get("query_name").map(String::as_str),
            Some("load_widget")
        );
        assert_eq!(
            event.fields.get("query_identity").map(String::as_str),
            Some("query:7f83")
        );
        assert_eq!(
            event.fields.get("kind").map(String::as_str),
            Some("unexpected_row_count")
        );
        assert_eq!(
            event.fields.get("actual_rows").map(String::as_str),
            Some("2")
        );
        assert!(event.fields.contains_key("duration_us"));
        assert!(!event.fields.keys().any(|field| field.contains("bind")));
    }

    #[derive(Clone, Debug)]
    struct RecordedEvent {
        name: &'static str,
        fields: std::collections::BTreeMap<&'static str, String>,
    }

    #[derive(Clone)]
    struct EventCollector {
        events: std::sync::Arc<std::sync::Mutex<Vec<RecordedEvent>>>,
    }

    impl tracing::Subscriber for EventCollector {
        fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
            true
        }

        fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
            tracing::span::Id::from_u64(1)
        }

        fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}

        fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}

        fn event(&self, event: &tracing::Event<'_>) {
            let mut fields = FieldVisitor::default();
            event.record(&mut fields);
            if let Ok(mut events) = self.events.lock() {
                events.push(RecordedEvent {
                    name: event.metadata().name(),
                    fields: fields.values,
                });
            }
        }

        fn enter(&self, _: &tracing::span::Id) {}

        fn exit(&self, _: &tracing::span::Id) {}
    }

    #[derive(Default)]
    struct FieldVisitor {
        values: std::collections::BTreeMap<&'static str, String>,
    }

    impl tracing::field::Visit for FieldVisitor {
        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            self.values.insert(field.name(), format!("{value:?}"));
        }

        fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
            self.values.insert(field.name(), value.to_owned());
        }

        fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
            self.values.insert(field.name(), value.to_string());
        }

        fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
            self.values.insert(field.name(), value.to_string());
        }
    }

    fn capture_events(f: impl FnOnce()) -> Vec<RecordedEvent> {
        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let subscriber = EventCollector {
            events: events.clone(),
        };
        let dispatch = tracing::Dispatch::new(subscriber);
        tracing::dispatcher::with_default(&dispatch, f);

        events
            .lock()
            .map(|events| events.clone())
            .unwrap_or_default()
    }

    fn completion_event(events: &[RecordedEvent]) -> &RecordedEvent {
        events
            .iter()
            .find(|event| event.name == "dibs_query_complete")
            .expect("completion event")
    }
}
