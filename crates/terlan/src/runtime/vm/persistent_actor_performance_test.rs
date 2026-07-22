use super::{
    estimate_persistent_actor_performance_budget, event_storm_fixture, small_actor_fixture,
    VmPersistentActorPerformanceError, VmPersistentActorPerformanceFixture,
};

#[test]
fn vm_persistent_actor_performance_estimates_small_actor_budget() {
    let budget =
        estimate_persistent_actor_performance_budget(&small_actor_fixture()).expect("budget");

    assert_eq!(budget.fixture_name, "small actor append");
    assert!(budget.p50_ticks > 0);
    assert!(budget.p95_ticks >= budget.p50_ticks);
    assert!(budget.p99_ticks >= budget.p95_ticks);
    assert!(budget.memory_bytes > 0);
    assert!(budget.disk_bytes > 0);
    assert!(budget.replay_bytes > 0);
    assert!(budget.scheduler_ticks > 0);
    assert!(budget.budget_pass);
}

#[test]
fn vm_persistent_actor_performance_scales_event_storm_above_small_actor() {
    let small =
        estimate_persistent_actor_performance_budget(&small_actor_fixture()).expect("small");
    let storm =
        estimate_persistent_actor_performance_budget(&event_storm_fixture()).expect("storm");

    assert!(storm.p99_ticks > small.p99_ticks);
    assert!(storm.memory_bytes > small.memory_bytes);
    assert!(storm.disk_bytes > small.disk_bytes);
    assert!(storm.replay_bytes > small.replay_bytes);
    assert!(storm.scheduler_ticks > small.scheduler_ticks);
}

#[test]
fn vm_persistent_actor_performance_compaction_reduces_replay_budget() {
    let uncompacted = event_storm_fixture();
    let mut compacted = event_storm_fixture();
    compacted.compacted_event_count = 9_000;

    let uncompacted_budget =
        estimate_persistent_actor_performance_budget(&uncompacted).expect("uncompacted");
    let compacted_budget =
        estimate_persistent_actor_performance_budget(&compacted).expect("compacted");

    assert!(compacted_budget.replay_bytes < uncompacted_budget.replay_bytes);
    assert!(compacted_budget.p99_ticks < uncompacted_budget.p99_ticks);
    assert!(compacted_budget.disk_bytes < uncompacted_budget.disk_bytes);
}

#[test]
fn vm_persistent_actor_performance_rejects_empty_fixture_name_and_workload() {
    let mut empty_name = small_actor_fixture();
    empty_name.name.clear();
    assert_eq!(
        estimate_persistent_actor_performance_budget(&empty_name),
        Err(VmPersistentActorPerformanceError::EmptyFixtureName)
    );

    let empty_workload = VmPersistentActorPerformanceFixture {
        name: "empty".to_string(),
        event_count: 0,
        snapshot_count: 0,
        mailbox_count: 0,
        timer_count: 0,
        resource_count: 0,
        compacted_event_count: 0,
    };
    assert_eq!(
        estimate_persistent_actor_performance_budget(&empty_workload),
        Err(VmPersistentActorPerformanceError::EmptyWorkload)
    );
}

#[test]
fn vm_persistent_actor_performance_rejects_invalid_compaction_count() {
    let mut fixture = small_actor_fixture();
    fixture.compacted_event_count = fixture.event_count + 1;

    assert_eq!(
        estimate_persistent_actor_performance_budget(&fixture),
        Err(
            VmPersistentActorPerformanceError::CompactedEventsExceedTotal {
                event_count: fixture.event_count,
                compacted_event_count: fixture.compacted_event_count,
            },
        )
    );
}
