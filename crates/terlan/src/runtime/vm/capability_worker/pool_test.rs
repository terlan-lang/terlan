use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Write};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::*;
use crate::runtime::vm::capability_worker::{
    VmCapabilityWorkerEventPump, VmCapabilityWorkerEventPumpEvent,
};
use crate::runtime::vm::execution_shard_epoch::{
    VmShardEpochOperation, VmShardOperationId, VmShardOperationKind, VmShardReplayPolicy,
};
use crate::runtime::vm::execution_shard_protocol::VmShardEpoch;
use crate::runtime::vm::process::{VmProcessSource, VmProcessState, VmProcessTable};
use crate::runtime::vm::scheduler::{VmScheduler, VmSchedulerConfig};
use crate::runtime::vm::timer::VmTimerTable;
use crate::terlan_native_boundary::capability_wire::{
    write_json_frame, CapabilityOutcome, CapabilityResponse, CapabilityValue,
    CAPABILITY_PROTOCOL_VERSION,
};
use crate::terlan_native_boundary::request::RequestId;
use crate::terlan_native_boundary::term::{NativeBoundaryReplyTerm, NativeBoundaryTerm};

/// Thread-safe sink retaining worker request frames for assertions.
#[derive(Clone, Default)]
struct CapturedFrames {
    bytes: Arc<Mutex<Vec<u8>>>,
}

impl CapturedFrames {
    /// Returns a stable snapshot of frames emitted before this call.
    fn snapshot(&self) -> Vec<u8> {
        self.bytes.lock().expect("captured frames lock").clone()
    }
}

impl Write for CapturedFrames {
    /// Appends one writer chunk to bounded in-memory test storage.
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.bytes
            .lock()
            .map_err(|_| std::io::Error::other("captured frames lock poisoned"))?
            .extend_from_slice(bytes);
        Ok(bytes.len())
    }

    /// Flushes the in-memory sink without an external side effect.
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Creates deterministic worker output containing the supplied responses.
fn response_bytes(responses: &[CapabilityResponse]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for response in responses {
        write_json_frame(&mut bytes, response, 4_096).expect("response frame");
    }
    bytes
}

/// Creates one in-memory worker client with explicit identity and credits.
fn client(
    name: &str,
    generation: u64,
    credits: u64,
    capabilities: &[&str],
    output: Vec<u8>,
) -> (VmCapabilityWorkerClient, CapturedFrames) {
    let captured = CapturedFrames::default();
    let transport = super::super::VmCapabilityWorkerTransport::from_streams(
        captured.clone(),
        Cursor::new(output),
        4_096,
        credits,
        None,
        None,
    )
    .expect("in-memory worker transport");
    let identity = VmCapabilityWorkerIdentity::new(
        super::super::VmCapabilityWorkerId::new(name).expect("worker id"),
        super::super::VmCapabilityWorkerGeneration::new(generation).expect("generation"),
    );
    (
        VmCapabilityWorkerClient {
            identity,
            transport,
            deadlines: super::super::VmNativeBoundaryDeadlineQueue::new(credits),
            pending_contexts: BTreeMap::new(),
            parked_contexts: BTreeMap::new(),
            capabilities: capabilities
                .iter()
                .map(|capability| (*capability).to_string())
                .collect::<BTreeSet<_>>(),
            last_request_id: RequestId { value: 0 },
            remote_credit_limit: credits,
        },
        captured,
    )
}

/// Creates one actor runtime table with the requested number of owners.
fn runtime(
    owner_count: usize,
) -> (
    VmProcessTable,
    VmScheduler,
    VmTimerTable,
    Vec<VmProcessId>,
) {
    let mut processes = VmProcessTable::default();
    let owners = (0..owner_count)
        .map(|index| {
            processes.spawn_root(VmProcessSource::new(
                "app.CapabilityPool",
                format!("owner_{index}"),
                0,
            ))
        })
        .collect();
    (
        processes,
        VmScheduler::new(VmSchedulerConfig::new(10, 100)),
        VmTimerTable::default(),
        owners,
    )
}

/// Creates exact capability-completion authority for one operation identity.
fn context(operation: u64, capability: &str) -> VmCapabilityRequestContext {
    VmCapabilityRequestContext::new(
        VmCapabilityId::new(capability).expect("capability"),
        VmShardEpochOperation::new(
            VmShardOperationId::new(operation).expect("operation id"),
            VmShardEpoch::new(1).expect("epoch"),
            VmShardOperationKind::CapabilityCompletion,
            VmShardReplayPolicy::AtMostOnce,
        ),
    )
    .expect("request context")
}

/// Builds one successful response with consistent remote credit telemetry.
fn reply(request_id: u64, credits: u64) -> CapabilityResponse {
    CapabilityResponse::Reply {
        version: CAPABILITY_PROTOCOL_VERSION,
        request_id,
        reserved_credits: 0,
        available_credits: credits,
        outcome: CapabilityOutcome::Ok {
            value: CapabilityValue::Unit,
        },
    }
}

/// Polls until the asynchronous transport produces one pool event.
fn wait_for_event(
    pool: &mut VmCapabilityWorkerPool,
    timers: &mut VmTimerTable,
    processes: &mut VmProcessTable,
    scheduler: &mut VmScheduler,
) -> VmCapabilityWorkerCompletion {
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        let event = pool
            .poll(&mut VmCapabilityWorkerRuntime {
                timers,
                processes,
                scheduler,
            })
            .expect("pool poll");
        if let Some(event) = event {
            return event;
        }
        assert!(Instant::now() < deadline, "worker event timed out");
        std::thread::yield_now();
    }
}

/// Proves a serial slot rejects oversubscription before parking another actor.
#[test]
fn capability_worker_pool_enforces_bounded_non_reentrant_admission() {
    let (worker, captured) = client("serial", 1, 2, &["example"], response_bytes(&[reply(1, 2)]));
    let slot = VmCapabilityWorkerPoolSlot::new(worker, 1).expect("serial slot");
    let mut pool = VmCapabilityWorkerPool::new(vec![slot]).expect("pool");
    let (mut processes, mut scheduler, mut timers, owners) = runtime(2);

    let first = pool
        .start_call(
            &mut VmCapabilityWorkerRuntime {
                timers: &mut timers,
                processes: &mut processes,
                scheduler: &mut scheduler,
            },
            owners[0],
            context(1, "example"),
            "std.example.call",
            Vec::new(),
            0,
            10,
        )
        .expect("first request");
    let rejection = pool
        .start_call(
            &mut VmCapabilityWorkerRuntime {
                timers: &mut timers,
                processes: &mut processes,
                scheduler: &mut scheduler,
            },
            owners[1],
            context(2, "example"),
            "std.example.call",
            Vec::new(),
            0,
            10,
        )
        .expect_err("serial slot must reject concurrent use");

    assert!(rejection.contains("pool_full"));
    assert_eq!(pool.configured_capacity(), 1);
    assert_eq!(pool.available_capacity(), 0);
    assert_eq!(first.worker.id.as_str(), "serial");
    assert_eq!(
        processes.get(owners[0]).expect("first owner").state,
        VmProcessState::Blocked
    );
    assert_eq!(
        processes.get(owners[1]).expect("second owner").state,
        VmProcessState::Runnable
    );
    assert!(matches!(
        wait_for_event(
            &mut pool,
            &mut timers,
            &mut processes,
            &mut scheduler
        ),
        VmCapabilityWorkerCompletion::Reply { request_id, .. }
            if request_id == RequestId { value: 1 }
    ));
    assert!(!captured.snapshot().is_empty());
    assert_eq!(pool.available_capacity(), 1);
    assert_eq!(
        processes.get(owners[0]).expect("first owner").state,
        VmProcessState::Runnable
    );
}

/// Proves exact duplicate replies cannot resume or consume pool capacity twice.
#[test]
fn capability_worker_pool_suppresses_duplicate_completion() {
    let responses = response_bytes(&[reply(1, 1), reply(1, 1)]);
    let (worker, _) = client("duplicate", 1, 1, &["example"], responses);
    let mut pool = VmCapabilityWorkerPool::new(vec![
        VmCapabilityWorkerPoolSlot::new(worker, 1).expect("slot"),
    ])
    .expect("pool");
    let (mut processes, mut scheduler, mut timers, owners) = runtime(1);
    pool.start_call(
        &mut VmCapabilityWorkerRuntime {
            timers: &mut timers,
            processes: &mut processes,
            scheduler: &mut scheduler,
        },
        owners[0],
        context(1, "example"),
        "std.example.call",
        Vec::new(),
        0,
        10,
    )
    .expect("request");

    assert!(matches!(
        wait_for_event(
            &mut pool,
            &mut timers,
            &mut processes,
            &mut scheduler
        ),
        VmCapabilityWorkerCompletion::Reply { .. }
    ));
    assert!(matches!(
        wait_for_event(
            &mut pool,
            &mut timers,
            &mut processes,
            &mut scheduler
        ),
        VmCapabilityWorkerCompletion::StaleReply { request_id, .. }
            if request_id == RequestId { value: 1 }
    ));
    assert_eq!(pool.available_capacity(), 1);
    assert_eq!(
        processes.get(owners[0]).expect("owner").state,
        VmProcessState::Runnable
    );
}

/// Proves a failed slot stays bounded and accepts only its next generation.
#[test]
fn capability_worker_pool_replaces_crashed_slot_without_capacity_bypass() {
    let (worker, _) = client("replaceable", 1, 1, &["example"], b"not-json\n".to_vec());
    let mut pool = VmCapabilityWorkerPool::new(vec![
        VmCapabilityWorkerPoolSlot::new(worker, 1).expect("slot"),
    ])
    .expect("pool");
    let (mut processes, mut scheduler, mut timers, owners) = runtime(1);
    let assignment = pool
        .start_call(
            &mut VmCapabilityWorkerRuntime {
                timers: &mut timers,
                processes: &mut processes,
                scheduler: &mut scheduler,
            },
            owners[0],
            context(1, "example"),
            "std.example.call",
            Vec::new(),
            0,
            10,
        )
        .expect("request");
    assert!(matches!(
        wait_for_event(
            &mut pool,
            &mut timers,
            &mut processes,
            &mut scheduler
        ),
        VmCapabilityWorkerCompletion::TransportFailed { cancelled, .. }
            if cancelled.len() == 1
    ));
    assert_eq!(pool.live_workers(), 0);
    assert_eq!(pool.configured_capacity(), 1);
    assert_eq!(pool.available_capacity(), 0);
    assert_eq!(
        processes.get(owners[0]).expect("owner").state,
        VmProcessState::Runnable
    );

    let (same_generation, _) = client("replaceable", 1, 1, &["example"], Vec::new());
    assert!(pool.replace(same_generation).is_err());
    let (replacement, _) = client("replaceable", 2, 1, &["example"], Vec::new());
    pool.replace(replacement).expect("next generation");
    assert_eq!(pool.live_workers(), 1);
    assert_eq!(pool.available_capacity(), 1);
    assert_eq!(pool.configured_capacity(), 1);

    let stale = pool
        .cancel(
            &mut VmCapabilityWorkerRuntime {
                timers: &mut timers,
                processes: &mut processes,
                scheduler: &mut scheduler,
            },
            &assignment,
        )
        .expect_err("old generation cannot cancel through replacement");
    assert!(stale.contains("stale_generation"));
}

/// Proves duplicate logical slots and undeclared capabilities fail closed.
#[test]
fn capability_worker_pool_rejects_identity_and_capability_bypass() {
    let (first, _) = client("same", 1, 1, &["example"], Vec::new());
    let (second, _) = client("same", 2, 1, &["other"], Vec::new());
    assert!(VmCapabilityWorkerPool::new(vec![
        VmCapabilityWorkerPoolSlot::new(first, 1).expect("first"),
        VmCapabilityWorkerPoolSlot::new(second, 1).expect("second"),
    ])
    .is_err());

    let (worker, captured) = client("closed", 1, 1, &["example"], Vec::new());
    let mut pool = VmCapabilityWorkerPool::new(vec![
        VmCapabilityWorkerPoolSlot::new(worker, 1).expect("slot"),
    ])
    .expect("pool");
    let (mut processes, mut scheduler, mut timers, owners) = runtime(1);
    let error = pool
        .start_call(
            &mut VmCapabilityWorkerRuntime {
                timers: &mut timers,
                processes: &mut processes,
                scheduler: &mut scheduler,
            },
            owners[0],
            context(1, "filesystem"),
            "std.io.file.read_text",
            vec![NativeBoundaryTerm::Text("/tmp/nope".to_string())],
            0,
            10,
        )
        .expect_err("capability bypass");
    assert!(error.contains("pool_capability"));
    assert!(captured.snapshot().is_empty());
    assert_eq!(
        processes.get(owners[0]).expect("owner").state,
        VmProcessState::Runnable
    );
}

/// Proves cancellation releases one exact assignment and suppresses its late reply.
#[test]
fn capability_worker_pool_cancellation_releases_exact_request_credit() {
    let (worker, _) = client("cancel", 1, 1, &["example"], response_bytes(&[reply(1, 1)]));
    let mut pool = VmCapabilityWorkerPool::new(vec![
        VmCapabilityWorkerPoolSlot::new(worker, 1).expect("slot"),
    ])
    .expect("pool");
    let (mut processes, mut scheduler, mut timers, owners) = runtime(1);
    let assignment = pool
        .start_call(
            &mut VmCapabilityWorkerRuntime {
                timers: &mut timers,
                processes: &mut processes,
                scheduler: &mut scheduler,
            },
            owners[0],
            context(1, "example"),
            "std.example.call",
            Vec::new(),
            0,
            10,
        )
        .expect("request");
    let terminal = pool
        .cancel(
            &mut VmCapabilityWorkerRuntime {
                timers: &mut timers,
                processes: &mut processes,
                scheduler: &mut scheduler,
            },
            &assignment,
        )
        .expect("cancel exact assignment");
    assert!(matches!(
        terminal.completion,
        crate::runtime::vm::native_boundary::deadline::VmNativeBoundaryDeadlineCompletion::Cancelled {
            request_id,
            ..
        } if request_id == RequestId { value: 1 }
    ));
    assert_eq!(pool.available_capacity(), 1);
    assert_eq!(
        processes.get(owners[0]).expect("owner").state,
        VmProcessState::Runnable
    );
    assert!(matches!(
        wait_for_event(
            &mut pool,
            &mut timers,
            &mut processes,
            &mut scheduler
        ),
        VmCapabilityWorkerCompletion::StaleReply { request_id, .. }
            if request_id == RequestId { value: 1 }
    ));
    assert_eq!(pool.available_capacity(), 1);
}

/// Proves generated continuations reuse bounded worker transport without proxy parking.
#[test]
fn capability_worker_pool_completes_an_already_parked_generated_request() {
    let (worker, _) = client(
        "generated",
        1,
        1,
        &["filesystem"],
        response_bytes(&[reply(1, 1)]),
    );
    let mut pool = VmCapabilityWorkerPool::new(vec![
        VmCapabilityWorkerPoolSlot::new(worker, 1).expect("slot"),
    ])
    .expect("pool");
    let (processes, _scheduler, _timers, owners) = runtime(1);
    let assignment = pool
        .start_parked_call(
            owners[0],
            context(7, "filesystem"),
            "std.io.file.exists",
            vec![NativeBoundaryTerm::Text("/tmp/example".to_string())],
        )
        .expect("parked request");

    assert_eq!(assignment.owner, owners[0]);
    assert_eq!(assignment.request_id, RequestId { value: 1 });
    assert_eq!(pool.available_capacity(), 0);
    assert_eq!(
        processes.get(owners[0]).expect("owner").state,
        VmProcessState::Runnable,
        "the generated shard, not the worker client, owns continuation parking"
    );

    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        if let Some(completion) = pool.poll_parked().expect("parked poll") {
            assert!(matches!(
                completion,
                VmCapabilityWorkerCompletion::Reply {
                    request_id,
                    context: completed,
                    ..
                } if request_id == assignment.request_id && completed == context(7, "filesystem")
            ));
            break;
        }
        assert!(Instant::now() < deadline, "parked completion timed out");
        std::thread::yield_now();
    }
    assert_eq!(pool.available_capacity(), 1);
}

/// Proves a cancelled generated request cannot consume a late worker reply.
#[test]
fn capability_worker_pool_suppresses_late_already_parked_reply() {
    let (worker, _) = client("generated", 1, 1, &["example"], response_bytes(&[reply(1, 1)]));
    let mut pool = VmCapabilityWorkerPool::new(vec![
        VmCapabilityWorkerPoolSlot::new(worker, 1).expect("slot"),
    ])
    .expect("pool");
    let (_, _, _, owners) = runtime(1);
    let assignment = pool
        .start_parked_call(
            owners[0],
            context(9, "example"),
            "std.example.call",
            Vec::new(),
        )
        .expect("parked request");
    pool.cancel_parked(&assignment).expect("cancel parked");
    assert_eq!(pool.available_capacity(), 1);

    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        if let Some(completion) = pool.poll_parked().expect("parked poll") {
            assert!(matches!(
                completion,
                VmCapabilityWorkerCompletion::StaleReply { request_id, .. }
                    if request_id == assignment.request_id
            ));
            break;
        }
        assert!(Instant::now() < deadline, "late completion timed out");
        std::thread::yield_now();
    }
}

/// Proves the event pump returns the exact retained owner payload with its reply.
#[test]
fn capability_event_pump_correlates_completion_with_fixed_owner_payload() {
    let (worker, _) = client("pump", 1, 1, &["example"], response_bytes(&[reply(1, 1)]));
    let pool = VmCapabilityWorkerPool::new(vec![
        VmCapabilityWorkerPoolSlot::new(worker, 1).expect("slot"),
    ])
    .expect("pool");
    let mut pump = VmCapabilityWorkerEventPump::new(pool);
    let (_, _, _, owners) = runtime(1);
    let assignment = pump
        .submit(
            owners[0],
            context(11, "example"),
            "std.example.call",
            Vec::new(),
            "fixed-owner-continuation",
        )
        .expect("submit");
    assert_eq!(pump.pending_len(), 1);
    assert_eq!(pump.available_capacity(), 0);

    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        if let Some(event) = pump.poll().expect("poll") {
            match event {
                VmCapabilityWorkerEventPumpEvent::Completed {
                    assignment: completed,
                    context: completed_context,
                    reply,
                    payload,
                } => {
                    assert_eq!(completed, assignment);
                    assert_eq!(completed_context, context(11, "example"));
                    assert_eq!(reply, NativeBoundaryReplyTerm::Ok(NativeBoundaryTerm::Unit));
                    assert_eq!(payload, "fixed-owner-continuation");
                }
                _ => panic!("unexpected event-pump event"),
            }
            break;
        }
        assert!(Instant::now() < deadline, "event-pump completion timed out");
        std::thread::yield_now();
    }
    assert_eq!(pump.pending_len(), 0);
    assert_eq!(pump.available_capacity(), 1);
}

/// Proves failed admission and explicit cancellation never lose owner payloads.
#[test]
fn capability_event_pump_returns_payload_on_backpressure_and_cancellation() {
    let (worker, _) = client("pump", 1, 1, &["example"], Vec::new());
    let pool = VmCapabilityWorkerPool::new(vec![
        VmCapabilityWorkerPoolSlot::new(worker, 1).expect("slot"),
    ])
    .expect("pool");
    let mut pump = VmCapabilityWorkerEventPump::new(pool);
    let (_, _, _, owners) = runtime(2);
    let assignment = pump
        .submit(
            owners[0],
            context(12, "example"),
            "std.example.call",
            Vec::new(),
            "first",
        )
        .expect("first submit");
    let (error, payload) = pump
        .submit(
            owners[1],
            context(13, "example"),
            "std.example.call",
            Vec::new(),
            "second",
        )
        .expect_err("bounded pump must reject second submit");
    assert!(error.contains("pool_full"));
    assert_eq!(payload, "second");
    assert_eq!(pump.cancel(&assignment).expect("cancel"), "first");
    assert_eq!(pump.pending_len(), 0);
    assert_eq!(pump.available_capacity(), 1);
}

/// Proves scheduler shutdown recovers every retained caller envelope.
#[test]
fn capability_event_pump_shutdown_returns_all_pending_payloads() {
    let (worker, _) = client("pump", 1, 2, &["example"], Vec::new());
    let pool = VmCapabilityWorkerPool::new(vec![
        VmCapabilityWorkerPoolSlot::new(worker, 2).expect("slot"),
    ])
    .expect("pool");
    let mut pump = VmCapabilityWorkerEventPump::new(pool);
    let (_, _, _, owners) = runtime(2);
    for (index, owner) in owners.into_iter().enumerate() {
        pump.submit(
            owner,
            context(30 + index as u64, "example"),
            "std.example.call",
            Vec::new(),
            index,
        )
        .expect("submit pending payload");
    }

    let (pending, errors) = pump.shutdown();
    assert!(errors.is_empty(), "shutdown transport errors: {errors:?}");
    let mut payloads = pending
        .into_iter()
        .map(|(_, payload)| payload)
        .collect::<Vec<_>>();
    payloads.sort_unstable();
    assert_eq!(payloads, vec![0, 1]);
    assert_eq!(pump.pending_len(), 0);
}

/// Proves worker termination returns every continuation attributed to its generation.
#[test]
fn capability_event_pump_drains_generation_payloads_on_worker_loss() {
    let (worker, _) = client("pump", 1, 2, &["example"], Vec::new());
    let pool = VmCapabilityWorkerPool::new(vec![
        VmCapabilityWorkerPoolSlot::new(worker, 2).expect("slot"),
    ])
    .expect("pool");
    let mut pump = VmCapabilityWorkerEventPump::new(pool);
    let (_, _, _, owners) = runtime(2);
    for (index, owner) in owners.into_iter().enumerate() {
        pump.submit(
            owner,
            context(20 + index as u64, "example"),
            "std.example.call",
            Vec::new(),
            index,
        )
        .expect("submit pending payload");
    }

    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        if let Some(event) = pump.poll().expect("poll") {
            match event {
                VmCapabilityWorkerEventPumpEvent::WorkerLost {
                    worker,
                    reason,
                    mut pending,
                } => {
                    assert_eq!(worker.id.as_str(), "pump");
                    assert!(reason.contains("closed"));
                    pending.sort_by_key(|(_, payload)| *payload);
                    assert_eq!(pending.len(), 2);
                    assert_eq!(pending[0].1, 0);
                    assert_eq!(pending[1].1, 1);
                }
                _ => panic!("unexpected event-pump event"),
            }
            break;
        }
        assert!(Instant::now() < deadline, "worker-loss event timed out");
        std::thread::yield_now();
    }
    assert_eq!(pump.pending_len(), 0);
    assert_eq!(pump.available_capacity(), 0);
}
