use super::super::super::memory::{VmMemoryLimits, VmMemoryPressureOutcome};
use super::super::super::process::{VmExitReason, VmProcessId, VmProcessSource, VmProcessState};
use super::super::super::ReplValue;
use super::super::actor_heap_limit::{VmActorHeapLimitOutcome, VmActorHeapLimitPolicy};
use super::super::{VmActorReceive, VmActorRuntime};

const ALLOCATION_PATHS: [&str; 16] = [
    "list-growth",
    "binary-growth",
    "known-heap-binary",
    "unknown-heap-binary",
    "known-binary-append",
    "unknown-binary-append",
    "known-private-append",
    "unknown-private-append",
    "receive-timeout",
    "receive-message",
    "bif-list-to-binary",
    "bif-binary-to-list",
    "known-bit-match",
    "unknown-bit-match",
    "known-bit-construction",
    "unknown-bit-construction",
];

const INVOCATION_PATHS: [&str; 3] = ["direct", "export-entry", "catch-wrapped"];

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("erts.ProcessMaxHeapSizeParity", name, 0)
}

fn receive_payload(runtime: &mut VmActorRuntime, recipient: VmProcessId) -> ReplValue {
    let VmActorReceive::Message(message) = runtime
        .receive_next_or_block(recipient)
        .expect("max-heap monitor completion should be receivable")
    else {
        panic!("max-heap monitor completion must be queued");
    };
    message.payload
}

#[test]
fn process_max_heap_size_suite_immediate_uncatchable_termination_contract() {
    for invocation in INVOCATION_PATHS {
        for allocation in ALLOCATION_PATHS {
            let mut runtime = VmActorRuntime::with_memory_limits(
                VmMemoryLimits::new(128, 233).expect("valid max-heap parity limits"),
            );
            let observer = runtime.spawn_root(source("observer"));
            let target = runtime.spawn_root(source(&format!("{invocation}-{allocation}")));
            let monitor_ref = runtime
                .monitor_actor(observer, target)
                .expect("monitor max-heap target");

            let initial = runtime
                .reserve_actor_heap(target, 200, VmActorHeapLimitPolicy::Reject)
                .expect("initial allocation remains below the hard limit");
            assert_eq!(
                initial.pressure.outcome,
                VmMemoryPressureOutcome::SoftLimitExceeded
            );
            assert!(!initial.exited);

            let terminal = runtime
                .reserve_actor_heap(target, 34, VmActorHeapLimitPolicy::Kill)
                .expect("hard-limit policy performs a process exit");
            assert_eq!(
                terminal,
                VmActorHeapLimitOutcome {
                    pressure: super::super::super::memory::VmMemoryPressureDecision {
                        pid: target.as_u64(),
                        requested_bytes: 34,
                        previous_bytes: 200,
                        projected_bytes: 234,
                        outcome: VmMemoryPressureOutcome::HardLimitRejected,
                    },
                    exited: true,
                },
                "allocation path {allocation} through {invocation}"
            );
            assert_eq!(runtime.process_info_snapshot(target), None);
            assert_eq!(
                runtime
                    .processes()
                    .snapshot(target)
                    .expect("postmortem max-heap state")
                    .state,
                VmProcessState::Exited(VmExitReason::Killed),
                "catch wrappers cannot observe or recover from max-heap kill"
            );
            assert_eq!(
                runtime
                    .memory_metrics(target)
                    .expect("exited process keeps memory telemetry")
                    .current_bytes,
                0
            );
            assert_eq!(
                receive_payload(&mut runtime, observer),
                ReplValue::Tuple(vec![
                    ReplValue::Atom("down".to_string()),
                    ReplValue::Int(monitor_ref.as_u64() as i64),
                    ReplValue::Int(target.as_u64() as i64),
                    ReplValue::Atom("killed".to_string()),
                ])
            );
            assert_eq!(
                runtime
                    .reserve_actor_heap(target, 1, VmActorHeapLimitPolicy::Kill)
                    .expect_err("an exited process cannot allocate again"),
                format!(
                    "exited process {} cannot own VM heap bytes",
                    target.as_u64()
                )
            );
        }
    }
}

#[test]
fn process_max_heap_size_suite_reject_policy_is_atomic_contract() {
    let mut runtime = VmActorRuntime::with_memory_limits(
        VmMemoryLimits::new(128, 233).expect("valid max-heap parity limits"),
    );
    let target = runtime.spawn_root(source("reject-only"));
    runtime
        .reserve_actor_heap(target, 200, VmActorHeapLimitPolicy::Reject)
        .expect("initial heap allocation");

    let rejected = runtime
        .reserve_actor_heap(target, usize::MAX, VmActorHeapLimitPolicy::Reject)
        .expect("reject-only policy returns a typed pressure decision");
    assert_eq!(
        rejected.pressure.outcome,
        VmMemoryPressureOutcome::HardLimitRejected
    );
    assert_eq!(rejected.pressure.projected_bytes, usize::MAX);
    assert!(!rejected.exited);
    assert!(runtime.is_alive(target));
    assert_eq!(
        runtime
            .memory_metrics(target)
            .expect("live process memory metrics")
            .current_bytes,
        200,
        "hard-limit rejection cannot partially mutate heap ownership"
    );
}
