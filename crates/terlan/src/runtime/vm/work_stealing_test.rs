use std::num::NonZeroU64;

use super::*;
use crate::runtime::vm::scheduler_topology::VmSchedulerTopology;

/// Returns stable scheduler identities for one test topology.
fn schedulers(width: usize) -> Vec<VmSchedulerId> {
    VmSchedulerTopology::new(width)
        .expect("test topology")
        .schedulers()
        .collect()
}

/// Creates one scheduler snapshot with no wait-age evidence.
fn work(scheduler: VmSchedulerId, runnable: [usize; 3]) -> VmSchedulerWorkSnapshot {
    VmSchedulerWorkSnapshot::new(scheduler, runnable, [0; 3])
}

#[test]
fn bounded_steal_owner_transfer_model() {
    let schedulers = schedulers(2);
    let mut policy = VmWorkStealingPolicy::new(2, VmWorkStealingConfig::default()).expect("policy");
    let snapshots = [
        work(schedulers[0], [0, 0, 0]),
        work(schedulers[1], [0, 10, 0]),
    ];

    let VmWorkDirective::Steal(plan) = policy
        .decide(schedulers[0], &snapshots)
        .expect("steal decision")
    else {
        panic!("idle scheduler must steal published work")
    };
    assert_eq!(plan.thief(), schedulers[0]);
    assert_eq!(plan.victim(), schedulers[1]);
    assert_eq!(plan.class(), VmSchedulerClass::Normal);
    assert_eq!(plan.maximum_actors(), 4);

    let topology = VmSchedulerTopology::new(2).expect("topology");
    let route = topology.route(NonZeroU64::new(2).expect("actor"));
    let candidate =
        VmStealCandidate::new(route, VmSchedulerClass::Normal, VmActorLifecycle::Queued);
    assert!(plan.accepts(candidate));
    assert!(!plan.accepts(VmStealCandidate::new(
        route,
        VmSchedulerClass::Priority,
        VmActorLifecycle::Queued,
    )));
}

#[test]
fn local_service_budget_forces_assistance_under_persistent_imbalance() {
    let schedulers = schedulers(2);
    let config = VmWorkStealingConfig::new(8, 3, 1, 1, 8, [6, 12, 24]).expect("bounded config");
    let mut policy = VmWorkStealingPolicy::new(2, config).expect("policy");
    let snapshots = [
        work(schedulers[0], [0, 1, 0]),
        work(schedulers[1], [0, 4, 0]),
    ];

    for _ in 0..8 {
        assert_eq!(
            policy.decide(schedulers[0], &snapshots).expect("local"),
            VmWorkDirective::ServeLocal(VmSchedulerClass::Normal)
        );
    }
    let VmWorkDirective::Steal(plan) = policy
        .decide(schedulers[0], &snapshots)
        .expect("budgeted steal")
    else {
        panic!("exhausted local budget must assist an overloaded peer")
    };
    assert_eq!(plan.maximum_actors(), 3);
}

#[test]
fn shard_wide_starvation_overrides_locality_and_priority_floods() {
    let schedulers = schedulers(2);
    let mut policy = VmWorkStealingPolicy::new(2, VmWorkStealingConfig::default()).expect("policy");
    let snapshots = [
        work(schedulers[0], [1, 1, 0]),
        VmSchedulerWorkSnapshot::new(schedulers[1], [100, 0, 1], [0, 0, 24]),
    ];

    let VmWorkDirective::Steal(plan) = policy
        .decide(schedulers[0], &snapshots)
        .expect("starvation steal")
    else {
        panic!("overdue background work must override local service")
    };
    assert_eq!(plan.class(), VmSchedulerClass::Background);
    assert_eq!(plan.maximum_actors(), 1);
}

#[test]
fn equal_victims_rotate_without_abandoning_home_locality() {
    let schedulers = schedulers(3);
    let mut policy = VmWorkStealingPolicy::new(3, VmWorkStealingConfig::default()).expect("policy");
    let snapshots = [
        work(schedulers[0], [0, 0, 0]),
        work(schedulers[1], [0, 4, 0]),
        work(schedulers[2], [0, 4, 0]),
    ];

    let VmWorkDirective::Steal(first) = policy.decide(schedulers[0], &snapshots).expect("first")
    else {
        panic!("first victim")
    };
    policy
        .record_steal_result(schedulers[0], first.maximum_actors())
        .expect("record first");
    let VmWorkDirective::Steal(second) = policy.decide(schedulers[0], &snapshots).expect("second")
    else {
        panic!("second victim")
    };
    assert_eq!(first.victim(), schedulers[1]);
    assert_eq!(second.victim(), schedulers[2]);
}

#[test]
fn one_scheduler_preserves_weighted_priority_normal_background_service() {
    let scheduler = schedulers(1)[0];
    let mut policy = VmWorkStealingPolicy::new(1, VmWorkStealingConfig::default()).expect("policy");
    let snapshot = [work(scheduler, [1, 1, 1])];
    let observed = (0..6)
        .map(|_| policy.decide(scheduler, &snapshot).expect("service"))
        .collect::<Vec<_>>();
    assert_eq!(
        observed,
        vec![
            VmWorkDirective::ServeLocal(VmSchedulerClass::Priority),
            VmWorkDirective::ServeLocal(VmSchedulerClass::Priority),
            VmWorkDirective::ServeLocal(VmSchedulerClass::Normal),
            VmWorkDirective::ServeLocal(VmSchedulerClass::Priority),
            VmWorkDirective::ServeLocal(VmSchedulerClass::Normal),
            VmWorkDirective::ServeLocal(VmSchedulerClass::Background),
        ]
    );
}

#[test]
fn failed_steals_back_off_exponentially_and_reset_after_progress() {
    let scheduler = schedulers(1)[0];
    let config = VmWorkStealingConfig::new(1, 2, 0, 2, 8, [1, 1, 1]).expect("backoff config");
    let mut policy = VmWorkStealingPolicy::new(1, config).expect("policy");

    policy
        .record_steal_result(scheduler, 0)
        .expect("first failure");
    assert_eq!(policy.backoff_remaining[0], 2);
    policy
        .record_steal_result(scheduler, 0)
        .expect("second failure");
    assert_eq!(policy.backoff_remaining[0], 4);
    policy
        .record_steal_result(scheduler, 0)
        .expect("third failure");
    policy
        .record_steal_result(scheduler, 0)
        .expect("bounded failure");
    assert_eq!(policy.backoff_remaining[0], 8);
    policy
        .record_steal_result(scheduler, 1)
        .expect("successful steal");
    assert_eq!(policy.backoff_remaining[0], 0);
    assert_eq!(policy.failed_steals[0], 0);
    assert!(policy.record_steal_result(scheduler, 3).is_err());
}

#[test]
fn idle_sleep_requires_only_one_wake_publication() {
    let scheduler = schedulers(1)[0];
    let mut policy = VmWorkStealingPolicy::new(1, VmWorkStealingConfig::default()).expect("policy");
    let empty = [work(scheduler, [0, 0, 0])];
    assert_eq!(
        policy.decide(scheduler, &empty).expect("sleep"),
        VmWorkDirective::Sleep
    );
    assert!(policy.is_sleeping(scheduler).expect("sleeping"));
    assert!(policy.publish_runnable(scheduler).expect("first wake"));
    assert!(!policy.publish_runnable(scheduler).expect("duplicate wake"));
}

#[test]
fn parked_borrowed_pinned_and_unpublished_candidates_are_ineligible() {
    let topology = VmSchedulerTopology::new(1).expect("topology");
    let route = topology.route(NonZeroU64::new(1).expect("actor"));
    let candidate =
        VmStealCandidate::new(route, VmSchedulerClass::Normal, VmActorLifecycle::Queued);
    assert!(candidate.is_eligible());
    assert!(
        !VmStealCandidate::new(route, VmSchedulerClass::Normal, VmActorLifecycle::Parked,)
            .is_eligible()
    );
    assert!(
        !VmStealCandidate::new(route, VmSchedulerClass::Normal, VmActorLifecycle::Yielding,)
            .is_eligible()
    );
    assert!(!candidate.unpublished().is_eligible());
    assert!(!candidate.borrowed().is_eligible());
    assert!(!candidate.with_lookup_pins(1).is_eligible());
    assert!(!candidate.pinned().is_eligible());
}

#[test]
fn shutdown_schedulers_neither_request_nor_supply_work() {
    let schedulers = schedulers(2);
    let mut policy = VmWorkStealingPolicy::new(2, VmWorkStealingConfig::default()).expect("policy");
    let stopped_thief = [
        work(schedulers[0], [0, 0, 0]).stopped(),
        work(schedulers[1], [0, 1, 0]),
    ];
    assert_eq!(
        policy
            .decide(schedulers[0], &stopped_thief)
            .expect("stopped thief"),
        VmWorkDirective::Stopped
    );
    let stopped_victim = [
        work(schedulers[0], [0, 0, 0]),
        work(schedulers[1], [0, 10, 0]).stopped(),
    ];
    assert_eq!(
        policy
            .decide(schedulers[0], &stopped_victim)
            .expect("stopped victim"),
        VmWorkDirective::Sleep
    );
}

#[test]
fn seeded_skew_burst_and_fanout_decisions_remain_bounded_and_work_conserving() {
    let schedulers = schedulers(4);
    let config = VmWorkStealingConfig::default();
    let mut policy = VmWorkStealingPolicy::new(4, config).expect("policy");
    let mut seed = 0x9e37_79b9_7f4a_7c15_u64;

    for round in 0..2_000 {
        let mut snapshots = Vec::new();
        for scheduler in &schedulers {
            let mut runnable = [0; 3];
            let mut waits = [0; 3];
            for class in 0..3 {
                seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                runnable[class] = ((seed >> 32) as usize) % 17;
                waits[class] = (seed >> 48) % 65;
            }
            snapshots.push(VmSchedulerWorkSnapshot::new(*scheduler, *&runnable, waits));
        }
        let thief = schedulers[round % schedulers.len()];
        match policy.decide(thief, &snapshots).expect("seeded decision") {
            VmWorkDirective::Steal(plan) => {
                assert_ne!(plan.thief(), plan.victim());
                assert!((1..=config.steal_batch_size()).contains(&plan.maximum_actors()));
                assert!(snapshots[plan.victim().index()].runnable_in(plan.class()) > 0);
                policy
                    .record_steal_result(thief, plan.maximum_actors())
                    .expect("bounded result");
            }
            VmWorkDirective::Sleep => {
                assert!(snapshots
                    .iter()
                    .filter(|snapshot| snapshot.accepting)
                    .all(|snapshot| snapshot.runnable_total() == 0));
            }
            VmWorkDirective::ServeLocal(class) => {
                assert!(snapshots[thief.index()].runnable_in(class) > 0);
            }
            VmWorkDirective::Backoff(_) | VmWorkDirective::Stopped => {
                panic!("seeded active workload did not record failure or shutdown")
            }
        }
    }
}

#[test]
fn malformed_snapshot_sets_fail_before_policy_state_changes() {
    let schedulers = schedulers(2);
    let mut policy = VmWorkStealingPolicy::new(2, VmWorkStealingConfig::default()).expect("policy");
    assert!(policy
        .decide(schedulers[0], &[work(schedulers[0], [0; 3])])
        .is_err());
    assert!(policy
        .decide(
            schedulers[0],
            &[work(schedulers[1], [0; 3]), work(schedulers[0], [0; 3])],
        )
        .is_err());
    assert!(VmWorkStealingPolicy::new(0, VmWorkStealingConfig::default()).is_err());
    assert!(VmWorkStealingConfig::new(0, 1, 0, 1, 1, [1; 3]).is_err());
    assert!(VmWorkStealingConfig::new(1, 0, 0, 1, 1, [1; 3]).is_err());
    assert!(VmWorkStealingConfig::new(1, 1, 0, 0, 1, [1; 3]).is_err());
    assert!(VmWorkStealingConfig::new(1, 1, 0, 2, 1, [1; 3]).is_err());
    assert!(VmWorkStealingConfig::new(1, 1, 0, 1, 1, [1, 0, 1]).is_err());
}
