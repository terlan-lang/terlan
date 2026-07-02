use super::{VmActorReceive, VmActorRuntime};
use crate::runtime::vm::process::{VmExitReason, VmProcessId, VmProcessSource, VmProcessState};
use crate::runtime::vm::scheduler::{VmSchedulerDecision, VmSchedulerOutcome};
use crate::runtime::vm::ReplValue;

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.Main", name, 0)
}

#[test]
fn actor_runtime_registers_names_idempotently_and_rejects_conflicts() {
    let mut runtime = VmActorRuntime::default();
    let first = runtime.spawn_root(source("first"));
    let second = runtime.spawn_root(source("second"));

    runtime
        .register_name("counter", first)
        .expect("first registration should succeed");
    runtime
        .register_name("counter", first)
        .expect("same registration should be idempotent");
    let conflict = runtime
        .register_name("counter", second)
        .expect_err("conflicting registration should fail");

    assert_eq!(runtime.lookup_name("counter"), Some(first));
    assert_eq!(
        conflict,
        "actor name `counter` is already registered to process 1"
    );
}

#[test]
fn actor_runtime_send_named_wakes_and_schedules_recipient() {
    let mut runtime = VmActorRuntime::default();
    let sender = runtime.spawn_root(source("sender"));
    let recipient = runtime.spawn_root(source("recipient"));
    runtime
        .register_name("recipient", recipient)
        .expect("name should register");
    runtime
        .receive_next_or_block(recipient)
        .expect("empty mailbox should block");

    let message_id = runtime
        .send_named(sender, "recipient", ReplValue::String("hello".to_string()))
        .expect("named send should succeed");

    assert_eq!(message_id, 1);
    assert_eq!(
        runtime
            .processes()
            .get(recipient)
            .expect("recipient should exist")
            .state,
        VmProcessState::Runnable
    );
    assert!(runtime.scheduled_len() >= 1);
}

#[test]
fn actor_runtime_receive_next_returns_message_or_blocks() {
    let mut runtime = VmActorRuntime::default();
    let sender = runtime.spawn_root(source("sender"));
    let recipient = runtime.spawn_root(source("recipient"));
    runtime
        .send(sender, recipient, ReplValue::Int(42))
        .expect("send should succeed");

    let received = runtime
        .receive_next_or_block(recipient)
        .expect("receive should succeed");
    let blocked = runtime
        .receive_next_or_block(recipient)
        .expect("empty receive should block");

    assert!(matches!(
        received,
        VmActorReceive::Message(message) if message.payload == ReplValue::Int(42)
    ));
    assert_eq!(blocked, VmActorReceive::Blocked);
    assert_eq!(
        runtime
            .processes()
            .get(recipient)
            .expect("recipient should exist")
            .state,
        VmProcessState::Blocked
    );
}

#[test]
fn actor_runtime_receive_with_zero_timeout_does_not_block() {
    let mut runtime = VmActorRuntime::default();
    let pid = runtime.spawn_root(source("worker"));

    let result = runtime
        .receive_with_timeout(pid, 0)
        .expect("timeout receive should succeed");

    assert_eq!(result, VmActorReceive::Timeout);
    assert_eq!(
        runtime
            .processes()
            .get(pid)
            .expect("pid should exist")
            .state,
        VmProcessState::Runnable
    );
}

#[test]
fn actor_runtime_selective_receive_preserves_skipped_messages() {
    let mut runtime = VmActorRuntime::default();
    let sender = runtime.spawn_root(source("sender"));
    let recipient = runtime.spawn_root(source("recipient"));
    runtime
        .send(sender, recipient, ReplValue::String("skip".to_string()))
        .expect("first send should succeed");
    runtime
        .send(sender, recipient, ReplValue::Atom("take".to_string()))
        .expect("second send should succeed");

    let selected = runtime
        .selective_receive_or_block(recipient, |message| {
            message.payload == ReplValue::Atom("take".to_string())
        })
        .expect("selective receive should succeed");
    let remaining = runtime
        .receive_next_or_block(recipient)
        .expect("remaining message should receive");

    assert!(matches!(
        selected,
        VmActorReceive::Message(message) if message.payload == ReplValue::Atom("take".to_string())
    ));
    assert!(matches!(
        remaining,
        VmActorReceive::Message(message) if message.payload == ReplValue::String("skip".to_string())
    ));
}

#[test]
fn actor_runtime_reports_missing_and_exited_context_diagnostics() {
    let mut runtime = VmActorRuntime::default();
    let pid = runtime.spawn_root(source("worker"));
    runtime
        .exit_actor(pid, VmExitReason::Normal)
        .expect("exit should succeed");
    let missing = VmProcessId::from_raw_for_test(99);

    assert_eq!(
        runtime
            .register_name("missing", missing)
            .expect_err("missing registration should fail"),
        "cannot register missing process 99"
    );
    assert_eq!(
        runtime
            .receive_next_or_block(pid)
            .expect_err("exited receive should fail"),
        "cannot receive exited process 1"
    );
    assert_eq!(
        runtime
            .send_named(pid, "missing", ReplValue::Unit)
            .expect_err("missing name should fail"),
        "actor name `missing` is not registered"
    );
}

#[test]
fn actor_runtime_run_next_delegates_to_scheduler() {
    let mut runtime = VmActorRuntime::default();
    let pid = runtime.spawn_root(source("worker"));

    let run = runtime
        .run_next(|process, _slice| {
            process.heap_bytes = 64;
            VmSchedulerDecision::Yield { reductions: 5 }
        })
        .expect("actor scheduler slice should run");

    assert_eq!(run.pid, Some(pid));
    assert_eq!(run.outcome, VmSchedulerOutcome::Ran);
    assert_eq!(
        runtime
            .processes()
            .get(pid)
            .expect("pid should exist")
            .heap_bytes,
        64
    );
}
