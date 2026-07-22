use super::*;

fn source(function: &str) -> VmProcessSource {
    VmProcessSource::new("app.TimerAccounting", function, 0)
}

fn process_reductions(runtime: &VmActorRuntime, pid: VmProcessId) -> u64 {
    runtime
        .processes()
        .get(pid)
        .expect("accounted process")
        .reductions
}

#[test]
fn actor_runtime_charges_only_successful_timer_scheduling_to_owner() {
    let mut runtime = VmActorRuntime::default();
    let owner = runtime.spawn_root(source("owner"));
    let recipient = runtime.spawn_root(source("recipient"));
    runtime
        .register_name("recipient", recipient)
        .expect("recipient name");
    let alias = runtime.create_alias(recipient).expect("recipient alias");

    let total_before_success = runtime.scheduler.metrics().total_reductions;
    let owner_before_success = process_reductions(&runtime, owner);
    let recipient_before_success = process_reductions(&runtime, recipient);
    runtime
        .send_after(owner, recipient, ReplValue::Int(1), 0, 5)
        .expect("direct timer");
    runtime
        .send_named_after(owner, "recipient", ReplValue::Int(2), 0, 5)
        .expect("named timer");
    runtime
        .send_alias_after(owner, alias, ReplValue::Int(3), 0, 5)
        .expect("alias timer");
    runtime
        .start_message_timer(owner, recipient, ReplValue::Int(4), 0, 5)
        .expect("correlated timer");
    assert_eq!(runtime.delayed_send_count(), 4);
    assert_eq!(
        runtime.scheduler.metrics().total_reductions - total_before_success,
        4
    );
    assert_eq!(
        process_reductions(&runtime, owner) - owner_before_success,
        4
    );
    assert_eq!(
        process_reductions(&runtime, recipient) - recipient_before_success,
        0
    );
    assert_eq!(runtime.total_memory_reductions(), 0);

    runtime.remove_alias(alias).expect("remove alias");
    let total_before_rejections = runtime.scheduler.metrics().total_reductions;
    assert_eq!(
        runtime
            .send_named_after(owner, "missing", ReplValue::Unit, 0, 1)
            .expect_err("missing name"),
        "actor name `missing` is not registered"
    );
    assert_eq!(
        runtime
            .send_alias_after(owner, alias, ReplValue::Unit, 0, 1)
            .expect_err("stale alias"),
        format!("process alias {} is not registered", alias.as_u64())
    );
    let missing = VmProcessId::from_raw_for_test(404);
    assert_eq!(
        runtime
            .send_after(owner, missing, ReplValue::Unit, 0, 1)
            .expect_err("missing recipient"),
        "missing recipient process 404"
    );
    assert_eq!(
        runtime
            .send_after(owner, recipient, ReplValue::Unit, u64::MAX, 1)
            .expect_err("deadline overflow"),
        format!(
            "delayed actor send deadline overflow at tick {} with delay 1",
            u64::MAX
        )
    );
    assert_eq!(
        runtime.scheduler.metrics().total_reductions,
        total_before_rejections
    );
    assert_eq!(runtime.delayed_send_count(), 4);

    runtime
        .exit_actor(owner, VmExitReason::Normal)
        .expect("exit owner");
    let total_before_exited = runtime.scheduler.metrics().total_reductions;
    assert_eq!(
        runtime
            .send_after(owner, recipient, ReplValue::Unit, 0, 1)
            .expect_err("exited owner"),
        format!("sender process {} has exited", owner.as_u64())
    );
    assert_eq!(
        runtime.scheduler.metrics().total_reductions,
        total_before_exited
    );
}
