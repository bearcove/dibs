use super::*;
use crate::parse_query_file;
use camino::Utf8Path;
use dibs_db_schema::{Column, ForeignKey, PgType, Schema, SourceLocation, Table};
use facet_testhelpers::test;
use indexmap::IndexMap;

fn parse_test(source: &str) -> crate::QueryFile {
    parse_query_file(Utf8Path::new("<test>"), source).unwrap()
}

fn make_test_table(name: &str, columns: &[&str], fks: Vec<ForeignKey>) -> Table {
    Table {
        name: name.to_string(),
        columns: columns
            .iter()
            .map(|&col| Column {
                name: col.to_string(),
                pg_type: PgType::Text,
                rust_type: None,
                nullable: false,
                default: None,
                primary_key: col == "id",
                unique: false,
                auto_generated: false,
                long: false,
                label: false,
                enum_variants: vec![],
                doc: None,
                icon: None,
                lang: None,
                subtype: None,
            })
            .collect(),
        check_constraints: vec![],
        trigger_checks: vec![],
        foreign_keys: fks,
        indices: vec![],
        source: SourceLocation::default(),
        doc: None,
        icon: None,
    }
}

fn make_test_schema(tables: Vec<Table>) -> Schema {
    Schema {
        tables: tables.into_iter().map(|t| (t.name.clone(), t)).collect(),
    }
}

// FIXME: All those tests are garbage and should just be using snapshots instead.

#[test]
fn test_generate_simple_query() {
    let source = r#"
AllProducts @select{
  from product
  select { id, handle, status }
}
"#;
    let file = parse_test(source);
    let code = generate_rust_code(&file);

    assert!(code.code.contains("pub struct AllProductsResult"));
    assert!(code.code.contains("pub async fn all_products"));
    assert!(code.code.contains("pub id: String"));
    assert!(code.code.contains("pub handle: String"));
    assert!(code.code.contains("#[derive(Debug, Clone, Facet)]"));
}

#[test]
fn test_generate_query_with_params() {
    let source = r#"
ProductByHandle @select{
  params { handle @string }
  from product
  where { handle $handle }
  first true
  select { id, handle }
}
"#;
    let file = parse_test(source);
    let code = generate_rust_code(&file);

    assert!(code.code.contains("handle: &String"));
    assert!(code.code.contains("from_row"));
    assert!(code.code.contains("Ok(None)"));
}

#[test]
fn test_generate_query_with_relation() {
    let source = r#"
ProductListing @select{
  from product
  select {
    id
    translation @rel{
      first true
      select { title, description }
    }
  }
}
"#;
    let file = parse_test(source);
    let code = generate_rust_code(&file);

    assert!(
        code.code
            .contains("pub translation: Option<ProductListingTranslation>")
    );
    assert!(code.code.contains("pub struct ProductListingTranslation"));
    assert!(code.code.contains("pub title: String"));
}

#[test]
fn test_generate_raw_sql_query() {
    let source = r#"
TrendingProducts @select{
  params { locale @string, days @int }
  sql <<SQL
    SELECT id, title FROM products WHERE locale = $1
  SQL
  returns { id @int, title @string }
}
"#;
    let file = parse_test(source);
    let code = generate_rust_code(&file);

    assert!(code.code.contains("locale: &String"));
    assert!(code.code.contains("days: &i64"));
    assert!(code.code.contains("pub id: i64"));
    assert!(code.code.contains("pub title: String"));
    assert!(code.code.contains("SELECT id, title FROM products"));
}

#[test]
fn test_snake_case() {
    assert_eq!(to_snake_case("ProductListing"), "product_listing");
    assert_eq!(to_snake_case("AllProducts"), "all_products");
    assert_eq!(to_snake_case("ID"), "i_d");
}

#[test]
fn test_pascal_case() {
    assert_eq!(to_pascal_case("translation"), "Translation");
    assert_eq!(to_pascal_case("product_variant"), "ProductVariant");
}

#[test]
fn test_generate_join_query() {
    let source = r#"
ProductWithTranslation @select{
  params { handle @string }
  from product
  where { handle $handle }
  first true
  select {
    id, handle, translation @rel{
      from product_translation
      first true
      select { title, description }
    }
  }
}
"#;
    let file = parse_test(source);

    let mut schema = SchemaInfo::default();
    let mut product_cols = HashMap::new();
    product_cols.insert(
        "id".to_string(),
        ColumnInfo {
            rust_type: "i64".to_string(),
            nullable: false,
        },
    );
    product_cols.insert(
        "handle".to_string(),
        ColumnInfo {
            rust_type: "String".to_string(),
            nullable: false,
        },
    );
    schema.tables.insert(
        "product".to_string(),
        TableInfo {
            columns: product_cols,
        },
    );

    let mut translation_cols = HashMap::new();
    translation_cols.insert(
        "title".to_string(),
        ColumnInfo {
            rust_type: "String".to_string(),
            nullable: false,
        },
    );
    translation_cols.insert(
        "description".to_string(),
        ColumnInfo {
            rust_type: "String".to_string(),
            nullable: true,
        },
    );
    schema.tables.insert(
        "product_translation".to_string(),
        TableInfo {
            columns: translation_cols,
        },
    );

    let planner_schema = make_test_schema(vec![
        make_test_table("product", &["id", "handle"], vec![]),
        make_test_table(
            "product_translation",
            &["id", "product_id", "title", "description"],
            vec![ForeignKey {
                columns: vec!["product_id".to_string()],
                references_table: "product".to_string(),
                references_columns: vec!["id".to_string()],
            }],
        ),
    ]);

    let code = generate_rust_code_with_planner(&file, &schema, Some(&planner_schema));

    assert!(
        code.code
            .contains("pub struct ProductWithTranslationResult")
    );
    assert!(code.code.contains("pub id: i64"));
    assert!(code.code.contains("pub handle: String"));
    assert!(
        code.code
            .contains("pub translation: Option<ProductWithTranslationTranslation>")
    );
    assert!(
        code.code
            .contains("pub struct ProductWithTranslationTranslation")
    );
    assert!(code.code.contains("LEFT JOIN"));
    assert!(code.code.contains("product_translation"));
    assert!(code.code.contains("translation_title"));
    assert!(code.code.contains("translation_description"));
    // Check that translation struct construction happens inside a .map() call
    // The exact variable name depends on HashMap iteration order
    assert!(
        code.code
            .contains(".map(|translation_description_val| ProductWithTranslationTranslation")
            || code
                .code
                .contains(".map(|translation_title_val| ProductWithTranslationTranslation"),
        "Expected relation construction inside .map() call"
    );
    // Check that title field is populated
    assert!(
        code.code.contains("title: translation_title")
            || code.code.contains("title: translation_title_val"),
        "Expected title field assignment"
    );
}

#[test]
fn test_generate_vec_relation_query() {
    let source = r#"
ProductWithVariants @select{
  from product
  select {
    id, handle, variants @rel{
      from product_variant
      select { id, sku }
    }
  }
}
"#;
    let file = parse_test(source);

    let mut schema = SchemaInfo::default();
    let mut product_cols = HashMap::new();
    product_cols.insert(
        "id".to_string(),
        ColumnInfo {
            rust_type: "i64".to_string(),
            nullable: false,
        },
    );
    product_cols.insert(
        "handle".to_string(),
        ColumnInfo {
            rust_type: "String".to_string(),
            nullable: false,
        },
    );
    schema.tables.insert(
        "product".to_string(),
        TableInfo {
            columns: product_cols,
        },
    );

    let mut variant_cols = HashMap::new();
    variant_cols.insert(
        "id".to_string(),
        ColumnInfo {
            rust_type: "i64".to_string(),
            nullable: false,
        },
    );
    variant_cols.insert(
        "sku".to_string(),
        ColumnInfo {
            rust_type: "String".to_string(),
            nullable: false,
        },
    );
    schema.tables.insert(
        "product_variant".to_string(),
        TableInfo {
            columns: variant_cols,
        },
    );

    let planner_schema = make_test_schema(vec![
        make_test_table("product", &["id", "handle"], vec![]),
        make_test_table(
            "product_variant",
            &["id", "product_id", "sku"],
            vec![ForeignKey {
                columns: vec!["product_id".to_string()],
                references_table: "product".to_string(),
                references_columns: vec!["id".to_string()],
            }],
        ),
    ]);

    let code = generate_rust_code_with_planner(&file, &schema, Some(&planner_schema));

    tracing::info!("Generated code:\n{}", code.code);

    assert!(
        code.code.contains("pub struct ProductWithVariantsResult"),
        "Should generate result struct"
    );
    assert!(code.code.contains("pub id: i64"), "Should have id field");
    assert!(
        code.code.contains("pub handle: String"),
        "Should have handle field"
    );
    assert!(
        code.code
            .contains("pub variants: Vec<ProductWithVariantsVariants>"),
        "Should have Vec variants field"
    );
    assert!(
        code.code.contains("pub struct ProductWithVariantsVariants"),
        "Should generate nested Variants struct"
    );
    assert!(code.code.contains("LEFT JOIN"), "Should use LEFT JOIN");
    assert!(
        code.code.contains("product_variant"),
        "Should join product_variant"
    );
    assert!(
        code.code.contains("HashMap"),
        "Should use HashMap for grouping"
    );
    assert!(
        code.code.contains("grouped.entry"),
        "Should use entry API for grouping"
    );
    assert!(code.code.contains(".push("), "Should push to Vec relation");
    assert!(
        code.code.contains("variants: vec![]"),
        "Should initialize Vec as empty"
    );
    assert!(
        code.code.contains("entry.variants.push"),
        "Should append to variants"
    );
}

#[test]
fn test_generate_count_query() {
    let source = r#"
ProductWithVariantCount @select{
  from product
  select { id, handle, variant_count @count(product_variant) }
}
"#;
    let file = parse_test(source);

    let mut schema = SchemaInfo::default();
    let mut product_cols = HashMap::new();
    product_cols.insert(
        "id".to_string(),
        ColumnInfo {
            rust_type: "i64".to_string(),
            nullable: false,
        },
    );
    product_cols.insert(
        "handle".to_string(),
        ColumnInfo {
            rust_type: "String".to_string(),
            nullable: false,
        },
    );
    schema.tables.insert(
        "product".to_string(),
        TableInfo {
            columns: product_cols,
        },
    );

    let planner_schema = make_test_schema(vec![
        make_test_table("product", &["id", "handle"], vec![]),
        make_test_table(
            "product_variant",
            &["id", "product_id", "sku"],
            vec![ForeignKey {
                columns: vec!["product_id".to_string()],
                references_table: "product".to_string(),
                references_columns: vec!["id".to_string()],
            }],
        ),
    ]);

    let code = generate_rust_code_with_planner(&file, &schema, Some(&planner_schema));

    tracing::info!("Generated code:\n{}", code.code);

    assert!(
        code.code
            .contains("pub struct ProductWithVariantCountResult"),
        "Should generate result struct"
    );
    assert!(
        code.code.contains("pub variant_count: i64"),
        "Should have variant_count field as i64"
    );
    assert!(
        code.code.contains("SELECT COUNT(*)"),
        "Should generate COUNT subquery in SQL"
    );
    assert!(
        code.code.contains("product_variant"),
        "Should reference product_variant table in COUNT"
    );
    assert!(
        code.code.contains("variant_count"),
        "Should alias the COUNT result"
    );
}

#[test]
fn test_generate_nested_vec_relation_query() {
    let source = r#"
ProductWithVariantsAndPrices @select{
  from product
  select {
    id, handle, variants @rel{
      from product_variant
      select { id, sku, prices @rel{
          from variant_price
          select { id, currency_code, amount }
      }}
    }
  }
}
"#;
    let file = parse_test(source);

    let mut schema = SchemaInfo::default();
    let mut product_cols = HashMap::new();
    product_cols.insert(
        "id".to_string(),
        ColumnInfo {
            rust_type: "i64".to_string(),
            nullable: false,
        },
    );
    product_cols.insert(
        "handle".to_string(),
        ColumnInfo {
            rust_type: "String".to_string(),
            nullable: false,
        },
    );
    schema.tables.insert(
        "product".to_string(),
        TableInfo {
            columns: product_cols,
        },
    );

    let mut variant_cols = HashMap::new();
    variant_cols.insert(
        "id".to_string(),
        ColumnInfo {
            rust_type: "i64".to_string(),
            nullable: false,
        },
    );
    variant_cols.insert(
        "sku".to_string(),
        ColumnInfo {
            rust_type: "String".to_string(),
            nullable: false,
        },
    );
    schema.tables.insert(
        "product_variant".to_string(),
        TableInfo {
            columns: variant_cols,
        },
    );

    let mut price_cols = HashMap::new();
    price_cols.insert(
        "id".to_string(),
        ColumnInfo {
            rust_type: "i64".to_string(),
            nullable: false,
        },
    );
    price_cols.insert(
        "currency_code".to_string(),
        ColumnInfo {
            rust_type: "String".to_string(),
            nullable: false,
        },
    );
    price_cols.insert(
        "amount".to_string(),
        ColumnInfo {
            rust_type: "i64".to_string(),
            nullable: false,
        },
    );
    schema.tables.insert(
        "variant_price".to_string(),
        TableInfo {
            columns: price_cols,
        },
    );

    let planner_schema = make_test_schema(vec![
        make_test_table("product", &["id", "handle"], vec![]),
        make_test_table(
            "product_variant",
            &["id", "product_id", "sku"],
            vec![ForeignKey {
                columns: vec!["product_id".to_string()],
                references_table: "product".to_string(),
                references_columns: vec!["id".to_string()],
            }],
        ),
        make_test_table(
            "variant_price",
            &["id", "variant_id", "currency_code", "amount"],
            vec![ForeignKey {
                columns: vec!["variant_id".to_string()],
                references_table: "product_variant".to_string(),
                references_columns: vec!["id".to_string()],
            }],
        ),
    ]);

    let code = generate_rust_code_with_planner(&file, &schema, Some(&planner_schema));

    tracing::info!("Generated code:\n{}", code.code);

    // Check result structs
    assert!(
        code.code
            .contains("pub struct ProductWithVariantsAndPricesResult"),
        "Should generate top-level result struct"
    );
    assert!(
        code.code
            .contains("pub struct ProductWithVariantsAndPricesVariants"),
        "Should generate nested Variants struct"
    );
    assert!(
        code.code
            .contains("pub struct ProductWithVariantsAndPricesVariantsPrices"),
        "Should generate nested Prices struct"
    );

    // Check field types
    assert!(
        code.code
            .contains("pub variants: Vec<ProductWithVariantsAndPricesVariants>"),
        "Should have Vec variants field"
    );
    assert!(
        code.code
            .contains("pub prices: Vec<ProductWithVariantsAndPricesVariantsPrices>"),
        "Should have Vec prices field in Variants"
    );

    // Check SQL JOINs
    assert!(code.code.contains("LEFT JOIN"), "Should use LEFT JOIN");
    assert!(
        code.code.contains("product_variant"),
        "Should join product_variant"
    );
    assert!(
        code.code.contains("variant_price"),
        "Should join variant_price"
    );

    // Check nested column aliases
    assert!(
        code.code.contains("variants_prices_currency_code"),
        "Should have nested column alias"
    );

    // Check deduplication logic for nested Vec relations
    assert!(
        code.code.contains("HashSet"),
        "Should use HashSet for deduplication"
    );
    assert!(
        code.code.contains("seen_variants"),
        "Should track seen variants"
    );
    assert!(
        code.code.contains("seen_variants_prices"),
        "Should track seen nested prices"
    );
}

#[test]
fn test_generate_insert_code() {
    let source = r#"
CreateUser @insert{
  params { name @string, email @string }
  into users
  values { name $name, email $email, created_at @now }
  returning { id, name, email, created_at }
}
"#;
    let file = parse_test(source);
    let code = generate_rust_code(&file);

    assert!(code.code.contains("pub struct CreateUserResult"));
    assert!(code.code.contains("pub async fn create_user"));
    assert!(code.code.contains("name: &String"));
    assert!(code.code.contains("email: &String"));
    assert!(code.code.contains("INSERT INTO"));
    assert!(code.code.contains("RETURNING"));
    assert!(
        code.code
            .contains("Result<Option<CreateUserResult>, QueryError>")
    );
}

#[test]
fn test_generate_upsert_code() {
    let source = r#"
UpsertProduct @upsert{
  params { id @uuid, name @string, price @decimal }
  into products
  on-conflict {
    target { id }
    update { name, price, updated_at @now }
  }
  values { id $id, name $name, price $price }
  returning { id, name, price, updated_at }
}
"#;
    let file = parse_test(source);
    let code = generate_rust_code(&file);

    assert!(code.code.contains("pub struct UpsertProductResult"));
    assert!(code.code.contains("pub async fn upsert_product"));
    assert!(code.code.contains("id: &Uuid"));
    assert!(code.code.contains("ON CONFLICT"));
    assert!(code.code.contains("DO UPDATE SET"));
}

#[test]
fn test_generate_update_code() {
    let source = r#"
UpdateUserEmail @update{
  params { id @uuid, email @string }
  table users
  set { email $email, updated_at @now }
  where { id $id }
  returning { id, email, updated_at }
}
"#;
    let file = parse_test(source);
    let code = generate_rust_code(&file);

    assert!(code.code.contains("pub struct UpdateUserEmailResult"));
    assert!(code.code.contains("pub async fn update_user_email"));
    assert!(code.code.contains("UPDATE"));
    assert!(code.code.contains("SET"));
    assert!(code.code.contains("WHERE"));
}

#[test]
fn test_generate_delete_code() {
    let source = r#"
DeleteUser @delete{
  params { id @uuid }
  from users
  where { id $id }
  returning { id }
}
"#;
    let file = parse_test(source);
    let code = generate_rust_code(&file);

    assert!(code.code.contains("pub struct DeleteUserResult"));
    assert!(code.code.contains("pub async fn delete_user"));
    assert!(code.code.contains("DELETE FROM"));
    assert!(code.code.contains("WHERE"));
}

#[test]
fn test_generate_insert_without_returning() {
    let source = r#"
InsertLog @insert{
  params { message @string }
  into logs
  values { message $message, created_at @now }
}
"#;
    let file = parse_test(source);
    let code = generate_rust_code(&file);

    // Should NOT generate a result struct
    assert!(!code.code.contains("pub struct InsertLogResult"));
    assert!(code.code.contains("pub async fn insert_log"));
    // Should use execute() instead of query()
    assert!(code.code.contains("client.execute"));
    assert!(code.code.contains("Result<u64, QueryError>"));
}

#[test]
fn test_generate_insert_many_code() {
    let source = r#"
BulkCreateProducts @insert-many{
  params { handle @string, status @string }
  into products
  values { handle $handle, status $status, created_at @now }
  returning { id, handle, status }
}
"#;
    let file = parse_test(source);
    let code = generate_rust_code(&file);

    // Should generate params struct
    assert!(
        code.code.contains("pub struct BulkCreateProductsParams"),
        "Should generate params struct"
    );
    assert!(
        code.code.contains("pub handle: String"),
        "Params struct should have handle field"
    );
    assert!(
        code.code.contains("pub status: String"),
        "Params struct should have status field"
    );

    // Should generate result struct
    assert!(
        code.code.contains("pub struct BulkCreateProductsResult"),
        "Should generate result struct"
    );

    // Should generate function that takes slice
    assert!(
        code.code.contains("pub async fn bulk_create_products"),
        "Should generate bulk_create_products function"
    );
    assert!(
        code.code.contains("items: &[BulkCreateProductsParams]"),
        "Function should take slice of params"
    );

    // Should return Vec of results
    assert!(
        code.code
            .contains("Result<Vec<BulkCreateProductsResult>, QueryError>"),
        "Should return Vec of results"
    );

    // Should convert to parallel arrays
    assert!(
        code.code.contains("handle_arr"),
        "Should create handle array"
    );
    assert!(
        code.code.contains("status_arr"),
        "Should create status array"
    );

    // Should use UNNEST
    assert!(code.code.contains("UNNEST"), "SQL should use UNNEST");
}

#[test]
fn test_generate_upsert_many_code() {
    let source = r#"
BulkUpsertProducts @upsert-many{
  params { handle @string, status @string }
  into products
  on-conflict {
    target { handle }
    update { status, updated_at @now }
  }
  values { handle $handle, status $status, created_at @now }
  returning { id, handle, status }
}
"#;
    let file = parse_test(source);
    let code = generate_rust_code(&file);

    // Should generate params struct
    assert!(
        code.code.contains("pub struct BulkUpsertProductsParams"),
        "Should generate params struct"
    );

    // Should generate result struct
    assert!(
        code.code.contains("pub struct BulkUpsertProductsResult"),
        "Should generate result struct"
    );

    // Should generate function
    assert!(
        code.code.contains("pub async fn bulk_upsert_products"),
        "Should generate bulk_upsert_products function"
    );
    assert!(
        code.code.contains("items: &[BulkUpsertProductsParams]"),
        "Function should take slice of params"
    );

    // Should have ON CONFLICT in SQL
    assert!(
        code.code.contains("ON CONFLICT"),
        "SQL should use ON CONFLICT"
    );
    assert!(
        code.code.contains("DO UPDATE SET"),
        "SQL should have DO UPDATE SET"
    );
}

#[test]
fn test_generate_insert_many_without_returning() {
    let source = r#"
BulkInsertLogs @insert-many{
  params { message @string }
  into logs
  values { message $message, created_at @now }
}
"#;
    let file = parse_test(source);
    let code = generate_rust_code(&file);

    // Should NOT generate result struct
    assert!(
        !code.code.contains("pub struct BulkInsertLogsResult"),
        "Should NOT generate result struct"
    );

    // Should generate params struct
    assert!(
        code.code.contains("pub struct BulkInsertLogsParams"),
        "Should generate params struct"
    );

    // Should use execute() and return u64
    assert!(code.code.contains("client.execute"), "Should use execute()");
    assert!(
        code.code.contains("Result<u64, QueryError>"),
        "Should return Result<u64>"
    );
}
