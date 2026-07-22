use super::*;
use crate::runtime::vm::{
    process::{VmExitReason, VmProcessSource},
    scheduler::VmSchedulerConfig,
};

fn source(function: &str) -> VmProcessSource {
    VmProcessSource::new("app.TimerAccounting", function, 0)
}

#[test]
fn timer_table_charges_only_successful_mailbox_deliveries() {
    let mut processes = VmProcessTable::default();
    let live_owner = processes.spawn_root(source("live_timer_owner"));
    let exited_owner = processes.spawn_root(source("exited_timer_owner"));
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(10, 100));
    let mut timers = VmTimerTable::default();
    timers
        .start_one_shot(&processes, live_owner, 5)
        .expect("live timer");
    timers
        .start_one_shot(&processes, exited_owner, 5)
        .expect("event retained across owner exit");
    let events = timers.advance_clock(&mut processes, &mut scheduler, 5);
    let live_event = events
        .iter()
        .find(|event| event.owner() == live_owner)
        .expect("live owner event");
    let stale_event = events
        .iter()
        .find(|event| event.owner() == exited_owner)
        .expect("event for owner that exits before delivery");
    processes.get_mut(live_owner).expect("live owner").block();
    processes
        .exit_process(exited_owner, VmExitReason::Killed)
        .expect("exit owner before delivery");

    assert_eq!(
        timers
            .deliver_event_to_mailbox(&mut processes, &mut scheduler, live_event)
            .expect("deliver live event"),
        Some(1)
    );
    assert!(timers
        .deliver_event_to_mailbox(&mut processes, &mut scheduler, stale_event)
        .expect_err("stale event cannot reach exited owner")
        .contains("exited"));

    assert_eq!(processes.get(live_owner).expect("live owner").reductions, 1);
    assert_eq!(
        processes
            .get(exited_owner)
            .expect("exited owner")
            .reductions,
        0
    );
    assert_eq!(scheduler.metrics().total_reductions, 1);
    assert_eq!(
        scheduler.metrics().processes[&live_owner.as_u64()].reductions,
        1
    );
    assert_eq!(timers.metrics().mailbox_deliveries, 1);
}
