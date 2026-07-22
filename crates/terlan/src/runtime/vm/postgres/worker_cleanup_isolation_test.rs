use super::*;

fn open_driver_transaction(
    worker: &mut VmPostgresDriverWorker<FixtureBackend>,
    request_id: u64,
) -> (
    VmPostgresDriverPool,
    VmPostgresDriverConnection,
    VmPostgresDriverTransaction,
) {
    let pool = match worker.execute(request(
        request_id,
        VmPostgresDriverOperation::Connect(config()),
    )) {
        VmPostgresDriverCompletion::Connected(pool) => pool,
        completion => panic!("expected pool completion, found {completion:?}"),
    };
    let connection = match worker.execute(request(
        request_id + 1,
        VmPostgresDriverOperation::Acquire {
            pool: VmPostgresPool(request_id),
            driver_pool: pool,
        },
    )) {
        VmPostgresDriverCompletion::Acquired(connection) => connection,
        completion => panic!("expected connection completion, found {completion:?}"),
    };
    let transaction = match worker.execute(request(
        request_id + 2,
        VmPostgresDriverOperation::Begin {
            connection: VmPostgresConnection(request_id + 1),
            driver_connection: connection,
        },
    )) {
        VmPostgresDriverCompletion::TransactionStarted(transaction) => transaction,
        completion => panic!("expected transaction completion, found {completion:?}"),
    };
    (pool, connection, transaction)
}

#[test]
fn failed_cleanup_rollback_drops_only_its_owned_execution_resources() {
    let mut worker = VmPostgresDriverWorker::new(FixtureBackend::default());
    let (first_pool, first_connection, first_transaction) =
        open_driver_transaction(&mut worker, 100);
    let (second_pool, second_connection, second_transaction) =
        open_driver_transaction(&mut worker, 200);
    worker.backend.fail_rollback_once.set(true);

    let failure = worker
        .apply_control(VmPostgresDriverControl::Rollback {
            transaction: VmPostgresTransaction(102),
            driver_transaction: first_transaction,
        })
        .expect_err("first cleanup rollback must expose its driver failure");
    assert_eq!(failure.code, "postgres.transaction.rollback");
    assert!(!worker.transactions.contains_key(&first_transaction));
    assert!(!worker.connections.contains_key(&first_connection));
    assert!(!worker.connection_pools.contains_key(&first_connection));

    worker
        .apply_control(VmPostgresDriverControl::Rollback {
            transaction: VmPostgresTransaction(202),
            driver_transaction: second_transaction,
        })
        .expect("independent cleanup rollback remains executable");
    assert!(!worker.transactions.contains_key(&second_transaction));
    assert!(!worker.connections.contains_key(&second_connection));
    assert!(!worker.connection_pools.contains_key(&second_connection));
    for (pool, logical_pool) in [
        (first_pool, VmPostgresPool(100)),
        (second_pool, VmPostgresPool(200)),
    ] {
        worker
            .apply_control(VmPostgresDriverControl::ClosePool {
                pool: logical_pool,
                driver_pool: pool,
            })
            .expect("independent pool remains cleanable");
    }
    assert!(worker.pools.is_empty());
}
