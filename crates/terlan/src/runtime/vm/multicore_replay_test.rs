//! Tests for bounded multicore capture and controlled replay.

use super::*;
use crate::runtime::vm::scheduler_topology::VmSchedulerTopology;

/// Builds one actor-specific context with all generation identities.
fn actor_context() -> VmMulticoreEventContext {
    VmMulticoreEventContext::scheduler()
        .with_actor(7)
        .expect("actor")
        .with_generations(3, 9)
        .expect("generations")
        .with_shard_epoch(4)
        .expect("epoch")
        .with_operation_sequence(11)
        .expect("operation")
        .with_execution_interval(5)
        .expect("interval")
}

/// Proves pressure evicts only the oldest event and marks capture incomplete.
#[test]
fn bounded_recording_reports_dropped_prefix_without_reordering() {
    let scheduler = VmSchedulerTopology::new(1)
        .expect("topology")
        .schedulers()
        .next()
        .expect("scheduler");
    let mut recorder = VmMulticoreReplayRecorder::recording(scheduler, 2).expect("recorder");
    for kind in [
        VmMulticoreEventKind::Entry,
        VmMulticoreEventKind::Parked,
        VmMulticoreEventKind::Wake,
    ] {
        recorder
            .observe(kind, actor_context())
            .expect("record event");
    }

    let capture = recorder.capture().expect("capture");
    assert_eq!(capture.first_sequence, 2);
    assert_eq!(capture.next_sequence, 4);
    assert_eq!(capture.dropped_events, 1);
    assert_eq!(
        capture
            .events
            .iter()
            .map(|event| event.kind)
            .collect::<Vec<_>>(),
        vec![VmMulticoreEventKind::Parked, VmMulticoreEventKind::Wake]
    );
    assert_eq!(
        VmMulticoreReplayRecorder::replaying(capture).expect_err("lossy capture"),
        VmMulticoreReplayError::IncompleteCapture
    );
}

/// Proves controlled replay consumes exact identities without wall-clock data.
#[test]
fn controlled_replay_matches_exact_scheduler_local_events() {
    let topology = VmSchedulerTopology::new(2).expect("topology");
    let scheduler = topology.schedulers().next().expect("scheduler");
    let peer = topology.schedulers().nth(1).expect("peer");
    let context = actor_context().with_peer_scheduler(peer);
    let mut recorder = VmMulticoreReplayRecorder::recording(scheduler, 8).expect("recorder");
    recorder
        .observe(VmMulticoreEventKind::MigrationStarted, context)
        .expect("migration start");
    recorder
        .observe(VmMulticoreEventKind::MigrationCompleted, context)
        .expect("migration complete");
    let capture = recorder.capture().expect("capture");
    let mut replay = VmMulticoreReplayRecorder::replaying(capture).expect("replay");

    let wrong_identity = VmMulticoreEventContext {
        operation_sequence: Some(12),
        ..context
    };
    let expected_start = VmMulticoreReplayEvent {
        sequence: 1,
        scheduler,
        kind: VmMulticoreEventKind::MigrationStarted,
        context,
    };
    assert_eq!(
        replay
            .observe(VmMulticoreEventKind::MigrationStarted, wrong_identity)
            .expect_err("identity mismatch"),
        VmMulticoreReplayError::ReplayMismatch {
            expected: expected_start,
            actual: VmMulticoreReplayEvent {
                context: wrong_identity,
                ..expected_start
            },
        }
    );
    replay
        .observe(VmMulticoreEventKind::MigrationStarted, context)
        .expect("replay start");
    let expected_complete = VmMulticoreReplayEvent {
        sequence: 2,
        scheduler,
        kind: VmMulticoreEventKind::MigrationCompleted,
        context,
    };
    assert_eq!(
        replay
            .observe(VmMulticoreEventKind::MigrationAborted, context)
            .expect_err("mismatch"),
        VmMulticoreReplayError::ReplayMismatch {
            expected: expected_complete,
            actual: VmMulticoreReplayEvent {
                kind: VmMulticoreEventKind::MigrationAborted,
                ..expected_complete
            },
        }
    );
    replay
        .observe(VmMulticoreEventKind::MigrationCompleted, context)
        .expect("retry exact event");
    replay.finish_replay().expect("complete replay");
    assert_eq!(
        replay
            .observe(
                VmMulticoreEventKind::Shutdown,
                VmMulticoreEventContext::scheduler()
            )
            .expect_err("past end"),
        VmMulticoreReplayError::ReplayExhausted {
            actual: VmMulticoreEventKind::Shutdown
        }
    );
}

/// Proves corrupt sequence, scheduler, and completion metadata fail closed.
#[test]
fn replay_rejects_corrupt_capture_metadata() {
    let topology = VmSchedulerTopology::new(2).expect("topology");
    let scheduler = topology.schedulers().next().expect("scheduler");
    let foreign = topology.schedulers().nth(1).expect("foreign");
    let mut recorder = VmMulticoreReplayRecorder::recording(scheduler, 4).expect("recorder");
    recorder
        .observe(
            VmMulticoreEventKind::Command,
            VmMulticoreEventContext::scheduler(),
        )
        .expect("command");
    let capture = recorder.capture().expect("capture");

    let mut bad_sequence = capture.clone();
    bad_sequence.events[0].sequence = 2;
    assert_eq!(
        VmMulticoreReplayRecorder::replaying(bad_sequence).expect_err("sequence"),
        VmMulticoreReplayError::CorruptSequence
    );
    let mut bad_scheduler = capture.clone();
    bad_scheduler.events[0].scheduler = foreign;
    assert_eq!(
        VmMulticoreReplayRecorder::replaying(bad_scheduler).expect_err("scheduler"),
        VmMulticoreReplayError::ForeignSchedulerEvent
    );
    let mut bad_next = capture;
    bad_next.next_sequence = 99;
    assert_eq!(
        VmMulticoreReplayRecorder::replaying(bad_next).expect_err("next sequence"),
        VmMulticoreReplayError::CorruptNextSequence
    );
}

/// Proves invalid zero and detached generation identities never enter capture.
#[test]
fn event_context_rejects_invalid_generation_identities() {
    assert_eq!(
        VmMulticoreEventContext::scheduler()
            .with_actor(0)
            .expect_err("zero actor"),
        VmMulticoreReplayError::ZeroActorIdentity
    );
    assert_eq!(
        VmMulticoreEventContext::scheduler()
            .with_generations(1, 1)
            .expect_err("detached generations"),
        VmMulticoreReplayError::GenerationWithoutActor
    );
    assert_eq!(
        VmMulticoreEventContext::scheduler()
            .with_actor(1)
            .expect("actor")
            .with_shard_epoch(0)
            .expect_err("zero epoch"),
        VmMulticoreReplayError::ZeroShardEpoch
    );
    assert_eq!(
        VmMulticoreReplayRecorder::recording(
            VmSchedulerTopology::new(1)
                .expect("topology")
                .schedulers()
                .next()
                .expect("scheduler"),
            0,
        )
        .expect_err("zero capacity"),
        VmMulticoreReplayError::ZeroCapacity
    );
}

/// Proves aggregation preserves scheduler-local streams without global ordering.
#[test]
fn bounded_evidence_orders_schedulers_and_classifies_lossy_captures() {
    let topology = VmSchedulerTopology::new(2).expect("topology");
    let first_scheduler = topology.schedulers().next().expect("first scheduler");
    let second_scheduler = topology.schedulers().nth(1).expect("second scheduler");
    let mut first = VmMulticoreReplayRecorder::recording(first_scheduler, 4).expect("first");
    first
        .observe(VmMulticoreEventKind::Entry, actor_context())
        .expect("first entry");
    let mut second = VmMulticoreReplayRecorder::recording(second_scheduler, 1).expect("second");
    second
        .observe(
            VmMulticoreEventKind::Command,
            VmMulticoreEventContext::scheduler(),
        )
        .expect("second command");
    second
        .observe(
            VmMulticoreEventKind::Shutdown,
            VmMulticoreEventContext::scheduler(),
        )
        .expect("second shutdown");

    let evidence = VmMulticoreReplayEvidence::new(
        9,
        2,
        5,
        vec![
            second.capture().expect("second capture"),
            first.capture().expect("first capture"),
        ],
    )
    .expect("aggregate evidence");

    assert_eq!(evidence.runtime_generation, 9);
    assert_eq!(evidence.retained_events, 2);
    assert_eq!(evidence.dropped_events, 1);
    assert!(!evidence.replayable);
    assert_eq!(evidence.schedulers[0].scheduler, first_scheduler);
    assert_eq!(evidence.schedulers[1].scheduler, second_scheduler);
    assert_eq!(
        evidence.schedulers[0].events[0].kind,
        VmMulticoreEventKind::Entry
    );
    assert_eq!(
        evidence.schedulers[1].events[0].kind,
        VmMulticoreEventKind::Shutdown
    );
}

/// Proves malformed topology, capacity, and sequence evidence fails closed.
#[test]
fn bounded_evidence_rejects_missing_duplicate_oversized_and_forged_captures() {
    let topology = VmSchedulerTopology::new(2).expect("topology");
    let first_scheduler = topology.schedulers().next().expect("first scheduler");
    let second_scheduler = topology.schedulers().nth(1).expect("second scheduler");
    let mut first = VmMulticoreReplayRecorder::recording(first_scheduler, 2).expect("first");
    first
        .observe(VmMulticoreEventKind::Entry, actor_context())
        .expect("entry");
    let first_capture = first.capture().expect("first capture");
    let mut second = VmMulticoreReplayRecorder::recording(second_scheduler, 2).expect("second");
    second
        .observe(
            VmMulticoreEventKind::Command,
            VmMulticoreEventContext::scheduler(),
        )
        .expect("command");
    let second_capture = second.capture().expect("second capture");

    assert!(matches!(
        VmMulticoreReplayEvidence::new(1, 2, 4, vec![first_capture.clone()]),
        Err(VmMulticoreReplayError::CaptureCountMismatch {
            expected: 2,
            actual: 1
        })
    ));
    assert!(matches!(
        VmMulticoreReplayEvidence::new(1, 2, 4, vec![first_capture.clone(), first_capture.clone()]),
        Err(VmMulticoreReplayError::UnexpectedScheduler {
            expected: 1,
            actual: 0
        })
    ));
    assert!(matches!(
        VmMulticoreReplayEvidence::new(
            1,
            2,
            0,
            vec![first_capture.clone(), second_capture.clone()]
        ),
        Err(VmMulticoreReplayError::ZeroAggregateCapacity)
    ));
    assert!(matches!(
        VmMulticoreReplayEvidence::new(
            1,
            2,
            1,
            vec![first_capture.clone(), second_capture.clone()]
        ),
        Err(VmMulticoreReplayError::AggregateCapacityExceeded {
            maximum: 1,
            actual: 2
        })
    ));
    let mut forged = first_capture;
    forged.first_sequence = 2;
    assert_eq!(
        VmMulticoreReplayEvidence::new(1, 2, 4, vec![forged, second_capture])
            .expect_err("forged first sequence"),
        VmMulticoreReplayError::CorruptFirstSequence
    );
}
