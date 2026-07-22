use super::*;
use crate::runtime::vm::{
    process::{VmExitReason, VmProcessSource, VmProcessState},
    scheduler::VmSchedulerConfig,
};
use crate::terlan_native::{json, postgres};

pub(super) fn harness() -> (VmProcessTable, VmScheduler, VmTimerTable, VmProcessId) {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(VmProcessSource::new("app.Database", "run", 0));
    (
        processes,
        VmScheduler::new(VmSchedulerConfig::new(100, 1_000)),
        VmTimerTable::default(),
        owner,
    )
}

fn config(max_connections: usize) -> VmPostgresConnectConfig {
    VmPostgresConnectConfig::new(
        postgres::Config::new("postgres://alice:secret@localhost/terlan")
            .with_pool_limits(1, max_connections),
    )
    .expect("valid config")
}

fn driver_rows(count: usize) -> Vec<VmPostgresDriverRow> {
    (0..count)
        .map(|offset| VmPostgresDriverRow(500 + offset as u64))
        .collect()
}

pub(super) fn connect_pool(
    runtime: &mut VmPostgresRuntime,
    processes: &mut VmProcessTable,
    scheduler: &mut VmScheduler,
    timers: &mut VmTimerTable,
    owner: VmProcessId,
    max_connections: usize,
) -> VmPostgresPool {
    let request = runtime
        .connect(
            timers,
            processes,
            scheduler,
            owner,
            config(max_connections),
            0,
            10,
        )
        .expect("connect request");
    assert_eq!(
        runtime
            .take_dispatch()
            .expect("connect dispatch")
            .request_id,
        request
    );
    runtime
        .complete(
            timers,
            processes,
            scheduler,
            request,
            VmPostgresDriverCompletion::Connected(VmPostgresDriverPool(100)),
        )
        .expect("connect completion");
    match runtime.take_reply(owner, request).expect("connect reply") {
        VmPostgresReply::Pool(pool) => pool,
        reply => panic!("expected pool reply, found {reply:?}"),
    }
}

pub(super) fn acquire_connection(
    runtime: &mut VmPostgresRuntime,
    processes: &mut VmProcessTable,
    scheduler: &mut VmScheduler,
    timers: &mut VmTimerTable,
    owner: VmProcessId,
    pool: VmPostgresPool,
) -> VmPostgresConnection {
    let request = runtime
        .acquire(timers, processes, scheduler, owner, pool, 10, 10)
        .expect("acquire request");
    runtime.take_dispatch().expect("acquire dispatch");
    runtime
        .complete(
            timers,
            processes,
            scheduler,
            request,
            VmPostgresDriverCompletion::Acquired(VmPostgresDriverConnection(200)),
        )
        .expect("acquire completion");
    match runtime.take_reply(owner, request).expect("acquire reply") {
        VmPostgresReply::Connection(connection) => connection,
        reply => panic!("expected connection reply, found {reply:?}"),
    }
}

pub(super) fn begin_transaction(
    runtime: &mut VmPostgresRuntime,
    processes: &mut VmProcessTable,
    scheduler: &mut VmScheduler,
    timers: &mut VmTimerTable,
    owner: VmProcessId,
    connection: VmPostgresConnection,
) -> VmPostgresTransaction {
    let request = runtime
        .begin(timers, processes, scheduler, owner, connection, 20, 10)
        .expect("begin request");
    runtime.take_dispatch().expect("begin dispatch");
    runtime
        .complete(
            timers,
            processes,
            scheduler,
            request,
            VmPostgresDriverCompletion::TransactionStarted(VmPostgresDriverTransaction(300)),
        )
        .expect("begin completion");
    match runtime.take_reply(owner, request).expect("begin reply") {
        VmPostgresReply::Transaction(transaction) => transaction,
        reply => panic!("expected transaction reply, found {reply:?}"),
    }
}

#[test]
fn postgres_runtime_parks_dispatches_and_wakes_owner() {
    let (mut processes, mut scheduler, mut timers, owner) = harness();
    let mut runtime = VmPostgresRuntime::new(4);
    let request = runtime
        .connect(
            &mut timers,
            &mut processes,
            &mut scheduler,
            owner,
            config(2),
            5,
            20,
        )
        .expect("connect request");

    assert_eq!(
        processes.get(owner).expect("owner").state,
        VmProcessState::Blocked
    );
    let dispatch = runtime.take_dispatch().expect("driver dispatch");
    assert_eq!(dispatch.request_id, request);
    assert_eq!(dispatch.owner, owner);
    assert_eq!(dispatch.operation.name(), "connect");
    runtime
        .complete(
            &mut timers,
            &mut processes,
            &mut scheduler,
            request,
            VmPostgresDriverCompletion::Connected(VmPostgresDriverPool(100)),
        )
        .expect("complete connect");

    assert_eq!(
        processes.get(owner).expect("owner").state,
        VmProcessState::Runnable
    );
    assert_eq!(scheduler.queued_len(), 1);
    assert!(matches!(
        runtime.take_reply(owner, request).expect("reply"),
        VmPostgresReply::Pool(_)
    ));
    assert!(processes.get(owner).expect("owner").reductions >= 3);
}

#[test]
fn postgres_runtime_enforces_pool_capacity_before_parking() {
    let (mut processes, mut scheduler, mut timers, owner) = harness();
    let mut runtime = VmPostgresRuntime::new(4);
    let pool = connect_pool(
        &mut runtime,
        &mut processes,
        &mut scheduler,
        &mut timers,
        owner,
        1,
    );
    acquire_connection(
        &mut runtime,
        &mut processes,
        &mut scheduler,
        &mut timers,
        owner,
        pool,
    );

    let error = runtime
        .acquire(
            &mut timers,
            &mut processes,
            &mut scheduler,
            owner,
            pool,
            20,
            10,
        )
        .expect_err("pool must be exhausted");

    assert!(error.contains("postgres.pool.exhausted"));
    assert_eq!(
        processes.get(owner).expect("owner").state,
        VmProcessState::Runnable
    );
    assert!(runtime.take_dispatch().is_none());
}

#[test]
fn postgres_runtime_transaction_has_one_way_terminal_state() {
    let (mut processes, mut scheduler, mut timers, owner) = harness();
    let mut runtime = VmPostgresRuntime::new(4);
    let pool = connect_pool(
        &mut runtime,
        &mut processes,
        &mut scheduler,
        &mut timers,
        owner,
        2,
    );
    let connection = acquire_connection(
        &mut runtime,
        &mut processes,
        &mut scheduler,
        &mut timers,
        owner,
        pool,
    );
    let transaction = begin_transaction(
        &mut runtime,
        &mut processes,
        &mut scheduler,
        &mut timers,
        owner,
        connection,
    );
    let commit = runtime
        .finish_transaction(
            &mut timers,
            &mut processes,
            &mut scheduler,
            owner,
            transaction,
            true,
            30,
            10,
        )
        .expect("commit request");
    runtime.take_dispatch().expect("commit dispatch");
    runtime
        .complete(
            &mut timers,
            &mut processes,
            &mut scheduler,
            commit,
            VmPostgresDriverCompletion::Unit,
        )
        .expect("commit completion");
    assert_eq!(
        runtime.transaction_state(transaction),
        Some(VmPostgresTransactionState::Committed)
    );
    assert!(matches!(
        runtime.take_reply(owner, commit).expect("commit reply"),
        VmPostgresReply::Unit
    ));
    assert_eq!(
        runtime.take_completion_control(),
        Some(VmPostgresDriverControl::Release {
            connection,
            driver_connection: VmPostgresDriverConnection(200),
        })
    );

    let error = runtime
        .finish_transaction(
            &mut timers,
            &mut processes,
            &mut scheduler,
            owner,
            transaction,
            false,
            40,
            10,
        )
        .expect_err("terminal transaction cannot roll back");
    assert!(error.contains("postgres.transaction.terminal"));
}

#[test]
fn postgres_runtime_cancellation_wins_and_rejects_late_driver_reply() {
    let (mut processes, mut scheduler, mut timers, owner) = harness();
    let mut runtime = VmPostgresRuntime::new(2);
    let request = runtime
        .connect(
            &mut timers,
            &mut processes,
            &mut scheduler,
            owner,
            config(1),
            0,
            20,
        )
        .expect("connect request");
    runtime.take_dispatch().expect("dispatch");

    assert_eq!(
        runtime
            .cancel(&mut timers, &mut processes, &mut scheduler, request,)
            .expect("cancel"),
        VmPostgresDriverControl::Cancel(request)
    );
    assert_eq!(
        processes.get(owner).expect("owner").state,
        VmProcessState::Runnable
    );
    assert!(matches!(
        runtime.take_reply(owner, request).expect("cancel reply"),
        VmPostgresReply::Error(VmPostgresFailure { ref code, .. })
            if code == "postgres.cancelled"
    ));
    assert!(runtime
        .complete(
            &mut timers,
            &mut processes,
            &mut scheduler,
            request,
            VmPostgresDriverCompletion::Connected(VmPostgresDriverPool(100)),
        )
        .expect_err("late completion")
        .contains("postgres.request.stale"));
}

#[test]
fn postgres_runtime_timeout_maps_to_typed_error() {
    let (mut processes, mut scheduler, mut timers, owner) = harness();
    let mut runtime = VmPostgresRuntime::new(1);
    let request = runtime
        .connect(
            &mut timers,
            &mut processes,
            &mut scheduler,
            owner,
            config(1),
            0,
            5,
        )
        .expect("connect request");
    runtime.take_dispatch().expect("dispatch");
    let events = timers.advance_clock(&mut processes, &mut scheduler, 5);

    assert_eq!(events.len(), 1);
    assert_eq!(
        runtime
            .handle_timer_event(&mut processes, &mut scheduler, &events[0])
            .expect("timeout event"),
        Some(VmPostgresDriverControl::Cancel(request))
    );
    assert!(matches!(
        runtime.take_reply(owner, request).expect("timeout reply"),
        VmPostgresReply::Error(VmPostgresFailure { ref code, .. })
            if code == "postgres.timed_out"
    ));
}

#[test]
fn postgres_runtime_rejects_driver_completion_after_deadline_fires() {
    let (mut processes, mut scheduler, mut timers, owner) = harness();
    let mut runtime = VmPostgresRuntime::new(1);
    let request = runtime
        .connect(
            &mut timers,
            &mut processes,
            &mut scheduler,
            owner,
            config(1),
            0,
            5,
        )
        .expect("connect request");
    runtime.take_dispatch().expect("dispatch");
    let events = timers.advance_clock(&mut processes, &mut scheduler, 5);

    let error = runtime
        .complete(
            &mut timers,
            &mut processes,
            &mut scheduler,
            request,
            VmPostgresDriverCompletion::Connected(VmPostgresDriverPool(100)),
        )
        .expect_err("expired completion must not mutate resources");
    assert!(error.contains("no longer owns completion"));
    assert_eq!(
        runtime
            .handle_timer_event(&mut processes, &mut scheduler, &events[0])
            .expect("timeout event"),
        Some(VmPostgresDriverControl::Cancel(request))
    );
    assert!(matches!(
        runtime.take_reply(owner, request).expect("timeout reply"),
        VmPostgresReply::Error(VmPostgresFailure { ref code, .. })
            if code == "postgres.timed_out"
    ));

    let path = std::env::temp_dir().join("terlan-postgres-expired-completion-report.json");
    runtime.write_report(&path).expect("write report");
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).expect("read report")).expect("parse report");
    assert_eq!(report["resources"]["pools"], 0);
}

#[test]
fn postgres_runtime_owner_cleanup_rolls_back_before_close() {
    let (mut processes, mut scheduler, mut timers, owner) = harness();
    let mut runtime = VmPostgresRuntime::new(4);
    let pool = connect_pool(
        &mut runtime,
        &mut processes,
        &mut scheduler,
        &mut timers,
        owner,
        2,
    );
    let connection = acquire_connection(
        &mut runtime,
        &mut processes,
        &mut scheduler,
        &mut timers,
        owner,
        pool,
    );
    let transaction = begin_transaction(
        &mut runtime,
        &mut processes,
        &mut scheduler,
        &mut timers,
        owner,
        connection,
    );
    runtime.prepared.insert(
        VmPostgresPreparedStatement(900),
        PreparedStatementState {
            owner,
            driver_statement: VmPostgresDriverPreparedStatement(901),
        },
    );
    runtime.result_sets.insert(VmPostgresResultSet(901), owner);
    runtime.rows.insert(
        VmPostgresRow(902),
        RowState {
            owner,
            driver_row: VmPostgresDriverRow(903),
        },
    );
    processes
        .exit_process(owner, VmExitReason::Killed)
        .expect("exit owner");

    let controls = runtime.cleanup_owner(owner);

    assert_eq!(
        controls,
        vec![
            VmPostgresDriverControl::Rollback {
                transaction,
                driver_transaction: VmPostgresDriverTransaction(300),
            },
            VmPostgresDriverControl::DropPreparedStatement {
                statement: VmPostgresPreparedStatement(900),
                driver_statement: VmPostgresDriverPreparedStatement(901),
            },
            VmPostgresDriverControl::DropRow {
                row: VmPostgresRow(902),
                driver_row: VmPostgresDriverRow(903),
            },
            VmPostgresDriverControl::ClosePool {
                pool,
                driver_pool: VmPostgresDriverPool(100),
            },
        ]
    );
    assert_eq!(
        runtime.transaction_state(transaction),
        Some(VmPostgresTransactionState::RolledBack)
    );
    assert!(runtime.prepared.is_empty());
    assert!(runtime.result_sets.is_empty());
    assert!(runtime.rows.is_empty());
    assert_eq!(runtime.cleanup.dropped_prepared_statements, 1);
    assert_eq!(runtime.cleanup.dropped_result_sets, 1);
    assert_eq!(runtime.cleanup.dropped_rows, 1);
    assert_eq!(format!("{pool:?}"), "VmPostgresPool(<opaque>)");
    assert_eq!(format!("{connection:?}"), "VmPostgresConnection(<opaque>)");
}

#[test]
fn postgres_runtime_rejects_cross_owner_and_empty_sql_without_dispatch() {
    let (mut processes, mut scheduler, mut timers, owner) = harness();
    let other = processes.spawn_root(VmProcessSource::new("app.Other", "run", 0));
    let mut runtime = VmPostgresRuntime::new(4);
    let pool = connect_pool(
        &mut runtime,
        &mut processes,
        &mut scheduler,
        &mut timers,
        owner,
        2,
    );

    let owner_error = runtime
        .query(
            &mut timers,
            &mut processes,
            &mut scheduler,
            other,
            VmPostgresQueryTarget::Pool(pool),
            "SELECT 1",
            Vec::new(),
            false,
            10,
            10,
        )
        .expect_err("cross-owner query");
    assert!(owner_error.contains("postgres.resource.owner"));
    let sql_error = runtime
        .query(
            &mut timers,
            &mut processes,
            &mut scheduler,
            owner,
            VmPostgresQueryTarget::Pool(pool),
            "   ",
            Vec::new(),
            false,
            10,
            10,
        )
        .expect_err("empty SQL");
    assert!(sql_error.contains("postgres.sql.empty"));
    assert!(runtime.take_dispatch().is_none());
}

#[test]
fn postgres_runtime_redacts_driver_failures_and_debug_values() {
    let config = config(2);
    let debug = format!("{config:?}");
    assert!(debug.contains("max_connections: 2"));
    assert!(!debug.contains("alice"));
    assert!(!debug.contains("secret"));
    let failure = VmPostgresFailure::new(
        "postgres.connect",
        "failed postgres://alice:secret@localhost/terlan timeout",
    );
    assert_eq!(failure.message, "failed <redacted-postgres-url> timeout");
}

#[test]
fn postgres_runtime_rejects_mismatched_driver_completion_without_waking_owner() {
    let (mut processes, mut scheduler, mut timers, owner) = harness();
    let mut runtime = VmPostgresRuntime::new(2);
    let request = runtime
        .connect(
            &mut timers,
            &mut processes,
            &mut scheduler,
            owner,
            config(1),
            0,
            10,
        )
        .expect("connect request");
    runtime.take_dispatch().expect("connect dispatch");

    let error = runtime
        .complete(
            &mut timers,
            &mut processes,
            &mut scheduler,
            request,
            VmPostgresDriverCompletion::Rows {
                rows: driver_rows(1),
            },
        )
        .expect_err("mismatched completion");
    assert!(error.contains("postgres.driver.protocol"));
    assert_eq!(
        processes.get(owner).expect("owner remains live").state,
        VmProcessState::Blocked
    );

    runtime
        .complete(
            &mut timers,
            &mut processes,
            &mut scheduler,
            request,
            VmPostgresDriverCompletion::Connected(VmPostgresDriverPool(100)),
        )
        .expect("valid retry completion");
    assert_eq!(
        processes.get(owner).expect("owner remains live").state,
        VmProcessState::Runnable
    );
    assert!(matches!(
        runtime.take_reply(owner, request).expect("connect reply"),
        VmPostgresReply::Pool(_)
    ));
}

#[test]
fn postgres_runtime_executes_transaction_query_prepare_decode_and_failure_shapes() {
    let (mut processes, mut scheduler, mut timers, owner) = harness();
    let mut runtime = VmPostgresRuntime::new(8);
    let pool = connect_pool(
        &mut runtime,
        &mut processes,
        &mut scheduler,
        &mut timers,
        owner,
        2,
    );
    let connection = acquire_connection(
        &mut runtime,
        &mut processes,
        &mut scheduler,
        &mut timers,
        owner,
        pool,
    );

    let prepare = runtime
        .prepare(
            &mut timers,
            &mut processes,
            &mut scheduler,
            owner,
            connection,
            "SELECT value FROM items WHERE id = $1",
            1,
            20,
            10,
        )
        .expect("prepare request");
    runtime.take_dispatch().expect("prepare dispatch");
    runtime
        .complete(
            &mut timers,
            &mut processes,
            &mut scheduler,
            prepare,
            VmPostgresDriverCompletion::Prepared(VmPostgresDriverPreparedStatement(400)),
        )
        .expect("prepare completion");
    assert!(matches!(
        runtime.take_reply(owner, prepare).expect("prepare reply"),
        VmPostgresReply::PreparedStatement(_)
    ));

    let transaction = begin_transaction(
        &mut runtime,
        &mut processes,
        &mut scheduler,
        &mut timers,
        owner,
        connection,
    );
    let query = runtime
        .query(
            &mut timers,
            &mut processes,
            &mut scheduler,
            owner,
            VmPostgresQueryTarget::Transaction(transaction),
            "SELECT value FROM items",
            Vec::new(),
            false,
            30,
            10,
        )
        .expect("transaction query");
    runtime.take_dispatch().expect("query dispatch");
    runtime
        .complete(
            &mut timers,
            &mut processes,
            &mut scheduler,
            query,
            VmPostgresDriverCompletion::Rows {
                rows: driver_rows(1),
            },
        )
        .expect("query completion");
    let row = match runtime.take_reply(owner, query).expect("query reply") {
        VmPostgresReply::Rows { rows, .. } => rows[0],
        reply => panic!("expected rows, found {reply:?}"),
    };

    for (expected, value) in [
        (
            VmPostgresDecodeType::String,
            VmPostgresDecodedValue::String("value".to_string()),
        ),
        (VmPostgresDecodeType::Int, VmPostgresDecodedValue::Int(7)),
        (
            VmPostgresDecodeType::Bool,
            VmPostgresDecodedValue::Bool(true),
        ),
        (
            VmPostgresDecodeType::Json,
            VmPostgresDecodedValue::Json("{\"ok\":true}".to_string()),
        ),
    ] {
        let decode = runtime
            .decode(
                &mut timers,
                &mut processes,
                &mut scheduler,
                owner,
                row,
                "value",
                expected,
                40,
                10,
            )
            .expect("decode request");
        runtime.take_dispatch().expect("decode dispatch");
        runtime
            .complete(
                &mut timers,
                &mut processes,
                &mut scheduler,
                decode,
                VmPostgresDriverCompletion::Decoded(value.clone()),
            )
            .expect("decode completion");
        assert_eq!(
            runtime.take_reply(owner, decode).expect("decode reply"),
            VmPostgresReply::Decoded(value)
        );
    }

    let execute = runtime
        .execute(
            &mut timers,
            &mut processes,
            &mut scheduler,
            owner,
            VmPostgresQueryTarget::Transaction(transaction),
            "UPDATE items SET value = $1",
            vec![json::Json::from_serde(serde_json::json!("updated"))],
            50,
            10,
        )
        .expect("execute request");
    runtime.take_dispatch().expect("execute dispatch");
    runtime
        .complete(
            &mut timers,
            &mut processes,
            &mut scheduler,
            execute,
            VmPostgresDriverCompletion::AffectedRows(3),
        )
        .expect("execute completion");
    assert_eq!(
        runtime.take_reply(owner, execute).expect("execute reply"),
        VmPostgresReply::AffectedRows(3)
    );

    let failed = runtime
        .query(
            &mut timers,
            &mut processes,
            &mut scheduler,
            owner,
            VmPostgresQueryTarget::Transaction(transaction),
            "SELECT broken FROM missing",
            Vec::new(),
            true,
            60,
            10,
        )
        .expect("failed query request");
    runtime.take_dispatch().expect("failed query dispatch");
    runtime
        .complete(
            &mut timers,
            &mut processes,
            &mut scheduler,
            failed,
            VmPostgresDriverCompletion::Failed(VmPostgresFailure::new(
                "postgres.query",
                "driver rejected postgres://user:password@localhost/db",
            )),
        )
        .expect("failed query completion");
    assert!(matches!(
        runtime.take_reply(owner, failed).expect("failed reply"),
        VmPostgresReply::Error(VmPostgresFailure { ref message, .. })
            if !message.contains("password")
    ));
}

#[test]
fn postgres_runtime_report_contains_lifecycle_without_secrets_or_sql() {
    let (mut processes, mut scheduler, mut timers, owner) = harness();
    let mut runtime = VmPostgresRuntime::new(4);
    let pool = connect_pool(
        &mut runtime,
        &mut processes,
        &mut scheduler,
        &mut timers,
        owner,
        2,
    );
    let query = runtime
        .query(
            &mut timers,
            &mut processes,
            &mut scheduler,
            owner,
            VmPostgresQueryTarget::Pool(pool),
            "SELECT private_value FROM secrets",
            Vec::new(),
            false,
            20,
            10,
        )
        .expect("query request");
    runtime.take_dispatch().expect("query dispatch");
    runtime
        .complete(
            &mut timers,
            &mut processes,
            &mut scheduler,
            query,
            VmPostgresDriverCompletion::Rows {
                rows: driver_rows(2),
            },
        )
        .expect("query completion");
    let rows = match runtime.take_reply(owner, query).expect("query reply") {
        VmPostgresReply::Rows { rows, .. } => rows,
        reply => panic!("expected row reply, found {reply:?}"),
    };
    assert_eq!(rows.len(), 2);
    processes
        .exit_process(owner, VmExitReason::Normal)
        .expect("exit report owner");
    assert_eq!(
        runtime.cleanup_owner(owner),
        [
            VmPostgresDriverControl::DropRow {
                row: rows[0],
                driver_row: VmPostgresDriverRow(500),
            },
            VmPostgresDriverControl::DropRow {
                row: rows[1],
                driver_row: VmPostgresDriverRow(501),
            },
            VmPostgresDriverControl::ClosePool {
                pool,
                driver_pool: VmPostgresDriverPool(100),
            },
        ]
    );
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/quality/vm-postgres-runtime-report.json");

    runtime.write_report(&path).expect("write report");
    let text = std::fs::read_to_string(&path).expect("read report");
    let report: serde_json::Value = serde_json::from_str(&text).expect("parse report");

    assert_eq!(report["schema"], "terlan-vm-postgres-runtime-report-v1");
    assert_eq!(report["runtimeOwner"], "terlan-vm");
    assert_eq!(report["resources"]["resultSets"], 0);
    assert_eq!(report["resources"]["rows"], 0);
    assert_eq!(report["cleanup"]["closedPools"], 1);
    assert_eq!(report["cleanup"]["droppedResultSets"], 1);
    assert_eq!(report["cleanup"]["droppedRows"], 2);
    assert_eq!(
        report["poolConfiguration"]["configuredConnectionCapacity"],
        2
    );
    assert_eq!(report["resourcePolicy"]["checkpoint"], "forbidden");
    assert_eq!(report["resourcePolicy"]["debug"], "opaque");
    assert_eq!(report["security"]["rawDriverErrors"], false);
    assert!(!text.contains("alice"));
    assert!(!text.contains("secret"));
    assert!(!text.contains("private_value"));
    assert!(text.contains("sha256:"));
}
