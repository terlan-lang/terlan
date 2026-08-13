use super::super::*;

fn source(function: &str) -> VmProcessSource {
    VmProcessSource::new("app.SendAccounting", function, 0)
}

fn process_reductions(runtime: &VmActorRuntime, pid: VmProcessId) -> u64 {
    runtime
        .processes
        .get(pid)
        .expect("accounted process")
        .reductions
}

#[test]
fn actor_runtime_charges_only_successful_send_operations_to_sender() {
    let limits = VmMemoryLimits::new(16, 24).expect("test limits");
    let mut runtime = VmActorRuntime::with_memory_limits(limits);
    let sender = runtime.spawn_root(source("sender"));
    let recipient = runtime.spawn_root(source("recipient"));
    let context = runtime.context(sender).expect("sender context");
    runtime
        .register_name("recipient", recipient)
        .expect("recipient name");
    let alias = runtime.create_alias(recipient).expect("recipient alias");

    let sender_before_routes = process_reductions(&runtime, sender);
    let recipient_before_routes = process_reductions(&runtime, recipient);
    let total_before_routes = runtime.scheduler.metrics().total_reductions;
    let memory_before_routes = runtime.total_memory_reductions();
    runtime
        .send(sender, recipient, ReplValue::Int(1))
        .expect("PID send");
    runtime
        .send_named(sender, "recipient", ReplValue::Int(2))
        .expect("named send");
    runtime
        .send_alias(sender, alias, ReplValue::Int(3))
        .expect("alias send");
    runtime
        .send_self(context, ReplValue::Int(4))
        .expect("self send");
    let memory_route_delta = runtime.total_memory_reductions() - memory_before_routes;
    assert_eq!(
        runtime.scheduler.metrics().total_reductions - total_before_routes,
        memory_route_delta + 4
    );
    assert_eq!(
        process_reductions(&runtime, sender) - sender_before_routes,
        runtime.memory_reductions(sender) + 4
    );
    assert_eq!(
        process_reductions(&runtime, recipient) - recipient_before_routes,
        runtime.memory_reductions(recipient)
    );
    assert_eq!(
        runtime
            .processes()
            .get(recipient)
            .expect("recipient")
            .mailbox_len(),
        3
    );

    let sender_before_rejections = process_reductions(&runtime, sender);
    let missing = VmProcessId::from_raw_for_test(404);
    assert_eq!(
        runtime
            .send(sender, missing, ReplValue::Unit)
            .expect_err("missing recipient"),
        "missing recipient process 404"
    );
    assert_eq!(
        runtime
            .send_named(sender, "missing", ReplValue::Unit)
            .expect_err("missing name"),
        "actor name `missing` is not registered"
    );
    runtime.remove_alias(alias).expect("remove alias");
    assert_eq!(
        runtime
            .send_alias(sender, alias, ReplValue::Unit)
            .expect_err("stale alias"),
        format!("process alias {} is not registered", alias.as_u64())
    );
    assert_eq!(
        process_reductions(&runtime, sender),
        sender_before_rejections
    );

    let oversized = ReplValue::String("x".repeat(32));
    assert_eq!(
        runtime
            .send(sender, recipient, oversized)
            .expect_err("hard-limit rejection"),
        format!(
            "actor process {} exceeded its VM mailbox memory hard limit",
            recipient.as_u64()
        )
    );
    assert_eq!(
        process_reductions(&runtime, sender),
        sender_before_rejections
    );

    runtime
        .exit_actor(sender, VmExitReason::Normal)
        .expect("exit sender");
    let total_before_invalid_sender = runtime.scheduler.metrics().total_reductions;
    assert_eq!(
        runtime
            .send(sender, recipient, ReplValue::Unit)
            .expect_err("exited sender"),
        format!("sender process {} has exited", sender.as_u64())
    );
    assert_eq!(
        runtime.scheduler.metrics().total_reductions,
        total_before_invalid_sender
    );
}
