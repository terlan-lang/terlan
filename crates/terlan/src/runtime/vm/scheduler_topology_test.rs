use std::num::NonZeroU64;

use super::*;
use crate::runtime::vm::actor::VmActorRuntime;
use crate::runtime::vm::process::{VmExitReason, VmProcessSource};
use crate::runtime::vm::scheduler::{VmSchedulerDecision, VmSchedulerOutcome};

#[test]
fn explicit_width_is_bounded_and_one_is_supported() {
    assert_eq!(VmSchedulerTopology::new(1).expect("one").width(), 1);
    assert_eq!(
        VmSchedulerTopology::new(VM_MAX_SCHEDULERS)
            .expect("maximum")
            .width(),
        VM_MAX_SCHEDULERS
    );
    assert!(VmSchedulerTopology::new(0).is_err());
    assert!(VmSchedulerTopology::new(VM_MAX_SCHEDULERS + 1).is_err());
}

#[test]
fn actor_home_placement_is_deterministic_and_balanced() {
    let topology = VmSchedulerTopology::new(3).expect("topology");
    let homes = (1..=9)
        .map(|actor| {
            topology
                .home_scheduler(NonZeroU64::new(actor).expect("actor"))
                .index()
        })
        .collect::<Vec<_>>();
    assert_eq!(homes, vec![0, 1, 2, 0, 1, 2, 0, 1, 2]);
    assert_eq!(
        topology
            .schedulers()
            .map(VmSchedulerId::index)
            .collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
}

#[test]
fn explicit_migration_changes_current_scheduler_but_not_home_identity() {
    let topology = VmSchedulerTopology::new(3).expect("topology");
    let route = topology.route(NonZeroU64::new(1).expect("actor"));
    let destination = topology.schedulers().nth(2).expect("destination");
    let migrated = route.migrated_to(destination).expect("migrated route");
    assert_eq!(route.home_scheduler().index(), 0);
    assert_eq!(migrated.home_scheduler(), route.home_scheduler());
    assert_eq!(migrated.scheduler(), destination);
    assert!(migrated.migrated_to(destination).is_err());
}

#[test]
fn scheduler_owner_words_are_unique_and_nonzero() {
    let topology = VmSchedulerTopology::new(4).expect("topology");
    assert_eq!(
        topology
            .schedulers()
            .map(|scheduler| scheduler.owner_word().get())
            .collect::<Vec<_>>(),
        vec![1, 2, 3, 4]
    );
}

#[test]
fn linux_cpu_lists_are_validated_and_deduplicated() {
    assert_eq!(cpu_list_count("0-2,2,4,6-7\n"), Some(6));
    assert_eq!(cpu_list_count(""), None);
    assert_eq!(cpu_list_count("3-1"), None);
    assert_eq!(cpu_list_count("0,a"), None);
}

#[test]
fn cgroup_quota_rounds_up_without_inventing_an_unbounded_limit() {
    assert_eq!(cgroup_v2_quota("200000 100000"), Some(2));
    assert_eq!(cgroup_v2_quota("150000 100000"), Some(2));
    assert_eq!(cgroup_v2_quota("1 100000"), Some(1));
    assert_eq!(cgroup_v2_quota("max 100000"), None);
    assert_eq!(cgroup_v2_quota("100 0"), None);
    assert_eq!(cgroup_v2_quota("100 100 extra"), None);
    assert_eq!(
        parse_cgroup_v2_quota("max 100000"),
        Some(CgroupV2CpuQuota {
            quota_micros: None,
            period_micros: 100000,
            scheduler_limit: None,
        })
    );
}

#[test]
fn effective_parallelism_uses_the_smallest_visible_limit() {
    assert_eq!(effective_parallelism(16, Some(8), Some(3)), 3);
    assert_eq!(effective_parallelism(2, Some(8), Some(6)), 2);
    assert_eq!(effective_parallelism(0, None, None), 1);
}

#[test]
fn default_width_prefers_physical_cores_without_exceeding_effective_limits() {
    assert_eq!(default_scheduler_width(24, Some(16)), 16);
    assert_eq!(default_scheduler_width(8, Some(16)), 8);
    assert_eq!(default_scheduler_width(6, None), 6);
}

#[test]
fn linux_affinity_and_physical_topology_parsers_are_deterministic() {
    let status = "Name:\tterlan\nCpus_allowed_list:\t0-2,4\n";
    assert_eq!(process_affinity_list(status).as_deref(), Some("0-2,4"));
    assert_eq!(process_affinity_list("Name:\tterlan\n"), None);

    let cpuinfo = "\
processor : 0
physical id : 0
core id : 0

processor : 1
physical id : 0
core id : 0

processor : 2
physical id : 0
core id : 1

processor : 3
physical id : 1
core id : 0
";
    assert_eq!(physical_core_count(cpuinfo), Some(3));
    assert_eq!(physical_core_count("processor : 0\n"), None);
}

#[test]
fn captured_host_snapshot_has_nonzero_effective_capacity() {
    let snapshot = VmSchedulerHostSnapshot::capture();
    assert!((1..=VM_MAX_SCHEDULERS).contains(&snapshot.effective_parallelism()));
    assert!(snapshot.host_logical_cpus > 0);
}

#[test]
fn actor_runtime_executes_under_its_assigned_scheduler_owner() {
    let mut runtime = VmActorRuntime::with_scheduler_owner(
        VmSchedulerTopology::new(2)
            .expect("topology")
            .home_scheduler(NonZeroU64::new(2).expect("actor"))
            .owner_word(),
    )
    .expect("scheduler-owned runtime");
    let pid = runtime.spawn_root(VmProcessSource::new("app.Main", "worker", 0));
    let run = runtime
        .run_next(|_, _| VmSchedulerDecision::Exit {
            reductions: 1,
            reason: VmExitReason::Normal,
        })
        .expect("assigned scheduler owns actor mutator");
    assert_eq!(run.pid, Some(pid));
    assert!(matches!(run.outcome, VmSchedulerOutcome::Exited(_)));
}
