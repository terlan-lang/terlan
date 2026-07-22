use super::{VmFailureMonitorSnapshot, VmFailureProcessSnapshot, VmFailureRuntime, VmMonitorRef};
use crate::runtime::vm::process::{
    VmExitReason, VmProcessId, VmProcessInspectionError, VmProcessSource, VmProcessTable,
};
use crate::runtime::vm::reference::VmReferenceAllocator;

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.Failure", name, 0)
}

fn references() -> VmReferenceAllocator {
    VmReferenceAllocator::new("inspection-node", 2).expect("test reference namespace")
}

#[test]
fn failure_snapshot_reports_sorted_links_monitors_and_trap_state() {
    let mut processes = VmProcessTable::default();
    let actor = processes.spawn_root(source("actor"));
    let lower_peer = processes.spawn_root(source("lower-peer"));
    let higher_peer = processes.spawn_root(source("higher-peer"));
    let target = processes.spawn_root(source("target"));
    let watcher = processes.spawn_root(source("watcher"));
    let mut failure = VmFailureRuntime::default();
    let mut references = references();
    failure
        .link(&processes, higher_peer, actor)
        .expect("higher link");
    failure
        .link(&processes, actor, lower_peer)
        .expect("lower link");
    let first_monitor = failure
        .monitor(&mut references, &processes, actor, target)
        .expect("first monitor");
    let second_monitor = failure
        .monitor(&mut references, &processes, actor, higher_peer)
        .expect("second monitor");
    let inbound_monitor = failure
        .monitor(&mut references, &processes, watcher, actor)
        .expect("inbound monitor");
    failure
        .set_trap_exits(&processes, actor, true)
        .expect("trap exits");

    assert_eq!(
        failure.snapshot(&processes, actor).expect("snapshot"),
        VmFailureProcessSnapshot {
            pid: actor,
            trap_exits: true,
            links: vec![lower_peer, higher_peer],
            monitoring: vec![
                VmFailureMonitorSnapshot {
                    monitor_ref: first_monitor,
                    peer: target,
                },
                VmFailureMonitorSnapshot {
                    monitor_ref: second_monitor,
                    peer: higher_peer,
                },
            ],
            monitored_by: vec![VmFailureMonitorSnapshot {
                monitor_ref: inbound_monitor,
                peer: watcher,
            }],
        }
    );
}

#[test]
fn failure_snapshot_filters_unrelated_and_stale_relationships() {
    let mut processes = VmProcessTable::default();
    let actor = processes.spawn_root(source("actor"));
    let stale_peer = processes.spawn_root(source("stale-peer"));
    let unrelated_left = processes.spawn_root(source("unrelated-left"));
    let unrelated_right = processes.spawn_root(source("unrelated-right"));
    let mut failure = VmFailureRuntime::default();
    let mut references = references();
    failure
        .link(&processes, actor, stale_peer)
        .expect("actor link");
    failure
        .link(&processes, unrelated_left, unrelated_right)
        .expect("unrelated link");
    failure
        .monitor(&mut references, &processes, actor, stale_peer)
        .expect("actor monitor");
    processes
        .exit_process(stale_peer, VmExitReason::Normal)
        .expect("peer should exit outside failure layer");

    assert_eq!(
        failure.snapshot(&processes, actor).expect("snapshot"),
        VmFailureProcessSnapshot {
            pid: actor,
            trap_exits: false,
            links: Vec::new(),
            monitoring: Vec::new(),
            monitored_by: Vec::new(),
        }
    );
}

#[test]
fn failure_snapshot_finds_process_on_right_side_of_canonical_link() {
    let mut processes = VmProcessTable::default();
    let lower_peer = processes.spawn_root(source("lower-peer"));
    let actor = processes.spawn_root(source("actor"));
    let mut failure = VmFailureRuntime::default();
    failure
        .link(&processes, actor, lower_peer)
        .expect("canonical link");

    assert_eq!(
        failure
            .snapshot(&processes, actor)
            .expect("actor snapshot")
            .links,
        [lower_peer]
    );
}

#[test]
fn failure_snapshot_retains_exited_identity_without_active_relationships() {
    let mut processes = VmProcessTable::default();
    let actor = processes.spawn_root(source("actor"));
    let peer = processes.spawn_root(source("peer"));
    let mut failure = VmFailureRuntime::default();
    let mut references = references();
    failure.link(&processes, actor, peer).expect("link");
    failure
        .monitor(&mut references, &processes, actor, peer)
        .expect("monitor");
    failure
        .set_trap_exits(&processes, actor, true)
        .expect("trap exits");

    failure
        .exit_process(&mut processes, actor, VmExitReason::Normal)
        .expect("actor exit");

    assert_eq!(
        failure
            .snapshot(&processes, actor)
            .expect("exited snapshot"),
        VmFailureProcessSnapshot {
            pid: actor,
            trap_exits: false,
            links: Vec::new(),
            monitoring: Vec::new(),
            monitored_by: Vec::new(),
        }
    );
    assert_eq!(
        failure
            .snapshot(&processes, peer)
            .expect("peer snapshot")
            .links,
        Vec::<VmProcessId>::new()
    );
}

#[test]
fn failure_snapshot_rejects_missing_process_with_typed_identity() {
    let processes = VmProcessTable::default();
    let failure = VmFailureRuntime::default();
    let missing = VmProcessId::from_raw_for_test(404);

    assert_eq!(
        failure
            .snapshot(&processes, missing)
            .expect_err("missing process should fail"),
        VmProcessInspectionError::MissingProcess(missing)
    );
}

#[test]
fn failure_monitor_snapshot_exposes_stable_reference_identity() {
    let monitor_ref = VmMonitorRef(
        references()
            .allocate_reference()
            .expect("monitor reference"),
    );
    let snapshot = VmFailureMonitorSnapshot {
        monitor_ref,
        peer: VmProcessId::from_raw_for_test(9),
    };

    assert_eq!(snapshot.monitor_ref.as_u64(), 1);
    assert_eq!(snapshot.peer.as_u64(), 9);
}
