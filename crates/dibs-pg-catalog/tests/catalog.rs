use dibs_db_schema::{
    CheckConstraint, Column, ForeignKey, Index, IndexColumn, NullsOrder, PgType, Schema, SortOrder,
    SourceLocation, Table, TriggerCheckConstraint,
};
use dibs_pg_catalog::{
    AggregateEmptyBehavior, ApiLanguage, ApiTypeId, CallableCardinality, CallableKind, CastContext,
    CatalogError, CatalogSnapshot, DomainCollation, DomainConstraint, Nullability, PgArray,
    PgArrayDimension, PgArrayError, PgTypeCategory, PgTypeKind, PolymorphicType,
    ScalarCallableFacts, ScalarSignature, TableCallableFacts, TableOutputColumn, TableSignature,
    TypeRegistration, TypeRegistrationKind, Volatility,
};
use indexmap::IndexMap;

const SCALAR_FACTS: ScalarCallableFacts = ScalarCallableFacts {
    volatility: Volatility::Immutable,
    strict: true,
    result_nullability: Nullability::Nullable,
};

const TABLE_FACTS: TableCallableFacts = TableCallableFacts {
    volatility: Volatility::Immutable,
    strict: true,
    cardinality: CallableCardinality::SetOfUnknown,
};

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
fn postgres_arrays_use_shape_aware_lossless_api_model() {
    let catalog = CatalogSnapshot::postgres_18_fixture();
    let bigints = catalog.resolve_type("pg_catalog.bigint[]").unwrap();

    assert_eq!(bigints.rust_api_type.as_str(), "PgArray<i64>");
    assert_eq!(bigints.typescript_api_type.as_str(), "PgArray<bigint>");
    assert!(!bigints.rust_api_type.as_str().starts_with("Vec<"));
    assert!(
        !bigints
            .typescript_api_type
            .as_str()
            .starts_with("ReadonlyArray<")
    );

    let value = PgArray::try_new(
        vec![Some(10_i64), None, Some(30), Some(40)],
        vec![
            PgArrayDimension {
                length: 2,
                lower_bound: -1,
            },
            PgArrayDimension {
                length: 2,
                lower_bound: 5,
            },
        ],
    )
    .unwrap();
    assert_eq!(value.rank(), 2);
    assert_eq!(value.dimensions()[0].lower_bound, -1);
    assert_eq!(value.dimensions()[1].length, 2);
    assert_eq!(value.elements(), &[Some(10), None, Some(30), Some(40)]);

    assert_eq!(
        PgArray::try_new(
            vec![Some(1_i64)],
            vec![PgArrayDimension {
                length: 2,
                lower_bound: 1,
            }],
        ),
        Err(PgArrayError::ElementCountMismatch {
            expected: 2,
            actual: 1,
        })
    );
}

#[test]
fn postgres_empty_array_has_zero_rank_and_zero_elements() {
    let value = PgArray::<i64>::try_new(Vec::new(), Vec::new()).unwrap();

    assert_eq!(value.rank(), 0);
    assert!(value.dimensions().is_empty());
    assert!(value.elements().is_empty());
}

#[test]
fn postgres_array_rejects_unencodable_dimension_length() {
    assert_eq!(
        PgArray::<i64>::try_new(
            Vec::new(),
            vec![PgArrayDimension {
                length: i32::MAX as usize + 1,
                lower_bound: 1,
            }],
        ),
        Err(PgArrayError::DimensionLengthOutOfRange {
            length: i32::MAX as usize + 1,
        })
    );
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
                base_typmod: None,
                not_null: true,
                default: None,
                collation: DomainCollation::None,
                constraints: vec![DomainConstraint {
                    name: "positive_bigint_check".to_string(),
                    expression: "VALUE > 0".to_string(),
                }],
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
    let definition = domain.domain.as_ref().unwrap();
    assert_eq!(
        definition.base_type,
        catalog.resolve_type("pg_catalog.bigint").unwrap().id
    );
    assert_eq!(definition.base_typmod, None);
    assert!(definition.not_null);
    assert_eq!(definition.default, None);
    assert_eq!(definition.collation, None);
    assert_eq!(definition.constraints[0].name, "positive_bigint_check");
    assert_eq!(definition.constraints[0].expression, "VALUE > 0");
    assert_eq!(domain.rust_api_type.as_str(), "i64");
    assert_eq!(domain.typescript_api_type.as_str(), "bigint");
    assert_ne!(enum_id, domain_id);
}

#[test]
fn domain_definition_changes_identity_and_fingerprint() {
    fn snapshot(default: Option<&str>, not_null: bool, expression: &str) -> CatalogSnapshot {
        let mut catalog = CatalogSnapshot::postgres_18_fixture();
        catalog
            .register_type(TypeRegistration {
                qualified_name: "app.constrained_bigint".to_string(),
                kind: TypeRegistrationKind::Domain {
                    base_type: "pg_catalog.bigint".to_string(),
                    base_typmod: Some("precision=18".to_string()),
                    not_null,
                    default: default.map(str::to_string),
                    collation: DomainCollation::None,
                    constraints: vec![DomainConstraint {
                        name: "constrained_bigint_check".to_string(),
                        expression: expression.to_string(),
                    }],
                },
            })
            .unwrap();
        catalog
    }

    let baseline = snapshot(Some("1"), true, "VALUE > 0");
    let changed_default = snapshot(Some("2"), true, "VALUE > 0");
    let changed_nullability = snapshot(Some("1"), false, "VALUE > 0");
    let changed_constraint = snapshot(Some("1"), true, "VALUE >= 0");

    let baseline_type = baseline.resolve_type("app.constrained_bigint").unwrap();
    assert_ne!(
        baseline_type.id,
        changed_default
            .resolve_type("app.constrained_bigint")
            .unwrap()
            .id
    );
    assert_ne!(baseline.fingerprint(), changed_default.fingerprint());
    assert_ne!(baseline.fingerprint(), changed_nullability.fingerprint());
    assert_ne!(baseline.fingerprint(), changed_constraint.fingerprint());
}

#[test]
fn domain_constraint_input_order_is_canonicalized_by_name() {
    fn snapshot(constraints: Vec<DomainConstraint>) -> CatalogSnapshot {
        let mut catalog = CatalogSnapshot::postgres_18_fixture();
        catalog
            .register_type(TypeRegistration {
                qualified_name: "app.ordered_checks".to_string(),
                kind: TypeRegistrationKind::Domain {
                    base_type: "pg_catalog.bigint".to_string(),
                    base_typmod: None,
                    not_null: false,
                    default: None,
                    collation: DomainCollation::None,
                    constraints,
                },
            })
            .unwrap();
        catalog
    }

    let alpha = DomainConstraint {
        name: "alpha_check".to_string(),
        expression: "VALUE > 0".to_string(),
    };
    let omega = DomainConstraint {
        name: "omega_check".to_string(),
        expression: "VALUE < 100".to_string(),
    };
    let forward = snapshot(vec![alpha.clone(), omega.clone()]);
    let reversed = snapshot(vec![omega, alpha]);
    let forward_type = forward.resolve_type("app.ordered_checks").unwrap();
    let reversed_type = reversed.resolve_type("app.ordered_checks").unwrap();

    assert_eq!(forward_type.id, reversed_type.id);
    assert_eq!(forward.fingerprint(), reversed.fingerprint());
    assert_eq!(
        reversed_type
            .domain
            .as_ref()
            .unwrap()
            .constraints
            .iter()
            .map(|constraint| constraint.name.as_str())
            .collect::<Vec<_>>(),
        vec!["alpha_check", "omega_check"]
    );
}

#[test]
fn domain_rejects_collation_for_noncollatable_base() {
    let mut catalog = CatalogSnapshot::postgres_18_fixture();

    assert_eq!(
        catalog.register_type(TypeRegistration {
            qualified_name: "app.collated_bigint".to_string(),
            kind: TypeRegistrationKind::Domain {
                base_type: "pg_catalog.bigint".to_string(),
                base_typmod: None,
                not_null: false,
                default: None,
                collation: DomainCollation::Explicit(dibs_pg_catalog::CollationId::new(
                    "pg18:collation:pg_catalog.default",
                )),
                constraints: Vec::new(),
            },
        }),
        Err(CatalogError::InvalidDomainCollation {
            qualified_name: "app.collated_bigint".to_string(),
            base_type: "pg_catalog.bigint".to_string(),
        })
    );
}

#[test]
fn domain_rejects_unknown_or_wrong_version_collation() {
    for collation in [
        dibs_pg_catalog::CollationId::new("pg18:collation:app.missing"),
        dibs_pg_catalog::CollationId::new("pg17:collation:pg_catalog.default"),
    ] {
        let mut catalog = CatalogSnapshot::postgres_18_fixture();
        assert_eq!(
            catalog.register_type(TypeRegistration {
                qualified_name: "app.bad_text_collation".to_string(),
                kind: TypeRegistrationKind::Domain {
                    base_type: "pg_catalog.text".to_string(),
                    base_typmod: None,
                    not_null: false,
                    default: None,
                    collation: DomainCollation::Explicit(collation.clone()),
                    constraints: Vec::new(),
                },
            }),
            Err(CatalogError::UnknownCollation { id: collation })
        );
    }
}

#[test]
fn domain_constraint_names_must_be_unique() {
    let mut catalog = CatalogSnapshot::postgres_18_fixture();

    assert_eq!(
        catalog.register_type(TypeRegistration {
            qualified_name: "app.duplicate_checks".to_string(),
            kind: TypeRegistrationKind::Domain {
                base_type: "pg_catalog.bigint".to_string(),
                base_typmod: None,
                not_null: false,
                default: None,
                collation: DomainCollation::None,
                constraints: vec![
                    DomainConstraint {
                        name: "value_check".to_string(),
                        expression: "VALUE > 0".to_string(),
                    },
                    DomainConstraint {
                        name: "value_check".to_string(),
                        expression: "VALUE < 100".to_string(),
                    },
                ],
            },
        }),
        Err(CatalogError::DuplicateDomainConstraintName {
            qualified_name: "app.duplicate_checks".to_string(),
            constraint: "value_check".to_string(),
        })
    );
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
                base_typmod: None,
                not_null: false,
                default: None,
                collation: DomainCollation::None,
                constraints: Vec::new(),
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
    let scalar_facts = ScalarCallableFacts {
        volatility: Volatility::Stable,
        strict: false,
        result_nullability: Nullability::Nullable,
    };
    let scalar_id = catalog
        .register_scalar(scalar.clone(), scalar_facts)
        .unwrap();
    assert_eq!(
        scalar_id.as_str(),
        "pg18:callable:function:app.add_one(pg18:type:base:pg_catalog.bigint)"
    );
    assert_eq!(
        catalog.callable_by_id(&scalar_id).unwrap().kind,
        CallableKind::Scalar
    );
    assert_eq!(
        catalog.register_scalar(scalar, scalar_facts),
        Err(CatalogError::DuplicateCallableSignature {
            qualified_name: "app.add_one".to_string(),
            arguments: vec![bigint.clone()],
        })
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
    let table_id = catalog
        .register_table(
            table,
            TableCallableFacts {
                volatility: Volatility::Volatile,
                strict: false,
                cardinality: CallableCardinality::SetOfUnknown,
            },
        )
        .unwrap();
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
        .register_scalar(
            ScalarSignature {
                qualified_name: "app.same_inputs".to_string(),
                arguments: vec![bigint.clone()],
                result: bigint.clone(),
            },
            SCALAR_FACTS,
        )
        .unwrap();

    assert_eq!(
        catalog.register_scalar(
            ScalarSignature {
                qualified_name: "app.same_inputs".to_string(),
                arguments: vec![bigint.clone()],
                result: text.clone(),
            },
            SCALAR_FACTS,
        ),
        Err(CatalogError::DuplicateCallableSignature {
            qualified_name: "app.same_inputs".to_string(),
            arguments: vec![bigint.clone()],
        })
    );

    assert_eq!(
        catalog.register_table(
            TableSignature {
                qualified_name: "app.same_inputs".to_string(),
                arguments: vec![bigint.clone()],
                columns: vec![TableOutputColumn {
                    name: "value".to_string(),
                    type_id: text,
                    nullability: Nullability::NotNull,
                }],
            },
            TABLE_FACTS,
        ),
        Err(CatalogError::DuplicateCallableSignature {
            qualified_name: "app.same_inputs".to_string(),
            arguments: vec![bigint],
        })
    );
}

#[test]
fn return_only_callable_changes_keep_identity_and_collide_in_postgres() {
    let mut catalog = CatalogSnapshot::postgres_18_fixture();
    let bigint = catalog
        .resolve_type("pg_catalog.bigint")
        .unwrap()
        .id
        .clone();
    let text = catalog.resolve_type("pg_catalog.text").unwrap().id.clone();
    let bigint_result = ScalarSignature {
        qualified_name: "app.same_identity".to_string(),
        arguments: vec![bigint.clone()],
        result: bigint.clone(),
    };
    let text_result = ScalarSignature {
        qualified_name: "app.same_identity".to_string(),
        arguments: vec![bigint.clone()],
        result: text,
    };

    let expected_id = text_result.postgres_18_id();
    assert_eq!(bigint_result.postgres_18_id(), expected_id);
    let id = catalog
        .register_scalar(bigint_result, SCALAR_FACTS)
        .unwrap();
    assert_eq!(id, expected_id);
    assert_eq!(
        catalog.register_scalar(text_result, SCALAR_FACTS),
        Err(CatalogError::DuplicateCallableSignature {
            qualified_name: "app.same_identity".to_string(),
            arguments: vec![bigint],
        })
    );
}

#[test]
fn table_output_columns_require_canonical_unquoted_identifiers() {
    let mut catalog = CatalogSnapshot::postgres_18_fixture();
    let text = catalog.resolve_type("pg_catalog.text").unwrap().id.clone();

    assert_eq!(
        catalog.register_table(
            TableSignature {
                qualified_name: "app.bad_output".to_string(),
                arguments: Vec::new(),
                columns: vec![TableOutputColumn {
                    name: "bad output".to_string(),
                    type_id: text,
                    nullability: Nullability::NotNull,
                }],
            },
            TABLE_FACTS,
        ),
        Err(CatalogError::InvalidOutputColumnName {
            qualified_name: "app.bad_output".to_string(),
            column: "bad output".to_string(),
        })
    );
}

#[test]
fn table_output_columns_must_be_unique() {
    let mut catalog = CatalogSnapshot::postgres_18_fixture();
    let text = catalog.resolve_type("pg_catalog.text").unwrap().id.clone();

    assert_eq!(
        catalog.register_table(
            TableSignature {
                qualified_name: "app.duplicate_output".to_string(),
                arguments: Vec::new(),
                columns: vec![
                    TableOutputColumn {
                        name: "value".to_string(),
                        type_id: text.clone(),
                        nullability: Nullability::NotNull,
                    },
                    TableOutputColumn {
                        name: "value".to_string(),
                        type_id: text,
                        nullability: Nullability::Nullable,
                    },
                ],
            },
            TABLE_FACTS,
        ),
        Err(CatalogError::DuplicateOutputColumnName {
            qualified_name: "app.duplicate_output".to_string(),
            column: "value".to_string(),
        })
    );
}
#[test]
fn registration_rejects_unknown_argument_result_and_column_types() {
    let mut catalog = CatalogSnapshot::postgres_18_fixture();
    let unknown = dibs_pg_catalog::TypeId::new("pg18:type:base:app.missing");

    let error = catalog
        .register_scalar(
            ScalarSignature {
                qualified_name: "app.bad".to_string(),
                arguments: vec![unknown.clone()],
                result: unknown.clone(),
            },
            SCALAR_FACTS,
        )
        .unwrap_err();
    assert_eq!(error, CatalogError::UnknownTypeId { id: unknown });

    let text = catalog.resolve_type("pg_catalog.text").unwrap().id.clone();
    let unknown = dibs_pg_catalog::TypeId::new("pg18:type:base:app.missing_output");
    let error = catalog
        .register_table(
            TableSignature {
                qualified_name: "app.bad_table".to_string(),
                arguments: vec![text],
                columns: vec![TableOutputColumn {
                    name: "value".to_string(),
                    type_id: unknown.clone(),
                    nullability: Nullability::Nullable,
                }],
            },
            TABLE_FACTS,
        )
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
        catalog.register_scalar(
            ScalarSignature {
                qualified_name: "app bad.function".to_string(),
                arguments: Vec::new(),
                result: bigint,
            },
            SCALAR_FACTS,
        ),
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
fn pseudo_types_are_not_bindable_value_or_storage_types() {
    let mut catalog = CatalogSnapshot::postgres_18_fixture();
    let unknown = catalog
        .resolve_type("pg_catalog.unknown")
        .unwrap()
        .id
        .clone();
    let anyelement = catalog
        .resolve_type("pg_catalog.anyelement")
        .unwrap()
        .id
        .clone();

    assert!(matches!(
        catalog.resolve_api_type(ApiLanguage::Rust, "PgPseudo<unknown>"),
        Err(CatalogError::UnsupportedApiType { .. })
    ));
    assert_eq!(
        catalog.register_scalar(
            ScalarSignature {
                qualified_name: "app.bad_pseudo_result".to_string(),
                arguments: Vec::new(),
                result: unknown.clone(),
            },
            SCALAR_FACTS,
        ),
        Err(CatalogError::NonBindablePseudoType {
            id: unknown.clone(),
            position: "scalar result",
        })
    );
    assert_eq!(
        catalog.register_type(TypeRegistration {
            qualified_name: "app.bad_pseudo_domain".to_string(),
            kind: TypeRegistrationKind::Domain {
                base_type: "pg_catalog.anyelement".to_string(),
                base_typmod: None,
                not_null: false,
                default: None,
                collation: DomainCollation::None,
                constraints: Vec::new(),
            },
        }),
        Err(CatalogError::NonBindablePseudoType {
            id: anyelement.clone(),
            position: "domain base",
        })
    );
    assert_eq!(
        catalog.register_type(TypeRegistration {
            qualified_name: "app.bad_pseudo_array".to_string(),
            kind: TypeRegistrationKind::Array {
                element_type: "pg_catalog.anyelement".to_string(),
            },
        }),
        Err(CatalogError::NonBindablePseudoType {
            id: anyelement,
            position: "array element",
        })
    );
}
#[test]
fn pg18_type_resolution_facts_cover_categories_preferences_and_pseudo_types() {
    let catalog = CatalogSnapshot::postgres_18_fixture();
    let boolean = catalog.resolve_type("pg_catalog.boolean").unwrap();
    let float8 = catalog.resolve_type("pg_catalog.double precision").unwrap();
    let text = catalog.resolve_type("pg_catalog.text").unwrap();
    let unknown = catalog.resolve_type("pg_catalog.unknown").unwrap();
    let anyelement = catalog.resolve_type("pg_catalog.anyelement").unwrap();

    assert_eq!(boolean.category, PgTypeCategory::Boolean);
    assert!(boolean.preferred);
    assert_eq!(float8.category, PgTypeCategory::Numeric);
    assert!(float8.preferred);
    assert_eq!(text.category, PgTypeCategory::String);
    assert!(text.preferred);
    assert_eq!(unknown.category, PgTypeCategory::Unknown);
    assert_eq!(unknown.kind, PgTypeKind::Pseudo);
    assert_eq!(unknown.polymorphic, None);
    assert_eq!(anyelement.kind, PgTypeKind::Pseudo);
    assert_eq!(anyelement.polymorphic, Some(PolymorphicType::AnyElement));
}

#[test]
fn candidate_lookup_accepts_qualified_or_unqualified_name_and_exact_arity() {
    let catalog = CatalogSnapshot::postgres_18_fixture();
    let qualified: Vec<_> = catalog.callable_candidates("pg_catalog.abs", 1).collect();
    let unqualified: Vec<_> = catalog.callable_candidates("abs", 1).collect();
    assert_eq!(qualified, unqualified);
    assert!(
        qualified.len() > 1,
        "abs must expose PG18 numeric overloads"
    );
    assert!(catalog.callable_candidates("abs", 2).next().is_none());

    let qualified: Vec<_> = catalog.operator_candidates("pg_catalog.+", 2).collect();
    let unqualified: Vec<_> = catalog.operator_candidates("+", 2).collect();
    assert_eq!(qualified, unqualified);
    assert!(qualified.len() > 1, "+ must expose PG18 numeric overloads");
    assert!(catalog.operator_candidates("+", 1).next().is_none());
}

#[test]
fn curated_callable_and_operator_semantics_are_explicit() {
    let catalog = CatalogSnapshot::postgres_18_fixture();
    let count = catalog
        .callable_candidates("count", 0)
        .find(|callable| callable.postgres_identity_arguments.is_empty())
        .unwrap();
    assert_eq!(count.kind, CallableKind::Aggregate);
    assert_eq!(count.volatility, Volatility::Immutable);
    assert!(!count.strict);
    assert_eq!(count.scalar_result_nullability, Some(Nullability::NotNull));
    assert_eq!(count.cardinality, CallableCardinality::ExactlyOne);
    assert_eq!(
        count.aggregate_empty,
        Some(AggregateEmptyBehavior::Identity)
    );

    let text = catalog.resolve_type("pg_catalog.text").unwrap();
    assert!(catalog.operator_candidates("=", 2).any(|operator| {
        operator.left.as_ref() == Some(&text.id)
            && operator.right.as_ref() == Some(&text.id)
            && operator.strict
    }));

    let sum = catalog
        .callable_candidates("sum", 1)
        .find(|callable| callable.postgres_identity_arguments == "bigint")
        .unwrap();
    assert_eq!(sum.kind, CallableKind::Aggregate);
    assert_eq!(sum.scalar_result_nullability, Some(Nullability::Nullable));
    assert_eq!(sum.aggregate_empty, Some(AggregateEmptyBehavior::Null));

    let row_number = catalog.callable_candidates("row_number", 0).next().unwrap();
    assert_eq!(row_number.kind, CallableKind::Window);
    assert_eq!(row_number.cardinality, CallableCardinality::OnePerInput);
    assert_eq!(
        row_number.scalar_result_nullability,
        Some(Nullability::NotNull)
    );

    assert!(
        catalog
            .operator_candidates("+", 2)
            .all(|operator| operator.strict)
    );
}

#[test]
fn cast_path_is_shortest_deterministic_and_context_aware() {
    let catalog = CatalogSnapshot::postgres_18_fixture();
    let int2 = catalog.resolve_type("pg_catalog.smallint").unwrap();
    let int4 = catalog.resolve_type("pg_catalog.integer").unwrap();
    let int8 = catalog.resolve_type("pg_catalog.bigint").unwrap();
    let numeric = catalog.resolve_type("pg_catalog.numeric").unwrap();

    let empty = catalog
        .cast_path(&int4.id, &int4.id, CastContext::Implicit)
        .unwrap();
    assert!(empty.is_empty());

    let direct = catalog
        .cast_path(&int4.id, &int8.id, CastContext::Implicit)
        .unwrap();
    assert_eq!(direct.len(), 1);
    assert_eq!(direct[0].source, int4.id);
    assert_eq!(direct[0].target, int8.id);

    let transitive = catalog
        .cast_path(&int4.id, &numeric.id, CastContext::Implicit)
        .unwrap();
    assert_eq!(transitive.len(), 2);
    assert_eq!(transitive[0].source, int4.id);
    assert_eq!(transitive[0].target, int8.id);
    assert_eq!(transitive[1].source, int8.id);
    assert_eq!(transitive[1].target, numeric.id);

    let longer = catalog
        .cast_path(&int2.id, &numeric.id, CastContext::Implicit)
        .unwrap();
    assert_eq!(longer.len(), 3);
    assert!(
        longer
            .windows(2)
            .all(|edges| edges[0].target == edges[1].source)
    );

    assert!(
        catalog
            .cast_path(&int8.id, &int4.id, CastContext::Implicit)
            .is_none()
    );
    let assignment = catalog
        .cast_path(&int8.id, &int4.id, CastContext::Assignment)
        .unwrap();
    assert_eq!(assignment.len(), 1);
    assert_eq!(assignment[0].context, CastContext::Assignment);
}

#[test]
fn cast_path_uses_lexicographic_cast_id_tie_break() {
    let mut catalog = CatalogSnapshot::postgres_18_fixture();
    let int2 = catalog
        .resolve_type("pg_catalog.smallint")
        .unwrap()
        .id
        .clone();
    let int4 = catalog
        .resolve_type("pg_catalog.integer")
        .unwrap()
        .id
        .clone();
    let int8 = catalog
        .resolve_type("pg_catalog.bigint")
        .unwrap()
        .id
        .clone();
    let numeric = catalog
        .resolve_type("pg_catalog.numeric")
        .unwrap()
        .id
        .clone();
    let method = catalog.casts[0].method;
    catalog.casts.push(dibs_pg_catalog::CatalogCast {
        id: dibs_pg_catalog::CastId::new("pg18:cast:a-first-route"),
        source: int2.clone(),
        target: int8.clone(),
        context: CastContext::Implicit,
        method,
        builtin: false,
    });
    catalog.casts.push(dibs_pg_catalog::CatalogCast {
        id: dibs_pg_catalog::CastId::new("pg18:cast:z-second-route"),
        source: int4.clone(),
        target: numeric.clone(),
        context: CastContext::Implicit,
        method,
        builtin: false,
    });

    let path = catalog
        .cast_path(&int2, &numeric, CastContext::Implicit)
        .unwrap();
    assert_eq!(path.len(), 2);
    assert_eq!(path[0].id.as_str(), "pg18:cast:a-first-route");
    assert_eq!(path[0].target, int8);
}

#[test]
fn cast_by_id_resolves_the_exact_catalog_fact() {
    let catalog = CatalogSnapshot::postgres_18_fixture();
    let int4 = catalog.resolve_type("pg_catalog.integer").unwrap();
    let int8 = catalog.resolve_type("pg_catalog.bigint").unwrap();
    let path = catalog
        .cast_path(&int4.id, &int8.id, CastContext::Implicit)
        .unwrap();
    let cast = path[0];

    assert_eq!(catalog.cast_by_id(&cast.id), Some(cast));
    assert!(
        catalog
            .cast_by_id(&dibs_pg_catalog::CastId::new("pg18:cast:missing"))
            .is_none()
    );
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
