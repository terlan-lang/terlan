use std::io::Cursor;
use std::time::{Duration, Instant};

use crate::compiler::accelerator::{
    AcceleratorAddressSpace, AcceleratorDeviceId, AcceleratorResourceId,
    AcceleratorResourcePrincipal, AcceleratorResourceRole,
};
use crate::runtime::vm::capability_worker::{
    VmCapabilityId, VmCapabilityWorkerClient, VmCapabilityWorkerGeneration, VmCapabilityWorkerId,
    VmCapabilityWorkerIdentity, VmCapabilityWorkerPool, VmCapabilityWorkerPoolSlot,
};
use crate::runtime::vm::execution_shard_epoch::{
    VmShardEpochOperation, VmShardOperationId, VmShardOperationKind, VmShardReplayPolicy,
};
use crate::runtime::vm::execution_shard_protocol::VmShardEpoch;
use crate::terlan_native_boundary::capability_wire::{
    write_json_frame, CapabilityOutcome, CapabilityResponse, CapabilityValue,
    CAPABILITY_PROTOCOL_VERSION,
};
use crate::terlan_native_boundary::term::{NativeBoundaryReplyTerm, NativeBoundaryTerm};

use super::*;

fn limits(operations: u64, bytes: u64) -> VmAcceleratorBudgetLimits {
    let limit = VmAcceleratorScopeLimit::new(operations, bytes).expect("limit");
    VmAcceleratorBudgetLimits {
        stream: limit,
        device: limit,
        actor: limit,
        supervisor: limit,
        application: limit,
        runtime: limit,
    }
}

fn scoped_limits(
    stream: (u64, u64),
    device: (u64, u64),
    hierarchy: (u64, u64),
) -> VmAcceleratorBudgetLimits {
    let hierarchy = VmAcceleratorScopeLimit::new(hierarchy.0, hierarchy.1).expect("hierarchy");
    VmAcceleratorBudgetLimits {
        stream: VmAcceleratorScopeLimit::new(stream.0, stream.1).expect("stream"),
        device: VmAcceleratorScopeLimit::new(device.0, device.1).expect("device"),
        actor: hierarchy,
        supervisor: hierarchy,
        application: hierarchy,
        runtime: hierarchy,
    }
}

fn stream(slot: u64) -> AcceleratorResourceHandle {
    AcceleratorResourceHandle {
        id: AcceleratorResourceId {
            slot,
            generation: 1,
        },
        class: AcceleratorResourceClass::Stream,
        address_space: AcceleratorAddressSpace::Device {
            device: AcceleratorDeviceId::new("cuda", 0).expect("device"),
        },
        role: AcceleratorResourceRole::Owned {
            principal: AcceleratorResourcePrincipal::new("actor").expect("owner"),
        },
    }
}

fn owner(value: u64) -> VmProcessId {
    VmProcessId::from_raw_for_test(value)
}

fn scope(value: u64, supervisor: &str) -> VmAcceleratorOperationScope {
    VmAcceleratorOperationScope::new(owner(value), supervisor, "vision").expect("scope")
}

fn context(value: u64) -> VmCapabilityRequestContext {
    VmCapabilityRequestContext::new(
        VmCapabilityId::new("accelerator.execute").expect("capability"),
        VmShardEpochOperation::new(
            VmShardOperationId::new(value).expect("operation"),
            VmShardEpoch::new(1).expect("epoch"),
            VmShardOperationKind::CapabilityCompletion,
            VmShardReplayPolicy::AtMostOnce,
        ),
    )
    .expect("context")
}

fn responses(values: &[CapabilityResponse]) -> Vec<u8> {
    let mut output = Vec::new();
    for value in values {
        write_json_frame(&mut output, value, 4_096).expect("frame");
    }
    output
}

fn reply(request: u64, credits: u64) -> CapabilityResponse {
    CapabilityResponse::Reply {
        version: CAPABILITY_PROTOCOL_VERSION,
        request_id: request,
        reserved_credits: 0,
        available_credits: credits,
        outcome: CapabilityOutcome::Ok {
            value: CapabilityValue::Unit,
        },
    }
}

fn runtime(
    credits: u64,
    output: Vec<u8>,
    budget: VmAcceleratorBudgetLimits,
) -> VmAcceleratorOperationRuntime<&'static str> {
    let identity = VmCapabilityWorkerIdentity::new(
        VmCapabilityWorkerId::new("accelerator-0").expect("worker"),
        VmCapabilityWorkerGeneration::new(1).expect("generation"),
    );
    let client = VmCapabilityWorkerClient::from_test_streams(
        identity,
        &["accelerator.execute"],
        credits,
        Vec::<u8>::new(),
        Cursor::new(output),
    )
    .expect("client");
    let slot = VmCapabilityWorkerPoolSlot::new(client, credits).expect("slot");
    let pool = VmCapabilityWorkerPool::new(vec![slot]).expect("pool");
    VmAcceleratorOperationRuntime::new(VmCapabilityWorkerEventPump::new(pool), budget)
}

fn submit(
    runtime: &mut VmAcceleratorOperationRuntime<&'static str>,
    actor: u64,
    supervisor: &str,
    deadline: u64,
    payload: &'static str,
) -> VmAcceleratorOperationId {
    runtime
        .submit(
            VmAcceleratorSubmission::new(
                scope(actor, supervisor),
                context(actor),
                "cuda.kernel.launch",
                Vec::new(),
                stream(actor),
                64,
                deadline,
            ),
            payload,
        )
        .expect("submit")
}

#[test]
fn independent_streams_complete_without_scheduler_blocking() {
    let mut runtime = runtime(2, responses(&[reply(1, 2), reply(2, 2)]), limits(2, 128));
    let first = submit(&mut runtime, 1, "left", 100, "left-continuation");
    let second = submit(&mut runtime, 2, "right", 100, "right-continuation");
    assert_ne!(first, second);
    assert_eq!(runtime.runtime_usage(), (2, 128));
    assert_eq!(runtime.snapshots().len(), 2);

    let deadline = Instant::now() + Duration::from_secs(1);
    let mut terminals = Vec::new();
    while terminals.len() != 2 {
        terminals.extend(runtime.poll().expect("poll"));
        assert!(Instant::now() < deadline, "completion timed out");
        std::thread::yield_now();
    }
    terminals.sort_by_key(|terminal| terminal.id);
    assert_eq!(terminals[0].owner, owner(1));
    assert_eq!(terminals[1].owner, owner(2));
    assert_eq!(terminals[0].payload, "left-continuation");
    assert_eq!(terminals[1].payload, "right-continuation");
    assert!(terminals
        .iter()
        .all(|terminal| terminal.kind == VmAcceleratorTerminalKind::Reply));
    assert_eq!(runtime.runtime_usage(), (0, 0));
    assert!(runtime.snapshots().is_empty());
}

#[test]
fn hierarchical_budgets_reject_without_consuming_payload_or_credit() {
    let mut runtime = runtime(2, Vec::new(), scoped_limits((2, 128), (2, 128), (1, 64)));
    submit(&mut runtime, 1, "vision", 100, "first");
    let (error, payload) = runtime
        .submit(
            VmAcceleratorSubmission::new(
                scope(2, "vision"),
                context(2),
                "cuda.kernel.launch",
                Vec::new(),
                stream(2),
                64,
                100,
            ),
            "second",
        )
        .expect_err("runtime budget");
    assert!(error.contains("supervisor") || error.contains("runtime"));
    assert_eq!(payload, "second");
    assert_eq!(runtime.runtime_usage(), (1, 64));
    let terminal = runtime.cancel(VmAcceleratorOperationId(1)).expect("cancel");
    assert_eq!(terminal.kind, VmAcceleratorTerminalKind::Cancelled);
    assert_eq!(runtime.runtime_usage(), (0, 0));
}

#[test]
fn stream_and_device_budgets_share_the_generic_admission_ledger() {
    let budget = scoped_limits((1, 64), (2, 128), (4, 256));
    let mut runtime = runtime(4, Vec::new(), budget);
    submit(&mut runtime, 1, "left", 100, "first");
    let (error, payload) = runtime
        .submit(
            VmAcceleratorSubmission::new(
                scope(2, "right"),
                context(2),
                "cuda.kernel.launch",
                Vec::new(),
                stream(1),
                64,
                100,
            ),
            "same-stream",
        )
        .expect_err("stream budget");
    assert!(error.contains("stream"));
    assert_eq!(payload, "same-stream");

    submit(&mut runtime, 2, "right", 100, "second-stream");
    let (error, payload) = runtime
        .submit(
            VmAcceleratorSubmission::new(
                scope(3, "third"),
                context(3),
                "cuda.kernel.launch",
                Vec::new(),
                stream(3),
                64,
                100,
            ),
            "same-device",
        )
        .expect_err("device budget");
    assert!(error.contains("device"));
    assert_eq!(payload, "same-device");
    assert_eq!(runtime.runtime_usage(), (2, 128));

    runtime.cancel(VmAcceleratorOperationId(1)).expect("first");
    runtime.cancel(VmAcceleratorOperationId(2)).expect("second");
    assert_eq!(runtime.runtime_usage(), (0, 0));
}

#[test]
fn timeout_owner_exit_and_shutdown_are_exactly_once_terminal_paths() {
    let mut runtime = runtime(3, Vec::new(), limits(3, 192));
    let timeout = submit(&mut runtime, 1, "vision", 10, "timeout");
    submit(&mut runtime, 2, "vision", 20, "owner-exit");
    submit(&mut runtime, 3, "vision", 30, "shutdown");

    let expired = runtime.expire(10);
    assert_eq!(expired.len(), 1);
    let expired = expired.into_iter().next().expect("entry").expect("timeout");
    assert_eq!(expired.id, timeout);
    assert_eq!(expired.kind, VmAcceleratorTerminalKind::TimedOut);
    assert!(runtime
        .cancel(timeout)
        .unwrap_err()
        .contains("operation_missing"));

    let exited = runtime.close_owner(owner(2));
    assert_eq!(exited.len(), 1);
    assert_eq!(
        exited
            .into_iter()
            .next()
            .expect("entry")
            .expect("exit")
            .kind,
        VmAcceleratorTerminalKind::OwnerExited
    );
    let (shutdown, errors) = runtime.shutdown();
    assert!(errors.is_empty(), "shutdown errors: {errors:?}");
    assert_eq!(shutdown.len(), 1);
    assert_eq!(shutdown[0].kind, VmAcceleratorTerminalKind::RuntimeShutdown);
    assert_eq!(runtime.runtime_usage(), (0, 0));
}

#[test]
fn malformed_admission_and_failed_worker_do_not_leak_accounting() {
    assert!(VmAcceleratorScopeLimit::new(0, 1).is_err());
    assert!(VmAcceleratorScopeLimit::new(1, 0).is_err());
    assert!(VmAcceleratorOperationScope::new(owner(1), "", "app").is_err());
    let mut runtime = runtime(1, Vec::new(), limits(1, 64));
    let mut wrong = stream(1);
    wrong.class = AcceleratorResourceClass::Allocation;
    let (_, payload) = runtime
        .submit(
            VmAcceleratorSubmission::new(
                scope(1, "vision"),
                context(1),
                "cuda.kernel.launch",
                Vec::new(),
                wrong,
                64,
                100,
            ),
            "wrong",
        )
        .expect_err("wrong resource class");
    assert_eq!(payload, "wrong");
    assert_eq!(runtime.runtime_usage(), (0, 0));

    submit(&mut runtime, 1, "vision", 100, "lost");
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        let terminal = runtime.poll().expect("poll");
        if !terminal.is_empty() {
            assert_eq!(terminal[0].kind, VmAcceleratorTerminalKind::WorkerFailed);
            assert!(matches!(
                terminal[0].reply,
                NativeBoundaryReplyTerm::Error { .. }
            ));
            break;
        }
        assert!(Instant::now() < deadline, "worker-loss event timed out");
        std::thread::yield_now();
    }
    assert_eq!(runtime.runtime_usage(), (0, 0));
}

#[test]
fn inspection_is_pointer_free_and_reply_classifier_handles_both_paths() {
    let mut runtime = runtime(1, Vec::new(), limits(1, 64));
    submit(&mut runtime, 7, "vision", 123, "pending");
    let snapshot = runtime.snapshots().pop().expect("snapshot");
    assert_eq!(snapshot.owner, 7);
    assert_eq!(snapshot.stream_slot, 7);
    assert_eq!(snapshot.stream_generation, 1);
    assert_eq!(snapshot.device_bytes, 64);
    assert!(matches!(
        NativeBoundaryReplyTerm::Ok(NativeBoundaryTerm::Unit),
        NativeBoundaryReplyTerm::Ok(_)
    ));
    assert!(matches!(
        error_reply("device_loss", "lost"),
        NativeBoundaryReplyTerm::Error { .. }
    ));
}
