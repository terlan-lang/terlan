use super::*;

fn source(function: &str) -> VmProcessSource {
    VmProcessSource::new("app.CheckpointAccounting", function, 0)
}

fn process_reductions(runtime: &VmActorRuntime, pid: VmProcessId) -> u64 {
    runtime
        .processes
        .get(pid)
        .expect("accounted process")
        .reductions
}

#[test]
fn actor_runtime_separates_checkpoint_operation_and_memory_reductions() {
    let limits = VmMemoryLimits::new(8, 12).expect("test limits");
    let mut runtime = VmActorRuntime::with_memory_limits(limits);
    let recipient = runtime.spawn_root(source("recipient"));

    let recipient_before_restore = process_reductions(&runtime, recipient);
    let total_before_restore = runtime.scheduler.metrics().total_reductions;
    let memory_before_restore = runtime.total_memory_reductions();
    assert_eq!(
        runtime
            .restore_mailbox_checkpoint(recipient, vec![ReplValue::Int(10)])
            .expect("restore checkpoint"),
        [1]
    );
    let restored_memory = runtime.total_memory_reductions() - memory_before_restore;
    assert_eq!(restored_memory, 2);
    assert_eq!(
        process_reductions(&runtime, recipient) - recipient_before_restore,
        restored_memory + 1
    );
    assert_eq!(
        runtime.scheduler.metrics().total_reductions - total_before_restore,
        restored_memory + 1
    );

    let total_before_empty = runtime.scheduler.metrics().total_reductions;
    let memory_before_empty = runtime.total_memory_reductions();
    assert!(runtime
        .restore_mailbox_checkpoint(recipient, Vec::new())
        .expect("empty restore")
        .is_empty());
    assert_eq!(runtime.total_memory_reductions() - memory_before_empty, 1);
    assert_eq!(
        runtime.scheduler.metrics().total_reductions - total_before_empty,
        2
    );

    let total_before_pressure = runtime.scheduler.metrics().total_reductions;
    let memory_before_pressure = runtime.total_memory_reductions();
    assert_eq!(
        runtime
            .restore_mailbox_checkpoint(recipient, vec![ReplValue::Int(20), ReplValue::Int(30)],)
            .expect_err("hard-limit restore must fail"),
        format!(
            "actor process {} checkpoint exceeds its VM mailbox memory hard limit",
            recipient.as_u64()
        )
    );
    let rejected_memory = runtime.total_memory_reductions() - memory_before_pressure;
    assert_eq!(rejected_memory, 2);
    assert_eq!(
        runtime.scheduler.metrics().total_reductions - total_before_pressure,
        rejected_memory
    );
    assert_eq!(
        runtime
            .processes
            .get(recipient)
            .expect("recipient")
            .mailbox_len(),
        1
    );

    let exited = runtime.spawn_root(source("exited"));
    runtime
        .exit_actor(exited, VmExitReason::Normal)
        .expect("exit actor");
    let total_before_invalid = runtime.scheduler.metrics().total_reductions;
    assert_eq!(
        runtime
            .restore_mailbox_checkpoint(VmProcessId::from_raw_for_test(404), vec![ReplValue::Unit],)
            .expect_err("missing recipient must fail"),
        "missing sender process 404"
    );
    assert_eq!(
        runtime
            .restore_mailbox_checkpoint(exited, vec![ReplValue::Unit])
            .expect_err("exited recipient must fail"),
        format!("sender process {} has exited", exited.as_u64())
    );
    assert_eq!(
        runtime.scheduler.metrics().total_reductions,
        total_before_invalid
    );
}
