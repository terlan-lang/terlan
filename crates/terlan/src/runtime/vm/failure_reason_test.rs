use super::{reason_value, VmFailureRuntime};
use crate::runtime::vm::process::{VmExitReason, VmProcessSource, VmProcessState, VmProcessTable};
use crate::runtime::vm::reference::VmReferenceAllocator;
use crate::runtime::vm::ReplValue;

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.Main", name, 0)
}

fn typed_failure_reasons() -> Vec<VmExitReason> {
    vec![
        VmExitReason::Error(String::new()),
        VmExitReason::Error("adapter timeout".to_string()),
        VmExitReason::Error("failure\nwith\0control".to_string()),
        VmExitReason::Error("x".repeat(4_096)),
        VmExitReason::Killed,
        VmExitReason::ShutdownTimeout { timeout_ms: 750 },
        VmExitReason::MemoryLimitExceeded {
            requested_bytes: 128,
            previous_bytes: 960,
            projected_bytes: 1_088,
        },
    ]
}

#[test]
fn typed_failure_reasons_survive_process_link_and_monitor_delivery() {
    for reason in typed_failure_reasons() {
        let mut table = VmProcessTable::default();
        let target = table.spawn_root(source("target"));
        let trapper = table.spawn_root(source("trapper"));
        let watcher = table.spawn_root(source("watcher"));
        let mut failure = VmFailureRuntime::default();
        let mut references =
            VmReferenceAllocator::new("test-node", 11).expect("test reference namespace");

        failure
            .link(&table, target, trapper)
            .expect("link should succeed");
        failure
            .set_trap_exits(&table, trapper, true)
            .expect("trap exits should enable");
        let monitor_ref = failure
            .monitor(&mut references, &table, watcher, target)
            .expect("monitor should register");

        let report = failure
            .exit_process(&mut table, target, reason.clone())
            .expect("typed failure exit should succeed");

        assert_eq!(report.exited, vec![target]);
        assert_eq!(report.delivered_exit_signals, 1);
        assert_eq!(report.delivered_down_messages, 1);
        assert_eq!(
            table.get(target).expect("target should exist").state,
            VmProcessState::Exited(reason.clone())
        );

        let trapped_exit = table
            .get_mut(trapper)
            .expect("trapper should exist")
            .receive_next()
            .expect("trapped exit should be delivered");
        assert_eq!(
            trapped_exit.payload,
            ReplValue::Tuple(vec![
                ReplValue::Atom("exit".to_string()),
                ReplValue::Int(target.as_u64() as i64),
                reason_value(&reason),
            ])
        );

        let monitor_down = table
            .get_mut(watcher)
            .expect("watcher should exist")
            .receive_next()
            .expect("monitor down should be delivered");
        assert_eq!(
            monitor_down.payload,
            ReplValue::Tuple(vec![
                ReplValue::Atom("down".to_string()),
                ReplValue::Int(monitor_ref.as_u64() as i64),
                ReplValue::Int(target.as_u64() as i64),
                reason_value(&reason),
            ])
        );
    }
}
