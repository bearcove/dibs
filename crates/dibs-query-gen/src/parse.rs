//! Parse styx tree into query AST.

use crate::ast::*;
use styx_tree::{Document, Payload, Value};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("styx parse error: {0}")]
    Styx(#[from] styx_tree::BuildError),

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
    let doc = Document::parse(source)?;
    let mut queries = Vec::new();

    for entry in &doc.root.entries {
        // Each entry should be QueryName @query{...}
        let name = entry
            .key
            .as_str()
            .ok_or(ParseError::ExpectedScalar)?
            .to_string();

        if entry.value.tag_name() != Some("query") {
            return Err(ParseError::ExpectedQueryTag { name });
        }

        let query = parse_query(&name, &entry.value)?;
        queries.push(query);
    }

    Ok(QueryFile { queries })
}

fn parse_query(name: &str, value: &Value) -> Result<Query, ParseError> {
    let obj = match &value.payload {
        Some(Payload::Object(obj)) => obj,
        _ => return Err(ParseError::ExpectedObjectPayload),
    };

    // Parse params
    let params = if let Some(params_val) = obj.get("params") {
        parse_params(params_val)?
    } else {
        Vec::new()
    };

    // Check for raw SQL mode
    if let Some(sql_val) = obj.get("sql") {
        let raw_sql = sql_val
            .scalar_text()
            .ok_or(ParseError::ExpectedScalar)?
            .to_string();

        let returns = if let Some(returns_val) = obj.get("returns") {
            parse_returns(returns_val)?
        } else {
            Vec::new()
        };

        return Ok(Query {
            name: name.to_string(),
            span: value.span,
            params,
            from: String::new(),
            filters: Vec::new(),
            order_by: Vec::new(),
            limit: None,
            offset: None,
            first: false,
            select: Vec::new(),
            raw_sql: Some(raw_sql),
            returns,
        });
    }

    // Parse from
    let from = obj
        .get("from")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ParseError::MissingFrom {
            name: name.to_string(),
        })?
        .to_string();

    // Parse where
    let filters = if let Some(where_val) = obj.get("where") {
        parse_filters(where_val)?
    } else {
        Vec::new()
    };

    // Parse order_by
    let order_by = if let Some(order_val) = obj.get("order_by") {
        parse_order_by(order_val)?
    } else {
        Vec::new()
    };

    // Parse limit
    let limit = obj.get("limit").map(parse_expr).transpose()?;

    // Parse offset
    let offset = obj.get("offset").map(parse_expr).transpose()?;

    // Parse first
    let first = obj
        .get("first")
        .and_then(|v| v.as_str())
        .map(|s| s == "true")
        .unwrap_or(false);

    // Parse select
    let select = obj
        .get("select")
        .map(parse_select)
        .transpose()?
        .ok_or_else(|| ParseError::MissingSelect {
            name: name.to_string(),
        })?;

    Ok(Query {
        name: name.to_string(),
        span: value.span,
        params,
        from,
        filters,
        order_by,
        limit,
        offset,
        first,
        select,
        raw_sql: None,
        returns: Vec::new(),
    })
}

fn parse_params(value: &Value) -> Result<Vec<Param>, ParseError> {
    let obj = match &value.payload {
        Some(Payload::Object(obj)) => obj,
        _ => return Ok(Vec::new()),
    };

    let mut params = Vec::new();
    for entry in &obj.entries {
        let name = entry
            .key
            .as_str()
            .ok_or(ParseError::ExpectedScalar)?
            .to_string();
        let ty = parse_param_type(&entry.value)?;
        params.push(Param {
            name,
            ty,
            span: entry.key.span,
        });
    }
    Ok(params)
}

fn parse_param_type(value: &Value) -> Result<ParamType, ParseError> {
    match value.tag_name() {
        Some("string") => Ok(ParamType::String),
        Some("int") => Ok(ParamType::Int),
        Some("bool") => Ok(ParamType::Bool),
        Some("uuid") => Ok(ParamType::Uuid),
        Some("decimal") => Ok(ParamType::Decimal),
        Some("timestamp") => Ok(ParamType::Timestamp),
        Some("optional") => {
            // @optional(@string) - payload is a sequence with one item
            if let Some(Payload::Sequence(seq)) = &value.payload
                && let Some(inner) = seq.items.first()
            {
                let inner_ty = parse_param_type(inner)?;
                return Ok(ParamType::Optional(Box::new(inner_ty)));
            }
            // @optional{...} - payload is object, look for inner type
            if let Some(Payload::Object(obj)) = &value.payload
                && let Some(inner) = obj.entries.first()
            {
                let inner_ty = parse_param_type(&inner.value)?;
                return Ok(ParamType::Optional(Box::new(inner_ty)));
            }
            Err(ParseError::UnknownParamType {
                tag: "optional (no inner type)".to_string(),
            })
        }
        Some(tag) => Err(ParseError::UnknownParamType {
            tag: tag.to_string(),
        }),
        None => {
            // No tag - might be a default or invalid
            Err(ParseError::UnknownParamType {
                tag: "(no tag)".to_string(),
            })
        }
    }
}

fn parse_filters(value: &Value) -> Result<Vec<Filter>, ParseError> {
    let obj = match &value.payload {
        Some(Payload::Object(obj)) => obj,
        _ => return Ok(Vec::new()),
    };

    let mut filters = Vec::new();
    for entry in &obj.entries {
        let column = entry
            .key
            .as_str()
            .ok_or(ParseError::ExpectedScalar)?
            .to_string();

        let (op, val) = parse_filter_value(&entry.value)?;
        filters.push(Filter {
            column,
            op,
            value: val,
            span: entry.key.span,
        });
    }
    Ok(filters)
}

fn parse_filter_value(value: &Value) -> Result<(FilterOp, Expr), ParseError> {
    // Check for tagged operators
    match value.tag_name() {
        Some("null") => return Ok((FilterOp::IsNull, Expr::Null)),
        Some("ilike") => {
            // @ilike($param) or @ilike("pattern")
            if let Some(Payload::Sequence(seq)) = &value.payload
                && let Some(inner) = seq.items.first()
            {
                let expr = parse_expr(inner)?;
                return Ok((FilterOp::ILike, expr));
            }
            return Ok((FilterOp::ILike, Expr::String("%".to_string())));
        }
        Some("like") => {
            if let Some(Payload::Sequence(seq)) = &value.payload
                && let Some(inner) = seq.items.first()
            {
                let expr = parse_expr(inner)?;
                return Ok((FilterOp::Like, expr));
            }
            return Ok((FilterOp::Like, Expr::String("%".to_string())));
        }
        Some("gt") => {
            if let Some(Payload::Sequence(seq)) = &value.payload
                && let Some(inner) = seq.items.first()
            {
                let expr = parse_expr(inner)?;
                return Ok((FilterOp::Gt, expr));
            }
        }
        Some("lt") => {
            if let Some(Payload::Sequence(seq)) = &value.payload
                && let Some(inner) = seq.items.first()
            {
                let expr = parse_expr(inner)?;
                return Ok((FilterOp::Lt, expr));
            }
        }
        _ => {}
    }

    // Default: equality
    let expr = parse_expr(value)?;
    Ok((FilterOp::Eq, expr))
}

fn parse_expr(value: &Value) -> Result<Expr, ParseError> {
    // Check for @null tag
    if value.tag_name() == Some("null") {
        return Ok(Expr::Null);
    }

    // Get scalar text
    let text = value.scalar_text().ok_or(ParseError::ExpectedScalar)?;

    // Check for parameter reference
    if let Some(param_name) = text.strip_prefix('$') {
        return Ok(Expr::Param(param_name.to_string()));
    }

    // Check for boolean
    if text == "true" {
        return Ok(Expr::Bool(true));
    }
    if text == "false" {
        return Ok(Expr::Bool(false));
    }

    // Check for integer
    if let Ok(n) = text.parse::<i64>() {
        return Ok(Expr::Int(n));
    }

    // Otherwise string
    Ok(Expr::String(text.to_string()))
}

fn parse_order_by(value: &Value) -> Result<Vec<OrderBy>, ParseError> {
    let obj = match &value.payload {
        Some(Payload::Object(obj)) => obj,
        _ => return Ok(Vec::new()),
    };

    let mut orders = Vec::new();
    for entry in &obj.entries {
        let column = entry
            .key
            .as_str()
            .ok_or(ParseError::ExpectedScalar)?
            .to_string();

        let direction = match entry.value.as_str() {
            Some("desc") | Some("DESC") => SortDir::Desc,
            _ => SortDir::Asc,
        };

        orders.push(OrderBy {
            column,
            direction,
            span: entry.key.span,
        });
    }
    Ok(orders)
}

fn parse_select(value: &Value) -> Result<Vec<Field>, ParseError> {
    let obj = match &value.payload {
        Some(Payload::Object(obj)) => obj,
        _ => return Ok(Vec::new()),
    };

    let mut fields = Vec::new();
    for entry in &obj.entries {
        let name = entry
            .key
            .as_str()
            .ok_or(ParseError::ExpectedScalar)?
            .to_string();

        // Check for @rel tag (relation)
        if entry.value.tag_name() == Some("rel") {
            let rel = parse_relation(&name, &entry.value)?;
            fields.push(rel);
            continue;
        }

        // Check for @count tag
        if entry.value.tag_name() == Some("count") {
            let table = if let Some(Payload::Sequence(seq)) = &entry.value.payload {
                seq.items
                    .first()
                    .and_then(|v| v.as_str())
                    .unwrap_or(&name)
                    .to_string()
            } else {
                name.clone()
            };
            fields.push(Field::Count {
                name: name.clone(),
                table,
                span: entry.key.span,
            });
            continue;
        }

        // Check if value is unit (simple column)
        if entry.value.is_unit() {
            fields.push(Field::Column {
                name,
                span: entry.key.span,
            });
            continue;
        }

        // Otherwise treat as simple column
        fields.push(Field::Column {
            name,
            span: entry.key.span,
        });
    }

    Ok(fields)
}

fn parse_relation(name: &str, value: &Value) -> Result<Field, ParseError> {
    let obj = match &value.payload {
        Some(Payload::Object(obj)) => obj,
        _ => {
            return Ok(Field::Relation {
                name: name.to_string(),
                span: value.span,
                from: None,
                filters: Vec::new(),
                order_by: Vec::new(),
                first: false,
                select: Vec::new(),
            });
        }
    };

    let from = obj.get("from").and_then(|v| v.as_str()).map(String::from);

    let filters = if let Some(where_val) = obj.get("where") {
        parse_filters(where_val)?
    } else {
        Vec::new()
    };

    let order_by = if let Some(order_val) = obj.get("order_by") {
        parse_order_by(order_val)?
    } else {
        Vec::new()
    };

    let first = obj
        .get("first")
        .and_then(|v| v.as_str())
        .map(|s| s == "true")
        .unwrap_or(false);

    let select = if let Some(select_val) = obj.get("select") {
        parse_select(select_val)?
    } else {
        Vec::new()
    };

    Ok(Field::Relation {
        name: name.to_string(),
        span: value.span,
        from,
        filters,
        order_by,
        first,
        select,
    })
}

fn parse_returns(value: &Value) -> Result<Vec<ReturnField>, ParseError> {
    let obj = match &value.payload {
        Some(Payload::Object(obj)) => obj,
        _ => return Ok(Vec::new()),
    };

    let mut fields = Vec::new();
    for entry in &obj.entries {
        let name = entry
            .key
            .as_str()
            .ok_or(ParseError::ExpectedScalar)?
            .to_string();
        let ty = parse_param_type(&entry.value)?;
        fields.push(ReturnField {
            name,
            ty,
            span: entry.key.span,
        });
    }
    Ok(fields)
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
        assert_eq!(q.params[0].name, "handle");
        assert_eq!(q.params[0].ty, ParamType::String);
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
        match &q.select[1] {
            Field::Relation {
                name,
                first,
                select,
                filters,
                ..
            } => {
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
}
