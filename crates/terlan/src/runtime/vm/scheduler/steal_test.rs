use super::*;
use crate::runtime::vm::actor_directory::VmActorLifecycle;
use crate::runtime::vm::process::{VmProcess, VmProcessSource};
use crate::runtime::vm::scheduler_topology::VmSchedulerTopology;
use crate::runtime::vm::work_stealing::{
    VmSchedulerWorkSnapshot, VmStealPlan, VmWorkDirective, VmWorkStealingConfig,
    VmWorkStealingPolicy,
};

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.Steal", name, 0)
}

fn two_scheduler_plan(maximum_actors: usize, victim_load: usize) -> VmStealPlan {
    let topology = VmSchedulerTopology::new(2).expect("topology");
    let schedulers = topology.schedulers().collect::<Vec<_>>();
    let config = VmWorkStealingConfig::new(1, maximum_actors, 0, 1, 8, [4, 8, 16])
        .expect("work-stealing config");
    let mut policy = VmWorkStealingPolicy::new(2, config).expect("policy");
    let snapshots = [
        VmSchedulerWorkSnapshot::new(schedulers[0], [0, victim_load, 0], [0, 1, 0]),
        VmSchedulerWorkSnapshot::new(schedulers[1], [0, 0, 0], [0, 0, 0]),
    ];
    match policy
        .decide(schedulers[1], &snapshots)
        .expect("steal decision")
    {
        VmWorkDirective::Steal(plan) => plan,
        directive => panic!("expected steal plan, found {directive:?}"),
    }
}

#[test]
fn victim_claim_removes_queue_tail_and_abort_restores_order_and_age() {
    let mut processes = VmProcessTable::default();
    let actors = ["first", "second", "third"].map(|name| processes.spawn_root(source(name)));
    let mut scheduler = VmScheduler::default();
    for actor in actors {
        scheduler
            .enqueue_runnable(&processes, actor)
            .expect("queue actor");
    }
    let before = scheduler.diagnostic_queued_processes();
    let claim = scheduler
        .claim_stealable_process(&processes, VmSchedulerClass::Normal)
        .expect("claim")
        .expect("candidate");
    assert_eq!(claim.process_id(), actors[2]);
    assert_eq!(claim.class(), VmSchedulerClass::Normal);
    assert_eq!(claim.enqueued_tick(), 0);
    assert_eq!(scheduler.diagnostic_queued_processes(), actors[..2]);
    assert_eq!(
        processes.actor_lifecycle(actors[2]).expect("lifecycle"),
        VmActorLifecycle::Migrating
    );

    scheduler
        .abort_steal_claim(&processes, claim)
        .expect("abort claim");
    assert_eq!(scheduler.diagnostic_queued_processes(), before);
    assert_eq!(
        processes.actor_lifecycle(actors[2]).expect("lifecycle"),
        VmActorLifecycle::Queued
    );
}

#[test]
fn class_claim_is_exact_and_empty_class_has_no_side_effect() {
    let mut processes = VmProcessTable::default();
    let normal = processes.spawn_root(source("normal"));
    let priority = processes.spawn_root(source("priority"));
    let mut scheduler = VmScheduler::default();
    scheduler
        .enqueue_runnable(&processes, normal)
        .expect("queue normal");
    scheduler
        .enqueue_runnable_with_class(&processes, priority, VmSchedulerClass::Priority)
        .expect("queue priority");
    assert!(scheduler
        .claim_stealable_process(&processes, VmSchedulerClass::Background)
        .expect("empty class")
        .is_none());
    let claim = scheduler
        .claim_stealable_process(&processes, VmSchedulerClass::Priority)
        .expect("priority claim")
        .expect("priority candidate");
    assert_eq!(claim.process_id(), priority);
    assert_eq!(claim.class(), VmSchedulerClass::Priority);
    assert_eq!(scheduler.diagnostic_queued_processes(), vec![normal]);
    scheduler
        .abort_steal_claim(&processes, claim)
        .expect("abort priority claim");
}

#[test]
fn stale_nonrunnable_tail_fails_without_queue_or_lifecycle_mutation() {
    let mut processes = VmProcessTable::default();
    let actor = processes.spawn_root(source("stale"));
    let mut scheduler = VmScheduler::default();
    scheduler
        .enqueue_runnable(&processes, actor)
        .expect("queue actor");
    processes
        .with_process_control_mutator(actor, VmProcess::suspend)
        .expect("mutate process")
        .expect("suspend process");
    let error = scheduler
        .claim_stealable_process(&processes, VmSchedulerClass::Normal)
        .expect_err("suspended queue tail must fail");
    assert!(error.contains("not runnable"), "{error}");
    assert_eq!(scheduler.diagnostic_queued_processes(), vec![actor]);
    assert_eq!(
        processes.actor_lifecycle(actor).expect("queued lifecycle"),
        VmActorLifecycle::Queued
    );
}

#[test]
fn scheduler_claim_is_send_and_cannot_duplicate_actor_authority() {
    fn assert_send<T: Send>() {}
    assert_send::<VmSchedulerStealClaim>();

    let mut processes = VmProcessTable::default();
    let actor = processes.spawn_root(source("linear"));
    let mut scheduler = VmScheduler::default();
    scheduler
        .enqueue_runnable(&processes, actor)
        .expect("queue actor");
    let claim = scheduler
        .claim_stealable_process(&processes, VmSchedulerClass::Normal)
        .expect("claim")
        .expect("candidate");
    assert!(scheduler
        .claim_stealable_process(&processes, VmSchedulerClass::Normal)
        .expect("empty after claim")
        .is_none());
    scheduler
        .abort_steal_claim(&processes, claim)
        .expect("abort linear claim");
}

#[test]
fn bounded_batch_moves_newest_work_and_preserves_class_and_wait_age() {
    let topology = VmSchedulerTopology::new(2).expect("topology");
    let schedulers = topology.schedulers().collect::<Vec<_>>();
    let mut processes = VmProcessTable::default();
    let actors = ["one", "two", "three", "four"].map(|name| processes.spawn_root(source(name)));
    let mut victim = VmScheduler::with_owner(Default::default(), schedulers[0].owner_word());
    let mut thief = VmScheduler::with_owner(Default::default(), schedulers[1].owner_word());
    for actor in actors {
        victim
            .enqueue_runnable(&processes, actor)
            .expect("queue victim actor");
    }
    victim.tick = 23;
    thief.tick = 101;

    let transferred = transfer_steal_batch(
        schedulers[0],
        &mut victim,
        schedulers[1],
        &mut thief,
        &processes,
        two_scheduler_plan(2, actors.len()),
    )
    .expect("bounded transfer");

    assert_eq!(transferred, 2);
    assert_eq!(victim.diagnostic_queued_processes(), actors[..2]);
    assert_eq!(
        thief.diagnostic_queued_processes(),
        vec![actors[3], actors[2]]
    );
    for actor in actors[2..].iter().copied() {
        assert_eq!(thief.classes.get(&actor), Some(&VmSchedulerClass::Normal));
        assert_eq!(thief.enqueued_at.get(&actor), Some(&78));
        assert_eq!(
            processes.actor_lifecycle(actor).expect("destination queue"),
            VmActorLifecycle::Queued
        );
    }
}

#[test]
fn destination_collision_rolls_current_claim_back_without_losing_victim_order() {
    let topology = VmSchedulerTopology::new(2).expect("topology");
    let schedulers = topology.schedulers().collect::<Vec<_>>();
    let mut processes = VmProcessTable::default();
    let actor = processes.spawn_root(source("collision"));
    let mut victim = VmScheduler::with_owner(Default::default(), schedulers[0].owner_word());
    let mut thief = VmScheduler::with_owner(Default::default(), schedulers[1].owner_word());
    victim
        .enqueue_runnable(&processes, actor)
        .expect("queue victim actor");
    thief.classes.insert(actor, VmSchedulerClass::Normal);

    let error = transfer_steal_batch(
        schedulers[0],
        &mut victim,
        schedulers[1],
        &mut thief,
        &processes,
        two_scheduler_plan(1, 1),
    )
    .expect_err("destination collision");

    assert!(error.contains("already has placement"), "{error}");
    assert_eq!(victim.diagnostic_queued_processes(), vec![actor]);
    assert_eq!(
        processes.actor_lifecycle(actor).expect("rolled back queue"),
        VmActorLifecycle::Queued
    );
}

#[test]
fn plan_and_scheduler_owner_mismatch_fail_before_victim_mutation() {
    let topology = VmSchedulerTopology::new(2).expect("topology");
    let schedulers = topology.schedulers().collect::<Vec<_>>();
    let mut processes = VmProcessTable::default();
    let actor = processes.spawn_root(source("identity"));
    let mut victim = VmScheduler::with_owner(Default::default(), schedulers[0].owner_word());
    let mut thief = VmScheduler::with_owner(Default::default(), schedulers[1].owner_word());
    victim
        .enqueue_runnable(&processes, actor)
        .expect("queue victim actor");
    let plan = two_scheduler_plan(1, 1);

    let error = transfer_steal_batch(
        schedulers[1],
        &mut victim,
        schedulers[0],
        &mut thief,
        &processes,
        plan,
    )
    .expect_err("plan identities must match");
    assert!(error.contains("identities"), "{error}");
    assert_eq!(victim.diagnostic_queued_processes(), vec![actor]);

    let mut wrong_owner = VmScheduler::with_owner(Default::default(), schedulers[1].owner_word());
    let error = transfer_steal_batch(
        schedulers[0],
        &mut wrong_owner,
        schedulers[1],
        &mut thief,
        &processes,
        two_scheduler_plan(1, 1),
    )
    .expect_err("owner identity must match");
    assert!(error.contains("mutator owner"), "{error}");
}

#[test]
fn live_snapshot_reports_exact_class_load_and_oldest_wait() {
    let topology = VmSchedulerTopology::new(1).expect("topology");
    let scheduler_id = topology.schedulers().next().expect("scheduler");
    let mut processes = VmProcessTable::default();
    let recent = processes.spawn_root(source("recent"));
    let oldest = processes.spawn_root(source("oldest"));
    let priority = processes.spawn_root(source("priority"));
    let mut scheduler = VmScheduler::with_owner(Default::default(), scheduler_id.owner_word());
    scheduler
        .enqueue_runnable(&processes, recent)
        .expect("recent normal");
    scheduler
        .enqueue_runnable(&processes, oldest)
        .expect("oldest normal");
    scheduler
        .enqueue_runnable_with_class(&processes, priority, VmSchedulerClass::Priority)
        .expect("priority");
    scheduler.tick = 50;
    scheduler.enqueued_at.insert(recent, 45);
    scheduler.enqueued_at.insert(oldest, 11);
    scheduler.enqueued_at.insert(priority, 40);

    let snapshot = scheduler
        .work_snapshot(scheduler_id, true)
        .expect("live snapshot");
    assert_eq!(snapshot.runnable_in(VmSchedulerClass::Priority), 1);
    assert_eq!(snapshot.runnable_in(VmSchedulerClass::Normal), 2);
    assert_eq!(snapshot.runnable_in(VmSchedulerClass::Background), 0);
    assert_eq!(snapshot.oldest_wait_in(VmSchedulerClass::Priority), 10);
    assert_eq!(snapshot.oldest_wait_in(VmSchedulerClass::Normal), 39);
    assert!(scheduler.work_snapshot(scheduler_id, false).is_ok());
}

#[test]
fn batch_abort_reconstructs_original_victim_tail_order() {
    let mut processes = VmProcessTable::default();
    let actors = ["one", "two", "three", "four"].map(|name| processes.spawn_root(source(name)));
    let mut scheduler = VmScheduler::default();
    for actor in actors {
        scheduler
            .enqueue_runnable(&processes, actor)
            .expect("queue actor");
    }
    let batch = scheduler
        .claim_stealable_batch(&processes, VmSchedulerClass::Normal, 3)
        .expect("claim batch");
    assert_eq!(batch.len(), 3);
    assert_eq!(scheduler.diagnostic_queued_processes(), vec![actors[0]]);
    scheduler
        .abort_steal_batch(&processes, batch)
        .expect("abort batch");
    assert_eq!(scheduler.diagnostic_queued_processes(), actors);
}
