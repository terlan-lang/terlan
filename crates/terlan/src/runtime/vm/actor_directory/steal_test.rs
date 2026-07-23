use super::*;

fn pid(value: u64) -> VmProcessId {
    VmProcessId::from_raw_for_test(value)
}

#[test]
fn queued_claim_is_linear_generation_qualified_and_losslessly_aborted() {
    let actor = pid(1);
    let mut directory = VmActorDirectory::<u64>::default();
    directory.insert(actor, 17).expect("insert actor");
    directory.mark_queued(actor).expect("queue actor");

    let claim = directory
        .claim_queued_for_steal(actor)
        .expect("claim queued actor");
    assert_eq!(claim.process_id(), actor);
    assert_eq!(
        directory.lifecycle(actor).expect("migrating lifecycle"),
        VmActorLifecycle::Migrating
    );
    assert_eq!(
        directory
            .pin_lookup(actor)
            .expect_err("migrating actor rejects new lookup pins"),
        VmActorDirectoryError::InvalidTransition {
            from: VmActorLifecycle::Migrating,
            to: VmActorLifecycle::Migrating,
        }
    );
    assert!(directory.claim_queued_for_steal(actor).is_err());

    directory
        .abort_steal_claim(claim)
        .expect("restore queued actor");
    assert_eq!(
        directory.lifecycle(actor).expect("queued lifecycle"),
        VmActorLifecycle::Queued
    );
    assert_eq!(*directory.get(actor).expect("actor value"), 17);
}

#[test]
fn claim_rejects_pending_publication_and_lookup_pin_before_transition() {
    let actor = pid(1);
    let mut directory = VmActorDirectory::<u64, u64>::default();
    directory.insert(actor, 19).expect("insert actor");
    directory.mark_queued(actor).expect("queue actor");
    directory
        .publish_fragment(actor, 23)
        .expect("publish complete fragment");
    assert!(matches!(
        directory
            .claim_queued_for_steal(actor)
            .expect_err("pending publication blocks claim"),
        VmActorDirectoryError::TransferMailboxNotDrained { pending: 1 }
    ));

    let token = directory
        .acquire_mutator(actor, 1)
        .expect("acquire receiver");
    directory.drain_payloads(&token).expect("drain publication");
    directory
        .release_mutator(token, VmActorLifecycle::Yielding)
        .expect("release receiver");
    directory.mark_queued(actor).expect("requeue actor");
    let handle = directory.pin_lookup(actor).expect("pin lookup");
    assert!(matches!(
        directory
            .claim_queued_for_steal(actor)
            .expect_err("lookup pin blocks claim"),
        VmActorDirectoryError::LookupPinned(1)
    ));
    directory.unpin_lookup(handle).expect("unpin lookup");
    let claim = directory
        .claim_queued_for_steal(actor)
        .expect("claim after publication and pin clear");
    directory.abort_steal_claim(claim).expect("abort claim");
}

#[test]
fn executing_parked_and_yielding_actors_cannot_be_claimed_as_queued() {
    for (actor_id, lifecycle) in [
        (1, VmActorLifecycle::Executing),
        (2, VmActorLifecycle::Parked),
        (3, VmActorLifecycle::Yielding),
    ] {
        let actor = pid(actor_id);
        let mut directory = VmActorDirectory::<u64>::default();
        directory.insert(actor, actor_id).expect("insert actor");
        if lifecycle == VmActorLifecycle::Executing {
            directory.mark_queued(actor).expect("queue actor");
            let _token = directory.acquire_mutator(actor, 1).expect("execute actor");
        } else if lifecycle == VmActorLifecycle::Parked {
            directory.mark_queued(actor).expect("queue actor");
            let token = directory.acquire_mutator(actor, 1).expect("execute actor");
            directory
                .release_mutator(token, VmActorLifecycle::Parked)
                .expect("park actor");
        }
        assert!(directory.claim_queued_for_steal(actor).is_err());
    }
}
