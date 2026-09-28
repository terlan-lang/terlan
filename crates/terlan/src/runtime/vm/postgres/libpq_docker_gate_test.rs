use crate::runtime::vm::postgres::docker_fixture_test::{
    DockerPostgres, POSTGRES_DATABASE, POSTGRES_PASSWORD, POSTGRES_USER,
};
use std::{collections::BTreeSet, time::Duration};

use super::test_support::{complete, config, request};
use super::*;
use crate::runtime::vm::postgres::{
    VmPostgresConnection, VmPostgresPool, VmPostgresQueryTarget, VmPostgresRow,
    VmPostgresTransaction,
};
use crate::terlan_native::postgres::libpq::DriverReadinessPoller;

fn register_pool(
    worker: &mut VmPostgresLibpqWorker,
    request_id: u64,
    url: &str,
) -> VmPostgresDriverPool {
    match complete(
        worker,
        request(request_id, VmPostgresDriverOperation::Connect(config(url))),
    ) {
        VmPostgresDriverCompletion::Connected(pool) => pool,
        completion => panic!("expected lazy pool registration, found {completion:?}"),
    }
}

fn query_one(
    worker: &mut VmPostgresLibpqWorker,
    request_id: u64,
    vm_pool: VmPostgresPool,
    driver_pool: VmPostgresDriverPool,
    sql: &str,
) -> VmPostgresDriverCompletion {
    complete(
        worker,
        request(
            request_id,
            VmPostgresDriverOperation::Query {
                target: VmPostgresQueryTarget::Pool(vm_pool),
                driver_target: VmPostgresDriverQueryTarget::Pool(driver_pool),
                sql: sql.to_string(),
                parameters: Vec::new(),
                one: true,
            },
        ),
    )
}

fn expect_failure(
    completion: VmPostgresDriverCompletion,
    expected_code: &str,
) -> VmPostgresFailure {
    let VmPostgresDriverCompletion::Failed(failure) = completion else {
        panic!("expected Postgres failure, found {completion:?}");
    };
    assert_eq!(failure.code, expected_code);
    failure
}

#[test]
#[ignore = "requires a local Docker daemon"]
fn libpq_docker_gate_validates_success_failure_cancellation_and_cleanup() {
    let fixture = DockerPostgres::start().expect("start Docker Postgres fixture");
    let valid_url = fixture.url(POSTGRES_PASSWORD);
    let vm_pool = VmPostgresPool(100);

    let mut worker = VmPostgresLibpqWorker::default();
    let driver_pool = register_pool(&mut worker, 1, &valid_url);
    assert_eq!(
        complete(
            &mut worker,
            request(
                20,
                VmPostgresDriverOperation::BatchExecute {
                    target: VmPostgresQueryTarget::Pool(vm_pool),
                    driver_target: VmPostgresDriverQueryTarget::Pool(driver_pool),
                    sql: "CREATE TEMP TABLE terlan_batch_probe (value bigint); INSERT INTO terlan_batch_probe VALUES (20), (22);".to_string(),
                },
            ),
        ),
        VmPostgresDriverCompletion::Unit
    );
    let batch_rows = match query_one(
        &mut worker,
        21,
        vm_pool,
        driver_pool,
        "SELECT sum(value)::bigint AS value FROM terlan_batch_probe",
    ) {
        VmPostgresDriverCompletion::Rows { rows } => rows,
        completion => panic!("expected batch probe rows, found {completion:?}"),
    };
    assert_eq!(
        complete(
            &mut worker,
            request(
                22,
                VmPostgresDriverOperation::Decode {
                    row: VmPostgresRow(102),
                    driver_row: batch_rows[0],
                    column: "value".to_string(),
                    expected: VmPostgresDecodeType::Int,
                },
            ),
        ),
        VmPostgresDriverCompletion::Decoded(VmPostgresDecodedValue::Int(42))
    );
    let rows = match query_one(
        &mut worker,
        2,
        vm_pool,
        driver_pool,
        "SELECT 42 AS value, '{\"role\":\"admin\"}'::jsonb AS payload, NULL::text AS nickname",
    ) {
        VmPostgresDriverCompletion::Rows { rows } => rows,
        completion => panic!("expected Docker query rows, found {completion:?}"),
    };
    assert_eq!(rows.len(), 1);
    assert_eq!(
        complete(
            &mut worker,
            request(
                3,
                VmPostgresDriverOperation::Decode {
                    row: VmPostgresRow(101),
                    driver_row: rows[0],
                    column: "value".to_string(),
                    expected: VmPostgresDecodeType::Int,
                },
            ),
        ),
        VmPostgresDriverCompletion::Decoded(VmPostgresDecodedValue::Int(42))
    );
    for (request_id, column, expected) in [
        (30, "value", VmPostgresDecodedValue::Int(42)),
        (
            31,
            "payload",
            VmPostgresDecodedValue::Json("{\"role\":\"admin\"}".to_string()),
        ),
        (32, "nickname", VmPostgresDecodedValue::Null),
    ] {
        assert_eq!(
            complete(
                &mut worker,
                request(
                    request_id,
                    VmPostgresDriverOperation::Decode {
                        row: VmPostgresRow(101),
                        driver_row: rows[0],
                        column: column.to_string(),
                        expected: VmPostgresDecodeType::Dynamic,
                    },
                ),
            ),
            VmPostgresDriverCompletion::Decoded(expected)
        );
    }

    for request_id in [40, 41] {
        worker.submit(request(
            request_id,
            VmPostgresDriverOperation::Query {
                target: VmPostgresQueryTarget::Pool(vm_pool),
                driver_target: VmPostgresDriverQueryTarget::Pool(driver_pool),
                sql: format!("SELECT {request_id}::bigint AS value FROM pg_sleep(0.1)"),
                parameters: Vec::new(),
                one: true,
            },
        ));
    }
    let mut parallel_completions = Vec::new();
    let mut max_active = 0;
    let poller = DriverReadinessPoller::new().expect("create Postgres readiness poller");
    let mut ready = BTreeSet::new();
    for _ in 0..10_000 {
        if let Some((request_id, completion)) = worker.drive_socket_ready(&ready) {
            assert!(matches!(
                completion,
                VmPostgresDriverCompletion::Rows { .. }
            ));
            parallel_completions.push(request_id.value);
        }
        ready.clear();
        max_active = max_active.max(worker.active.len());
        if parallel_completions.len() == 2 {
            break;
        }
        if worker.completions.is_empty() && !worker.waits().is_empty() {
            ready = worker
                .wait_ready(&poller, Some(Duration::from_secs(5)))
                .expect("wait for independent query readiness");
        }
    }
    parallel_completions.sort_unstable();
    assert_eq!(parallel_completions, [40, 41]);
    assert!(
        max_active >= 2,
        "independent pool queries never overlapped in the libpq worker"
    );

    let driver_connection = match complete(
        &mut worker,
        request(
            50,
            VmPostgresDriverOperation::Acquire {
                pool: vm_pool,
                driver_pool,
            },
        ),
    ) {
        VmPostgresDriverCompletion::Acquired(connection) => connection,
        completion => panic!("expected transaction connection, found {completion:?}"),
    };
    let driver_transaction = match complete(
        &mut worker,
        request(
            51,
            VmPostgresDriverOperation::Begin {
                connection: VmPostgresConnection(200),
                driver_connection,
            },
        ),
    ) {
        VmPostgresDriverCompletion::TransactionStarted(transaction) => transaction,
        completion => panic!("expected transaction start, found {completion:?}"),
    };
    for request_id in [52, 53] {
        worker.submit(request(
            request_id,
            VmPostgresDriverOperation::Query {
                target: VmPostgresQueryTarget::Transaction(VmPostgresTransaction(201)),
                driver_target: VmPostgresDriverQueryTarget::Transaction(driver_transaction),
                sql: format!("SELECT {request_id}::bigint AS value FROM pg_sleep(0.05)"),
                parameters: Vec::new(),
                one: true,
            },
        ));
    }
    let mut transaction_completions = Vec::new();
    let mut max_transaction_active = 0;
    let mut ready = BTreeSet::new();
    for _ in 0..10_000 {
        if let Some((request_id, completion)) = worker.drive_socket_ready(&ready) {
            assert!(matches!(
                completion,
                VmPostgresDriverCompletion::Rows { .. }
            ));
            transaction_completions.push(request_id.value);
        }
        ready.clear();
        max_transaction_active = max_transaction_active.max(worker.active.len());
        if transaction_completions.len() == 2 {
            break;
        }
        if worker.completions.is_empty() && !worker.waits().is_empty() {
            ready = worker
                .wait_ready(&poller, Some(Duration::from_secs(5)))
                .expect("wait for transaction query readiness");
        }
    }
    assert_eq!(transaction_completions, [52, 53]);
    assert_eq!(
        max_transaction_active, 1,
        "requests sharing a transaction must remain serialized"
    );
    assert_eq!(
        complete(
            &mut worker,
            request(
                54,
                VmPostgresDriverOperation::Commit {
                    transaction: VmPostgresTransaction(201),
                    driver_transaction,
                },
            ),
        ),
        VmPostgresDriverCompletion::Unit
    );
    worker
        .apply_control(VmPostgresDriverControl::Release {
            connection: VmPostgresConnection(200),
            driver_connection,
        })
        .expect("release transaction connection");

    worker.submit(request(
        4,
        VmPostgresDriverOperation::Query {
            target: VmPostgresQueryTarget::Pool(vm_pool),
            driver_target: VmPostgresDriverQueryTarget::Pool(driver_pool),
            sql: "SELECT pg_sleep(10)".to_string(),
            parameters: Vec::new(),
            one: true,
        },
    ));
    let mut ready = BTreeSet::new();
    for _ in 0..1_000 {
        assert!(worker.drive_socket_ready(&ready).is_none());
        ready.clear();
        if matches!(
            worker.wait(),
            Some(VmPostgresDriverWait {
                interest: VmPostgresIoInterest::Read,
                ..
            })
        ) {
            break;
        }
        if !worker.waits().is_empty() {
            ready = worker
                .wait_ready(&poller, Some(Duration::from_secs(5)))
                .expect("wait for cancellable query readiness");
        }
    }
    worker
        .apply_control(VmPostgresDriverControl::Cancel(RequestId { value: 4 }))
        .expect("cancel active Docker query");
    assert!(worker.wait().is_none());

    worker
        .apply_control(VmPostgresDriverControl::ClosePool {
            pool: vm_pool,
            driver_pool,
        })
        .expect("close Docker pool");
    let stale = expect_failure(
        query_one(&mut worker, 5, vm_pool, driver_pool, "SELECT 1"),
        "postgres.driver.stale_resource",
    );
    assert!(!stale.message.contains(&valid_url));

    let invalid_password = "terlan-invalid-password-never-log";
    let invalid_url = fixture.url(invalid_password);
    let mut invalid_worker = VmPostgresLibpqWorker::default();
    let invalid_pool = register_pool(&mut invalid_worker, 10, &invalid_url);
    let invalid = expect_failure(
        query_one(
            &mut invalid_worker,
            11,
            VmPostgresPool(110),
            invalid_pool,
            "SELECT 1",
        ),
        "postgres.connect",
    );
    assert!(!invalid.message.contains(invalid_password));
    assert!(!invalid.message.contains(&invalid_url));

    let unreachable_url =
        format!("postgres://{POSTGRES_USER}:{POSTGRES_PASSWORD}@127.0.0.1:1/{POSTGRES_DATABASE}");
    let mut unreachable_worker = VmPostgresLibpqWorker::default();
    let unreachable_pool = register_pool(&mut unreachable_worker, 20, &unreachable_url);
    let unreachable = expect_failure(
        query_one(
            &mut unreachable_worker,
            21,
            VmPostgresPool(120),
            unreachable_pool,
            "SELECT 1",
        ),
        "postgres.connect",
    );
    assert!(!unreachable.message.contains(POSTGRES_PASSWORD));
    assert!(!unreachable.message.contains(&unreachable_url));
}
