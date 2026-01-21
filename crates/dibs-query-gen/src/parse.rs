//! Parse styx into query AST.
//!
//! Uses facet-styx for parsing, then converts to AST types.

use crate::ast::*;
use crate::schema;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("styx parse error: {0}")]
    Styx(String),

    #[error("expected @query tag on '{name}'")]
    ExpectedQueryTag { name: String },

    #[error("missing 'from' clause in query '{name}'")]
    MissingFrom { name: String },

    #[error("missing 'select' clause in query '{name}'")]
    MissingSelect { name: String },

    #[error("expected object payload for @query")]
    ExpectedObjectPayload,

    #[error("unknown param type: @{tag}")]
    UnknownParamType { tag: String },

    #[error("expected scalar value")]
    ExpectedScalar,
}

/// Parse a styx source string into a QueryFile.
pub fn parse_query_file(source: &str) -> Result<QueryFile, ParseError> {
    // Use facet-styx for parsing
    let schema_file: schema::QueryFile =
        facet_styx::from_str(source).map_err(|e| ParseError::Styx(e.to_string()))?;

    // Convert to AST types
    let mut queries = Vec::new();
    let mut inserts = Vec::new();
    let mut upserts = Vec::new();
    let mut updates = Vec::new();
    let mut deletes = Vec::new();

    for (name, decl) in schema_file.decls {
        match decl {
            schema::Decl::Query(q) => {
                queries.push(convert_query(&name, &q)?);
            }
            schema::Decl::Insert(i) => {
                inserts.push(convert_insert(&name, &i));
            }
            schema::Decl::Upsert(u) => {
                upserts.push(convert_upsert(&name, &u));
            }
            schema::Decl::Update(u) => {
                updates.push(convert_update(&name, &u));
            }
            schema::Decl::Delete(d) => {
                deletes.push(convert_delete(&name, &d));
            }
        }
    }

    Ok(QueryFile {
        queries,
        inserts,
        upserts,
        updates,
        deletes,
    })
}

/// Convert schema Query to AST Query.
fn convert_query(name: &str, q: &schema::Query) -> Result<Query, ParseError> {
    // Check for raw SQL mode
    if let Some(sql) = &q.sql {
        let returns = if let Some(returns) = &q.returns {
            returns
                .fields
                .iter()
                .map(|(name, ty)| ReturnField {
                    name: name.clone(),
                    ty: convert_param_type(ty),
                    span: None,
                })
                .collect()
        } else {
            Vec::new()
        };

        return Ok(Query {
            name: name.to_string(),
            span: None,
            params: convert_params(&q.params),
            from: String::new(),
            filters: Vec::new(),
            order_by: Vec::new(),
            limit: None,
            offset: None,
            first: false,
            select: Vec::new(),
            raw_sql: Some(sql.clone()),
            returns,
        });
    }

    // Structured query
    let from = q.from.clone().ok_or_else(|| ParseError::MissingFrom {
        name: name.to_string(),
    })?;

    let select_schema = q.select.as_ref().ok_or_else(|| ParseError::MissingSelect {
        name: name.to_string(),
    })?;

    Ok(Query {
        name: name.to_string(),
        span: None,
        params: convert_params(&q.params),
        from,
        filters: convert_filters(&q.where_clause),
        order_by: convert_order_by(&q.order_by),
        limit: q.limit.as_ref().map(|s| parse_expr_string(s)),
        offset: q.offset.as_ref().map(|s| parse_expr_string(s)),
        first: q.first.unwrap_or(false),
        select: convert_select(select_schema),
        raw_sql: None,
        returns: Vec::new(),
    })
}

/// Convert schema Params to AST Vec<Param>.
fn convert_params(params: &Option<schema::Params>) -> Vec<Param> {
    let Some(params) = params else {
        return Vec::new();
    };
    params
        .params
        .iter()
        .map(|(name, ty)| Param {
            name: name.clone(),
            ty: convert_param_type(ty),
            span: None,
        })
        .collect()
}

/// Convert schema ParamType to AST ParamType.
fn convert_param_type(ty: &schema::ParamType) -> ParamType {
    match ty {
        schema::ParamType::String => ParamType::String,
        schema::ParamType::Int => ParamType::Int,
        schema::ParamType::Bool => ParamType::Bool,
        schema::ParamType::Uuid => ParamType::Uuid,
        schema::ParamType::Decimal => ParamType::Decimal,
        schema::ParamType::Timestamp => ParamType::Timestamp,
        schema::ParamType::Optional(inner) => {
            // Take the first inner type
            let inner_ty = inner
                .first()
                .map(convert_param_type)
                .unwrap_or(ParamType::String);
            ParamType::Optional(Box::new(inner_ty))
        }
    }
}

/// Convert schema Where to AST Vec<Filter>.
fn convert_filters(where_clause: &Option<schema::Where>) -> Vec<Filter> {
    let Some(where_clause) = where_clause else {
        return Vec::new();
    };
    where_clause
        .filters
        .iter()
        .map(|(column, value)| {
            let (op, expr) = convert_filter_value(value);
            Filter {
                column: column.clone(),
                op,
                value: expr,
                span: None,
            }
        })
        .collect()
}

/// Convert schema FilterValue to (FilterOp, Expr).
fn convert_filter_value(value: &schema::FilterValue) -> (FilterOp, Expr) {
    match value {
        schema::FilterValue::Null => (FilterOp::IsNull, Expr::Null),
        schema::FilterValue::Eq(s) => (FilterOp::Eq, parse_expr_string(s)),
        schema::FilterValue::Ilike(args) => {
            let expr = args
                .first()
                .map(|s| parse_expr_string(s))
                .unwrap_or(Expr::String("%".to_string()));
            (FilterOp::ILike, expr)
        }
        schema::FilterValue::Like(args) => {
            let expr = args
                .first()
                .map(|s| parse_expr_string(s))
                .unwrap_or(Expr::String("%".to_string()));
            (FilterOp::Like, expr)
        }
        schema::FilterValue::Gt(args) => {
            let expr = args
                .first()
                .map(|s| parse_expr_string(s))
                .unwrap_or(Expr::Null);
            (FilterOp::Gt, expr)
        }
        schema::FilterValue::Lt(args) => {
            let expr = args
                .first()
                .map(|s| parse_expr_string(s))
                .unwrap_or(Expr::Null);
            (FilterOp::Lt, expr)
        }
    }
}

/// Parse expression string to Expr.
fn parse_expr_string(s: &str) -> Expr {
    if let Some(param) = s.strip_prefix('$') {
        return Expr::Param(param.to_string());
    }
    if s == "true" {
        return Expr::Bool(true);
    }
    if s == "false" {
        return Expr::Bool(false);
    }
    if let Ok(n) = s.parse::<i64>() {
        return Expr::Int(n);
    }
    Expr::String(s.to_string())
}

/// Convert schema OrderBy to AST Vec<OrderBy>.
fn convert_order_by(order_by: &Option<schema::OrderBy>) -> Vec<OrderBy> {
    let Some(order_by) = order_by else {
        return Vec::new();
    };
    order_by
        .columns
        .iter()
        .map(|(column, direction)| OrderBy {
            column: column.clone(),
            direction: match direction.as_deref() {
                Some("desc") | Some("DESC") => SortDir::Desc,
                _ => SortDir::Asc,
            },
            span: None,
        })
        .collect()
}

/// Convert schema Select to AST Vec<Field>.
fn convert_select(select: &schema::Select) -> Vec<Field> {
    select
        .fields
        .iter()
        .map(|(name, field_def)| match field_def {
            None => Field::Column {
                name: name.clone(),
                span: None,
            },
            Some(schema::FieldDef::Rel(rel)) => Field::Relation {
                name: name.clone(),
                span: None,
                from: rel.from.clone(),
                filters: convert_filters(&rel.where_clause),
                order_by: convert_order_by(&rel.order_by),
                first: rel.first.unwrap_or(false),
                select: rel.select.as_ref().map(convert_select).unwrap_or_default(),
            },
            Some(schema::FieldDef::Count(tables)) => Field::Count {
                name: name.clone(),
                table: tables.first().cloned().unwrap_or_default(),
                span: None,
            },
        })
        .collect()
}

/// Convert schema Insert to AST InsertMutation.
fn convert_insert(name: &str, i: &schema::Insert) -> InsertMutation {
    InsertMutation {
        name: name.to_string(),
        span: None,
        params: convert_params(&i.params),
        table: i.into.clone(),
        values: convert_values(&i.values),
        returning: convert_returning(&i.returning),
    }
}

/// Convert schema Upsert to AST UpsertMutation.
fn convert_upsert(name: &str, u: &schema::Upsert) -> UpsertMutation {
    UpsertMutation {
        name: name.to_string(),
        span: None,
        params: convert_params(&u.params),
        table: u.into.clone(),
        conflict_columns: u.conflict.columns.keys().cloned().collect(),
        values: convert_values(&u.values),
        returning: convert_returning(&u.returning),
    }
}

/// Convert schema Update to AST UpdateMutation.
fn convert_update(name: &str, u: &schema::Update) -> UpdateMutation {
    UpdateMutation {
        name: name.to_string(),
        span: None,
        params: convert_params(&u.params),
        table: u.table.clone(),
        values: convert_values(&u.set),
        filters: convert_filters(&u.where_clause),
        returning: convert_returning(&u.returning),
    }
}

/// Convert schema Delete to AST DeleteMutation.
fn convert_delete(name: &str, d: &schema::Delete) -> DeleteMutation {
    DeleteMutation {
        name: name.to_string(),
        span: None,
        params: convert_params(&d.params),
        table: d.from.clone(),
        filters: convert_filters(&d.where_clause),
        returning: convert_returning(&d.returning),
    }
}

/// Convert schema Values to AST Vec<(String, ValueExpr)>.
fn convert_values(values: &schema::Values) -> Vec<(String, ValueExpr)> {
    values
        .columns
        .iter()
        .map(|(col, expr)| (col.clone(), convert_value_expr(expr)))
        .collect()
}

/// Convert schema ValueExpr to AST ValueExpr.
fn convert_value_expr(expr: &schema::ValueExpr) -> ValueExpr {
    match expr {
        schema::ValueExpr::Now => ValueExpr::Now,
        schema::ValueExpr::Default => ValueExpr::Default,
        schema::ValueExpr::Expr(s) => {
            if let Some(param) = s.strip_prefix('$') {
                ValueExpr::Param(param.to_string())
            } else if s == "true" {
                ValueExpr::Bool(true)
            } else if s == "false" {
                ValueExpr::Bool(false)
            } else if s == "null" {
                ValueExpr::Null
            } else if let Ok(n) = s.parse::<i64>() {
                ValueExpr::Int(n)
            } else {
                ValueExpr::String(s.clone())
            }
        }
    }
}

/// Convert schema Returning to Vec<String>.
fn convert_returning(returning: &Option<schema::Returning>) -> Vec<String> {
    returning
        .as_ref()
        .map(|r| r.columns.keys().cloned().collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_query() {
        let source = r#"
AllProducts @query{
  from product
  select{ id, handle, status }
}
"#;
        let file = parse_query_file(source).unwrap();
        assert_eq!(file.queries.len(), 1);

        let q = &file.queries[0];
        assert_eq!(q.name, "AllProducts");
        assert_eq!(q.from, "product");
        assert_eq!(q.select.len(), 3);
    }

    #[test]
    fn test_parse_query_with_params() {
        let source = r#"
ProductByHandle @query{
  params{
    handle @string
    locale @string
  }
  from product
  where{ handle $handle }
  first true
  select{ id, handle }
}
"#;
        let file = parse_query_file(source).unwrap();
        let q = &file.queries[0];

        assert_eq!(q.params.len(), 2);
        assert!(q.first);
        assert_eq!(q.filters.len(), 1);
        assert_eq!(q.filters[0].column, "handle");
        assert!(matches!(q.filters[0].value, Expr::Param(ref p) if p == "handle"));
    }

    #[test]
    fn test_parse_query_with_relation() {
        let source = r#"
ProductListing @query{
  from product
  select{
    id
    translation @rel{
      where{ locale $locale }
      first true
      select{ title, description }
    }
  }
}
"#;
        let file = parse_query_file(source).unwrap();
        let q = &file.queries[0];

        assert_eq!(q.select.len(), 2);
        // Find the relation field (order not guaranteed with HashMap)
        let rel = q
            .select
            .iter()
            .find(|f| matches!(f, Field::Relation { .. }));
        match rel {
            Some(Field::Relation {
                name,
                first,
                select,
                filters,
                ..
            }) => {
                assert_eq!(name, "translation");
                assert!(*first);
                assert_eq!(select.len(), 2);
                assert_eq!(filters.len(), 1);
            }
            _ => panic!("expected relation"),
        }
    }

    #[test]
    fn test_parse_raw_sql_query() {
        let source = r#"
TrendingProducts @query{
  params{
    locale @string
    days @int
  }
  sql <<SQL
    SELECT id, title FROM products
  SQL
  returns{
    id @int
    title @string
  }
}
"#;
        let file = parse_query_file(source).unwrap();
        let q = &file.queries[0];

        assert!(q.is_raw());
        assert!(q.raw_sql.is_some());
        assert_eq!(q.returns.len(), 2);
    }

    #[test]
    fn test_parse_insert() {
        let source = r#"
CreateUser @insert{
  params{
    name @string
    email @string
  }
  into users
  values{
    name $name
    email $email
    created_at @now
  }
  returning{ id, name, email, created_at }
}
"#;
        let file = parse_query_file(source).unwrap();
        assert_eq!(file.inserts.len(), 1);

        let i = &file.inserts[0];
        assert_eq!(i.name, "CreateUser");
        assert_eq!(i.table, "users");
        assert_eq!(i.params.len(), 2);
        assert_eq!(i.values.len(), 3);
        assert_eq!(i.returning.len(), 4);

        // Check value expressions
        assert!(
            matches!(i.values.iter().find(|(c, _)| c == "name"), Some((_, ValueExpr::Param(p))) if p == "name")
        );
        assert!(matches!(
            i.values.iter().find(|(c, _)| c == "created_at"),
            Some((_, ValueExpr::Now))
        ));
    }

    #[test]
    fn test_parse_upsert() {
        let source = r#"
UpsertProduct @upsert{
  params{
    id @uuid
    name @string
    price @decimal
  }
  into products
  conflict{ id }
  values{
    id $id
    name $name
    price $price
    updated_at @now
  }
  returning{ id, name, price, updated_at }
}
"#;
        let file = parse_query_file(source).unwrap();
        assert_eq!(file.upserts.len(), 1);

        let u = &file.upserts[0];
        assert_eq!(u.name, "UpsertProduct");
        assert_eq!(u.table, "products");
        assert_eq!(u.conflict_columns, vec!["id"]);
        assert_eq!(u.values.len(), 4);
        assert_eq!(u.returning.len(), 4);
    }

    #[test]
    fn test_parse_update() {
        let source = r#"
UpdateUserEmail @update{
  params{
    id @uuid
    email @string
  }
  table users
  set{
    email $email
    updated_at @now
  }
  where{ id $id }
  returning{ id, email, updated_at }
}
"#;
        let file = parse_query_file(source).unwrap();
        assert_eq!(file.updates.len(), 1);

        let u = &file.updates[0];
        assert_eq!(u.name, "UpdateUserEmail");
        assert_eq!(u.table, "users");
        assert_eq!(u.values.len(), 2);
        assert_eq!(u.filters.len(), 1);
        assert_eq!(u.filters[0].column, "id");
        assert_eq!(u.returning.len(), 3);
    }

    #[test]
    fn test_parse_delete() {
        let source = r#"
DeleteUser @delete{
  params{
    id @uuid
  }
  from users
  where{ id $id }
  returning{ id }
}
"#;
        let file = parse_query_file(source).unwrap();
        assert_eq!(file.deletes.len(), 1);

        let d = &file.deletes[0];
        assert_eq!(d.name, "DeleteUser");
        assert_eq!(d.table, "users");
        assert_eq!(d.filters.len(), 1);
        assert_eq!(d.returning.len(), 1);
    }
}
