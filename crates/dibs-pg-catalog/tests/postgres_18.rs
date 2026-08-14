use dibs_pg_catalog::{CallableKind, CatalogSnapshot, PgTypeCategory, PgTypeKind, Volatility};
use dockside::{Container, containers};
use std::collections::BTreeSet;
use std::time::Duration;
use tokio_postgres::{Client, NoTls};

struct OracleGuard(Option<Container>);

async fn setup_postgres() -> (OracleGuard, Client) {
    if let Ok(connection_string) = std::env::var("DIBS_PG18_ORACLE_URL") {
        let (client, connection) = tokio_postgres::connect(&connection_string, NoTls)
            .await
            .expect("connect to DIBS_PG18_ORACLE_URL");
        tokio::spawn(async move {
            if let Err(error) = connection.await {
                tracing::error!(%error, "external PostgreSQL 18 oracle connection failed");
            }
        });
        return (OracleGuard(None), client);
    }

    let container = Container::run(containers::postgres("18-alpine", "test"))
        .expect("failed to start PostgreSQL 18");
    container
        .wait_for_log(
            "database system is ready to accept connections",
            Duration::from_secs(30),
        )
        .expect("PostgreSQL 18 did not become ready");
    let port = container
        .wait_for_port(5432, Duration::from_secs(10))
        .expect("PostgreSQL 18 port did not become ready");
    let connection_string = format!("host=127.0.0.1 port={port} user=postgres password=test");

    let (client, connection) = {
        let mut last_error = None;
        let mut connected = None;
        for _ in 0..10 {
            match tokio_postgres::connect(&connection_string, NoTls).await {
                Ok(value) => {
                    connected = Some(value);
                    break;
                }
                Err(error) => {
                    last_error = Some(error);
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            }
        }
        connected.unwrap_or_else(|| {
            panic!(
                "failed to connect to PostgreSQL 18: {}",
                last_error.expect("at least one connection attempt")
            )
        })
    };

    tokio::spawn(async move {
        if let Err(error) = connection.await {
            tracing::error!(%error, "Dockside PostgreSQL 18 oracle connection failed");
        }
    });

    (OracleGuard(Some(container)), client)
}

fn require_postgres_18(server_version_num: i32) -> Result<(), String> {
    if (180_000..190_000).contains(&server_version_num) {
        Ok(())
    } else {
        Err(format!(
            "PostgreSQL 18 required, server_version_num was {server_version_num}"
        ))
    }
}

#[test]
fn server_version_gate_accepts_only_postgres_18() {
    assert_eq!(require_postgres_18(180_000), Ok(()));
    assert_eq!(require_postgres_18(189_999), Ok(()));
    assert!(require_postgres_18(179_999).is_err());
    assert!(require_postgres_18(190_000).is_err());
}

#[tokio::test]
async fn curated_postgres_18_catalog_matches_live_stable_signatures() {
    let (oracle_guard, client) = setup_postgres().await;
    let _ = &oracle_guard.0;
    let server_version_num: i32 = client
        .query_one("SHOW server_version_num", &[])
        .await
        .expect("query server_version_num")
        .get::<_, String>(0)
        .parse()
        .expect("server_version_num is an integer");
    require_postgres_18(server_version_num).expect("live oracle must be PostgreSQL 18");
    let catalog = CatalogSnapshot::postgres_18_fixture();

    let type_rows = client
        .query(
            r#"
            SELECT
                n.nspname,
                t.typname,
                t.typtype::text,
                t.typcategory::text,
                t.typispreferred,
                COALESCE(en.nspname || '.' || e.typname, ''),
                COALESCE(bn.nspname || '.' || b.typname, '')
            FROM pg_type t
            JOIN pg_namespace n ON n.oid = t.typnamespace
            LEFT JOIN pg_type e ON e.oid = t.typelem AND t.typelem <> 0
            LEFT JOIN pg_namespace en ON en.oid = e.typnamespace
            LEFT JOIN pg_type b ON b.oid = t.typbasetype AND t.typbasetype <> 0
            LEFT JOIN pg_namespace bn ON bn.oid = b.typnamespace
            WHERE n.nspname = 'pg_catalog'
              AND t.typname = ANY($1)
            ORDER BY n.nspname, t.typname
            "#,
            &[&catalog.live_type_internal_names()],
        )
        .await
        .expect("query pg_type");

    let live_types: BTreeSet<_> = type_rows
        .into_iter()
        .map(|row| {
            (
                format!("{}.{}", row.get::<_, String>(0), row.get::<_, String>(1)),
                row.get::<_, String>(2),
                row.get::<_, String>(3),
                row.get::<_, bool>(4),
                row.get::<_, String>(5),
                row.get::<_, String>(6),
            )
        })
        .collect();
    let expected_types: BTreeSet<_> = catalog
        .builtin_types()
        .map(|ty| {
            (
                ty.internal_qualified_name.clone(),
                match ty.kind {
                    PgTypeKind::Base | PgTypeKind::Array => "b".to_string(),
                    PgTypeKind::Domain => "d".to_string(),
                    PgTypeKind::Enum => "e".to_string(),
                    PgTypeKind::Pseudo => "p".to_string(),
                },
                match ty.category {
                    PgTypeCategory::Array => "A",
                    PgTypeCategory::Boolean => "B",
                    PgTypeCategory::DateTime => "D",
                    PgTypeCategory::Numeric => "N",
                    PgTypeCategory::Pseudo => "P",
                    PgTypeCategory::String => "S",
                    PgTypeCategory::Timespan => "T",
                    PgTypeCategory::Unknown => "X",
                    PgTypeCategory::UserDefined => "U",
                }
                .to_string(),
                ty.preferred,
                ty.element_type
                    .as_ref()
                    .and_then(|id| catalog.type_by_id(id))
                    .map(|element| element.internal_qualified_name.clone())
                    .unwrap_or_default(),
                ty.domain
                    .as_ref()
                    .and_then(|domain| catalog.type_by_id(&domain.base_type))
                    .map(|base| base.internal_qualified_name.clone())
                    .unwrap_or_default(),
            )
        })
        .collect();
    assert_eq!(live_types, expected_types);

    let proc_rows = client
        .query(
            r#"
            SELECT
                n.nspname,
                p.proname,
                p.prokind::text,
                p.proretset,
                p.provolatile::text,
                p.proisstrict,
                pg_catalog.pg_get_function_identity_arguments(p.oid),
                pg_catalog.format_type(p.prorettype, NULL)
            FROM pg_proc p
            JOIN pg_namespace n ON n.oid = p.pronamespace
            WHERE n.nspname = 'pg_catalog'
              AND p.proname = ANY($1)
            ORDER BY n.nspname, p.proname, pg_catalog.pg_get_function_identity_arguments(p.oid)
            "#,
            &[&catalog.live_callable_names()],
        )
        .await
        .expect("query pg_proc");

    for callable in catalog.builtin_callables() {
        let matching: Vec<_> = proc_rows
            .iter()
            .filter(|row| {
                format!("{}.{}", row.get::<_, String>(0), row.get::<_, String>(1))
                    == callable.qualified_name
                    && row.get::<_, String>(6) == callable.postgres_identity_arguments
            })
            .collect();
        assert_eq!(
            matching.len(),
            1,
            "expected exactly one live pg_proc row for {}({})",
            callable.qualified_name,
            callable.postgres_identity_arguments
        );
        let row = matching[0];
        assert_eq!(
            row.get::<_, String>(2),
            match callable.kind {
                CallableKind::Scalar | CallableKind::Table => "f",
                CallableKind::Aggregate => "a",
                CallableKind::Window => "w",
            }
        );
        assert_eq!(row.get::<_, bool>(3), callable.kind == CallableKind::Table);
        assert_eq!(
            row.get::<_, String>(4),
            match callable.volatility {
                Volatility::Immutable => "i",
                Volatility::Stable => "s",
                Volatility::Volatile => "v",
            }
        );
        assert_eq!(row.get::<_, bool>(5), callable.strict);
        assert_eq!(
            row.get::<_, String>(7),
            callable.postgres_result_type,
            "result type mismatch for {}({})",
            callable.qualified_name,
            callable.postgres_identity_arguments
        );
    }

    let operator_rows = client
        .query(
            r#"
            SELECT
                n.nspname,
                o.oprname,
                COALESCE(pg_catalog.format_type(o.oprleft, NULL), ''),
                COALESCE(pg_catalog.format_type(o.oprright, NULL), ''),
                pg_catalog.format_type(o.oprresult, NULL)
            FROM pg_operator o
            JOIN pg_namespace n ON n.oid = o.oprnamespace
            WHERE n.nspname = 'pg_catalog'
              AND o.oprname = ANY($1)
            ORDER BY n.nspname, o.oprname, o.oprleft, o.oprright
            "#,
            &[&catalog.live_operator_names()],
        )
        .await
        .expect("query pg_operator");

    let live_operators: BTreeSet<_> = operator_rows
        .into_iter()
        .map(|row| {
            (
                format!("{}.{}", row.get::<_, String>(0), row.get::<_, String>(1)),
                row.get::<_, String>(2),
                row.get::<_, String>(3),
                row.get::<_, String>(4),
            )
        })
        .collect();
    for operator in catalog.builtin_operators() {
        let signature = operator
            .live_signature(&catalog)
            .expect("curated operator references registered types");
        assert_eq!(
            live_operators
                .iter()
                .filter(|live| **live == signature)
                .count(),
            1,
            "expected exactly one live pg_operator row for {signature:?}"
        );
    }

    let cast_rows = client
        .query(
            r#"
            SELECT
                pg_catalog.format_type(c.castsource, NULL),
                pg_catalog.format_type(c.casttarget, NULL),
                c.castcontext::text,
                c.castmethod::text
            FROM pg_cast c
            WHERE pg_catalog.format_type(c.castsource, NULL) = ANY($1)
              AND pg_catalog.format_type(c.casttarget, NULL) = ANY($2)
            ORDER BY c.castsource, c.casttarget
            "#,
            &[
                &catalog.live_cast_source_names(),
                &catalog.live_cast_target_names(),
            ],
        )
        .await
        .expect("query pg_cast");

    let live_casts: BTreeSet<_> = cast_rows
        .into_iter()
        .map(|row| {
            (
                row.get::<_, String>(0),
                row.get::<_, String>(1),
                row.get::<_, String>(2),
                row.get::<_, String>(3),
            )
        })
        .collect();
    for cast in catalog.builtin_casts() {
        let signature = cast
            .live_signature(&catalog)
            .expect("curated cast references registered types");
        assert_eq!(
            live_casts.iter().filter(|live| **live == signature).count(),
            1,
            "expected exactly one live pg_cast row for {signature:?}"
        );
    }

    for coercion in catalog.builtin_io_coercions() {
        let (source, target) = coercion
            .live_signature(&catalog)
            .expect("I/O coercion references registered types");
        let sql = format!("SELECT $1::{source}::{target}");
        client.prepare(&sql).await.unwrap_or_else(|error| {
            panic!("PostgreSQL rejected explicit I/O coercion {sql}: {error}")
        });
    }
}
