use dibs::{ColumnInfo, SchemaInfo, TableInfo, generate_rust_code_with_schema, parse_query_file};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;

fn main() {
    println!("cargo::rerun-if-changed=queries.styx");

    // Force the linker to include my_app_db's inventory submissions
    // by referencing a type from the crate.
    let _ = std::any::TypeId::of::<my_app_db::Product>();

    // Collect schema from registered tables via inventory
    let schema = collect_schema();

    let queries_path = Path::new("queries.styx");
    let source = fs::read_to_string(queries_path).expect("Failed to read queries.styx");

    let file = parse_query_file(&source).expect("Failed to parse queries.styx");
    let generated = generate_rust_code_with_schema(&file, &schema);

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR not set");
    let dest_path = Path::new(&out_dir).join("queries.rs");

    fs::write(&dest_path, &generated.code).expect("Failed to write generated queries.rs");

    println!("cargo::rustc-env=QUERIES_PATH={}", dest_path.display());
}

/// Collect schema information from dibs tables registered via inventory.
fn collect_schema() -> SchemaInfo {
    let dibs_schema = dibs::Schema::collect();

    eprintln!(
        "cargo::warning=Found {} tables in schema",
        dibs_schema.tables.len()
    );

    let mut tables = HashMap::new();

    for table in &dibs_schema.tables {
        eprintln!(
            "cargo::warning=Table: {} with {} columns",
            table.name,
            table.columns.len()
        );
        let mut columns = HashMap::new();

        for col in &table.columns {
            // Map PgType back to Rust type name for codegen
            let rust_type = col
                .rust_type
                .clone()
                .unwrap_or_else(|| pg_type_to_rust(&col.pg_type));

            columns.insert(
                col.name.clone(),
                ColumnInfo {
                    rust_type,
                    nullable: col.nullable,
                },
            );
        }

        tables.insert(table.name.clone(), TableInfo { columns });
    }

    SchemaInfo { tables }
}

/// Map PgType to a Rust type string.
/// These names match what's exported in dibs_runtime::prelude.
fn pg_type_to_rust(pg_type: &dibs::PgType) -> String {
    use dibs::PgType;
    match pg_type {
        PgType::SmallInt => "i16".to_string(),
        PgType::Integer => "i32".to_string(),
        PgType::BigInt => "i64".to_string(),
        PgType::Real => "f32".to_string(),
        PgType::DoublePrecision => "f64".to_string(),
        PgType::Numeric => "Decimal".to_string(),
        PgType::Boolean => "bool".to_string(),
        PgType::Text => "String".to_string(),
        PgType::Bytea => "Vec<u8>".to_string(),
        PgType::Timestamptz => "Timestamp".to_string(),
        PgType::Date => "Date".to_string(),
        PgType::Time => "Time".to_string(),
        PgType::Uuid => "Uuid".to_string(),
        PgType::Jsonb => "JsonValue".to_string(),
    }
}
