use super::super::{VmActorRuntime, VmExitReason, VmProcessSource, VmRuntimeEnvironmentProfile};
use crate::runtime::vm::ReplValue;

const ACTOR_COUNT: usize = 64;

fn source(index: usize) -> VmProcessSource {
    VmProcessSource::new("parity.FinalHealth", format!("worker_{index}"), 0)
}

fn profile(process_limit: usize) -> VmRuntimeEnvironmentProfile {
    VmRuntimeEnvironmentProfile::new(process_limit, 4).expect("final-health profile")
}

/// Replaces a_SUITE's first-test baseline holders with one immutable VM-owned
/// health snapshot before any suite workload exists.
#[test]
fn a_suite_initial_runtime_health_is_clean_stable_and_fully_owned() {
    let runtime = VmActorRuntime::default();
    let initial = runtime
        .observation_snapshot(profile(ACTOR_COUNT))
        .expect("initial runtime-health snapshot");

    assert_eq!(initial.environment.total_processes, 0);
    assert_eq!(initial.environment.live_processes, 0);
    assert_eq!(initial.environment.exited_processes, 0);
    assert_eq!(initial.environment.run_queue, 0);
    assert_eq!(initial.environment.mailbox_messages, 0);
    assert_eq!(initial.environment.resource_handles, 0);
    assert_eq!(initial.environment.active_timers, 0);
    assert_eq!(initial.environment.timers_started, 0);
    assert!(initial.timers.is_empty());
    assert!(runtime.live_process_ids().is_empty());
    assert!(runtime.registered_names().is_empty());
    assert_eq!(runtime.alias_count(), 0);
    assert_eq!(runtime.scheduled_len(), 0);
    assert_eq!(runtime.delayed_send_count(), 0);
    assert!(runtime.resource_snapshots().is_empty());
    assert_eq!(
        initial,
        runtime
            .observation_snapshot(profile(ACTOR_COUNT))
            .expect("repeated initial runtime-health snapshot")
    );
}

/// Replaces z_SUITE's last-test process, timer, registry, resource, and
/// collector inspection with a deterministic VM-owned shutdown invariant.
#[test]
fn z_suite_final_runtime_health_is_clean_stable_and_fully_owned() {
    let mut runtime = VmActorRuntime::default();
    let mut actors = Vec::with_capacity(ACTOR_COUNT);

    for index in 0..ACTOR_COUNT {
        let pid = runtime.spawn_root(source(index));
        runtime
            .register_name(format!("final-health-{index}"), pid)
            .expect("register final-health actor");
        runtime.create_alias(pid).expect("create owned alias");
        runtime
            .send_after(
                pid,
                pid,
                ReplValue::Int(index as i64),
                0,
                3_600_000 + index as u64,
            )
            .expect("schedule long-horizon final-health timer");

        if index % 8 == 0 {
            let request = 10_000 + index as u64;
            let continuation = 20_000 + index as u64;
            runtime
                .park_native_continuation(pid.as_u64(), request, continuation)
                .expect("park resource allocation continuation");
            runtime
                .service_native_resource(pid.as_u64(), request, continuation, index as u64 + 1)
                .expect("register actor-owned resource");
        }
        actors.push(pid);
    }

    let busy = runtime
        .observation_snapshot(profile(ACTOR_COUNT))
        .expect("busy final-health snapshot");
    assert_eq!(busy.environment.live_processes, ACTOR_COUNT);
    assert_eq!(busy.environment.active_timers, ACTOR_COUNT);
    assert_eq!(busy.environment.resource_handles, ACTOR_COUNT / 8);
    assert_eq!(runtime.alias_count(), ACTOR_COUNT);
    assert_eq!(runtime.registered_names().len(), ACTOR_COUNT);

    for pid in actors.into_iter().rev() {
        runtime
            .exit_actor(pid, VmExitReason::Normal)
            .expect("exit final-health actor");
    }

    let clean = runtime
        .observation_snapshot(profile(ACTOR_COUNT))
        .expect("clean final-health snapshot");
    assert_eq!(clean.environment.total_processes, ACTOR_COUNT);
    assert_eq!(clean.environment.live_processes, 0);
    assert_eq!(clean.environment.exited_processes, ACTOR_COUNT);
    assert_eq!(clean.environment.run_queue, 0);
    assert_eq!(clean.environment.mailbox_messages, 0);
    assert_eq!(clean.environment.resource_handles, 0);
    assert_eq!(clean.environment.active_timers, 0);
    assert_eq!(clean.environment.timers_started, ACTOR_COUNT as u64);
    assert_eq!(clean.timer_metrics.owner_exited, ACTOR_COUNT as u64);
    assert!(clean.timers.is_empty());
    assert!(runtime.live_process_ids().is_empty());
    assert!(runtime.registered_names().is_empty());
    assert_eq!(runtime.alias_count(), 0);
    assert_eq!(runtime.scheduled_len(), 0);
    assert_eq!(runtime.delayed_send_count(), 0);
    assert!(runtime.resource_snapshots().is_empty());
    assert_eq!(
        clean,
        runtime
            .observation_snapshot(profile(ACTOR_COUNT))
            .expect("repeated final-health snapshot")
    );
}
