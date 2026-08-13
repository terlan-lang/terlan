use super::*;

/// Builds the request-id wrapper used by worker lifecycle helpers.
fn request_id(value: u64) -> RequestId {
    RequestId { value }
}

/// Extracts a successful resource handle from a worker reply.
fn handle_reply(reply: NativeBoundaryWorkerReply) -> Option<NativeBoundaryHandle> {
    let NativeBoundaryReplyTerm::Ok(NativeBoundaryTerm::Handle { id, generation }) = reply.result
    else {
        return None;
    };
    Some(NativeBoundaryHandle { id, generation })
}

/// Verifies primitive calls execute and release worker credit.
#[test]
fn worker_call_executes_runtime_and_releases_credit() {
    let mut worker = NativeBoundaryWorker::new(2);

    let reply = worker.call(
        request_id(1),
        "std.encoding.base64.encode",
        &[NativeBoundaryTerm::Text(String::from("hello"))],
    );

    assert_eq!(reply.request_id, request_id(1));
    assert_eq!(
        reply.result,
        NativeBoundaryReplyTerm::Ok(NativeBoundaryTerm::Text(String::from("aGVsbG8=")))
    );
    assert_eq!(reply.reserved_credits, 0);
    assert_eq!(reply.available_credits, 2);
    assert_eq!(worker.reserved_credits(), 0);
}

/// Verifies a pre-dispatch cancellation wins and releases worker credit.
#[test]
fn worker_cooperative_call_observes_request_token_before_dispatch() {
    let mut worker = NativeBoundaryWorker::new(1);
    let cancellation = NativeBoundaryCancellationToken::new();
    cancellation.cancel();

    let reply = worker.call_for_process_with_policy_and_cancellation(
        crate::terlan_native_boundary::worker::NativeBoundaryWorkerCall {
            request_id: request_id(1),
            owner_process_id: 7,
            granted_capabilities: &["postgres"],
            admitted_worker_classes: &[
                crate::terlan_native_boundary::metadata::NativeBoundaryWorkerClass::ResourceOwning,
            ],
            operation: "std.db.postgres.connect",
            args: &[],
        },
        &cancellation,
    );

    assert_eq!(reply.result, worker_error_reply(ErrorKind::Cancelled));
    assert_eq!(reply.reserved_credits, 0);
    assert_eq!(reply.available_credits, 1);
    assert_eq!(
        worker.events().last().map(|event| event.outcome),
        Some(NativeBoundaryWorkerOutcome::Cancelled)
    );
}

/// Verifies command terms use worker request and credit accounting.
#[test]
fn worker_execute_command_runs_call_terms() {
    let mut worker = NativeBoundaryWorker::new(2);
    let command = NativeBoundaryCommandTerm::Call {
        request_id: 10,
        operation: String::from("std.encoding.base64.encode"),
        args: vec![NativeBoundaryTerm::Text(String::from("hello"))],
    };

    let reply = worker.execute_command(&command);

    assert_eq!(reply.request_id, request_id(10));
    assert_eq!(
        reply.result,
        NativeBoundaryReplyTerm::Ok(NativeBoundaryTerm::Text(String::from("aGVsbG8=")))
    );
    assert_eq!(reply.reserved_credits, 0);
    assert_eq!(reply.available_credits, 2);
}

/// Verifies the worker enforces its pending-request credit limit.
#[test]
fn worker_begin_request_rejects_backpressure_limit() {
    let mut worker = NativeBoundaryWorker::new(1);

    assert_eq!(worker.begin_request(request_id(1)), Ok(()));
    assert_eq!(
        worker.begin_request(request_id(2)),
        Err(worker_error_reply(ErrorKind::BackpressureLimit))
    );
    assert_eq!(worker.reserved_credits(), 1);
    assert_eq!(worker.available_credits(), 0);
}

/// Verifies duplicate request ids do not reserve another credit.
#[test]
fn worker_begin_request_rejects_duplicate_request_id() {
    let mut worker = NativeBoundaryWorker::new(2);

    assert_eq!(worker.begin_request(request_id(1)), Ok(()));
    assert_eq!(
        worker.begin_request(request_id(1)),
        Err(worker_error_reply(ErrorKind::InvalidRequest))
    );
    assert_eq!(worker.reserved_credits(), 1);
    assert_eq!(worker.available_credits(), 1);
}

/// Verifies an unknown completion cannot release a pending credit.
#[test]
fn worker_finish_request_rejects_mismatched_request_id() {
    let mut worker = NativeBoundaryWorker::new(2);

    assert_eq!(worker.begin_request(request_id(1)), Ok(()));
    assert_eq!(
        worker.finish_request(request_id(2)),
        Err(worker_error_reply(ErrorKind::InvalidRequest))
    );
    assert_eq!(worker.reserved_credits(), 1);
    assert_eq!(worker.available_credits(), 1);
}

/// Verifies handles produced by one worker call can be disposed safely.
///
/// Inputs:
/// - JSON parse operation followed by dispose operation.
///
/// Output:
/// - Test passes when disposal succeeds and stale reuse is rejected.
///
/// Transformation:
/// - Exercises resource-backed runtime ownership through the worker
///   request/credit envelope.
#[test]
fn worker_disposes_runtime_resources() {
    let mut worker = NativeBoundaryWorker::new(2);
    let Some(handle) = handle_reply(worker.call(
        request_id(1),
        "std.data.json.parse",
        &[NativeBoundaryTerm::Text(String::from("null"))],
    )) else {
        return;
    };

    let disposed = worker.dispose(request_id(2), handle);
    assert_eq!(
        disposed.result,
        NativeBoundaryReplyTerm::Ok(NativeBoundaryTerm::Unit)
    );
    assert_eq!(disposed.reserved_credits, 0);

    let stale = worker.call(
        request_id(3),
        "std.data.json.is_null",
        &[NativeBoundaryTerm::Handle {
            id: handle.id,
            generation: handle.generation,
        }],
    );
    assert!(matches!(
        stale.result,
        NativeBoundaryReplyTerm::Error { .. }
    ));
    assert_eq!(stale.reserved_credits, 0);
}

/// Verifies command terms execute through the worker dispose path.
///
/// Inputs:
/// - JSON parse call followed by a dispose command term for the returned
///   handle.
///
/// Output:
/// - Test passes when disposal succeeds and all credits are released.
///
/// Transformation:
/// - Exercises command-level disposal without duplicating resource cleanup
///   logic outside the worker.
#[test]
fn worker_execute_command_runs_dispose_terms() {
    let mut worker = NativeBoundaryWorker::new(2);
    let Some(handle) = handle_reply(worker.call(
        request_id(20),
        "std.data.json.parse",
        &[NativeBoundaryTerm::Text(String::from("null"))],
    )) else {
        return;
    };
    let command = NativeBoundaryCommandTerm::Dispose {
        request_id: 21,
        handle,
    };

    let reply = worker.execute_command(&command);

    assert_eq!(reply.request_id, request_id(21));
    assert_eq!(
        reply.result,
        NativeBoundaryReplyTerm::Ok(NativeBoundaryTerm::Unit)
    );
    assert_eq!(reply.reserved_credits, 0);
    assert_eq!(reply.available_credits, 2);
}

/// Verifies cancelling a pending request releases credit and returns a stable error.
///
/// Inputs:
/// - Worker with one pending request.
///
/// Output:
/// - Cancellation error reply, released credit, and rejected late completion.
///
/// Transformation:
/// - Exercises the async cancellation path without executing a native
///   operation synchronously.
#[test]
fn worker_cancel_request_releases_credit_and_rejects_late_reply() {
    let mut worker = NativeBoundaryWorker::new(1);

    assert_eq!(worker.begin_request(request_id(1)), Ok(()));
    let cancelled = worker.cancel_request(request_id(1));

    assert_eq!(cancelled, worker_error_reply(ErrorKind::Cancelled));
    assert_eq!(worker.reserved_credits(), 0);
    assert_eq!(worker.available_credits(), 1);
    assert!(worker.requests.is_empty());
    assert_eq!(
        worker.finish_request(request_id(1)),
        Err(worker_error_reply(ErrorKind::InvalidRequest))
    );
    assert_eq!(
        worker.begin_request(request_id(1)),
        Err(worker_error_reply(ErrorKind::InvalidRequest))
    );
}

/// Verifies completion and cancellation cannot race through request-id reuse.
///
/// Inputs:
/// - A request completed before a late cancellation and a newer request id.
///
/// Output:
/// - Late cancellation and stale id reuse fail without consuming credit.
/// - The next monotonic id remains usable and independently cancellable.
///
/// Transformation:
/// - Exercises the completion-wins ordering that complements the existing
///   cancellation-wins regression and prevents an ABA lifecycle transition.
#[test]
fn worker_completion_wins_cancellation_race_without_request_id_reuse() {
    let mut worker = NativeBoundaryWorker::new(1);

    assert_eq!(worker.begin_request(request_id(1)), Ok(()));
    assert_eq!(worker.finish_request(request_id(1)), Ok(()));
    assert_eq!(
        worker.cancel_request(request_id(1)),
        worker_error_reply(ErrorKind::InvalidRequest)
    );
    assert_eq!(
        worker.begin_request(request_id(1)),
        Err(worker_error_reply(ErrorKind::InvalidRequest))
    );
    assert_eq!(worker.reserved_credits(), 0);

    assert_eq!(worker.begin_request(request_id(2)), Ok(()));
    assert_eq!(
        worker.cancel_request(request_id(2)),
        worker_error_reply(ErrorKind::Cancelled)
    );
    assert_eq!(worker.reserved_credits(), 0);
}

/// Verifies timing out a pending request releases credit and returns a stable error.
///
/// Inputs:
/// - Worker with one pending request.
///
/// Output:
/// - Timeout error reply, released credit, and rejected late completion.
///
/// Transformation:
/// - Exercises the async timeout path that actor scheduling will use when a
///   native worker does not reply in time.
#[test]
fn worker_timeout_request_releases_credit_and_rejects_late_reply() {
    let mut worker = NativeBoundaryWorker::new(1);

    assert_eq!(worker.begin_request(request_id(1)), Ok(()));
    let timed_out = worker.timeout_request(request_id(1));

    assert_eq!(timed_out, worker_error_reply(ErrorKind::Timeout));
    assert_eq!(worker.reserved_credits(), 0);
    assert_eq!(worker.available_credits(), 1);
    assert!(worker.requests.is_empty());
    assert_eq!(
        worker.finish_request(request_id(1)),
        Err(worker_error_reply(ErrorKind::InvalidRequest))
    );
}

/// Verifies cancelling an unknown request returns the invalid-request error.
///
/// Inputs:
/// - Worker with no pending request for the supplied id.
///
/// Output:
/// - Stable invalid-request error and unchanged credit accounting.
///
/// Transformation:
/// - Prevents cancellation commands from fabricating terminal request state.
#[test]
fn worker_cancel_request_rejects_unknown_request() {
    let mut worker = NativeBoundaryWorker::new(1);

    assert_eq!(
        worker.cancel_request(request_id(1)),
        worker_error_reply(ErrorKind::InvalidRequest)
    );
    assert_eq!(worker.reserved_credits(), 0);
    assert_eq!(worker.available_credits(), 1);
}

/// Verifies worker-level duplicate cleanup keeps credit accounting stable.
///
/// Inputs:
/// - JSON parse handle disposed twice through separate request ids.
///
/// Output:
/// - First cleanup succeeds.
/// - Second cleanup returns a stale-handle error and releases its request
///   credit.
///
/// Transformation:
/// - Exercises cleanup race handling through the worker envelope rather than
///   directly through the resource store.
#[test]
fn worker_duplicate_dispose_returns_stale_handle_and_releases_credit() {
    let mut worker = NativeBoundaryWorker::new(1);
    let Some(handle) = handle_reply(worker.call(
        request_id(1),
        "std.data.json.parse",
        &[NativeBoundaryTerm::Text(String::from("null"))],
    )) else {
        return;
    };

    assert_eq!(
        worker.dispose(request_id(2), handle).result,
        NativeBoundaryReplyTerm::Ok(NativeBoundaryTerm::Unit)
    );
    let duplicate = worker.dispose(request_id(3), handle);

    assert_eq!(duplicate.request_id, request_id(3));
    assert_eq!(duplicate.reserved_credits, 0);
    assert_eq!(duplicate.available_credits, 1);
    assert!(matches!(
        duplicate.result,
        NativeBoundaryReplyTerm::Error { ref code, .. } if code == "resource.stale_handle"
    ));
}

#[test]
fn worker_lifecycle_events_write_native_boundary_report() {
    let mut worker = NativeBoundaryWorker::new(1);
    assert_eq!(worker.begin_request(request_id(1)), Ok(()));
    worker.cancel_request(request_id(1));
    assert!(worker.finish_request(request_id(1)).is_err());
    assert_eq!(worker.begin_request(request_id(2)), Ok(()));
    worker.timeout_request(request_id(2));
    worker.call(
        request_id(3),
        "std.encoding.base64.encode",
        &[NativeBoundaryTerm::Text("report".to_string())],
    );
    assert_eq!(worker.begin_request(request_id(4)), Ok(()));
    assert!(worker.begin_request(request_id(5)).is_err());
    assert_eq!(worker.finish_request(request_id(4)), Ok(()));

    let outcomes = worker
        .events()
        .map(|event| event.outcome)
        .collect::<Vec<_>>();
    assert_eq!(
        outcomes,
        vec![
            NativeBoundaryWorkerOutcome::Accepted,
            NativeBoundaryWorkerOutcome::Cancelled,
            NativeBoundaryWorkerOutcome::RejectedInvalidRequest,
            NativeBoundaryWorkerOutcome::Accepted,
            NativeBoundaryWorkerOutcome::TimedOut,
            NativeBoundaryWorkerOutcome::Accepted,
            NativeBoundaryWorkerOutcome::Completed,
            NativeBoundaryWorkerOutcome::Accepted,
            NativeBoundaryWorkerOutcome::RejectedBackpressure,
            NativeBoundaryWorkerOutcome::Completed,
        ]
    );
    let handle = handle_reply(worker.call(
        request_id(6),
        "std.data.json.parse",
        &[NativeBoundaryTerm::Text("null".to_string())],
    ))
    .expect("JSON handle");
    assert_eq!(
        worker.dispose(request_id(7), handle).result,
        NativeBoundaryReplyTerm::Ok(NativeBoundaryTerm::Unit)
    );
    assert!(matches!(
        worker.dispose(request_id(8), handle).result,
        NativeBoundaryReplyTerm::Error { ref code, .. } if code == "resource.stale_handle"
    ));
    worker.call(
        request_id(9),
        "std.db.postgres.connect",
        &[NativeBoundaryTerm::PostgresConfig(
            crate::terlan_native::postgres::Config::new("sqlite://local.db"),
        )],
    );
    worker.call(
        request_id(10),
        "std.db.postgres.string",
        &[
            NativeBoundaryTerm::Handle {
                id: 999,
                generation: 1,
            },
            NativeBoundaryTerm::Text("status".to_string()),
        ],
    );
    worker.call(
        request_id(11),
        "std.db.postgres.execute",
        &[
            NativeBoundaryTerm::Handle {
                id: 999,
                generation: 1,
            },
            NativeBoundaryTerm::Text("SELECT 1".to_string()),
            NativeBoundaryTerm::List(Vec::new()),
        ],
    );
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/quality/vm-native-boundary-report.json");
    worker.write_report(&path).expect("write report");
    let report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).expect("read NativeBoundary report"))
            .expect("parse NativeBoundary report");
    assert_eq!(report["schema"], "terlan-vm-native-boundary-report-v1");
    assert_eq!(report["events"].as_array().expect("events").len(), 22);
    assert_eq!(
        report["resourceEvents"]
            .as_array()
            .expect("resources")
            .len(),
        5
    );
    assert_eq!(report["resourceEvents"][0]["outcome"], "created");
    assert_eq!(report["resourceEvents"][1]["outcome"], "disposed");
    assert_eq!(report["resourceEvents"][2]["outcome"], "rejected");
    assert_eq!(
        report["resourceEvents"][2]["errorCode"],
        "resource.stale_handle"
    );
    assert_eq!(report["workerClassUsage"]["fast"], 1);
    assert_eq!(report["workerClassUsage"]["blocking"], 1);
    assert_eq!(report["workerClassUsage"]["resource_owning"], 1);
    assert_eq!(report["workerClassUsage"]["unclassified"], 2);
    let proof = &report["proofManifestCorrelation"];
    assert_eq!(proof["featureClass"], "native-boundary");
    assert_eq!(proof["status"], "current");
    assert_eq!(proof["proofFamily"], "native-boundary");
    assert_eq!(
        proof["proofPath"],
        "proofs/lean/native_boundary/NativeBoundary.lean"
    );
    assert!(proof["proofDigest"]
        .as_str()
        .is_some_and(|digest| digest.starts_with("sha256:") && digest.len() == 71));
    assert_eq!(
        proof["bridgeStatus"],
        "runtime-sources-fingerprinted; full Aeneas/Rust refinement pending"
    );
    assert_eq!(proof["runtimeManifest"], "std.db.Postgres");
    assert_eq!(proof["runtimeManifestExports"], 9);
    assert_eq!(proof["correlatedDispatches"], 3);
    assert_eq!(proof["unmanifestedDispatches"], 2);
    let gaps = include_str!("../../../../../docs/compiler/proof_track/lean_proof_gaps.tsv");
    let gap = gaps
        .lines()
        .find(|line| line.starts_with("native-boundary contracts\t"))
        .expect("NativeBoundary proof gap");
    let gap = gap.split('\t').collect::<Vec<_>>();
    assert_eq!(proof["gapCategory"], gap[0]);
    assert_eq!(proof["owner"], gap[4]);
    assert_eq!(proof["plannedGate"], gap[5]);
    let replay = include_str!("../../../../../proofs/lean/artifacts/native-boundary.json");
    let replay: serde_json::Value = serde_json::from_str(replay).expect("proof replay metadata");
    assert_eq!(proof["proofDigest"], replay["source_digest"]);
    assert_eq!(report["reservedCredits"], 0);
}

#[test]
fn worker_lifecycle_event_history_is_bounded() {
    let mut worker = NativeBoundaryWorker::new(1);
    for id in 1..=1_100 {
        worker.call(
            request_id(id),
            "std.encoding.base64.encode",
            &[NativeBoundaryTerm::Text("bounded".to_string())],
        );
    }
    let events = worker.events().copied().collect::<Vec<_>>();
    assert_eq!(events.len(), EVENT_HISTORY_LIMIT);
    assert_eq!(
        worker.runtime.dispatch_events().count(),
        EVENT_HISTORY_LIMIT
    );
    assert_eq!(events.first().expect("first").request_id, 589);
    assert_eq!(events.last().expect("last").request_id, 1_100);
    assert_eq!(
        events.last().expect("last").outcome,
        NativeBoundaryWorkerOutcome::Completed
    );
}
