use dibs_pg_catalog::{CatalogSnapshot, CollationId, TypeId};
use dibs_query_ir::{CatalogRenderName, CatalogRenderNames};

#[test]
fn reviewed_postgres_18_catalog_builds_canonical_render_vocabulary() {
    let catalog = CatalogSnapshot::postgres_18_fixture();

    let render_names = CatalogRenderNames::from_catalog(&catalog).unwrap();
    let bigint = catalog.resolve_type("pg_catalog.bigint").unwrap();
    assert_eq!(
        render_names.type_name(&bigint.id).unwrap(),
        &["pg_catalog".to_string(), "int8".to_string()]
    );
    assert_eq!(
        render_names
            .collation(&CollationId::new("pg18:collation:pg_catalog.default",))
            .unwrap(),
        &["pg_catalog".to_string(), "default".to_string()]
    );
    assert!(render_names.entries().iter().all(|entry| match entry {
        CatalogRenderName::Index { id, name } => {
            !id.as_str().is_empty() && !name.is_empty()
        }
        _ => true,
    }));
}

#[test]
fn catalog_render_vocabulary_is_deterministic_across_catalog_ordering() {
    let catalog = CatalogSnapshot::postgres_18_fixture();
    let expected = CatalogRenderNames::from_catalog(&catalog).unwrap();
    let mut reordered = catalog.clone();
    reordered.types.reverse();
    reordered.callables.reverse();
    reordered.operators.reverse();
    reordered.collations.reverse();
    reordered.tables.reverse();

    let actual = CatalogRenderNames::from_catalog(&reordered).unwrap();

    assert_eq!(expected, actual);
    assert_eq!(
        facet_json::to_string(&expected).unwrap(),
        facet_json::to_string(&actual).unwrap()
    );
}

#[test]
fn catalog_render_vocabulary_rejects_duplicate_stable_identities() {
    assert!(
        CatalogRenderNames::try_new(vec![
            CatalogRenderName::Type {
                id: TypeId::new("pg18:type:pg_catalog.int8"),
                qualified_name: vec!["pg_catalog".to_string(), "int8".to_string()],
            },
            CatalogRenderName::Type {
                id: TypeId::new("pg18:type:pg_catalog.int8"),
                qualified_name: vec!["pg_catalog".to_string(), "bigint".to_string()],
            },
        ])
        .is_err()
    );
}

#[test]
fn catalog_render_vocabulary_invariant_applies_during_decode() {
    let invalid = r#"{"entries":[{"Table":{"id":"pg18:table:public.job","qualified_name":[]}}]}"#;
    assert!(facet_json::from_str::<CatalogRenderNames>(invalid).is_err());
}
