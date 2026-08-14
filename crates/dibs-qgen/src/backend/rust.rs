//! Deterministic Rust source generation from completed compiled-query artifacts.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use dibs_query_ir::{
    ApiOperationName, ApiResultTypeName, CompiledQuery, CompiledQueryError, FieldId, OrderedBind,
    OutputField, Parameter, ParameterApiContract, ParameterBindAdapter, ParameterId,
    ParameterPassing, ResultMode, RuntimeAssertion, TargetLanguage,
};
/// Complete Rust source emitted for one compiled query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedRust {
    /// Deterministic generated Rust source.
    pub source: String,
}

/// A completed artifact lacks a fact required for sound Rust source generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RustGenerationError {
    /// The immutable artifact fails its own cross-surface invariants.
    InvalidCompiledQuery(CompiledQueryError),
    /// The artifact does not contain executable static SQL.
    MissingDeterministicSql,
    /// No Rust operation name is present in the artifact.
    MissingRustOperationName,
    /// More than one Rust operation name is present in the artifact.
    AmbiguousRustOperationName,
    /// The target-owned Rust operation name is not snake_case.
    InvalidRustOperationName {
        /// Invalid operation name from the artifact.
        name: String,
    },
    /// No Rust result type name is present in the artifact.
    MissingRustResultTypeName,
    /// More than one Rust result type name is present in the artifact.
    AmbiguousRustResultTypeName,
    /// No Rust API contract is present for a declared parameter.
    MissingRustParameterContract {
        /// Parameter whose target contract is absent.
        parameter_id: ParameterId,
    },
    /// More than one Rust API contract is present for a declared parameter.
    AmbiguousRustParameterContract {
        /// Parameter whose target contract is ambiguous.
        parameter_id: ParameterId,
    },
    /// A nullable parameter cannot use an owned API boundary without moving the bind value.
    NullableOwnedParameter {
        /// Parameter whose contract cannot be rendered soundly.
        parameter_id: ParameterId,
    },
    /// The artifact requests a bind adapter with no executable lowering in the current runtime.
    UnsupportedParameterBindAdapter {
        /// Parameter whose adapter cannot be lowered.
        parameter_id: ParameterId,
        /// Completed adapter contract from the artifact.
        adapter: ParameterBindAdapter,
    },
    /// A dynamic LIMIT assertion targets a non-scalar Rust parameter contract.
    UnsupportedLimitParameterAssertion {
        /// Parameter whose API passing mode cannot yield an `i64` value.
        parameter_id: ParameterId,
    },
    /// No Rust API type is present for an output field.
    MissingRustOutputType {
        /// Output field whose target mapping is absent.
        field_id: FieldId,
    },
    /// More than one Rust API type is present for an output field.
    AmbiguousRustOutputType {
        /// Output field whose target mapping is ambiguous.
        field_id: FieldId,
    },
    /// No Rust member name is present for an output field.
    MissingRustOutputName {
        /// Output field whose target name is absent.
        field_id: FieldId,
    },
    /// More than one Rust member name is present for an output field.
    AmbiguousRustOutputName {
        /// Output field whose target name is ambiguous.
        field_id: FieldId,
    },
    /// An output target name is not a valid Rust identifier.
    InvalidOutputName {
        /// Output field whose target name is invalid.
        field_id: FieldId,
        /// Invalid target-language name.
        name: String,
    },
    /// Two output fields map to the same Rust member name.
    DuplicateOutputName {
        /// Duplicate target-language name.
        name: String,
    },
    /// A bind references no ordered parameter contract.
    UnknownBindParameter {
        /// Missing parameter identity.
        parameter_id: ParameterId,
    },
    /// The artifact is internally inconsistent for the declared result mode.
    ResultModeShapeMismatch {
        /// Declared result mode.
        mode: ResultMode,
    },
    /// Runtime assertions do not match the selected generated runtime helper.
    RuntimeAssertionMismatch {
        /// Declared result mode.
        mode: ResultMode,
    },
}

impl std::fmt::Display for RustGenerationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "cannot generate compiled Rust: {self:?}")
    }
}

impl std::error::Error for RustGenerationError {}

/// Generates one async Rust query function and its flat Facet result struct.
///
/// This backend consumes only immutable execution and public contracts already
/// stored on `CompiledQuery`. It does not inspect schemas, parse identifiers,
/// render SQL, synthesize target names, or reconstruct codec policy.
pub fn generate_compiled_rust(query: &CompiledQuery) -> Result<GeneratedRust, RustGenerationError> {
    query
        .validate()
        .map_err(RustGenerationError::InvalidCompiledQuery)?;
    validate_static_sql(query)?;

    let operation_name = rust_operation_name(&query.manifest.operation_names)?;
    let parameters = query
        .ordered_parameters
        .iter()
        .map(RustParameter::from_contract)
        .collect::<Result<Vec<_>, _>>()?;
    let outputs = query
        .ordered_output_fields
        .iter()
        .map(RustOutput::from_contract)
        .collect::<Result<Vec<_>, _>>()?;
    validate_unique_output_names(&outputs)?;
    validate_result_contract(query, &outputs, &parameters)?;
    let bind_arguments = bind_arguments(&query.ordered_bind_map, &parameters)?;
    let result_name = match query.declared_result_mode {
        ResultMode::Exec => None,
        ResultMode::Many | ResultMode::Optional | ResultMode::One => {
            Some(rust_result_type_name(&query.manifest.result_type_names)?)
        }
    };

    let mut source = String::new();
    writeln!(source, "// Generated by dibs-qgen. Do not edit.").unwrap();
    writeln!(source, "use dibs_runtime::prelude::*;").unwrap();
    writeln!(source, "use dibs_runtime::tokio_postgres;").unwrap();
    writeln!(source).unwrap();

    if let Some(result_name) = result_name {
        render_result_struct(&mut source, result_name, &outputs);
        writeln!(source).unwrap();
    }

    render_query_function(
        &mut source,
        query,
        operation_name,
        result_name,
        &parameters,
        &bind_arguments,
    );

    Ok(GeneratedRust { source })
}

#[derive(Debug)]
struct RustParameter<'a> {
    id: ParameterId,
    name: &'a str,
    argument_type: String,
    bind_expression: String,
    validation_expression: Option<String>,
}

impl<'a> RustParameter<'a> {
    fn from_contract(parameter: &'a Parameter) -> Result<Self, RustGenerationError> {
        let contract =
            one_rust_parameter_contract(&parameter.api_contracts).map_err(|kind| match kind {
                TargetFactError::Missing => RustGenerationError::MissingRustParameterContract {
                    parameter_id: parameter.id,
                },
                TargetFactError::Ambiguous => RustGenerationError::AmbiguousRustParameterContract {
                    parameter_id: parameter.id,
                },
            })?;
        let argument_type = parameter_argument_type(parameter, contract)?;
        let bind_expression = parameter_bind(parameter, contract)?;
        let validation_expression = match contract.passing {
            ParameterPassing::Owned => Some(contract.name.clone()),
            ParameterPassing::SharedReference => Some(format!("*{}", contract.name)),
            ParameterPassing::StringSlice | ParameterPassing::ByteSlice => None,
        };
        Ok(Self {
            id: parameter.id,
            name: &contract.name,
            argument_type,
            bind_expression,
            validation_expression,
        })
    }
}

#[derive(Debug)]
struct RustOutput<'a> {
    sql_label: &'a str,
    name: &'a str,
    field_type: String,
}

impl<'a> RustOutput<'a> {
    fn from_contract(output: &'a OutputField) -> Result<Self, RustGenerationError> {
        let rust_type = one_rust_output_type(output).map_err(|kind| match kind {
            TargetFactError::Missing => RustGenerationError::MissingRustOutputType {
                field_id: output.id,
            },
            TargetFactError::Ambiguous => RustGenerationError::AmbiguousRustOutputType {
                field_id: output.id,
            },
        })?;
        let name = one_rust_output_name(output).map_err(|kind| match kind {
            TargetFactError::Missing => RustGenerationError::MissingRustOutputName {
                field_id: output.id,
            },
            TargetFactError::Ambiguous => RustGenerationError::AmbiguousRustOutputName {
                field_id: output.id,
            },
        })?;
        if !is_rust_identifier(name) {
            return Err(RustGenerationError::InvalidOutputName {
                field_id: output.id,
                name: name.to_string(),
            });
        }
        let field_type = if output.nullability.is_nullable() {
            format!("Option<{rust_type}>")
        } else {
            rust_type.to_string()
        };
        Ok(Self {
            sql_label: &output.sql_label,
            name,
            field_type,
        })
    }
}

#[derive(Debug, Clone, Copy)]
enum TargetFactError {
    Missing,
    Ambiguous,
}

fn rust_operation_name(names: &[ApiOperationName]) -> Result<&str, RustGenerationError> {
    let name = one_target_value(
        names
            .iter()
            .filter(|name| name.language == TargetLanguage::Rust)
            .map(|name| name.name.as_str()),
    )
    .map_err(|kind| match kind {
        TargetFactError::Missing => RustGenerationError::MissingRustOperationName,
        TargetFactError::Ambiguous => RustGenerationError::AmbiguousRustOperationName,
    })?;
    if !is_snake_case_identifier(name) {
        return Err(RustGenerationError::InvalidRustOperationName {
            name: name.to_string(),
        });
    }
    Ok(name)
}

fn rust_result_type_name(names: &[ApiResultTypeName]) -> Result<&str, RustGenerationError> {
    one_target_value(
        names
            .iter()
            .filter(|name| name.language == TargetLanguage::Rust)
            .map(|name| name.name.as_str()),
    )
    .map_err(|kind| match kind {
        TargetFactError::Missing => RustGenerationError::MissingRustResultTypeName,
        TargetFactError::Ambiguous => RustGenerationError::AmbiguousRustResultTypeName,
    })
}
fn one_rust_parameter_contract(
    contracts: &[ParameterApiContract],
) -> Result<&ParameterApiContract, TargetFactError> {
    one_target(
        contracts
            .iter()
            .filter(|contract| contract.language == TargetLanguage::Rust),
    )
}

fn one_rust_output_type(output: &OutputField) -> Result<&str, TargetFactError> {
    one_target_value(
        output
            .api_types
            .iter()
            .filter(|mapping| mapping.language == TargetLanguage::Rust)
            .map(|mapping| mapping.type_id.as_str()),
    )
}

fn one_rust_output_name(output: &OutputField) -> Result<&str, TargetFactError> {
    one_target_value(
        output
            .api_names
            .iter()
            .filter(|mapping| mapping.language == TargetLanguage::Rust)
            .map(|mapping| mapping.name.as_str()),
    )
}

fn one_target<T>(mut values: impl Iterator<Item = T>) -> Result<T, TargetFactError> {
    let Some(value) = values.next() else {
        return Err(TargetFactError::Missing);
    };
    if values.next().is_some() {
        return Err(TargetFactError::Ambiguous);
    }
    Ok(value)
}

fn one_target_value<'a>(values: impl Iterator<Item = &'a str>) -> Result<&'a str, TargetFactError> {
    one_target(values)
}

fn parameter_argument_type(
    parameter: &Parameter,
    contract: &ParameterApiContract,
) -> Result<String, RustGenerationError> {
    let api_type = contract.api_type.as_str();
    if parameter.nullable {
        if contract.passing == ParameterPassing::Owned {
            return Err(RustGenerationError::NullableOwnedParameter {
                parameter_id: parameter.id,
            });
        }
        return Ok(format!("&Option<{api_type}>"));
    }
    Ok(match contract.passing {
        ParameterPassing::Owned => api_type.to_string(),
        ParameterPassing::SharedReference => format!("&{api_type}"),
        ParameterPassing::StringSlice => "&str".to_string(),
        ParameterPassing::ByteSlice => "&[u8]".to_string(),
    })
}

fn parameter_bind(
    parameter: &Parameter,
    contract: &ParameterApiContract,
) -> Result<String, RustGenerationError> {
    let name = &contract.name;
    if parameter.nullable {
        return match contract.bind_adapter {
            ParameterBindAdapter::Direct => Ok(name.clone()),
            _ => Err(RustGenerationError::UnsupportedParameterBindAdapter {
                parameter_id: parameter.id,
                adapter: contract.bind_adapter.clone(),
            }),
        };
    }
    match &contract.bind_adapter {
        ParameterBindAdapter::Direct => Ok(name.clone()),
        ParameterBindAdapter::Deref => Ok(format!("&**{name}")),
        ParameterBindAdapter::FacetJsonb
        | ParameterBindAdapter::PgArray
        | ParameterBindAdapter::Named(_) => {
            Err(RustGenerationError::UnsupportedParameterBindAdapter {
                parameter_id: parameter.id,
                adapter: contract.bind_adapter.clone(),
            })
        }
    }
}

fn validate_unique_output_names(outputs: &[RustOutput<'_>]) -> Result<(), RustGenerationError> {
    let mut names = BTreeSet::new();
    for output in outputs {
        if !names.insert(output.name) {
            return Err(RustGenerationError::DuplicateOutputName {
                name: output.name.to_string(),
            });
        }
    }
    Ok(())
}

fn validate_result_contract(
    query: &CompiledQuery,
    outputs: &[RustOutput<'_>],
    parameters: &[RustParameter<'_>],
) -> Result<(), RustGenerationError> {
    let shape_valid = match query.declared_result_mode {
        ResultMode::Many | ResultMode::Optional | ResultMode::One => !outputs.is_empty(),
        ResultMode::Exec => outputs.is_empty(),
    };
    if !shape_valid {
        return Err(RustGenerationError::ResultModeShapeMismatch {
            mode: query.declared_result_mode,
        });
    }

    for assertion in &query.runtime_assertions {
        let valid = match assertion {
            RuntimeAssertion::AtMostRows { maximum } => match query.declared_result_mode {
                ResultMode::Many => false,
                ResultMode::Optional | ResultMode::One => *maximum == 1,
                ResultMode::Exec => false,
            },
            RuntimeAssertion::AtLeastRows { minimum } => match query.declared_result_mode {
                ResultMode::Many | ResultMode::Optional => false,
                ResultMode::One => *minimum == 1,
                ResultMode::Exec => false,
            },
            RuntimeAssertion::Rowless => query.declared_result_mode == ResultMode::Exec,
            RuntimeAssertion::ValidLimitParameter { parameter_id } => {
                let Some(parameter) = parameters
                    .iter()
                    .find(|parameter| parameter.id == *parameter_id)
                else {
                    return Err(RustGenerationError::RuntimeAssertionMismatch {
                        mode: query.declared_result_mode,
                    });
                };
                if parameter.validation_expression.is_none() {
                    return Err(RustGenerationError::UnsupportedLimitParameterAssertion {
                        parameter_id: *parameter_id,
                    });
                }
                true
            }
        };
        if !valid {
            return Err(RustGenerationError::RuntimeAssertionMismatch {
                mode: query.declared_result_mode,
            });
        }
    }
    Ok(())
}

fn validate_static_sql(query: &CompiledQuery) -> Result<(), RustGenerationError> {
    (!query.deterministic_sql.is_empty())
        .then_some(())
        .ok_or(RustGenerationError::MissingDeterministicSql)
}

fn bind_arguments<'a>(
    binds: &[OrderedBind],
    parameters: &'a [RustParameter<'a>],
) -> Result<Vec<&'a str>, RustGenerationError> {
    let by_id: BTreeMap<_, _> = parameters
        .iter()
        .map(|parameter| (parameter.id, parameter.bind_expression.as_str()))
        .collect();
    binds
        .iter()
        .map(|bind| {
            by_id.get(&bind.parameter_id).copied().ok_or(
                RustGenerationError::UnknownBindParameter {
                    parameter_id: bind.parameter_id,
                },
            )
        })
        .collect()
}

fn render_result_struct(source: &mut String, result_name: &str, outputs: &[RustOutput<'_>]) {
    writeln!(source, "#[derive(Debug, Clone, Facet)]").unwrap();
    writeln!(source, "#[facet(crate = dibs_runtime::facet)]").unwrap();
    writeln!(source, "pub struct {result_name} {{").unwrap();
    for output in outputs {
        if output.sql_label != output.name {
            writeln!(
                source,
                "    #[facet(rename = {})]",
                rust_string_literal(output.sql_label)
            )
            .unwrap();
        }
        writeln!(source, "    pub {}: {},", output.name, output.field_type).unwrap();
    }
    writeln!(source, "}}").unwrap();
}
fn render_query_function(
    source: &mut String,
    query: &CompiledQuery,
    operation_name: &str,
    result_name: Option<&str>,
    parameters: &[RustParameter<'_>],
    bind_arguments: &[&str],
) {
    writeln!(source, "#[allow(clippy::too_many_arguments)]").unwrap();
    write!(source, "pub async fn {operation_name}<C>(client: &C").unwrap();
    for parameter in parameters {
        write!(source, ", {}: {}", parameter.name, parameter.argument_type).unwrap();
    }
    let return_type = return_type(query.declared_result_mode, result_name);
    writeln!(source, ") -> {return_type}").unwrap();
    writeln!(source, "where").unwrap();
    writeln!(source, "    C: tokio_postgres::GenericClient,").unwrap();
    writeln!(source, "{{").unwrap();
    writeln!(
        source,
        "    const CONTEXT: QueryContext = QueryContext::from_static({}, {});",
        rust_string_literal(&query.query_name),
        rust_string_literal(query.execution_semantics_id.as_str())
    )
    .unwrap();
    writeln!(
        source,
        "    const SQL: &str = {};",
        rust_string_literal(&query.deterministic_sql)
    )
    .unwrap();
    writeln!(source, "    let started = std::time::Instant::now();").unwrap();
    writeln!(source, "    let result = async {{").unwrap();
    for assertion in &query.runtime_assertions {
        if let RuntimeAssertion::ValidLimitParameter { parameter_id } = assertion {
            let parameter = parameters
                .iter()
                .find(|parameter| parameter.id == *parameter_id)
                .expect("limit assertion parameter was validated");
            let validation = parameter
                .validation_expression
                .as_deref()
                .expect("limit assertion validation expression was validated");
            writeln!(
                source,
                "        valid_limit(&CONTEXT, {}, {validation})?;",
                rust_string_literal(parameter.name),
            )
            .unwrap();
        }
    }

    match query.declared_result_mode {
        ResultMode::Many | ResultMode::Optional | ResultMode::One => {
            render_row_execution(
                source,
                result_name.expect("row result name was validated"),
                query.declared_result_mode,
                bind_arguments,
            );
        }
        ResultMode::Exec => render_exec_execution(source, bind_arguments),
    }

    writeln!(source, "    }}").unwrap();
    writeln!(source, "    .await;").unwrap();
    writeln!(source, "    result.trace_query_err(&CONTEXT, started)").unwrap();
    writeln!(source, "}}").unwrap();
}

fn render_row_execution(
    source: &mut String,
    result_name: &str,
    mode: ResultMode,
    bind_arguments: &[&str],
) {
    writeln!(source, "        let postgres_rows = client").unwrap();
    writeln!(
        source,
        "            .query(SQL, {})",
        bind_slice(bind_arguments)
    )
    .unwrap();
    writeln!(
        source,
        "            .await.with_query_context(CONTEXT.clone())?;"
    )
    .unwrap();
    writeln!(
        source,
        "        let mut rows = Vec::<{result_name}>::with_capacity(postgres_rows.len());"
    )
    .unwrap();
    writeln!(source, "        for row in postgres_rows {{").unwrap();
    writeln!(
        source,
        "            rows.push(from_row(&row).with_query_context(CONTEXT.clone())?);"
    )
    .unwrap();
    writeln!(source, "        }}").unwrap();
    writeln!(source, "        let row_count = rows.len();").unwrap();
    let helper = match mode {
        ResultMode::Many => "many(rows)",
        ResultMode::Optional => "optional(&CONTEXT, rows)",
        ResultMode::One => "one(&CONTEXT, rows)",
        ResultMode::Exec => unreachable!("exec is rendered separately"),
    };
    writeln!(
        source,
        "        {helper}.trace_rows(&CONTEXT, started, row_count)"
    )
    .unwrap();
}

fn render_exec_execution(source: &mut String, bind_arguments: &[&str]) {
    writeln!(source, "        let affected = client").unwrap();
    writeln!(
        source,
        "            .execute(SQL, {})",
        bind_slice(bind_arguments)
    )
    .unwrap();
    writeln!(
        source,
        "            .await.with_query_context(CONTEXT.clone())?;"
    )
    .unwrap();
    writeln!(
        source,
        "        exec(affected).trace_affected(&CONTEXT, started, affected)"
    )
    .unwrap();
}

fn return_type(mode: ResultMode, result_name: Option<&str>) -> String {
    match mode {
        ResultMode::Many => format!(
            "QueryResult<Vec<{}>>",
            result_name.expect("row result name")
        ),
        ResultMode::Optional => {
            format!(
                "QueryResult<Option<{}>>",
                result_name.expect("row result name")
            )
        }
        ResultMode::One => format!("QueryResult<{}>", result_name.expect("row result name")),
        ResultMode::Exec => "QueryResult<u64>".to_string(),
    }
}

fn bind_slice(bind_arguments: &[&str]) -> String {
    if bind_arguments.is_empty() {
        return "&[]".to_string();
    }
    let mut rendered = String::from("&[");
    for (index, expression) in bind_arguments.iter().enumerate() {
        if index > 0 {
            rendered.push_str(", ");
        }
        write!(rendered, "&{expression}").unwrap();
    }
    rendered.push(']');
    rendered
}

fn is_snake_case_identifier(identifier: &str) -> bool {
    is_rust_identifier(identifier)
        && identifier
            .bytes()
            .all(|byte| byte == b'_' || byte.is_ascii_lowercase() || byte.is_ascii_digit())
        && !identifier.contains("__")
        && !identifier.ends_with('_')
}

fn is_rust_identifier(identifier: &str) -> bool {
    let mut characters = identifier.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
        && !RUST_KEYWORDS.contains(&identifier)
}

const RUST_KEYWORDS: &[&str] = &[
    "Self", "abstract", "as", "async", "await", "become", "box", "break", "const", "continue",
    "crate", "do", "dyn", "else", "enum", "extern", "false", "final", "fn", "for", "gen", "if",
    "impl", "in", "let", "loop", "macro", "match", "mod", "move", "mut", "override", "priv", "pub",
    "ref", "return", "self", "static", "struct", "super", "trait", "true", "try", "type", "typeof",
    "union", "unsafe", "unsized", "use", "virtual", "where", "while", "yield",
];

fn rust_string_literal(value: &str) -> String {
    format!("{value:?}")
}
