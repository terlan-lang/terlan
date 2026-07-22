use std::collections::BTreeSet;

use super::super::process::{VmProcessSource, VmProcessTable};
use super::super::scheduler::{VmScheduler, VmSchedulerConfig};
use super::{VmTimerEvent, VmTimerKind, VmTimerTable};

/// Replaces OTP's randomized timer load suite with a deterministic mixed load.
///
/// Every deadline is observed at its exact logical tick. This proves timer
/// cardinality and interval rescheduling without host-clock tolerances or a
/// long-running watchdog that can fail because the CI machine is busy.
#[test]
fn mixed_timer_load_fires_exactly_once_per_due_deadline() {
    const ONE_SHOT_COUNT: usize = 200;
    const INTERVAL_COUNT: usize = 25;
    const FINAL_TICK: u64 = 100;

    let mut processes = VmProcessTable::default();
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(10, 1_000));
    let mut timers = VmTimerTable::default();
    let mut one_shots = BTreeSet::new();
    let mut intervals = Vec::with_capacity(INTERVAL_COUNT);

    for index in 0..ONE_SHOT_COUNT {
        let owner = processes.spawn_root(VmProcessSource::new("timer.Load", "one_shot", 0));
        let deadline = 1 + (index as u64 % 50);
        let timer = timers
            .start_one_shot(&processes, owner, deadline)
            .expect("one-shot timer should start");
        assert!(one_shots.insert(timer));
    }
    for index in 0..INTERVAL_COUNT {
        let owner = processes.spawn_root(VmProcessSource::new("timer.Load", "interval", 0));
        let first_deadline = 1 + (index as u64 % 5);
        let period = 7 + (index as u64 % 7);
        let timer = timers
            .start_interval(&processes, owner, first_deadline, period)
            .expect("interval timer should start");
        intervals.push((timer, first_deadline, period));
    }

    let mut observed_one_shots = BTreeSet::new();
    let mut observed_interval_fires = 0_usize;
    for tick in 1..=FINAL_TICK {
        for event in timers.advance_clock(&mut processes, &mut scheduler, tick) {
            let VmTimerEvent::Fired { timer_id, kind, .. } = event else {
                panic!("exact logical ticks must not produce a late or failed event: {event:?}");
            };
            match kind {
                VmTimerKind::OneShot => {
                    assert!(one_shots.contains(&timer_id));
                    assert!(
                        observed_one_shots.insert(timer_id),
                        "one-shot timer fired more than once: {timer_id:?}"
                    );
                }
                VmTimerKind::Interval => observed_interval_fires += 1,
                VmTimerKind::ReceiveTimeout => {
                    panic!("mixed load did not install receive timeouts")
                }
            }
        }
    }

    let expected_interval_fires = intervals
        .iter()
        .map(|(_, first_deadline, period)| 1 + (FINAL_TICK - first_deadline) / period)
        .sum::<u64>() as usize;
    assert_eq!(observed_one_shots.len(), ONE_SHOT_COUNT);
    assert_eq!(observed_interval_fires, expected_interval_fires);
    assert_eq!(timers.active_count(), INTERVAL_COUNT);

    for (timer, _, _) in intervals {
        assert!(matches!(
            timers.cancel(timer),
            Ok(VmTimerEvent::Cancelled {
                kind: VmTimerKind::Interval,
                ..
            })
        ));
    }
    assert_eq!(timers.active_count(), 0);
}
