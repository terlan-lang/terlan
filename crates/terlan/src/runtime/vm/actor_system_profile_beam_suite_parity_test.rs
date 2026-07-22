use std::collections::BTreeMap;

use super::{VmActorRuntime, VmExitReason, VmProcessSource};
use crate::runtime::vm::scheduler::{VmSchedulerClass, VmSchedulerDecision};
use crate::runtime::vm::system_profile::{VmSystemProfileActivity, VmSystemProfileCursor};
use crate::runtime::vm::ReplValue;

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("parity.SystemProfile", name, 0)
}

#[test]
fn system_profile_suite_runnable_flow_is_ordered_replayable_and_observer_free() {
    let mut runtime = VmActorRuntime::default();
    let driver = runtime.spawn_root(source("driver"));
    let workers = (0..10)
        .map(|index| runtime.spawn_root(source(&format!("worker-{index}"))))
        .collect::<Vec<_>>();
    runtime.suspend(driver).expect("suspend profile driver");
    let process_count = runtime.live_process_ids().len();
    let cursor = runtime.system_profile_cursor();

    for lap in 0..10 {
        let mut blocked = Vec::new();
        for _ in 0..workers.len() {
            let run = runtime
                .run_next(|_, _| VmSchedulerDecision::Block { reductions: 1 })
                .expect("block next ring worker");
            blocked.push(run.pid.expect("worker slice"));
        }
        assert_eq!(runtime.scheduled_len(), 0);
        for worker in blocked {
            runtime
                .send(driver, worker, ReplValue::Int(lap))
                .expect("wake ring worker");
        }
        assert_eq!(runtime.scheduled_len(), workers.len());
    }

    let profile = runtime
        .system_profile_since(cursor)
        .expect("capture runnable profile");
    assert_eq!(runtime.live_process_ids().len(), process_count);
    assert_eq!(profile.events.len(), workers.len() * 10 * 2);
    assert_eq!(profile.total_slices, 100);
    assert_eq!(profile.total_preemptions, 0);
    assert!(profile.total_reductions >= 100);
    assert!(profile
        .events
        .windows(2)
        .all(|events| events[0].sequence + 1 == events[1].sequence));
    assert!(profile
        .events
        .windows(2)
        .all(|events| events[0].tick <= events[1].tick));
    assert!(profile.events.iter().all(|event| {
        event.scheduler_class == VmSchedulerClass::Normal
            && event.run_queue_length <= workers.len()
            && event.location.source.module == "parity.SystemProfile"
            && event.location.source.function.starts_with("worker-")
    }));

    let mut events_by_pid = BTreeMap::<u64, Vec<_>>::new();
    for event in &profile.events {
        events_by_pid.entry(event.pid).or_default().push(event);
    }
    assert_eq!(events_by_pid.len(), workers.len());
    for events in events_by_pid.values() {
        assert_eq!(events.len(), 20);
        assert!(events.chunks_exact(2).all(|pair| {
            pair[0].activity == VmSystemProfileActivity::Inactive
                && pair[0].transition == "dequeue"
                && pair[1].activity == VmSystemProfileActivity::Runnable
                && pair[1].transition == "enqueue"
        }));
    }

    assert_eq!(
        profile,
        runtime
            .system_profile_since(cursor)
            .expect("profile replay must be immutable")
    );
    let empty = runtime
        .system_profile_since(profile.next_cursor)
        .expect("profile from current end");
    assert!(empty.events.is_empty());
    assert_eq!(empty.next_cursor, profile.next_cursor);
}

#[test]
fn system_profile_suite_exit_location_and_cursor_validation_contract() {
    let mut runtime = VmActorRuntime::default();
    let worker = runtime.spawn_root(source("exiting-worker"));
    let cursor = runtime.system_profile_cursor();
    runtime
        .run_next(|_, _| VmSchedulerDecision::Yield { reductions: 3 })
        .expect("yield worker");
    runtime
        .exit_actor(worker, VmExitReason::Normal)
        .expect("exit queued worker");

    let profile = runtime
        .system_profile_since(cursor)
        .expect("capture exit profile");
    assert_eq!(profile.events.len(), 3);
    assert_eq!(
        profile
            .events
            .iter()
            .map(|event| (event.transition, event.activity))
            .collect::<Vec<_>>(),
        vec![
            ("dequeue", VmSystemProfileActivity::Inactive),
            ("enqueue", VmSystemProfileActivity::Runnable),
            ("exit", VmSystemProfileActivity::Inactive),
        ]
    );
    assert!(profile.events.iter().all(|event| {
        event.pid == worker.as_u64()
            && event.location.source.function == "exiting-worker"
            && event.location.instruction_offset == 0
    }));
    assert_eq!(runtime.scheduled_len(), 0);

    let transition_count = profile.next_cursor.position();
    let invalid = VmSystemProfileCursor::from_position(transition_count + 1);
    assert_eq!(
        runtime
            .system_profile_since(invalid)
            .expect_err("future cursor must fail"),
        format!(
            "VM system profile cursor {} exceeds transition count {}",
            transition_count + 1,
            transition_count
        )
    );
}
