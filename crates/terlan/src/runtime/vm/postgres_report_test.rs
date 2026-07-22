use super::postgres_test::{acquire_connection, begin_transaction, connect_pool, harness};
use super::*;
use crate::runtime::vm::process::VmExitReason;

fn complete_unit(
    runtime: &mut VmPostgresRuntime,
    processes: &mut VmProcessTable,
    scheduler: &mut VmScheduler,
    timers: &mut VmTimerTable,
    owner: VmProcessId,
    request: RequestId,
) {
    runtime.take_dispatch().expect("Postgres dispatch");
    runtime
        .complete(
            timers,
            processes,
            scheduler,
            request,
            VmPostgresDriverCompletion::Unit,
        )
        .expect("Postgres unit completion");
    assert_eq!(
        runtime.take_reply(owner, request).expect("Postgres reply"),
        VmPostgresReply::Unit
    );
}

fn read_report(runtime: &VmPostgresRuntime, name: &str) -> (String, serde_json::Value) {
    let path = std::env::temp_dir().join(format!(
        "terlan-postgres-{name}-{}-report.json",
        std::process::id()
    ));
    runtime.write_report(&path).expect("write Postgres report");
    let text = std::fs::read_to_string(path).expect("read Postgres report");
    let value = serde_json::from_str(&text).expect("parse Postgres report");
    (text, value)
}

#[test]
fn postgres_report_derives_lifecycle_evidence_and_cleanup_proof_from_runtime_state() {
    let (mut processes, mut scheduler, mut timers, owner) = harness();
    let mut runtime = VmPostgresRuntime::new(8);
    let pool = connect_pool(
        &mut runtime,
        &mut processes,
        &mut scheduler,
        &mut timers,
        owner,
        1,
    );
    let committed_connection = acquire_connection(
        &mut runtime,
        &mut processes,
        &mut scheduler,
        &mut timers,
        owner,
        pool,
    );

    let committed = begin_transaction(
        &mut runtime,
        &mut processes,
        &mut scheduler,
        &mut timers,
        owner,
        committed_connection,
    );
    let (_, active_transaction_report) = read_report(&runtime, "active-transaction");
    assert_eq!(
        active_transaction_report["cleanupProof"]["typedTerminalStates"],
        false
    );
    let commit = runtime
        .finish_transaction(
            &mut timers,
            &mut processes,
            &mut scheduler,
            owner,
            committed,
            true,
            30,
            10,
        )
        .expect("commit request");
    complete_unit(
        &mut runtime,
        &mut processes,
        &mut scheduler,
        &mut timers,
        owner,
        commit,
    );
    assert!(matches!(
        runtime.take_completion_control(),
        Some(VmPostgresDriverControl::Release { connection, .. })
            if connection == committed_connection
    ));

    let rolled_back_connection = acquire_connection(
        &mut runtime,
        &mut processes,
        &mut scheduler,
        &mut timers,
        owner,
        pool,
    );
    let rolled_back = begin_transaction(
        &mut runtime,
        &mut processes,
        &mut scheduler,
        &mut timers,
        owner,
        rolled_back_connection,
    );
    let rollback = runtime
        .finish_transaction(
            &mut timers,
            &mut processes,
            &mut scheduler,
            owner,
            rolled_back,
            false,
            40,
            10,
        )
        .expect("rollback request");
    complete_unit(
        &mut runtime,
        &mut processes,
        &mut scheduler,
        &mut timers,
        owner,
        rollback,
    );
    assert!(matches!(
        runtime.take_completion_control(),
        Some(VmPostgresDriverControl::Release { connection, .. })
            if connection == rolled_back_connection
    ));

    let successful_query = runtime
        .query(
            &mut timers,
            &mut processes,
            &mut scheduler,
            owner,
            VmPostgresQueryTarget::Pool(pool),
            "SELECT private_value FROM secrets",
            Vec::new(),
            false,
            50,
            10,
        )
        .expect("successful query request");
    runtime.take_dispatch().expect("successful query dispatch");
    runtime
        .complete(
            &mut timers,
            &mut processes,
            &mut scheduler,
            successful_query,
            VmPostgresDriverCompletion::Rows {
                rows: vec![VmPostgresDriverRow(500)],
            },
        )
        .expect("successful query completion");
    let row = match runtime
        .take_reply(owner, successful_query)
        .expect("successful query reply")
    {
        VmPostgresReply::Rows { rows, .. } => rows[0],
        reply => panic!("expected rows, found {reply:?}"),
    };

    let failed_query = runtime
        .query(
            &mut timers,
            &mut processes,
            &mut scheduler,
            owner,
            VmPostgresQueryTarget::Pool(pool),
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
            failed_query,
            VmPostgresDriverCompletion::Failed(VmPostgresFailure::new(
                "postgres.query",
                "driver rejected postgres://alice:secret@localhost/terlan",
            )),
        )
        .expect("failed query completion");
    assert!(matches!(
        runtime
            .take_reply(owner, failed_query)
            .expect("failed query reply"),
        VmPostgresReply::Error(_)
    ));

    let cancelled_query = runtime
        .query(
            &mut timers,
            &mut processes,
            &mut scheduler,
            owner,
            VmPostgresQueryTarget::Pool(pool),
            "SELECT pg_sleep(10)",
            Vec::new(),
            false,
            70,
            10,
        )
        .expect("cancelled query request");
    runtime.take_dispatch().expect("cancelled query dispatch");
    let (_, active_report) = read_report(&runtime, "active");
    assert_eq!(active_report["cleanupProof"]["noPendingRequests"], false);
    assert_eq!(active_report["cleanupProof"]["noReservedCredits"], false);
    assert_eq!(active_report["cleanupProof"]["ownerCleanupComplete"], false);
    runtime
        .cancel(&mut timers, &mut processes, &mut scheduler, cancelled_query)
        .expect("cancel query");
    assert!(matches!(
        runtime
            .take_reply(owner, cancelled_query)
            .expect("cancelled query reply"),
        VmPostgresReply::Error(_)
    ));

    let decode = runtime
        .decode(
            &mut timers,
            &mut processes,
            &mut scheduler,
            owner,
            row,
            "private_value",
            VmPostgresDecodeType::Int,
            80,
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
            VmPostgresDriverCompletion::Failed(VmPostgresFailure::new(
                "postgres.decode.type",
                "private_value cannot decode as Int",
            )),
        )
        .expect("decode failure completion");
    assert!(matches!(
        runtime
            .take_reply(owner, decode)
            .expect("decode failure reply"),
        VmPostgresReply::Error(_)
    ));

    let (_, before_cleanup) = read_report(&runtime, "before-cleanup");
    assert_eq!(before_cleanup["cleanupProof"]["noPendingRequests"], true);
    assert_eq!(before_cleanup["cleanupProof"]["noReservedCredits"], true);
    assert_eq!(before_cleanup["cleanupProof"]["noLiveResources"], false);
    assert_eq!(
        before_cleanup["cleanupProof"]["ownerCleanupComplete"],
        false
    );
    processes
        .exit_process(owner, VmExitReason::Normal)
        .expect("exit Postgres owner");
    let controls = runtime.cleanup_owner(owner);
    assert!(controls
        .iter()
        .any(|control| matches!(control, VmPostgresDriverControl::ClosePool { .. })));
    assert!(controls
        .iter()
        .any(|control| matches!(control, VmPostgresDriverControl::DropRow { .. })));

    let (text, report) = read_report(&runtime, "evidence");
    assert_eq!(report["evidence"]["queryLifecycle"]["dispatched"], 3);
    assert_eq!(report["evidence"]["queryLifecycle"]["succeeded"], 1);
    assert_eq!(report["evidence"]["queryLifecycle"]["failed"], 1);
    assert_eq!(report["evidence"]["queryLifecycle"]["cancelled"], 1);
    assert_eq!(report["evidence"]["transactionOutcomes"]["committed"], 1);
    assert_eq!(report["evidence"]["transactionOutcomes"]["rolledBack"], 1);
    assert_eq!(report["evidence"]["cancellationDecisions"]["explicit"], 1);
    assert_eq!(report["evidence"]["rowDecodeFailures"]["count"], 1);
    assert_eq!(
        report["evidence"]["rowDecodeFailures"]["errorCodes"]["postgres.decode.type"],
        1
    );
    assert_eq!(report["cleanupProof"]["typedTerminalStates"], true);
    assert_eq!(report["cleanupProof"]["noPendingRequests"], true);
    assert_eq!(report["cleanupProof"]["noReservedCredits"], true);
    assert_eq!(report["cleanupProof"]["noLiveResources"], true);
    assert_eq!(report["cleanupProof"]["ownerCleanupComplete"], true);
    assert!(!text.contains("alice"));
    assert!(!text.contains("secret"));
    assert!(!text.contains("private_value"));
    assert!(!text.contains("pg_sleep"));
}
