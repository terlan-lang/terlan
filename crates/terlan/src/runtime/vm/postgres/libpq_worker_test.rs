use std::{thread, time::Duration};

use super::test_support::{complete, config, request};
use super::*;
use crate::{
    runtime::vm::postgres::{
        VmPostgresConnection, VmPostgresPool, VmPostgresQueryTarget, VmPostgresRow,
        VmPostgresTransaction,
    },
    terlan_native::json::Json,
};

#[test]
fn libpq_worker_registers_pool_without_opening_socket_or_exposing_credentials() {
    let mut worker = VmPostgresLibpqWorker::default();
    let completion = complete(
        &mut worker,
        request(
            1,
            VmPostgresDriverOperation::Connect(config(
                "postgres://alice:secret@127.0.0.1:9/database",
            )),
        ),
    );
    assert!(matches!(
        completion,
        VmPostgresDriverCompletion::Connected(_)
    ));
    let debug = format!("{worker:?}");
    assert!(!debug.contains("alice"));
    assert!(!debug.contains("secret"));
    assert!(worker.wait().is_none());
}

#[test]
fn libpq_worker_live_roundtrip_validates_generated_c_abi_lifecycle() {
    let Ok(url) = std::env::var("TERLAN_TEST_POSTGRES_URL") else {
        return;
    };
    let mut worker = VmPostgresLibpqWorker::default();
    let driver_pool = match complete(
        &mut worker,
        request(1, VmPostgresDriverOperation::Connect(config(&url))),
    ) {
        VmPostgresDriverCompletion::Connected(pool) => pool,
        completion => panic!("expected connected pool, found {completion:?}"),
    };

    let rows = match complete(
        &mut worker,
        request(
            2,
            VmPostgresDriverOperation::Query {
                target: VmPostgresQueryTarget::Pool(VmPostgresPool(10)),
                driver_target: VmPostgresDriverQueryTarget::Pool(driver_pool),
                sql: "SELECT $1::bigint AS id, $2::text AS name, $3::jsonb AS payload, NULL::text AS nickname".to_string(),
                parameters: vec![
                    Json::from_serde(serde_json::json!(42)),
                    Json::from_serde(serde_json::json!("Ada")),
                    Json::from_serde(serde_json::json!({"role": "admin"})),
                ],
                one: true,
            },
        ),
    ) {
        VmPostgresDriverCompletion::Rows { rows } => rows,
        completion => panic!("expected rows, found {completion:?}"),
    };
    assert_eq!(rows.len(), 1);
    let driver_row = rows[0];
    assert_eq!(
        complete(
            &mut worker,
            request(
                3,
                VmPostgresDriverOperation::Decode {
                    row: VmPostgresRow(11),
                    driver_row,
                    column: "id".to_string(),
                    expected: VmPostgresDecodeType::Int,
                },
            ),
        ),
        VmPostgresDriverCompletion::Decoded(VmPostgresDecodedValue::Int(42))
    );
    assert_eq!(
        complete(
            &mut worker,
            request(
                4,
                VmPostgresDriverOperation::Decode {
                    row: VmPostgresRow(11),
                    driver_row,
                    column: "name".to_string(),
                    expected: VmPostgresDecodeType::String,
                },
            ),
        ),
        VmPostgresDriverCompletion::Decoded(VmPostgresDecodedValue::String("Ada".to_string()))
    );
    assert_eq!(
        complete(
            &mut worker,
            request(
                50,
                VmPostgresDriverOperation::Decode {
                    row: VmPostgresRow(11),
                    driver_row,
                    column: "id".to_string(),
                    expected: VmPostgresDecodeType::Dynamic,
                },
            ),
        ),
        VmPostgresDriverCompletion::Decoded(VmPostgresDecodedValue::Int(42))
    );
    assert_eq!(
        complete(
            &mut worker,
            request(
                51,
                VmPostgresDriverOperation::Decode {
                    row: VmPostgresRow(11),
                    driver_row,
                    column: "payload".to_string(),
                    expected: VmPostgresDecodeType::Dynamic,
                },
            ),
        ),
        VmPostgresDriverCompletion::Decoded(VmPostgresDecodedValue::Json(
            "{\"role\":\"admin\"}".to_string()
        ))
    );
    assert_eq!(
        complete(
            &mut worker,
            request(
                52,
                VmPostgresDriverOperation::Decode {
                    row: VmPostgresRow(11),
                    driver_row,
                    column: "nickname".to_string(),
                    expected: VmPostgresDecodeType::Dynamic,
                },
            ),
        ),
        VmPostgresDriverCompletion::Decoded(VmPostgresDecodedValue::Null)
    );

    let driver_connection = match complete(
        &mut worker,
        request(
            5,
            VmPostgresDriverOperation::Acquire {
                pool: VmPostgresPool(10),
                driver_pool,
            },
        ),
    ) {
        VmPostgresDriverCompletion::Acquired(connection) => connection,
        completion => panic!("expected connection, found {completion:?}"),
    };
    let driver_transaction = match complete(
        &mut worker,
        request(
            6,
            VmPostgresDriverOperation::Begin {
                connection: VmPostgresConnection(12),
                driver_connection,
            },
        ),
    ) {
        VmPostgresDriverCompletion::TransactionStarted(transaction) => transaction,
        completion => panic!("expected transaction, found {completion:?}"),
    };
    assert_eq!(
        complete(
            &mut worker,
            request(
                7,
                VmPostgresDriverOperation::Execute {
                    target: VmPostgresQueryTarget::Transaction(VmPostgresTransaction(13)),
                    driver_target: VmPostgresDriverQueryTarget::Transaction(driver_transaction),
                    sql: "CREATE TEMP TABLE terlan_c_abi_probe (id bigint)".to_string(),
                    parameters: Vec::new(),
                },
            ),
        ),
        VmPostgresDriverCompletion::AffectedRows(0)
    );
    assert_eq!(
        complete(
            &mut worker,
            request(
                8,
                VmPostgresDriverOperation::Commit {
                    transaction: VmPostgresTransaction(13),
                    driver_transaction,
                },
            ),
        ),
        VmPostgresDriverCompletion::Unit
    );

    worker.submit(request(
        9,
        VmPostgresDriverOperation::Query {
            target: VmPostgresQueryTarget::Pool(VmPostgresPool(10)),
            driver_target: VmPostgresDriverQueryTarget::Pool(driver_pool),
            sql: "SELECT pg_sleep(10)".to_string(),
            parameters: Vec::new(),
            one: true,
        },
    ));
    for _ in 0..1_000 {
        assert!(worker.drive_once().is_none());
        if matches!(
            worker.active.values().next().map(|pending| &pending.phase),
            Some(PendingPhase::Reading)
        ) {
            break;
        }
        thread::sleep(Duration::from_millis(1));
    }
    worker
        .apply_control(VmPostgresDriverControl::Cancel(RequestId { value: 9 }))
        .expect("active libpq request cancels by closing its connection");
    assert!(worker.active.is_empty());
    assert!(worker.wait().is_none());
}

#[test]
fn libpq_worker_live_invalid_credentials_return_redacted_typed_failure() {
    let Ok(valid_url) = std::env::var("TERLAN_TEST_POSTGRES_URL") else {
        return;
    };
    let invalid_password = "terlan-deliberately-invalid-password";
    let mut invalid_url = url::Url::parse(&valid_url).expect("valid live Postgres fixture URL");
    invalid_url
        .set_password(Some(invalid_password))
        .expect("Postgres URL accepts password credentials");
    let invalid_url = invalid_url.to_string();

    let mut worker = VmPostgresLibpqWorker::default();
    let driver_pool = match complete(
        &mut worker,
        request(20, VmPostgresDriverOperation::Connect(config(&invalid_url))),
    ) {
        VmPostgresDriverCompletion::Connected(pool) => pool,
        completion => panic!("expected lazy pool registration, found {completion:?}"),
    };
    let completion = complete(
        &mut worker,
        request(
            21,
            VmPostgresDriverOperation::Query {
                target: VmPostgresQueryTarget::Pool(VmPostgresPool(20)),
                driver_target: VmPostgresDriverQueryTarget::Pool(driver_pool),
                sql: "SELECT 1".to_string(),
                parameters: Vec::new(),
                one: true,
            },
        ),
    );

    let VmPostgresDriverCompletion::Failed(failure) = &completion else {
        panic!("invalid credentials unexpectedly completed: {completion:?}");
    };
    assert_eq!(failure.code, "postgres.connect");
    let diagnostics = format!("{completion:?}\n{worker:?}");
    assert!(!diagnostics.contains(invalid_password));
    assert!(!diagnostics.contains(&invalid_url));
    assert!(!diagnostics.contains(&valid_url));
    assert!(worker.wait().is_none());
}
