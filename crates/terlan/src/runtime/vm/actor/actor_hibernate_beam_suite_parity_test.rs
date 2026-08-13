use super::super::actor_heap_limit::VmActorHeapLimitPolicy;
use super::super::*;
use crate::runtime::native_image::managed::ManagedExecutionRuntime;
use crate::runtime::native_image::TvmBoundaryType;
use crate::runtime::vm::process::VmProcessState;

fn source(function: &str) -> VmProcessSource {
    VmProcessSource::new("app.HibernateParity", function, 0)
}

#[test]
fn hibernate_suite_actor_reclaims_heap_and_wakes_exactly_once_for_one_hundred_cycles() {
    let mut runtime = VmActorRuntime::default();
    let sender = runtime.spawn_root(source("sender"));
    let hibernator = runtime.spawn_root(source("hibernator"));
    let peer = runtime.spawn_root(source("peer"));

    for cycle in 0_i64..100 {
        runtime
            .reserve_actor_heap(hibernator, 4096, VmActorHeapLimitPolicy::Reject)
            .expect("reserve transient actor heap");
        let outcome = runtime.hibernate(hibernator).expect("hibernate live actor");
        assert_eq!(outcome.released_heap_bytes, 4096);
        assert_eq!(outcome.retained_mailbox_bytes, 0);
        assert!(!outcome.awakened_immediately);
        assert_eq!(
            runtime
                .processes()
                .get(hibernator)
                .expect("hibernator")
                .state,
            VmProcessState::Hibernated
        );

        runtime
            .send(sender, hibernator, ReplValue::Int(cycle))
            .expect("wake hibernated actor");
        assert_eq!(
            runtime
                .processes()
                .get(hibernator)
                .expect("hibernator")
                .state,
            VmProcessState::Runnable
        );
        assert!(matches!(
            runtime
                .receive_next_or_block(hibernator)
                .expect("receive wake message"),
            VmActorReceive::Message(message)
                if message.sender == sender && message.payload == ReplValue::Int(cycle)
        ));
        assert_eq!(
            runtime
                .processes()
                .get(hibernator)
                .expect("hibernator")
                .heap_bytes,
            0
        );
        assert!(runtime.is_alive(peer));
    }
}

#[test]
fn hibernate_suite_queued_message_survives_compaction_and_prevents_lost_wakeup() {
    let mut runtime = VmActorRuntime::default();
    let sender = runtime.spawn_root(source("sender"));
    let hibernator = runtime.spawn_root(source("queued_hibernator"));
    runtime
        .reserve_actor_heap(hibernator, 8192, VmActorHeapLimitPolicy::Reject)
        .expect("reserve transient heap");
    runtime
        .send(sender, hibernator, ReplValue::Int(37))
        .expect("queue wake message before hibernation");

    let outcome = runtime
        .hibernate(hibernator)
        .expect("hibernate with queued message");
    assert_eq!(outcome.released_heap_bytes, 8192);
    assert_eq!(outcome.retained_mailbox_bytes, 8);
    assert!(outcome.awakened_immediately);
    let process = runtime.processes().get(hibernator).expect("hibernator");
    assert_eq!(process.state, VmProcessState::Runnable);
    assert_eq!(process.mailbox_len(), 1);
    assert_eq!(process.heap_bytes, 8);

    assert!(matches!(
        runtime
            .receive_next_or_block(hibernator)
            .expect("receive retained message"),
        VmActorReceive::Message(message) if message.payload == ReplValue::Int(37)
    ));
    assert_eq!(
        runtime
            .processes()
            .get(hibernator)
            .expect("hibernator")
            .heap_bytes,
        0
    );
}

#[test]
fn hibernate_suite_timer_delivery_wakes_without_polling_or_explicit_resume() {
    let mut runtime = VmActorRuntime::default();
    let sender = runtime.spawn_root(source("timer_sender"));
    let hibernator = runtime.spawn_root(source("timer_hibernator"));
    runtime
        .send_after(sender, hibernator, ReplValue::Int(89), 10, 5)
        .expect("schedule wake message");
    runtime
        .hibernate(hibernator)
        .expect("hibernate until timer delivery");
    assert_eq!(
        runtime
            .processes()
            .get(hibernator)
            .expect("hibernator")
            .state,
        VmProcessState::Hibernated
    );

    assert!(runtime.advance_actor_timers(14).deliveries.is_empty());
    assert_eq!(
        runtime
            .processes()
            .get(hibernator)
            .expect("hibernator")
            .state,
        VmProcessState::Hibernated
    );
    assert_eq!(runtime.advance_actor_timers(15).deliveries.len(), 1);
    assert_eq!(
        runtime
            .processes()
            .get(hibernator)
            .expect("hibernator")
            .state,
        VmProcessState::Runnable
    );
    assert!(matches!(
        runtime
            .receive_next_or_block(hibernator)
            .expect("receive timer wake"),
        VmActorReceive::Message(message) if message.payload == ReplValue::Int(89)
    ));
}

#[test]
fn hibernate_suite_rejects_invalid_lifecycle_without_partial_mutation() {
    let mut runtime = VmActorRuntime::default();
    let explicitly_suspended = runtime.spawn_root(source("suspended"));
    runtime
        .reserve_actor_heap(explicitly_suspended, 64, VmActorHeapLimitPolicy::Reject)
        .expect("reserve suspended heap");
    runtime
        .suspend(explicitly_suspended)
        .expect("explicitly suspend actor");
    assert_eq!(
        runtime
            .hibernate(explicitly_suspended)
            .expect_err("explicit suspension cannot become implicit hibernation"),
        format!(
            "cannot hibernate process {}: cannot hibernate explicitly suspended process",
            explicitly_suspended.as_u64()
        )
    );
    assert_eq!(
        runtime
            .processes()
            .get(explicitly_suspended)
            .expect("suspended actor")
            .heap_bytes,
        64
    );

    let exited = runtime.spawn_root(source("exited"));
    runtime
        .exit_actor(exited, VmExitReason::Normal)
        .expect("exit actor");
    assert_eq!(
        runtime
            .hibernate(exited)
            .expect_err("exited actor cannot hibernate"),
        format!("cannot hibernate exited process {}", exited.as_u64())
    );
    let missing = VmProcessId::from_raw_for_test(404);
    assert_eq!(
        runtime
            .hibernate(missing)
            .expect_err("missing actor cannot hibernate"),
        "cannot hibernate missing process 404"
    );
}

#[test]
fn hibernate_suite_managed_heap_retains_only_precise_continuation_roots() {
    let owner = 41;
    let mut runtime = ManagedExecutionRuntime::runtime_default().expect("managed runtime");
    let mut live = runtime
        .allocate_string_value(owner, "live")
        .expect("allocate live root");
    let mut stable_usage = None;

    for cycle in 1_u64..=100 {
        for garbage in 0..8 {
            runtime
                .allocate_string_value(owner, &format!("garbage-{cycle}-{garbage}"))
                .expect("allocate unreachable object");
        }
        let (_, mut pending) = runtime
            .park_continuation_captures(owner, cycle, &[TvmBoundaryType::String], &[live])
            .expect("capture live root");
        let collection = runtime
            .hibernate_owner(owner, pending.as_mut())
            .expect("compact hibernating owner")
            .expect("materialized heap was collected");
        assert!(collection.objects_before > collection.objects_after);
        assert_eq!(collection.objects_after, 1);
        let restored = runtime
            .restore_continuation_captures(owner, cycle, &[TvmBoundaryType::String], &[], pending)
            .expect("restore relocated live root");
        live = restored[0];
        assert_eq!(
            runtime
                .materialize_string_value(owner, live)
                .expect("read relocated root"),
            "live"
        );
        let usage = runtime.heap_usage(owner).expect("owner heap usage");
        assert_eq!(*stable_usage.get_or_insert(usage), usage);
    }
}

#[test]
fn hibernate_suite_managed_mailbox_root_survives_then_reclaims_after_consume() {
    let owner = 53;
    let mut runtime = ManagedExecutionRuntime::runtime_default().expect("managed runtime");
    let message = runtime
        .allocate_string_value(owner, "queued")
        .expect("allocate message");
    let fragment = runtime
        .copy_mailbox_value(owner, owner, &TvmBoundaryType::String, message)
        .expect("retain same-owner mailbox graph");
    runtime
        .hibernate_owner(owner, None)
        .expect("compact around mailbox root")
        .expect("heap exists");
    let relocated = runtime
        .mailbox_value_word(fragment.fragment_id(), owner, &TvmBoundaryType::String)
        .expect("read relocated mailbox root");
    assert_eq!(
        runtime
            .materialize_string_value(owner, relocated)
            .expect("materialize queued value"),
        "queued"
    );

    runtime
        .consume_mailbox_value(fragment.fragment_id())
        .expect("consume mailbox root");
    let collection = runtime
        .hibernate_owner(owner, None)
        .expect("collect empty owner")
        .expect("heap exists");
    assert_eq!(collection.objects_after, 0);
    assert_eq!(runtime.heap_usage(owner), Some((0, 0)));
}
