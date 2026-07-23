use std::num::NonZeroU64;
use std::sync::Arc;
use std::thread;

use super::*;
use crate::runtime::vm::actor_directory::VmActorLifecycle;
use crate::runtime::vm::execution_shard_protocol::VmShardEpoch;
use crate::runtime::vm::fixed_scheduler_control::VmFixedSchedulerControl;
use crate::runtime::vm::scheduler_topology::VmSchedulerTopology;

#[test]
fn bounded_trace_evicts_oldest_events_and_retains_monotonic_sequence() {
    let topology = VmSchedulerTopology::new(1).expect("topology");
    let scheduler = topology.home_scheduler(NonZeroU64::new(1).expect("actor"));
    let route = topology.route(NonZeroU64::new(1).expect("actor"));
    let telemetry = VmFixedSchedulerTelemetry::new(scheduler, 3).expect("telemetry");
    for kind in [
        VmFixedSchedulerEventKind::Command,
        VmFixedSchedulerEventKind::Entry,
        VmFixedSchedulerEventKind::Parked,
        VmFixedSchedulerEventKind::Completed,
    ] {
        telemetry.record(kind, Some(route)).expect("record event");
    }
    let trace = telemetry.trace().expect("trace");
    assert_eq!(
        trace.iter().map(|event| event.sequence).collect::<Vec<_>>(),
        vec![2, 3, 4]
    );
    assert_eq!(telemetry.snapshot().trace_evictions, 1);
}

#[test]
fn concurrent_producers_update_only_their_scheduler_local_telemetry() {
    let topology = VmSchedulerTopology::new(2).expect("topology");
    let route = topology.route(NonZeroU64::new(2).expect("actor"));
    let telemetry =
        Arc::new(VmFixedSchedulerTelemetry::new(route.scheduler(), 64).expect("telemetry"));
    let workers = (0..4)
        .map(|_| {
            let telemetry = Arc::clone(&telemetry);
            thread::spawn(move || {
                for _ in 0..10 {
                    telemetry
                        .record(
                            VmFixedSchedulerEventKind::IoCompletionPublished,
                            Some(route),
                        )
                        .expect("record message");
                }
            })
        })
        .collect::<Vec<_>>();
    for worker in workers {
        worker.join().expect("producer");
    }
    let snapshot = telemetry.snapshot();
    assert_eq!(snapshot.events, 40);
    assert_eq!(snapshot.io_completions, 40);
    assert_eq!(snapshot.signals, 0);
    assert_eq!(telemetry.trace().expect("trace").len(), 40);
}

#[test]
fn metrics_classify_entry_signal_completion_and_failure() {
    let scheduler = VmSchedulerId::primary();
    let telemetry = VmFixedSchedulerTelemetry::new(scheduler, 8).expect("telemetry");
    for kind in [
        VmFixedSchedulerEventKind::Entry,
        VmFixedSchedulerEventKind::SignalPublished,
        VmFixedSchedulerEventKind::CapabilityCompletionPublished,
        VmFixedSchedulerEventKind::Completed,
        VmFixedSchedulerEventKind::Failed,
    ] {
        telemetry.record(kind, None).expect("record event");
    }
    let snapshot = telemetry.snapshot();
    assert_eq!(snapshot.entries, 1);
    assert_eq!(snapshot.signals, 1);
    assert_eq!(snapshot.capability_completions, 1);
    assert_eq!(snapshot.completions, 1);
    assert_eq!(snapshot.failures, 1);
    assert!(VmFixedSchedulerTelemetry::new(scheduler, 0).is_err());
}

/// Proves generation-qualified events use the same metrics and replay path.
#[test]
fn generation_qualified_events_share_metrics_and_canonical_replay_storage() {
    let topology = VmSchedulerTopology::new(2).expect("topology");
    let scheduler = topology.schedulers().next().expect("scheduler");
    let peer = topology.schedulers().nth(1).expect("peer");
    let telemetry = VmFixedSchedulerTelemetry::new(scheduler, 4).expect("telemetry");
    let context = VmMulticoreEventContext::scheduler()
        .with_actor(17)
        .expect("actor")
        .with_generations(3, 8)
        .expect("generations")
        .with_shard_epoch(5)
        .expect("epoch")
        .with_operation_sequence(13)
        .expect("operation")
        .with_peer_scheduler(peer);

    telemetry
        .record_with_context(VmFixedSchedulerEventKind::MigrationCompleted, context)
        .expect("record migration");

    let capture = telemetry.replay_capture().expect("capture");
    assert!(capture.is_complete());
    assert_eq!(capture.events.len(), 1);
    assert_eq!(capture.events[0].context, context);
    assert_eq!(capture.events[0].scheduler, scheduler);
    assert_eq!(telemetry.snapshot().events, 1);
}

/// Proves publication and dispatch join the same sequence and actor generation.
#[test]
fn identified_publication_and_dispatch_retain_authoritative_runtime_identity() {
    let topology = VmSchedulerTopology::new(1).expect("topology");
    let route = topology.route(NonZeroU64::new(31).expect("actor"));
    let epoch = VmShardEpoch::new(7).expect("epoch");
    let telemetry =
        VmFixedSchedulerTelemetry::for_shard(route.scheduler(), epoch, 8).expect("telemetry");
    let control = VmFixedSchedulerControl::default();
    control.register(route).expect("register");
    let lease = control.acquire(route, route.scheduler()).expect("acquire");
    control
        .release(lease, VmActorLifecycle::Parked)
        .expect("park");
    let (publication, wake) = control.publish_identified(route, "wake").expect("publish");
    assert_eq!(
        wake,
        crate::runtime::vm::actor_directory::VmMailboxWake::Enqueue
    );
    let foreign = topology.route(NonZeroU64::new(32).expect("foreign actor"));
    assert!(telemetry
        .record_publication(
            VmFixedSchedulerEventKind::TimerPublished,
            foreign,
            publication,
        )
        .expect_err("foreign publication identity")
        .contains("does not match route"));
    assert_eq!(telemetry.snapshot().events, 0);
    telemetry
        .record_publication(
            VmFixedSchedulerEventKind::TimerPublished,
            route,
            publication,
        )
        .expect("record publication");
    let lease = control
        .acquire(route, route.scheduler())
        .expect("reacquire");
    assert_eq!(
        control.drain_identified(&lease).expect("drain"),
        vec![(publication, "wake")]
    );
    telemetry
        .record_dispatch(
            VmFixedSchedulerEventKind::TimerDispatched,
            &lease,
            publication,
        )
        .expect("record dispatch");

    let capture = telemetry.replay_capture().expect("capture");
    let published = capture.events[0].context;
    let dispatched = capture.events[1].context;
    assert_eq!(published.actor_id, Some(31));
    assert_eq!(published.actor_generation, dispatched.actor_generation);
    assert_eq!(published.operation_sequence, Some(1));
    assert_eq!(published.operation_sequence, dispatched.operation_sequence);
    assert_eq!(published.owner_generation, None);
    assert_eq!(dispatched.owner_generation, Some(2));
    assert_eq!(published.shard_epoch, Some(7));
    assert_eq!(published.shard_epoch, dispatched.shard_epoch);

    control
        .release(lease, VmActorLifecycle::Exiting)
        .expect("exit");
    control.reclaim(route).expect("reclaim");
}

/// Proves execution intervals pair without wall-clock or thread identity.
#[test]
fn execution_intervals_are_unique_paired_and_generation_qualified() {
    let topology = VmSchedulerTopology::new(1).expect("topology");
    let route = topology.route(NonZeroU64::new(41).expect("actor"));
    let telemetry = VmFixedSchedulerTelemetry::new(route.scheduler(), 16).expect("telemetry");
    let control = VmFixedSchedulerControl::<()>::default();
    control.register(route).expect("register");
    let lease = control.acquire(route, route.scheduler()).expect("acquire");

    let first = telemetry.begin_execution(&lease).expect("first interval");
    telemetry.finish_execution(first).expect("finish first");
    let second = telemetry.begin_execution(&lease).expect("second interval");
    telemetry.finish_execution(second).expect("finish second");

    assert_eq!(first.execution_interval, Some(1));
    assert_eq!(second.execution_interval, Some(2));
    assert_eq!(first.actor_id, Some(41));
    assert_eq!(first.actor_generation, Some(1));
    assert_eq!(first.owner_generation, Some(1));
    let capture = telemetry.replay_capture().expect("capture");
    assert_eq!(capture.events.len(), 6);
    for events in capture.events.chunks_exact(3) {
        assert_eq!(events[0].kind, VmFixedSchedulerEventKind::SchedulerSelected);
        assert_eq!(events[1].kind, VmFixedSchedulerEventKind::ExecutionStarted);
        assert_eq!(events[2].kind, VmFixedSchedulerEventKind::ExecutionFinished);
        assert_eq!(events[0].context, events[1].context);
        assert_eq!(events[1].context, events[2].context);
    }

    control
        .release(lease, VmActorLifecycle::Exiting)
        .expect("exit");
    control.reclaim(route).expect("reclaim");
}

/// Proves panic evidence consumes and preserves the active actor interval.
#[test]
fn scheduler_panic_records_the_active_generation_qualified_interval() {
    let topology = VmSchedulerTopology::new(1).expect("topology");
    let route = topology.route(NonZeroU64::new(43).expect("actor"));
    let epoch = VmShardEpoch::new(9).expect("epoch");
    let telemetry =
        VmFixedSchedulerTelemetry::for_shard(route.scheduler(), epoch, 8).expect("telemetry");
    let control = VmFixedSchedulerControl::<()>::default();
    control.register(route).expect("register");
    let lease = control.acquire(route, route.scheduler()).expect("acquire");
    let active = telemetry.begin_execution(&lease).expect("begin interval");

    let panic_context = telemetry
        .record_scheduler_panic()
        .expect("record scheduler panic");

    assert_eq!(panic_context, active);
    assert_eq!(telemetry.snapshot().failures, 1);
    let capture = telemetry.replay_capture().expect("panic capture");
    assert_eq!(
        capture
            .events
            .iter()
            .map(|event| event.kind)
            .collect::<Vec<_>>(),
        vec![
            VmFixedSchedulerEventKind::SchedulerSelected,
            VmFixedSchedulerEventKind::ExecutionStarted,
            VmFixedSchedulerEventKind::SchedulerPanicked,
        ]
    );
    assert!(capture.events.iter().all(|event| event.context == active));
    assert!(telemetry
        .finish_execution(active)
        .expect_err("panic consumed active interval")
        .contains("does not match"));
    control
        .release(lease, VmActorLifecycle::Exiting)
        .expect("release failed actor");
    control.reclaim(route).expect("reclaim failed actor");
}

/// Proves interval identities fail closed instead of wrapping to zero.
#[test]
fn execution_interval_identity_exhaustion_does_not_emit_partial_evidence() {
    let topology = VmSchedulerTopology::new(1).expect("topology");
    let route = topology.route(NonZeroU64::new(42).expect("actor"));
    let telemetry = VmFixedSchedulerTelemetry::new(route.scheduler(), 8).expect("telemetry");
    let control = VmFixedSchedulerControl::<()>::default();
    control.register(route).expect("register");
    let lease = control.acquire(route, route.scheduler()).expect("acquire");
    telemetry
        .next_execution_interval
        .store(u64::MAX, std::sync::atomic::Ordering::Relaxed);

    assert!(telemetry
        .begin_execution(&lease)
        .expect_err("interval exhaustion")
        .contains("identity exhausted"));
    assert!(telemetry
        .replay_capture()
        .expect("empty capture")
        .events
        .is_empty());

    control
        .release(lease, VmActorLifecycle::Exiting)
        .expect("exit");
    control.reclaim(route).expect("reclaim");
}
