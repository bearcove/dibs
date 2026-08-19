use dibs_db_schema::{
    CheckConstraint, Column, ForeignKey, Index, IndexColumn, NullsOrder, PgType, Schema, SortOrder,
    SourceLocation, Table, TriggerCheckConstraint,
};
use dibs_pg_catalog::{
    ApiLanguage, ApiTypeId, CallableKind, CatalogError, CatalogSnapshot, Nullability, PgTypeKind,
    ScalarSignature, TableOutputColumn, TableSignature, TypeRegistration, TypeRegistrationKind,
};
use indexmap::IndexMap;

fn column(name: &str, pg_type: PgType, nullable: bool) -> Column {
    Column {
        name: name.to_string(),
        pg_type,
        rust_type: Some(pg_type.to_rust_type().to_string()),
        nullable,
        default: None,
        primary_key: false,
        unique: false,
        auto_generated: false,
        long: false,
        label: false,
        enum_variants: Vec::new(),
        doc: None,
        lang: None,
        icon: None,
        subtype: None,
    }
}

fn table(name: &str, columns: Vec<Column>) -> Table {
    Table {
        name: name.to_string(),
        columns,
        check_constraints: Vec::new(),
        trigger_checks: Vec::new(),
        foreign_keys: Vec::new(),
        indices: Vec::new(),
        source: SourceLocation::default(),
        doc: None,
        icon: None,
    }
}

fn schema(tables: Vec<Table>) -> Schema {
    Schema {
        tables: tables
            .into_iter()
            .map(|table| (table.name.clone(), table))
            .collect(),
    }
}

#[test]
fn bigint_has_separate_pg_wire_and_api_identities() {
    let catalog = CatalogSnapshot::postgres_18_fixture();
    let bigint = catalog.resolve_type("pg_catalog.bigint").unwrap();

    assert_ne!(bigint.pg_codec_id.as_str(), bigint.wire_codec_id.as_str());
    assert_ne!(bigint.wire_codec_id.as_str(), bigint.rust_api_type.as_str());
    assert_eq!(bigint.rust_api_type.as_str(), "i64");
    assert_eq!(bigint.typescript_api_type.as_str(), "bigint");
    assert_ne!(bigint.typescript_api_type.as_str(), "number");
}

#[test]
fn schema_fingerprint_is_order_independent() {
    let alpha = table("alpha", vec![column("id", PgType::BigInt, false)]);
    let beta = table("beta", vec![column("label", PgType::Text, true)]);
    let schema_ab = schema(vec![alpha.clone(), beta.clone()]);
    let schema_ba = schema(vec![beta, alpha]);

    let snapshot_ab = CatalogSnapshot::from_schema_postgres_18(&schema_ab).unwrap();
    let snapshot_ba = CatalogSnapshot::from_schema_postgres_18(&schema_ba).unwrap();

    assert_eq!(snapshot_ab.fingerprint(), snapshot_ba.fingerprint());
    assert_eq!(
        snapshot_ab.schema_fingerprint,
        snapshot_ba.schema_fingerprint
    );
}

#[test]
fn stable_type_identity_ignores_runtime_oids() {
    let catalog = CatalogSnapshot::postgres_18_fixture();
    let bigint = catalog.resolve_type("pg_catalog.bigint").unwrap();

    assert_eq!(bigint.id.as_str(), "pg18:type:base:pg_catalog.bigint");
    assert!(!bigint.id.as_str().contains("20"));
}

#[test]
fn schema_conversion_preserves_tables_columns_constraints_indexes_foreign_keys_and_types() {
    let mut account_id = column("id", PgType::BigInt, false);
    account_id.primary_key = true;
    account_id.auto_generated = true;

    let mut email = column("email", PgType::Text, false);
    email.unique = true;

    let account = table("account", vec![account_id, email]);

    let mut session_id = column("id", PgType::Uuid, false);
    session_id.primary_key = true;
    session_id.default = Some("gen_random_uuid()".to_string());
    session_id.auto_generated = true;

    let mut role = column("role", PgType::Text, false);
    role.enum_variants = vec!["owner".to_string(), "member".to_string()];

    let mut session = table(
        "session",
        vec![
            session_id,
            column("account_id", PgType::BigInt, false),
            column("expires_at", PgType::Timestamptz, true),
            role,
        ],
    );
    session.check_constraints.push(CheckConstraint {
        name: "ck_session_role".to_string(),
        expr: "role <> ''".to_string(),
    });
    session.trigger_checks.push(TriggerCheckConstraint {
        name: "trg_session_expiry".to_string(),
        expr: "NEW.expires_at IS NULL OR NEW.expires_at > now()".to_string(),
        message: Some("session expiry must be in the future".to_string()),
    });
    session.foreign_keys.push(ForeignKey {
        columns: vec!["account_id".to_string()],
        references_table: "account".to_string(),
        references_columns: vec!["id".to_string()],
    });
    session.indices.push(Index {
        name: "idx_session_account_expiry".to_string(),
        columns: vec![
            IndexColumn::new("account_id"),
            IndexColumn {
                name: "expires_at".to_string(),
                order: SortOrder::Desc,
                nulls: NullsOrder::Last,
            },
        ],
        unique: false,
        where_clause: Some("expires_at IS NOT NULL".to_string()),
    });

    let snapshot =
        CatalogSnapshot::from_schema_postgres_18(&schema(vec![account, session])).unwrap();
    let session = snapshot.resolve_table("public.session").unwrap();

    assert_eq!(session.columns.len(), 4);
    assert_eq!(session.primary_key.columns, vec!["id"]);
    assert_eq!(session.unique_constraints.len(), 0);
    assert_eq!(session.check_constraints[0].name, "ck_session_role");
    assert_eq!(session.trigger_checks[0].name, "trg_session_expiry");
    assert_eq!(session.foreign_keys[0].columns, vec!["account_id"]);
    assert_eq!(session.foreign_keys[0].references_table, "public.account");
    assert_eq!(session.indexes[0].columns[1].order, SortOrder::Desc);
    assert_eq!(session.indexes[0].columns[1].nulls, NullsOrder::Last);
    assert_eq!(
        session.indexes[0].where_clause.as_deref(),
        Some("expires_at IS NOT NULL")
    );

    let id = session.column("id").unwrap();
    assert_eq!(
        id.type_id,
        snapshot.resolve_type("pg_catalog.uuid").unwrap().id
    );
    assert_eq!(id.nullability, Nullability::NotNull);
    assert!(id.primary_key);
    assert!(id.auto_generated);
    assert_eq!(id.default.as_deref(), Some("gen_random_uuid()"));

    let expires_at = session.column("expires_at").unwrap();
    assert_eq!(expires_at.nullability, Nullability::Nullable);
    assert_eq!(
        expires_at.type_id,
        snapshot
            .resolve_type("pg_catalog.timestamp with time zone")
            .unwrap()
            .id
    );

    let role = session.column("role").unwrap();
    let role_type = snapshot.type_by_id(&role.type_id).unwrap();
    assert_eq!(role_type.qualified_name, "pg_catalog.text");
    assert_eq!(role_type.kind, PgTypeKind::Base);
}

#[test]
fn registered_enum_and_domain_have_distinct_stable_type_relationships() {
    let mut catalog = CatalogSnapshot::postgres_18_fixture();
    let enum_id = catalog
        .register_type(TypeRegistration {
            qualified_name: "app.order_state".to_string(),
            kind: TypeRegistrationKind::Enum {
                variants: vec!["new".to_string(), "paid".to_string()],
            },
        })
        .unwrap();
    let domain_id = catalog
        .register_type(TypeRegistration {
            qualified_name: "app.positive_bigint".to_string(),
            kind: TypeRegistrationKind::Domain {
                base_type: "pg_catalog.bigint".to_string(),
            },
        })
        .unwrap();

    let registered_enum = catalog.type_by_id(&enum_id).unwrap();
    assert_eq!(registered_enum.kind, PgTypeKind::Enum);
    assert_eq!(registered_enum.enum_variants, vec!["new", "paid"]);
    assert_eq!(
        registered_enum.pg_codec_id.as_str(),
        "pg18:pg-codec:enum-text"
    );
    assert_eq!(registered_enum.wire_codec_id.as_str(), "wire:postgres:text");

    let domain = catalog.type_by_id(&domain_id).unwrap();
    assert_eq!(domain.kind, PgTypeKind::Domain);
    assert_eq!(
        domain.base_type.as_ref(),
        Some(&catalog.resolve_type("pg_catalog.bigint").unwrap().id)
    );
    assert_eq!(domain.rust_api_type.as_str(), "i64");
    assert_eq!(domain.typescript_api_type.as_str(), "bigint");
    assert_ne!(enum_id, domain_id);
}

#[test]
fn unsupported_mappings_return_typed_errors_without_lossy_fallback() {
    let catalog = CatalogSnapshot::postgres_18_fixture();

    assert_eq!(
        catalog.resolve_api_type(ApiLanguage::TypeScript, "number"),
        Err(CatalogError::AmbiguousApiType {
            language: ApiLanguage::TypeScript,
            api_type: ApiTypeId::new("number"),
        })
    );

    let mut catalog = CatalogSnapshot::postgres_18_fixture();
    assert_eq!(
        catalog.register_type(TypeRegistration {
            qualified_name: "app.bad_domain".to_string(),
            kind: TypeRegistrationKind::Domain {
                base_type: "pg_catalog.xml".to_string(),
            },
        }),
        Err(CatalogError::UnknownType {
            qualified_name: "pg_catalog.xml".to_string(),
        })
    );
}

#[test]
fn scalar_and_table_registration_are_exact_and_duplicate_safe() {
    let mut catalog = CatalogSnapshot::postgres_18_fixture();
    let bigint = catalog
        .resolve_type("pg_catalog.bigint")
        .unwrap()
        .id
        .clone();
    let text = catalog.resolve_type("pg_catalog.text").unwrap().id.clone();

    let scalar = ScalarSignature {
        qualified_name: "app.add_one".to_string(),
        arguments: vec![bigint.clone()],
        result: bigint.clone(),
    };
    let scalar_id = catalog.register_scalar(scalar.clone()).unwrap();
    assert_eq!(
        scalar_id.as_str(),
        "pg18:callable:scalar:app.add_one(pg18:type:base:pg_catalog.bigint)->pg18:type:base:pg_catalog.bigint"
    );
    assert_eq!(
        catalog.callable_by_id(&scalar_id).unwrap().kind,
        CallableKind::Scalar
    );
    assert_eq!(
        catalog.register_scalar(scalar),
        Err(CatalogError::DuplicateCallable { id: scalar_id })
    );

    let table = TableSignature {
        qualified_name: "app.expand".to_string(),
        arguments: vec![text.clone()],
        columns: vec![
            TableOutputColumn {
                name: "value".to_string(),
                type_id: text.clone(),
                nullability: Nullability::NotNull,
            },
            TableOutputColumn {
                name: "ordinality".to_string(),
                type_id: bigint,
                nullability: Nullability::NotNull,
            },
        ],
    };
    let table_id = catalog.register_table(table).unwrap();
    let registered = catalog.callable_by_id(&table_id).unwrap();
    assert_eq!(registered.kind, CallableKind::Table);
    assert_eq!(registered.table_columns.len(), 2);
}

#[test]
fn registration_rejects_postgres_overload_collisions_by_name_and_inputs() {
    let mut catalog = CatalogSnapshot::postgres_18_fixture();
    let bigint = catalog
        .resolve_type("pg_catalog.bigint")
        .unwrap()
        .id
        .clone();
    let text = catalog.resolve_type("pg_catalog.text").unwrap().id.clone();

    catalog
        .register_scalar(ScalarSignature {
            qualified_name: "app.same_inputs".to_string(),
            arguments: vec![bigint.clone()],
            result: bigint.clone(),
        })
        .unwrap();

    assert_eq!(
        catalog.register_scalar(ScalarSignature {
            qualified_name: "app.same_inputs".to_string(),
            arguments: vec![bigint.clone()],
            result: text.clone(),
        }),
        Err(CatalogError::DuplicateCallableSignature {
            qualified_name: "app.same_inputs".to_string(),
            arguments: vec![bigint.clone()],
        })
    );

    assert_eq!(
        catalog.register_table(TableSignature {
            qualified_name: "app.same_inputs".to_string(),
            arguments: vec![bigint.clone()],
            columns: vec![TableOutputColumn {
                name: "value".to_string(),
                type_id: text,
                nullability: Nullability::NotNull,
            }],
        }),
        Err(CatalogError::DuplicateCallableSignature {
            qualified_name: "app.same_inputs".to_string(),
            arguments: vec![bigint],
        })
    );
}
#[test]
fn registration_rejects_unknown_argument_result_and_column_types() {
    let mut catalog = CatalogSnapshot::postgres_18_fixture();
    let unknown = dibs_pg_catalog::TypeId::new("pg18:type:base:app.missing");

    let error = catalog
        .register_scalar(ScalarSignature {
            qualified_name: "app.bad".to_string(),
            arguments: vec![unknown.clone()],
            result: unknown.clone(),
        })
        .unwrap_err();
    assert_eq!(error, CatalogError::UnknownTypeId { id: unknown });

    let text = catalog.resolve_type("pg_catalog.text").unwrap().id.clone();
    let unknown = dibs_pg_catalog::TypeId::new("pg18:type:base:app.missing_output");
    let error = catalog
        .register_table(TableSignature {
            qualified_name: "app.bad_table".to_string(),
            arguments: vec![text],
            columns: vec![TableOutputColumn {
                name: "value".to_string(),
                type_id: unknown.clone(),
                nullability: Nullability::Nullable,
            }],
        })
        .unwrap_err();
    assert_eq!(error, CatalogError::UnknownTypeId { id: unknown });
}

#[test]
fn registration_rejects_invalid_unquoted_qualified_names() {
    let mut catalog = CatalogSnapshot::postgres_18_fixture();
    let bigint = catalog
        .resolve_type("pg_catalog.bigint")
        .unwrap()
        .id
        .clone();

    assert_eq!(
        catalog.register_scalar(ScalarSignature {
            qualified_name: "app bad.function".to_string(),
            arguments: Vec::new(),
            result: bigint,
        }),
        Err(CatalogError::UnqualifiedName {
            name: "app bad.function".to_string(),
        })
    );
}

#[test]
fn every_curated_type_has_distinct_codec_and_api_identity_namespaces() {
    let catalog = CatalogSnapshot::postgres_18_fixture();

    for ty in &catalog.types {
        assert!(ty.pg_codec_id.as_str().contains(":pg-codec:"));
        assert!(ty.wire_codec_id.as_str().starts_with("wire:"));
        assert!(!ty.rust_api_type.as_str().starts_with("wire:"));
        assert!(!ty.typescript_api_type.as_str().starts_with("wire:"));
    }
}

#[test]
fn schema_fingerprint_changes_when_schema_truth_changes() {
    let original = schema(vec![table(
        "account",
        vec![column("id", PgType::BigInt, false)],
    )]);
    let changed = schema(vec![table(
        "account",
        vec![column("id", PgType::BigInt, true)],
    )]);

    let original = CatalogSnapshot::from_schema_postgres_18(&original).unwrap();
    let changed = CatalogSnapshot::from_schema_postgres_18(&changed).unwrap();

    assert_ne!(original.fingerprint(), changed.fingerprint());
}

#[test]
fn table_order_does_not_change_stable_table_lookup() {
    let mut tables = IndexMap::new();
    tables.insert(
        "zeta".to_string(),
        table("zeta", vec![column("id", PgType::Integer, false)]),
    );
    tables.insert(
        "alpha".to_string(),
        table("alpha", vec![column("id", PgType::Integer, false)]),
    );

    let snapshot = CatalogSnapshot::from_schema_postgres_18(&Schema { tables }).unwrap();
    assert_eq!(snapshot.tables[0].qualified_name, "public.alpha");
    assert_eq!(snapshot.tables[1].qualified_name, "public.zeta");
}

#[test]
fn schema_fingerprint_preserves_column_order() {
    let schema_ab = schema(vec![table(
        "record",
        vec![
            column("alpha", PgType::Integer, false),
            column("beta", PgType::Text, false),
        ],
    )]);
    let schema_ba = schema(vec![table(
        "record",
        vec![
            column("beta", PgType::Text, false),
            column("alpha", PgType::Integer, false),
        ],
    )]);

    let snapshot_ab = CatalogSnapshot::from_schema_postgres_18(&schema_ab).unwrap();
    let snapshot_ba = CatalogSnapshot::from_schema_postgres_18(&schema_ba).unwrap();

    assert_ne!(snapshot_ab.fingerprint(), snapshot_ba.fingerprint());
}
