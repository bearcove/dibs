use dibs_pg_catalog::ApiTypeId;
use dibs_query_ir::{
    ApiOperationName, ParameterApiContract, ParameterBindAdapter, ParameterPassing, TargetLanguage,
};

fn rust_contract(
    name: &str,
    api_type: &str,
    passing: ParameterPassing,
    bind_adapter: ParameterBindAdapter,
) -> ParameterApiContract {
    ParameterApiContract {
        language: TargetLanguage::Rust,
        name: name.to_string(),
        api_type: ApiTypeId::new(api_type),
        passing,
        bind_adapter,
    }
}

#[test]
fn operation_and_bind_contracts_round_trip_with_facet_json() {
    let operation = ApiOperationName::try_new(TargetLanguage::Rust, "find_job").unwrap();
    let contracts = vec![
        rust_contract(
            "payload",
            "JobPayload",
            ParameterPassing::SharedReference,
            ParameterBindAdapter::FacetJsonb,
        ),
        rust_contract(
            "tags",
            "Vec<String>",
            ParameterPassing::SharedReference,
            ParameterBindAdapter::PgArray,
        ),
        rust_contract(
            "bytes",
            "Vec<u8>",
            ParameterPassing::ByteSlice,
            ParameterBindAdapter::Direct,
        ),
        rust_contract(
            "owner",
            "String",
            ParameterPassing::StringSlice,
            ParameterBindAdapter::Direct,
        ),
        rust_contract(
            "id",
            "i64",
            ParameterPassing::SharedReference,
            ParameterBindAdapter::Direct,
        ),
        rust_contract(
            "model",
            "TenantModel",
            ParameterPassing::SharedReference,
            ParameterBindAdapter::Named(ApiTypeId::new("TenantModelBind")),
        ),
    ];

    let operation_json = facet_json::to_string(&operation).unwrap();
    let decoded_operation: ApiOperationName = facet_json::from_str(&operation_json).unwrap();
    assert_eq!(decoded_operation, operation);

    let contracts_json = facet_json::to_string(&contracts).unwrap();
    let decoded_contracts: Vec<ParameterApiContract> =
        facet_json::from_str(&contracts_json).unwrap();
    assert_eq!(decoded_contracts, contracts);
}

#[test]
fn target_owned_names_reject_invalid_identifiers() {
    assert!(ApiOperationName::try_new(TargetLanguage::Rust, "Find-Job").is_err());
    assert!(ApiOperationName::try_new(TargetLanguage::Rust, "async").is_err());
    assert!(
        ParameterApiContract::try_new(
            TargetLanguage::Rust,
            "type",
            ApiTypeId::new("i64"),
            ParameterPassing::SharedReference,
            ParameterBindAdapter::Direct,
        )
        .is_err()
    );
    let invalid_operation_json = r#"{"language":"Rust","name":"async"}"#;
    assert!(facet_json::from_str::<ApiOperationName>(invalid_operation_json).is_err());
}

#[test]
fn bind_contract_rejects_incoherent_adapter_and_passing_pairs() {
    assert!(
        ParameterApiContract::try_new(
            TargetLanguage::Rust,
            "payload",
            ApiTypeId::new("JobPayload"),
            ParameterPassing::Owned,
            ParameterBindAdapter::FacetJsonb,
        )
        .is_err()
    );
    assert!(
        ParameterApiContract::try_new(
            TargetLanguage::Rust,
            "tags",
            ApiTypeId::new("Vec<String>"),
            ParameterPassing::StringSlice,
            ParameterBindAdapter::PgArray,
        )
        .is_err()
    );
    let incoherent_contract_json = r#"{"language":"Rust","name":"payload","api_type":"JobPayload","passing":"Owned","bind_adapter":"FacetJsonb"}"#;
    assert!(facet_json::from_str::<ParameterApiContract>(incoherent_contract_json).is_err());
}
