//! Lint: Type mismatches between params and columns.

use super::{DiagnosticBuilder, LintContext};
use dibs_proto::TableInfo;
use dibs_query_schema::*;

/// Check if a param type is compatible with a SQL column type.
fn types_compatible(param_type: &str, sql_type: &str) -> bool {
    match param_type {
        "string" => matches!(
            sql_type.to_uppercase().as_str(),
            "TEXT" | "VARCHAR" | "CHAR" | "CHARACTER VARYING"
        ),
        "int" => matches!(
            sql_type.to_uppercase().as_str(),
            "INT" | "INTEGER" | "BIGINT" | "SMALLINT" | "INT4" | "INT8" | "INT2"
        ),
        "bool" | "boolean" => matches!(sql_type.to_uppercase().as_str(), "BOOLEAN" | "BOOL"),
        "float" => matches!(
            sql_type.to_uppercase().as_str(),
            "FLOAT" | "DOUBLE" | "REAL" | "NUMERIC" | "DECIMAL" | "FLOAT4" | "FLOAT8"
        ),
        _ => true, // Unknown types are assumed compatible
    }
}

fn param_type_name(param_type: &ParamType) -> String {
    match param_type {
        ParamType::String => "string".to_string(),
        ParamType::Int => "int".to_string(),
        ParamType::Bool => "bool".to_string(),
        ParamType::Uuid => "uuid".to_string(),
        ParamType::Decimal => "decimal".to_string(),
        ParamType::Timestamp => "timestamp".to_string(),
        ParamType::Bytes => "bytes".to_string(),
        ParamType::Optional(inner) => {
            if let Some(first) = inner.first() {
                format!("optional({})", param_type_name(first))
            } else {
                "optional".to_string()
            }
        }
    }
}

pub fn lint_param_types_in_where(
    where_clause: &Where,
    table: &TableInfo,
    params: &Params,
    ctx: &mut LintContext<'_>,
) {
    for (col_name, filter) in &where_clause.filters {
        let Some(column) = table.columns.iter().find(|c| c.name == col_name.as_str()) else {
            continue;
        };

        // Extract param name from filter
        let param_name = match filter {
            FilterValue::Eq(s) => s.strip_prefix('$'),
            FilterValue::Ilike(args)
            | FilterValue::Like(args)
            | FilterValue::Gt(args)
            | FilterValue::Lt(args)
            | FilterValue::Gte(args)
            | FilterValue::Lte(args)
            | FilterValue::Ne(args) => args.first().and_then(|a| a.as_str().strip_prefix('$')),
            _ => None,
        };

        if let Some(param_name) = param_name {
            if let Some((param_meta, param_type)) =
                params.params.iter().find(|(k, _)| k.as_str() == param_name)
            {
                let type_name = param_type_name(param_type);
                if !types_compatible(&type_name, &column.sql_type) {
                    DiagnosticBuilder::error("param-type-mismatch")
                        .at(param_meta.span)
                        .msg(format!(
                            "type mismatch: param '{}' is @{} but column '{}' is {}",
                            param_name, type_name, column.name, column.sql_type
                        ))
                        .emit(ctx.diagnostics);
                }
            }
        }
    }
}
