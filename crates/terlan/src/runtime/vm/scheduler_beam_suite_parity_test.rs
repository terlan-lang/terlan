use std::collections::{BTreeMap, BTreeSet};

use super::{
    VmScheduler, VmSchedulerClass, VmSchedulerConfig, VmSchedulerDecision, VmSchedulerOutcome,
};
use crate::runtime::vm::process::{
    VmProcessResumeState, VmProcessSource, VmProcessState, VmProcessTable,
};

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("parity.Scheduler", name, 0)
}

#[test]
fn otp_scheduler_suite_priority_pressure_preserves_weight_and_progress() {
    let mut processes = VmProcessTable::default();
    let mut classes = BTreeMap::new();
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(1, 64));

    for (class, prefix) in [
        (VmSchedulerClass::Priority, "priority"),
        (VmSchedulerClass::Normal, "normal"),
        (VmSchedulerClass::Background, "background"),
    ] {
        for worker in 0..6 {
            let pid = processes.spawn_root(source(&format!("{prefix}-{worker}")));
            classes.insert(pid, class);
            scheduler
                .enqueue_runnable_with_class(&processes, pid, class)
                .expect("classified worker should enqueue");
        }
    }

    let mut class_slices = BTreeMap::<VmSchedulerClass, usize>::new();
    let mut seen = BTreeSet::new();
    for _ in 0..60 {
        let run = scheduler
            .run_next(&mut processes, |_process, slice| {
                let class = classes[&slice.pid];
                *class_slices.entry(class).or_default() += 1;
                seen.insert(slice.pid);
                VmSchedulerDecision::Yield { reductions: 1 }
            })
            .expect("mixed-priority pressure should keep scheduling");
        assert_eq!(run.outcome, VmSchedulerOutcome::Ran);
    }

    assert_eq!(class_slices[&VmSchedulerClass::Priority], 30);
    assert_eq!(class_slices[&VmSchedulerClass::Normal], 20);
    assert_eq!(class_slices[&VmSchedulerClass::Background], 10);
    assert_eq!(
        seen.len(),
        18,
        "every worker must make progress under pressure"
    );
    assert!(scheduler
        .metrics()
        .processes
        .values()
        .all(|metrics| { metrics.slices > 0 && metrics.max_wait_ticks <= 36 }));
}

#[test]
fn otp_scheduler_suite_suspend_resume_is_atomic_and_peer_fair() {
    let mut processes = VmProcessTable::default();
    let suspended = processes.spawn_root(source("suspended"));
    let peer = processes.spawn_root(source("peer"));
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(1, 16));
    scheduler
        .enqueue_runnable_with_class(&processes, suspended, VmSchedulerClass::Priority)
        .expect("suspend target should enqueue");
    scheduler
        .enqueue_runnable(&processes, peer)
        .expect("peer should enqueue");

    for _ in 0..64 {
        scheduler
            .suspend_process(&mut processes, suspended)
            .expect("repeated suspend should be idempotent");
    }
    assert_eq!(scheduler.queued_len(), 1);
    assert_eq!(
        processes.get(suspended).expect("suspended process").state,
        VmProcessState::Suspended(VmProcessResumeState::Runnable)
    );

    let peer_run = scheduler
        .run_next(&mut processes, |_process, slice| {
            assert_eq!(slice.pid, peer);
            VmSchedulerDecision::Yield { reductions: 1 }
        })
        .expect("peer must run while priority process is suspended");
    assert_eq!(peer_run.outcome, VmSchedulerOutcome::Ran);

    scheduler
        .wake_process(&mut processes, suspended)
        .expect("wake while suspended records runnable resume state");
    assert_eq!(scheduler.queued_len(), 1, "wake must not bypass suspension");
    scheduler
        .resume_process(&mut processes, suspended)
        .expect("resume should restore runnable priority process");
    assert_eq!(scheduler.queued_len(), 2);

    let resumed = scheduler
        .run_next(&mut processes, |_process, slice| {
            assert_eq!(slice.pid, suspended);
            VmSchedulerDecision::Block { reductions: 1 }
        })
        .expect("resumed priority process should run without duplicate entries");
    assert_eq!(resumed.outcome, VmSchedulerOutcome::Blocked);
    assert_eq!(scheduler.queued_len(), 1);
}
