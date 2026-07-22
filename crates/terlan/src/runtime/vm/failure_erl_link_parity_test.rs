use super::{VmFailureProcessSnapshot, VmFailureRuntime};
use crate::runtime::vm::process::{VmExitReason, VmProcessSource, VmProcessTable};
use crate::runtime::vm::reference::VmReferenceAllocator;
use crate::runtime::vm::ReplValue;

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.ErlLinkParity", name, 0)
}

fn references() -> VmReferenceAllocator {
    VmReferenceAllocator::new("erl-link-parity", 1).expect("test reference namespace")
}

#[test]
fn erl_link_suite_portable_link_monitor_race_contract() {
    let mut processes = VmProcessTable::default();
    let target = processes.spawn_root(source("target"));
    let trapped_peer = processes.spawn_root(source("trapped-peer"));
    let watcher = processes.spawn_root(source("watcher"));
    let unrelated = processes.spawn_root(source("unrelated"));
    let mut failure = VmFailureRuntime::default();
    let mut references = references();

    failure
        .set_trap_exits(&processes, trapped_peer, true)
        .expect("peer should trap exits");

    // Model both sides repeatedly racing link and unlink. The VM serializes
    // mutations, so every prefix must expose one canonical, symmetric state.
    for round in 0..128 {
        failure
            .link(&processes, target, trapped_peer)
            .expect("forward link should succeed");
        failure
            .link(&processes, trapped_peer, target)
            .expect("reverse duplicate link should succeed");
        assert!(failure.is_linked(target, trapped_peer));
        assert_eq!(
            failure
                .snapshot(&processes, target)
                .expect("target link snapshot")
                .links,
            [trapped_peer]
        );

        if round % 3 != 0 {
            failure.unlink(trapped_peer, target);
            failure.unlink(target, trapped_peer);
            assert!(!failure.is_linked(target, trapped_peer));
        }
    }
    failure
        .link(&processes, trapped_peer, target)
        .expect("final link should succeed");

    let retained_monitor = failure
        .monitor(&mut references, &processes, watcher, target)
        .expect("retained monitor should register");
    let cancelled_monitor = failure
        .monitor(&mut references, &processes, watcher, target)
        .expect("cancelled monitor should register");
    assert!(failure
        .demonitor_for(watcher, cancelled_monitor.clone())
        .expect("monitor owner should demonitor"));
    assert!(!failure
        .demonitor_for(watcher, cancelled_monitor)
        .expect("repeated demonitor should be a no-op"));

    let report = failure
        .exit_process(
            &mut processes,
            target,
            VmExitReason::Error("link-race".to_string()),
        )
        .expect("target exit should complete");

    assert_eq!(report.exited, [target]);
    assert_eq!(report.delivered_exit_signals, 1);
    assert_eq!(report.delivered_down_messages, 1);
    assert_eq!(failure.monitor_count(), 0);
    assert!(!failure.is_linked(target, trapped_peer));

    let watcher_message = processes
        .get_mut(watcher)
        .expect("watcher should remain live")
        .receive_next()
        .expect("one down message should arrive");
    assert_eq!(
        watcher_message.payload,
        ReplValue::Tuple(vec![
            ReplValue::Atom("down".to_string()),
            ReplValue::Int(retained_monitor.as_u64() as i64),
            ReplValue::Int(target.as_u64() as i64),
            ReplValue::Tuple(vec![
                ReplValue::Atom("error".to_string()),
                ReplValue::String("link-race".to_string()),
            ]),
        ])
    );
    assert!(processes
        .get_mut(watcher)
        .expect("watcher should remain live")
        .receive_next()
        .is_none());

    let trapped_message = processes
        .get_mut(trapped_peer)
        .expect("trapped peer should remain live")
        .receive_next()
        .expect("one trapped exit should arrive");
    assert_eq!(
        trapped_message.payload,
        ReplValue::Tuple(vec![
            ReplValue::Atom("exit".to_string()),
            ReplValue::Int(target.as_u64() as i64),
            ReplValue::Tuple(vec![
                ReplValue::Atom("error".to_string()),
                ReplValue::String("link-race".to_string()),
            ]),
        ])
    );
    assert!(processes
        .get_mut(trapped_peer)
        .expect("trapped peer should remain live")
        .receive_next()
        .is_none());

    assert_eq!(
        failure
            .snapshot(&processes, trapped_peer)
            .expect("peer cleanup snapshot"),
        VmFailureProcessSnapshot {
            pid: trapped_peer,
            trap_exits: true,
            links: Vec::new(),
            monitoring: Vec::new(),
            monitored_by: Vec::new(),
        }
    );
    assert_eq!(
        failure
            .snapshot(&processes, unrelated)
            .expect("unrelated process snapshot"),
        VmFailureProcessSnapshot {
            pid: unrelated,
            trap_exits: false,
            links: Vec::new(),
            monitoring: Vec::new(),
            monitored_by: Vec::new(),
        }
    );

    let duplicate = failure
        .exit_process(&mut processes, target, VmExitReason::Killed)
        .expect("duplicate exit should be a no-op");
    assert_eq!(duplicate.delivered_exit_signals, 0);
    assert_eq!(duplicate.delivered_down_messages, 0);
    assert!(duplicate.exited.is_empty());
}
