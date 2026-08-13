use super::super::super::memory::VmMemoryLimits;
use super::super::super::process::{VmExitReason, VmProcessId, VmProcessSource, VmProcessState};
use super::super::super::scheduler::{VmSchedulerDecision, VmSchedulerOutcome};
use super::super::super::ReplValue;
use super::super::{VmActorReceive, VmActorRuntime};

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
fn actor_runtime_lists_and_unregisters_names() {
    let mut runtime = VmActorRuntime::default();
    let worker = runtime.spawn_root(source("worker"));
    runtime
        .register_name("worker.secondary", worker)
        .expect("secondary name should register");
    runtime
        .register_name("worker.primary", worker)
        .expect("primary name should register");

    assert_eq!(
        runtime.registered_names(),
        ["worker.primary", "worker.secondary"]
    );
    assert_eq!(
        runtime
            .unregister_name("worker.primary")
            .expect("primary name should unregister"),
        worker
    );
    assert_eq!(runtime.registered_names(), ["worker.secondary"]);
    assert_eq!(
        runtime
            .unregister_name("missing")
            .expect_err("missing name should fail"),
        "actor name `missing` is not registered"
    );
}

#[test]
fn actor_runtime_lists_live_processes_in_allocation_order() {
    let mut runtime = VmActorRuntime::default();
    let first = runtime.spawn_root(source("first"));
    let exited = runtime.spawn_root(source("exited"));
    let third = runtime.spawn_root(source("third"));
    runtime
        .exit_actor(exited, VmExitReason::Normal)
        .expect("middle actor should exit");

    assert_eq!(runtime.live_process_ids(), [first, third]);
}

#[test]
fn actor_runtime_unknown_message_preserves_liveness_until_explicit_exit() {
    let mut runtime = VmActorRuntime::default();
    let sender = runtime.spawn_root(source("sender"));
    let recipient = runtime.spawn_root(source("recipient"));
    runtime
        .send(sender, recipient, ReplValue::Atom("unknown".to_string()))
        .expect("unknown message should send");

    assert_eq!(
        runtime
            .selective_receive_or_block(recipient, |message| {
                message.payload == ReplValue::Atom("known".to_string())
            })
            .expect("unknown message should not fail selective receive"),
        VmActorReceive::Blocked
    );
    assert!(runtime.is_alive(recipient));

    runtime
        .send(sender, recipient, ReplValue::Atom("known".to_string()))
        .expect("known message should send");
    assert!(matches!(
        runtime
            .selective_receive_or_block(recipient, |message| {
                message.payload == ReplValue::Atom("known".to_string())
            })
            .expect("known message should be received"),
        VmActorReceive::Message(message)
            if message.payload == ReplValue::Atom("known".to_string())
    ));
    assert!(runtime.is_alive(recipient));
    assert!(matches!(
        runtime
            .receive_next_or_block(recipient)
            .expect("unknown message should remain queued"),
        VmActorReceive::Message(message)
            if message.payload == ReplValue::Atom("unknown".to_string())
    ));

    runtime
        .exit_actor(recipient, VmExitReason::Normal)
        .expect("recipient should exit normally");
    assert!(!runtime.is_alive(recipient));
    assert!(!runtime.is_alive(VmProcessId::from_raw_for_test(99)));
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
fn actor_message_wakeup_is_deduplicated_and_missing_target_is_side_effect_free() {
    let mut runtime = VmActorRuntime::default();
    let sender = runtime.spawn_root(source("sender"));
    let recipient = runtime.spawn_root(source("recipient"));
    runtime
        .register_name("recipient", recipient)
        .expect("name should register");

    for _ in 0..2 {
        runtime
            .run_next(|_, _| VmSchedulerDecision::Block { reductions: 1 })
            .expect("initial actor should block");
    }
    assert_eq!(runtime.scheduled_len(), 0);

    runtime
        .send_named(sender, "recipient", ReplValue::Int(1))
        .expect("first send should wake recipient");
    runtime
        .send_named(sender, "recipient", ReplValue::Int(2))
        .expect("second send should retain one ready entry");
    assert_eq!(runtime.scheduled_len(), 1);

    assert_eq!(
        runtime
            .send_named(sender, "missing", ReplValue::Int(3))
            .expect_err("missing recipient should fail"),
        "actor name `missing` is not registered"
    );
    assert_eq!(runtime.scheduled_len(), 1);

    let first = runtime
        .receive_next_or_block(recipient)
        .expect("first message should be available");
    let second = runtime
        .receive_next_or_block(recipient)
        .expect("second message should be available");
    assert!(matches!(
        first,
        VmActorReceive::Message(message) if message.payload == ReplValue::Int(1)
    ));
    assert!(matches!(
        second,
        VmActorReceive::Message(message) if message.payload == ReplValue::Int(2)
    ));
}

#[test]
fn actor_runtime_spawns_child_and_reports_missing_parent() {
    let mut runtime = VmActorRuntime::default();
    let parent = runtime.spawn_root(source("parent"));

    let child = runtime
        .spawn_child(parent, source("child"))
        .expect("child should spawn under live parent");
    let missing_parent = runtime
        .spawn_child(VmProcessId::from_raw_for_test(99), source("orphan"))
        .expect_err("missing parent should be rejected");

    assert_eq!(
        runtime
            .processes()
            .get(child)
            .expect("child should exist")
            .parent,
        Some(parent)
    );
    assert_eq!(missing_parent, "missing parent process 99");
    assert!(runtime.scheduled_len() >= 2);
}

#[test]
fn actor_runtime_rejects_empty_name_and_invalid_sends() {
    let mut runtime = VmActorRuntime::default();
    let sender = runtime.spawn_root(source("sender"));
    let recipient = runtime.spawn_root(source("recipient"));
    runtime
        .exit_actor(recipient, VmExitReason::Normal)
        .expect("recipient exit should succeed");

    assert_eq!(
        runtime
            .register_name("  ", sender)
            .expect_err("empty name should fail"),
        "actor name cannot be empty"
    );
    assert_eq!(
        runtime
            .send(sender, recipient, ReplValue::Unit)
            .expect_err("send to exited recipient should fail"),
        "recipient process 2 has exited"
    );
}

#[test]
fn actor_runtime_rejects_missing_sender_and_unknown_name() {
    let mut runtime = VmActorRuntime::default();
    let sender = runtime.spawn_root(source("sender"));
    let recipient = runtime.spawn_root(source("recipient"));
    let missing = VmProcessId::from_raw_for_test(99);

    assert_eq!(
        runtime
            .send(missing, recipient, ReplValue::Unit)
            .expect_err("missing sender should fail"),
        "missing sender process 99"
    );
    assert_eq!(
        runtime
            .send_named(sender, "missing", ReplValue::Unit)
            .expect_err("unknown actor name should fail"),
        "actor name `missing` is not registered"
    );
    assert_eq!(
        runtime
            .receive_next_or_block(missing)
            .expect_err("missing actor receive should fail"),
        "cannot receive missing process 99"
    );
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
fn actor_runtime_accounts_and_releases_mailbox_memory() {
    let mut runtime = VmActorRuntime::default();
    let sender = runtime.spawn_root(source("sender"));
    let recipient = runtime.spawn_root(source("recipient"));

    runtime
        .send(
            sender,
            recipient,
            ReplValue::String("accounted".to_string()),
        )
        .expect("send should reserve mailbox memory");
    let after_send = runtime
        .memory_metrics(recipient)
        .expect("recipient memory metrics after send");
    assert_eq!(after_send.current_bytes, 25);
    assert_eq!(after_send.high_water_bytes, 25);
    assert_eq!(runtime.memory_reductions(recipient), 2);
    assert_eq!(runtime.total_memory_reductions(), 2);

    assert!(matches!(
        runtime
            .receive_next_or_block(recipient)
            .expect("receive should release mailbox memory"),
        VmActorReceive::Message(_)
    ));
    let after_receive = runtime
        .memory_metrics(recipient)
        .expect("recipient memory metrics after receive");
    assert_eq!(after_receive.current_bytes, 0);
    assert_eq!(after_receive.high_water_bytes, 25);
    assert_eq!(after_receive.released_bytes, 25);
    assert_eq!(runtime.memory_reductions(recipient), 4);
    assert_eq!(runtime.total_memory_reductions(), 4);
}

#[test]
fn actor_runtime_rejects_hard_memory_pressure_without_mailbox_or_id_mutation() {
    let limits = VmMemoryLimits::new(16, 24).expect("test limits");
    let mut runtime = VmActorRuntime::with_memory_limits(limits);
    let sender = runtime.spawn_root(source("sender"));
    let recipient = runtime.spawn_root(source("recipient"));

    assert_eq!(
        runtime
            .send(
                sender,
                recipient,
                ReplValue::String("too-large".to_string()),
            )
            .expect_err("25-byte payload must exceed hard limit"),
        "actor process 2 exceeded its VM mailbox memory hard limit"
    );
    assert_eq!(
        runtime
            .processes()
            .get(recipient)
            .expect("recipient")
            .mailbox_len(),
        0
    );
    let rejected = runtime
        .memory_metrics(recipient)
        .expect("hard-pressure decision creates metrics");
    assert_eq!(rejected.current_bytes, 0);
    assert_eq!(rejected.high_water_bytes, 0);
    assert_eq!(runtime.memory_reductions(recipient), 2);

    assert_eq!(
        runtime
            .send(sender, recipient, ReplValue::Int(1))
            .expect("smaller payload should succeed"),
        1
    );
    assert_eq!(
        runtime
            .processes()
            .get(recipient)
            .expect("recipient")
            .mailbox_len(),
        1
    );
    assert_eq!(
        runtime
            .memory_metrics(recipient)
            .expect("accounted metrics")
            .current_bytes,
        8
    );
    assert_eq!(runtime.memory_reductions(recipient), 4);
}

#[test]
fn actor_runtime_restores_accounted_mailbox_checkpoint_in_order() {
    let mut runtime = VmActorRuntime::default();
    let recipient = runtime.spawn_root(source("restored"));

    assert_eq!(
        runtime
            .restore_mailbox_checkpoint(recipient, vec![ReplValue::Int(10), ReplValue::Int(20)],)
            .expect("checkpoint restore"),
        vec![1, 2]
    );
    assert_eq!(
        runtime
            .memory_metrics(recipient)
            .expect("checkpoint metrics")
            .current_bytes,
        16
    );
    assert_eq!(runtime.memory_reductions(recipient), 2);
    for (expected, remaining_bytes) in [(10, 8), (20, 0)] {
        assert!(matches!(
            runtime
                .receive_next_or_block(recipient)
                .expect("restored receive"),
            VmActorReceive::Message(message)
                if message.sender == recipient && message.payload == ReplValue::Int(expected)
        ));
        assert_eq!(
            runtime
                .memory_metrics(recipient)
                .expect("released checkpoint metrics")
                .current_bytes,
            remaining_bytes
        );
    }
    assert_eq!(runtime.memory_reductions(recipient), 6);
}

#[test]
fn actor_runtime_empty_checkpoint_is_a_noop() {
    let mut runtime = VmActorRuntime::default();
    let recipient = runtime.spawn_root(source("restored"));

    assert_eq!(
        runtime
            .restore_mailbox_checkpoint(recipient, Vec::new())
            .expect("empty checkpoint should be accepted"),
        Vec::<u64>::new()
    );
    assert_eq!(runtime.memory_reductions(recipient), 1);
    assert_eq!(
        runtime
            .processes()
            .get(recipient)
            .expect("recipient should remain live")
            .mailbox_len(),
        0
    );
}

#[test]
fn actor_runtime_rejects_checkpoint_pressure_without_partial_restore_or_ids() {
    let mut runtime =
        VmActorRuntime::with_memory_limits(VmMemoryLimits::new(8, 12).expect("limits"));
    let recipient = runtime.spawn_root(source("restored"));

    assert_eq!(
        runtime
            .restore_mailbox_checkpoint(
                recipient,
                vec![
                    ReplValue::Int(10),
                    ReplValue::RandomGenerator(crate::terlan_native::random::Generator::from_seed(
                        7
                    ),),
                ],
            )
            .expect_err("opaque checkpoint value is rejected before reservation"),
        "error[vm_memory_unaccounted_value]: `RandomGenerator` requires a dedicated ownership contract"
    );
    assert_eq!(
        runtime
            .processes()
            .get(recipient)
            .expect("recipient")
            .mailbox_len(),
        0
    );
    assert_eq!(runtime.memory_reductions(recipient), 0);

    assert_eq!(
        runtime
            .restore_mailbox_checkpoint(recipient, vec![ReplValue::Int(10), ReplValue::Int(20)],)
            .expect_err("checkpoint exceeds hard limit"),
        "actor process 1 checkpoint exceeds its VM mailbox memory hard limit"
    );
    assert_eq!(
        runtime
            .processes()
            .get(recipient)
            .expect("recipient")
            .mailbox_len(),
        0
    );
    assert_eq!(runtime.memory_reductions(recipient), 2);
    assert_eq!(
        runtime
            .memory_metrics(recipient)
            .expect("rejected checkpoint metrics")
            .current_bytes,
        0
    );
    assert_eq!(
        runtime
            .restore_mailbox_checkpoint(recipient, vec![ReplValue::Int(30)])
            .expect("smaller checkpoint"),
        vec![1]
    );
    assert_eq!(runtime.memory_reductions(recipient), 4);
}

#[test]
fn actor_runtime_receive_timeout_returns_message_or_blocks_for_future_timeout() {
    let mut runtime = VmActorRuntime::default();
    let sender = runtime.spawn_root(source("sender"));
    let recipient = runtime.spawn_root(source("recipient"));
    runtime
        .send(sender, recipient, ReplValue::String("ready".to_string()))
        .expect("message should send");

    let received = runtime
        .receive_with_timeout(recipient, 5)
        .expect("timeout receive should return queued message");
    let blocked = runtime
        .receive_with_timeout(recipient, 5)
        .expect("future timeout should block while waiting");

    assert!(matches!(
        received,
        VmActorReceive::Message(message) if message.payload == ReplValue::String("ready".to_string())
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
fn actor_runtime_selective_receive_blocks_when_no_message_matches() {
    let mut runtime = VmActorRuntime::default();
    let sender = runtime.spawn_root(source("sender"));
    let recipient = runtime.spawn_root(source("recipient"));
    runtime
        .send(sender, recipient, ReplValue::String("skip".to_string()))
        .expect("message should send");

    let result = runtime
        .selective_receive_or_block(recipient, |message| {
            message.payload == ReplValue::Atom("take".to_string())
        })
        .expect("selective receive should succeed");

    assert_eq!(result, VmActorReceive::Blocked);
    assert_eq!(
        runtime
            .processes()
            .get(recipient)
            .expect("recipient should exist")
            .mailbox_len(),
        1
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
fn actor_runtime_selective_receive_retries_after_matching_message_wakes_actor() {
    let mut runtime = VmActorRuntime::default();
    let sender = runtime.spawn_root(source("sender"));
    let recipient = runtime.spawn_root(source("recipient"));
    runtime
        .send(sender, recipient, ReplValue::Atom("skip".to_string()))
        .expect("unmatched message should send");

    assert_eq!(
        runtime
            .selective_receive_or_block(recipient, |message| {
                message.payload == ReplValue::Atom("take".to_string())
            })
            .expect("initial selective receive should block"),
        VmActorReceive::Blocked
    );
    assert_eq!(
        runtime
            .processes()
            .get(recipient)
            .expect("recipient should exist")
            .state,
        VmProcessState::Blocked
    );

    runtime
        .send(sender, recipient, ReplValue::Atom("take".to_string()))
        .expect("matching message should wake recipient");
    assert_eq!(
        runtime
            .processes()
            .get(recipient)
            .expect("recipient should exist")
            .state,
        VmProcessState::Runnable
    );

    assert!(matches!(
        runtime
            .selective_receive_or_block(recipient, |message| {
                message.payload == ReplValue::Atom("take".to_string())
            })
            .expect("retry should receive matching message"),
        VmActorReceive::Message(message)
            if message.payload == ReplValue::Atom("take".to_string())
    ));
    assert!(matches!(
        runtime
            .receive_next_or_block(recipient)
            .expect("unmatched message should remain queued"),
        VmActorReceive::Message(message)
            if message.payload == ReplValue::Atom("skip".to_string())
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
            .register_name("exited", pid)
            .expect_err("exited registration should fail"),
        "cannot register exited process 1"
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
            .expect_err("exited sender should fail before name resolution"),
        "sender process 1 has exited"
    );
    assert_eq!(
        runtime
            .selective_receive_or_block(pid, |_| true)
            .expect_err("exited selective receive should fail"),
        "cannot receive exited process 1"
    );
    assert_eq!(
        runtime
            .receive_with_timeout(pid, 1)
            .expect_err("exited timeout receive should fail"),
        "cannot receive exited process 1"
    );
    assert_eq!(
        runtime
            .exit_actor(missing, VmExitReason::Normal)
            .expect_err("missing exit should fail"),
        "missing process 99"
    );
}

#[test]
fn actor_runtime_exit_removes_registered_names_and_returns_cleanup_handles() {
    let mut runtime = VmActorRuntime::default();
    let pid = runtime.spawn_root(source("worker"));
    runtime
        .register_name("worker", pid)
        .expect("name should register");
    runtime
        .processes
        .get_mut(pid)
        .expect("process should exist")
        .add_resource_handle("vector:1");

    let cleanup = runtime
        .exit_actor(pid, VmExitReason::Killed)
        .expect("exit should succeed");

    assert_eq!(cleanup, vec!["vector:1".to_string()]);
    assert_eq!(runtime.lookup_name("worker"), None);
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
