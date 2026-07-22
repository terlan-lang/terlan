use super::*;

fn source(function: &str) -> VmProcessSource {
    VmProcessSource::new("app.ReceiveAccounting", function, 0)
}

fn total_reductions(runtime: &VmActorRuntime) -> u64 {
    runtime.scheduler.metrics().total_reductions
}

fn process_reductions(runtime: &VmActorRuntime, pid: VmProcessId) -> u64 {
    runtime
        .processes
        .get(pid)
        .expect("accounted process")
        .reductions
}

#[test]
fn actor_runtime_charges_receive_operations_without_charging_invalid_attempts() {
    let mut runtime = VmActorRuntime::default();
    let sender = runtime.spawn_root(source("sender"));
    let ordinary = runtime.spawn_root(source("ordinary"));
    let selective = runtime.spawn_root(source("selective"));
    let timeout = runtime.spawn_root(source("timeout"));
    runtime
        .send(sender, ordinary, ReplValue::Int(42))
        .expect("ordinary message");
    runtime
        .send(sender, selective, ReplValue::Atom("skip".to_string()))
        .expect("selective message");

    let total_before_message = total_reductions(&runtime);
    let memory_before_message = runtime.total_memory_reductions();
    let process_before_message = process_reductions(&runtime, ordinary);
    assert!(matches!(
        runtime
            .receive_next_or_block(ordinary)
            .expect("ordinary receive"),
        VmActorReceive::Message(message) if message.payload == ReplValue::Int(42)
    ));
    let memory_message_delta = runtime.total_memory_reductions() - memory_before_message;
    assert_eq!(
        total_reductions(&runtime) - total_before_message,
        memory_message_delta + 1
    );
    assert_eq!(
        process_reductions(&runtime, ordinary) - process_before_message,
        memory_message_delta + 1
    );

    let total_before_block = total_reductions(&runtime);
    let memory_before_block = runtime.total_memory_reductions();
    assert_eq!(
        runtime
            .selective_receive_or_block(selective, |_| false)
            .expect("selective miss"),
        VmActorReceive::Blocked
    );
    assert_eq!(total_reductions(&runtime) - total_before_block, 1);
    assert_eq!(runtime.total_memory_reductions(), memory_before_block);

    let scan_recipient = runtime.spawn_root(source("selective_scan"));
    for value in [1, 2, 3, 4] {
        runtime
            .send(sender, scan_recipient, ReplValue::Int(value))
            .expect("scan message");
    }
    let total_before_scan = total_reductions(&runtime);
    let memory_before_scan = runtime.total_memory_reductions();
    assert!(matches!(
        runtime
            .selective_receive_or_block(scan_recipient, |message| {
                message.payload == ReplValue::Int(3)
            })
            .expect("select third message"),
        VmActorReceive::Message(message) if message.payload == ReplValue::Int(3)
    ));
    let memory_scan_delta = runtime.total_memory_reductions() - memory_before_scan;
    assert_eq!(
        total_reductions(&runtime) - total_before_scan,
        memory_scan_delta + 3
    );

    let total_before_full_miss = total_reductions(&runtime);
    let memory_before_full_miss = runtime.total_memory_reductions();
    assert_eq!(
        runtime
            .selective_receive_or_block(scan_recipient, |_| false)
            .expect("scan all remaining messages"),
        VmActorReceive::Blocked
    );
    assert_eq!(total_reductions(&runtime) - total_before_full_miss, 3);
    assert_eq!(runtime.total_memory_reductions(), memory_before_full_miss);

    let total_before_timeout = total_reductions(&runtime);
    assert_eq!(
        runtime
            .receive_with_timeout(timeout, 0)
            .expect("immediate timeout"),
        VmActorReceive::Timeout
    );
    assert_eq!(total_reductions(&runtime) - total_before_timeout, 1);

    let total_before_invalid = total_reductions(&runtime);
    let missing = VmProcessId::from_raw_for_test(999);
    assert_eq!(
        runtime
            .receive_next_or_block(missing)
            .expect_err("missing receive"),
        "cannot receive missing process 999"
    );
    assert_eq!(total_reductions(&runtime), total_before_invalid);
}
