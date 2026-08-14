use crate::{ApiTypeId, CatalogError, PgCodecId, WireCodecId};

/// Generated API language whose type identity is being resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ApiLanguage {
    /// Rust generated API.
    Rust,
    /// TypeScript generated API.
    TypeScript,
}

/// Lossless storage, wire, and generated API identities for one logical type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodecBinding {
    /// PostgreSQL storage codec identity.
    pub pg_codec_id: PgCodecId,
    /// Client wire codec identity.
    pub wire_codec_id: WireCodecId,
    /// Rust API identity.
    pub rust_api_type: ApiTypeId,
    /// TypeScript API identity.
    pub typescript_api_type: ApiTypeId,
}

impl CodecBinding {
    pub(crate) fn new(
        pg_codec: impl Into<String>,
        wire_codec: impl Into<String>,
        rust_api: impl Into<String>,
        typescript_api: impl Into<String>,
    ) -> Self {
        Self {
            pg_codec_id: PgCodecId::new(pg_codec),
            wire_codec_id: WireCodecId::new(wire_codec),
            rust_api_type: ApiTypeId::new(rust_api),
            typescript_api_type: ApiTypeId::new(typescript_api),
        }
    }
}

pub(crate) fn builtin_codec(canonical_name: &str) -> Result<CodecBinding, CatalogError> {
    let codec = match canonical_name {
        "boolean" => CodecBinding::new(
            "pg18:pg-codec:bool",
            "wire:postgres:binary:bool",
            "bool",
            "boolean",
        ),
        "smallint" => CodecBinding::new(
            "pg18:pg-codec:int2",
            "wire:postgres:binary:int16",
            "i16",
            "number",
        ),
        "integer" => CodecBinding::new(
            "pg18:pg-codec:int4",
            "wire:postgres:binary:int32",
            "i32",
            "number",
        ),
        "bigint" => CodecBinding::new(
            "pg18:pg-codec:int8",
            "wire:postgres:binary:int64-be",
            "i64",
            "bigint",
        ),
        "real" => CodecBinding::new(
            "pg18:pg-codec:float4",
            "wire:postgres:binary:float32",
            "f32",
            "number",
        ),
        "double precision" => CodecBinding::new(
            "pg18:pg-codec:float8",
            "wire:postgres:binary:float64",
            "f64",
            "number",
        ),
        "numeric" => CodecBinding::new(
            "pg18:pg-codec:numeric",
            "wire:postgres:text:decimal",
            "Decimal",
            "string",
        ),
        "text" => CodecBinding::new(
            "pg18:pg-codec:text",
            "wire:postgres:text:utf8",
            "String",
            "string",
        ),
        "bytea" => CodecBinding::new(
            "pg18:pg-codec:bytea",
            "wire:postgres:binary:bytes",
            "Vec<u8>",
            "Uint8Array",
        ),
        "uuid" => CodecBinding::new(
            "pg18:pg-codec:uuid",
            "wire:postgres:binary:uuid",
            "Uuid",
            "string",
        ),
        "date" => CodecBinding::new(
            "pg18:pg-codec:date",
            "wire:postgres:binary:date-days",
            "Date",
            "string",
        ),
        "time without time zone" => CodecBinding::new(
            "pg18:pg-codec:time",
            "wire:postgres:binary:time-micros",
            "Time",
            "string",
        ),
        "timestamp without time zone" => CodecBinding::new(
            "pg18:pg-codec:timestamp",
            "wire:postgres:binary:timestamp-micros",
            "Timestamp",
            "string",
        ),
        "timestamp with time zone" => CodecBinding::new(
            "pg18:pg-codec:timestamptz",
            "wire:postgres:binary:timestamptz-micros-utc",
            "Timestamp",
            "string",
        ),
        "jsonb" => CodecBinding::new(
            "pg18:pg-codec:jsonb",
            "wire:postgres:binary:jsonb-v1",
            "Jsonb<facet_value::Value>",
            "unknown",
        ),
        "boolean[]" => array_codec("bool", "bool", "bool", "boolean"),
        "smallint[]" => array_codec("int2", "int16", "i16", "number"),
        "integer[]" => array_codec("int4", "int32", "i32", "number"),
        "bigint[]" => array_codec("int8", "int64-be", "i64", "bigint"),
        "numeric[]" => array_codec("numeric", "decimal", "Decimal", "string"),
        "text[]" => array_codec("text", "utf8", "String", "string"),
        "bytea[]" => array_codec("bytea", "bytes", "Vec<u8>", "Uint8Array"),
        "uuid[]" => array_codec("uuid", "uuid", "Uuid", "string"),
        "date[]" => array_codec("date", "date-days", "Date", "string"),
        "time without time zone[]" => array_codec("time", "time-micros", "Time", "string"),
        "timestamp without time zone[]" => {
            array_codec("timestamp", "timestamp-micros", "Timestamp", "string")
        }
        "timestamp with time zone[]" => array_codec(
            "timestamptz",
            "timestamptz-micros-utc",
            "Timestamp",
            "string",
        ),
        "jsonb[]" => array_codec("jsonb", "jsonb-v1", "Jsonb<facet_value::Value>", "unknown"),
        _ => {
            return Err(CatalogError::UnsupportedTypeMapping {
                qualified_name: format!("pg_catalog.{canonical_name}"),
            });
        }
    };
    Ok(codec)
}

pub(crate) fn pseudo_codec(canonical_name: &str) -> CodecBinding {
    CodecBinding::new(
        format!("pg18:pg-codec:non-bindable-pseudo:{canonical_name}"),
        format!("wire:postgres:non-bindable-pseudo:{canonical_name}"),
        format!("PgPseudo<{canonical_name}>"),
        format!("PgPseudo<{canonical_name}>"),
    )
}

pub(crate) fn enum_codec(qualified_name: &str) -> CodecBinding {
    CodecBinding::new(
        "pg18:pg-codec:enum-text",
        "wire:postgres:text",
        qualified_name,
        qualified_name,
    )
}

pub(crate) fn array_codec_for_registered(
    postgres_major: u16,
    element: &CodecBinding,
) -> CodecBinding {
    CodecBinding::new(
        format!(
            "pg{postgres_major}:pg-codec:array<{}>",
            element.pg_codec_id.as_str()
        ),
        format!("wire:postgres:array<{}>", element.wire_codec_id.as_str()),
        format!("PgArray<{}>", element.rust_api_type.as_str()),
        format!("PgArray<{}>", element.typescript_api_type.as_str()),
    )
}

fn array_codec(
    pg_element: &str,
    wire_element: &str,
    rust_element: &str,
    typescript_element: &str,
) -> CodecBinding {
    CodecBinding::new(
        format!("pg18:pg-codec:array<{pg_element}>"),
        format!("wire:postgres:array<{wire_element}>"),
        format!("PgArray<{rust_element}>"),
        format!("PgArray<{typescript_element}>"),
    )
}
