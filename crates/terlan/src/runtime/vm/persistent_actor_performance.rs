#![allow(dead_code)]

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VmPersistentActorPerformanceFixture {
    pub(crate) name: String,
    pub(crate) event_count: u64,
    pub(crate) snapshot_count: u64,
    pub(crate) mailbox_count: u64,
    pub(crate) timer_count: u64,
    pub(crate) resource_count: u64,
    pub(crate) compacted_event_count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VmPersistentActorPerformanceBudget {
    pub(crate) fixture_name: String,
    pub(crate) p50_ticks: u64,
    pub(crate) p95_ticks: u64,
    pub(crate) p99_ticks: u64,
    pub(crate) throughput_events_per_tick: u64,
    pub(crate) memory_bytes: u64,
    pub(crate) disk_bytes: u64,
    pub(crate) replay_bytes: u64,
    pub(crate) scheduler_ticks: u64,
    pub(crate) budget_pass: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum VmPersistentActorPerformanceError {
    EmptyFixtureName,
    EmptyWorkload,
    CompactedEventsExceedTotal {
        event_count: u64,
        compacted_event_count: u64,
    },
}

pub(crate) fn estimate_persistent_actor_performance_budget(
    fixture: &VmPersistentActorPerformanceFixture,
) -> Result<VmPersistentActorPerformanceBudget, VmPersistentActorPerformanceError> {
    if fixture.name.is_empty() {
        return Err(VmPersistentActorPerformanceError::EmptyFixtureName);
    }
    if fixture.event_count == 0
        && fixture.snapshot_count == 0
        && fixture.mailbox_count == 0
        && fixture.timer_count == 0
        && fixture.resource_count == 0
    {
        return Err(VmPersistentActorPerformanceError::EmptyWorkload);
    }
    if fixture.compacted_event_count > fixture.event_count {
        return Err(
            VmPersistentActorPerformanceError::CompactedEventsExceedTotal {
                event_count: fixture.event_count,
                compacted_event_count: fixture.compacted_event_count,
            },
        );
    }

    let live_events = fixture.event_count - fixture.compacted_event_count;
    let scheduler_ticks = 1
        + live_events
        + fixture.snapshot_count * 3
        + fixture.mailbox_count * 2
        + fixture.timer_count * 2
        + fixture.resource_count * 4;
    let serialization_ticks = fixture.snapshot_count * 5 + live_events / 4;
    let adapter_ticks = fixture.snapshot_count * 7 + fixture.resource_count * 2;
    let p50_ticks = scheduler_ticks + serialization_ticks;
    let p95_ticks = p50_ticks + adapter_ticks + fixture.mailbox_count;
    let p99_ticks = p95_ticks + fixture.timer_count + fixture.resource_count;
    let memory_bytes = 128
        + live_events * 32
        + fixture.mailbox_count * 48
        + fixture.timer_count * 40
        + fixture.resource_count * 96;
    let disk_bytes = fixture.snapshot_count * 256 + live_events * 24 + fixture.resource_count * 64;
    let replay_bytes = live_events * 24 + fixture.mailbox_count * 32 + fixture.timer_count * 16;
    let throughput_events_per_tick = fixture.event_count.max(1) / scheduler_ticks.max(1);
    let budget_pass = p99_ticks <= 50_000 && memory_bytes <= 8 * 1024 * 1024;

    Ok(VmPersistentActorPerformanceBudget {
        fixture_name: fixture.name.clone(),
        p50_ticks,
        p95_ticks,
        p99_ticks,
        throughput_events_per_tick,
        memory_bytes,
        disk_bytes,
        replay_bytes,
        scheduler_ticks,
        budget_pass,
    })
}

pub(crate) fn small_actor_fixture() -> VmPersistentActorPerformanceFixture {
    VmPersistentActorPerformanceFixture {
        name: "small actor append".to_string(),
        event_count: 8,
        snapshot_count: 1,
        mailbox_count: 1,
        timer_count: 1,
        resource_count: 0,
        compacted_event_count: 0,
    }
}

pub(crate) fn event_storm_fixture() -> VmPersistentActorPerformanceFixture {
    VmPersistentActorPerformanceFixture {
        name: "event storm".to_string(),
        event_count: 10_000,
        snapshot_count: 4,
        mailbox_count: 128,
        timer_count: 32,
        resource_count: 4,
        compacted_event_count: 0,
    }
}

#[cfg(test)]
#[path = "persistent_actor_performance_test.rs"]
mod persistent_actor_performance_test;
