use super::super::process::{VmExitReason, VmProcessId, VmProcessSource};
use super::super::process_alias::VmProcessAlias;
use super::super::ReplValue;
use super::VmActorRuntime;

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.Alias", name, 0)
}

#[test]
fn actor_aliases_are_monotonic_resolvable_and_owner_ordered() {
    let mut runtime = VmActorRuntime::default();
    let first = runtime.spawn_root(source("first"));
    let second = runtime.spawn_root(source("second"));

    let first_alias = runtime.create_alias(first).expect("first alias");
    let second_alias = runtime.create_alias(first).expect("second alias");
    let third_alias = runtime.create_alias(second).expect("third alias");

    assert_eq!(first_alias.as_u64(), 1);
    assert_eq!(second_alias.as_u64(), 2);
    assert_eq!(third_alias.as_u64(), 3);
    assert_eq!(runtime.alias_count(), 3);
    assert_eq!(runtime.resolve_alias(first_alias), Some(first));
    assert_eq!(
        runtime.aliases_for_process(first),
        [first_alias, second_alias]
    );
}

#[test]
fn actor_alias_removal_is_explicit_and_typed() {
    let mut runtime = VmActorRuntime::default();
    let actor = runtime.spawn_root(source("worker"));
    let alias = runtime.create_alias(actor).expect("alias should create");

    assert_eq!(
        runtime
            .remove_alias(alias)
            .expect("alias should be removed"),
        actor
    );
    assert_eq!(runtime.resolve_alias(alias), None);
    assert_eq!(runtime.alias_count(), 0);
    assert_eq!(
        runtime
            .remove_alias(alias)
            .expect_err("removed alias should fail"),
        "process alias 1 is not registered"
    );
}

#[test]
fn actor_alias_send_uses_memory_accounted_actor_delivery() {
    let mut runtime = VmActorRuntime::default();
    let sender = runtime.spawn_root(source("sender"));
    let recipient = runtime.spawn_root(source("recipient"));
    let alias = runtime
        .create_alias(recipient)
        .expect("recipient alias should create");

    let message_id = runtime
        .send_alias(sender, alias, ReplValue::String("hello".to_string()))
        .expect("alias send should succeed");

    assert_eq!(message_id, 1);
    assert_eq!(
        runtime
            .processes()
            .get(recipient)
            .expect("recipient should exist")
            .mailbox_len(),
        1
    );
    assert!(runtime.memory_metrics(recipient).is_some());
}

#[test]
fn actor_alias_creation_rejects_missing_exited_and_exhausted_identity() {
    let mut runtime = VmActorRuntime::default();
    let exited = runtime.spawn_root(source("exited"));
    runtime
        .exit_actor(exited, VmExitReason::Normal)
        .expect("actor should exit");

    assert_eq!(
        runtime
            .create_alias(VmProcessId::from_raw_for_test(404))
            .expect_err("missing actor should fail"),
        "cannot alias missing process 404"
    );
    assert_eq!(
        runtime
            .create_alias(exited)
            .expect_err("exited actor should fail"),
        "cannot alias exited process 1"
    );

    let live = runtime.spawn_root(source("live"));
    runtime.aliases.exhaust_for_test();
    assert_eq!(
        runtime
            .create_alias(live)
            .expect_err("exhausted alias identity should fail"),
        "process alias identity space is exhausted"
    );
    assert_eq!(runtime.alias_count(), 0);
}

#[test]
fn actor_alias_send_rejects_unknown_alias_without_mailbox_mutation() {
    let mut runtime = VmActorRuntime::default();
    let sender = runtime.spawn_root(source("sender"));
    let recipient = runtime.spawn_root(source("recipient"));

    assert_eq!(
        runtime
            .send_alias(
                sender,
                VmProcessAlias::from_raw_for_test(404),
                ReplValue::Unit,
            )
            .expect_err("unknown alias should fail"),
        "process alias 404 is not registered"
    );
    assert_eq!(
        runtime
            .processes()
            .get(recipient)
            .expect("recipient should exist")
            .mailbox_len(),
        0
    );
}

#[test]
fn actor_exit_revokes_all_owned_aliases_without_touching_other_owners() {
    let mut runtime = VmActorRuntime::default();
    let exiting = runtime.spawn_root(source("exiting"));
    let survivor = runtime.spawn_root(source("survivor"));
    let first = runtime.create_alias(exiting).expect("first alias");
    let second = runtime.create_alias(exiting).expect("second alias");
    let surviving = runtime.create_alias(survivor).expect("surviving alias");

    runtime
        .exit_actor(exiting, VmExitReason::Killed)
        .expect("actor should exit");

    assert_eq!(runtime.resolve_alias(first), None);
    assert_eq!(runtime.resolve_alias(second), None);
    assert_eq!(runtime.resolve_alias(surviving), Some(survivor));
    assert_eq!(runtime.alias_count(), 1);
}
