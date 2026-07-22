use crate::{
    runtime::vm::{
        actor::VmActorRuntime,
        postgres::{
            VmPostgresConnectConfig, VmPostgresDriverCompletion, VmPostgresDriverConnection,
            VmPostgresDriverControl, VmPostgresDriverOperation, VmPostgresDriverPool,
            VmPostgresDriverTransaction, VmPostgresPool, VmPostgresQueryTarget, VmPostgresReply,
        },
        process::{VmExitReason, VmProcessId, VmProcessSource},
    },
    terlan_native::postgres,
};

fn source() -> VmProcessSource {
    VmProcessSource::new("app.DatabaseActor", "run", 0)
}

#[test]
fn actor_runtime_dispatches_multi_statement_batches_as_an_explicit_operation() {
    let mut runtime = VmActorRuntime::default();
    let owner = runtime.spawn_root(source());
    let pool = connect(&mut runtime, owner);
    let request = runtime
        .postgres_batch_execute(
            owner,
            VmPostgresQueryTarget::Pool(pool),
            "CREATE TABLE one(id INT); CREATE TABLE two(id INT);",
            0,
            10,
        )
        .expect("batch request");

    let dispatch = runtime.take_postgres_dispatch().expect("batch dispatch");
    match dispatch.operation {
        VmPostgresDriverOperation::BatchExecute { target, sql, .. } => {
            assert_eq!(target, VmPostgresQueryTarget::Pool(pool));
            assert!(sql.contains("CREATE TABLE one"));
            assert!(sql.contains("CREATE TABLE two"));
        }
        operation => panic!("expected batch execute dispatch, found {operation:?}"),
    }
    runtime
        .complete_postgres(request, VmPostgresDriverCompletion::Unit)
        .expect("batch completion");
    assert_eq!(
        runtime
            .take_postgres_reply(owner, request)
            .expect("batch reply"),
        VmPostgresReply::Unit
    );
}

fn config(max_connections: usize) -> VmPostgresConnectConfig {
    VmPostgresConnectConfig::new(
        postgres::Config::new("postgres://actor:secret@localhost/app")
            .with_pool_limits(1, max_connections),
    )
    .expect("valid config")
}

fn connect(runtime: &mut VmActorRuntime, owner: VmProcessId) -> VmPostgresPool {
    let request = runtime
        .postgres_connect(owner, config(2), 0, 10)
        .expect("connect request");
    runtime.take_postgres_dispatch().expect("connect dispatch");
    runtime
        .complete_postgres(
            request,
            VmPostgresDriverCompletion::Connected(VmPostgresDriverPool(100)),
        )
        .expect("connect completion");
    match runtime
        .take_postgres_reply(owner, request)
        .expect("connect reply")
    {
        VmPostgresReply::Pool(pool) => pool,
        reply => panic!("expected pool, found {reply:?}"),
    }
}

#[test]
fn actor_runtime_releases_connection_after_successful_transaction_commit() {
    let mut runtime = VmActorRuntime::default();
    let owner = runtime.spawn_root(source());
    let pool = connect(&mut runtime, owner);
    let acquire = runtime
        .postgres_acquire(owner, pool, 10, 10)
        .expect("acquire request");
    runtime.take_postgres_dispatch().expect("acquire dispatch");
    runtime
        .complete_postgres(
            acquire,
            VmPostgresDriverCompletion::Acquired(VmPostgresDriverConnection(200)),
        )
        .expect("acquire completion");
    let VmPostgresReply::Connection(connection) = runtime
        .take_postgres_reply(owner, acquire)
        .expect("acquire reply")
    else {
        panic!("expected connection reply");
    };
    let begin = runtime
        .postgres_begin(owner, connection, 20, 10)
        .expect("begin request");
    runtime.take_postgres_dispatch().expect("begin dispatch");
    runtime
        .complete_postgres(
            begin,
            VmPostgresDriverCompletion::TransactionStarted(VmPostgresDriverTransaction(300)),
        )
        .expect("begin completion");
    let VmPostgresReply::Transaction(transaction) = runtime
        .take_postgres_reply(owner, begin)
        .expect("begin reply")
    else {
        panic!("expected transaction reply");
    };
    let commit = runtime
        .postgres_finish_transaction(owner, transaction, true, 30, 10)
        .expect("commit request");
    runtime.take_postgres_dispatch().expect("commit dispatch");
    runtime
        .complete_postgres(commit, VmPostgresDriverCompletion::Unit)
        .expect("commit completion");

    assert_eq!(
        runtime.take_postgres_control(),
        Some(VmPostgresDriverControl::Release {
            connection,
            driver_connection: VmPostgresDriverConnection(200),
        })
    );
    let snapshot = runtime.postgres_inspection_snapshot();
    assert_eq!(snapshot.owners[0].open_connections, 0);
    assert_eq!(snapshot.owners[0].terminal_transactions, 1);
    assert_eq!(snapshot.cleanup.releases, 1);
}

#[test]
fn actor_runtime_owns_postgres_resources_and_cleans_them_on_exit() {
    let mut runtime = VmActorRuntime::default();
    let owner = runtime.spawn_root(source());
    let pool = connect(&mut runtime, owner);
    assert_eq!(format!("{pool:?}"), "VmPostgresPool(<opaque>)");

    let acquire = runtime
        .postgres_acquire(owner, pool, 10, 10)
        .expect("acquire request");
    runtime.take_postgres_dispatch().expect("acquire dispatch");
    runtime
        .complete_postgres(
            acquire,
            VmPostgresDriverCompletion::Acquired(VmPostgresDriverConnection(200)),
        )
        .expect("acquire completion");
    let connection = match runtime
        .take_postgres_reply(owner, acquire)
        .expect("acquire reply")
    {
        VmPostgresReply::Connection(connection) => connection,
        reply => panic!("expected connection, found {reply:?}"),
    };

    let begin = runtime
        .postgres_begin(owner, connection, 20, 10)
        .expect("begin request");
    runtime.take_postgres_dispatch().expect("begin dispatch");
    runtime
        .complete_postgres(
            begin,
            VmPostgresDriverCompletion::TransactionStarted(VmPostgresDriverTransaction(300)),
        )
        .expect("begin completion");
    let transaction = match runtime
        .take_postgres_reply(owner, begin)
        .expect("begin reply")
    {
        VmPostgresReply::Transaction(transaction) => transaction,
        reply => panic!("expected transaction, found {reply:?}"),
    };

    let active = runtime.postgres_inspection_snapshot();
    assert!(active.pending_requests.is_empty());
    assert_eq!(active.owners.len(), 1);
    let database = &active.owners[0];
    assert_eq!(database.owner, owner);
    assert_eq!(database.registered_pools, 1);
    assert_eq!(database.open_pools, 1);
    assert_eq!(database.registered_connections, 1);
    assert_eq!(database.open_connections, 1);
    assert_eq!(database.active_transactions, 1);
    assert_eq!(database.terminal_transactions, 0);
    let diagnostics = format!("{active:?}");
    assert!(!diagnostics.contains("actor"));
    assert!(!diagnostics.contains("secret"));
    assert!(!diagnostics.contains("postgres://"));

    runtime
        .exit_actor(owner, VmExitReason::Killed)
        .expect("actor exit");
    assert_eq!(
        [
            runtime.take_postgres_control(),
            runtime.take_postgres_control(),
            runtime.take_postgres_control(),
        ],
        [
            Some(VmPostgresDriverControl::Rollback {
                transaction,
                driver_transaction: VmPostgresDriverTransaction(300),
            }),
            Some(VmPostgresDriverControl::ClosePool {
                pool,
                driver_pool: VmPostgresDriverPool(100),
            }),
            None,
        ]
    );
    let cleaned = runtime.postgres_inspection_snapshot();
    let database = &cleaned.owners[0];
    assert_eq!(database.open_pools, 0);
    assert_eq!(database.open_connections, 0);
    assert_eq!(database.active_transactions, 0);
    assert_eq!(database.terminal_transactions, 1);
    assert_eq!(cleaned.cleanup.rollbacks, 1);
    assert_eq!(cleaned.cleanup.releases, 1);
    assert_eq!(cleaned.cleanup.closed_pools, 1);
}

#[test]
fn actor_timer_loop_cancels_timed_out_postgres_request() {
    let mut runtime = VmActorRuntime::default();
    let owner = runtime.spawn_root(source());
    let pool = connect(&mut runtime, owner);
    let query = runtime
        .postgres_query(
            owner,
            VmPostgresQueryTarget::Pool(pool),
            "SELECT value FROM items",
            Vec::new(),
            false,
            10,
            5,
        )
        .expect("query request");
    runtime.take_postgres_dispatch().expect("query dispatch");

    let pending = runtime.postgres_inspection_snapshot();
    assert_eq!(pending.pending_requests.len(), 1);
    let request = &pending.pending_requests[0];
    assert_eq!(request.request_id, query.value);
    assert_eq!(request.owner, owner);
    assert_eq!(request.operation, "query");
    assert!(request
        .sql_fingerprint
        .as_deref()
        .is_some_and(|fingerprint| fingerprint.starts_with("sha256:")));
    assert_eq!(request.deadline_tick, 15);
    assert!(!format!("{pending:?}").contains("items"));

    let advance = runtime.advance_actor_timers(15);
    assert!(advance.deliveries.is_empty());
    assert!(advance.postgres_diagnostics.is_empty());
    assert_eq!(
        advance.postgres_controls,
        [VmPostgresDriverControl::Cancel(query)]
    );
    assert_eq!(
        runtime.take_postgres_control(),
        Some(VmPostgresDriverControl::Cancel(query))
    );
    assert!(matches!(
        runtime
            .take_postgres_reply(owner, query)
            .expect("timeout reply"),
        VmPostgresReply::Error(ref error) if error.code == "postgres.timed_out"
    ));
    let timed_out = runtime.postgres_inspection_snapshot();
    assert!(timed_out.pending_requests.is_empty());
    assert_eq!(timed_out.cleanup.cancellations, 1);
}
