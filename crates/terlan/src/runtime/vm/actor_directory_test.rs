use super::{VmActorDirectory, VmActorDirectoryError, VmActorLifecycle, VmActorMutatorToken};
use crate::runtime::vm::process::VmProcessId;
use crate::runtime::vm::process::{VmProcessSource, VmProcessState, VmProcessTable};
use crate::runtime::vm::scheduler::{VmScheduler, VmSchedulerDecision, VmSchedulerOutcome};
use std::sync::Arc;
use std::thread;

fn pid(value: u64) -> VmProcessId {
    VmProcessId::from_raw_for_test(value)
}

fn directory<T>() -> VmActorDirectory<T> {
    VmActorDirectory::default()
}

#[test]
fn ownership_cycle_records_stable_generations_and_events() {
    let actor = pid(1);
    let mut directory = directory();
    let handle = directory.insert(actor, 10_u64).expect("insert actor");

    directory.mark_queued(actor).expect("queue actor");
    let first = directory.acquire_mutator(actor, 1).expect("acquire actor");
    assert_eq!(first.handle(), handle);
    assert_eq!(first.owner(), 1);
    directory
        .with_mutator(&first, |value| *value += 5)
        .expect("mutate actor");
    let first_generation = first.owner_generation();
    directory
        .release_mutator(first, VmActorLifecycle::Yielding)
        .expect("release actor");

    directory.mark_queued(actor).expect("requeue actor");
    let second = directory
        .acquire_mutator(actor, 1)
        .expect("reacquire actor");
    assert_ne!(second.owner_generation(), first_generation);
    assert_eq!(directory.resolve_handle(handle), Ok(&15));
    directory
        .release_mutator(second, VmActorLifecycle::Parked)
        .expect("park actor");

    let events = directory.transition_events();
    assert_eq!(events.len(), 6);
    assert!(events
        .windows(2)
        .all(|pair| pair[0].sequence < pair[1].sequence));
    assert_eq!(events[0].from, VmActorLifecycle::Yielding);
    assert_eq!(events[0].to, VmActorLifecycle::Queued);
    assert_eq!(events[5].to, VmActorLifecycle::Parked);
}

#[test]
fn double_acquisition_and_stale_release_are_rejected() {
    let actor = pid(2);
    let mut directory = directory();
    directory.insert(actor, 1_u8).expect("insert actor");
    directory.mark_queued(actor).expect("queue actor");
    let token = directory.acquire_mutator(actor, 7).expect("acquire actor");
    let stale = token.duplicate_for_test();

    assert!(matches!(
        directory.acquire_mutator(actor, 8),
        Err(VmActorDirectoryError::AlreadyOwned { owner: 7, .. })
    ));
    assert!(directory.get_mut_unowned(actor).is_none());
    directory
        .release_mutator(token, VmActorLifecycle::Yielding)
        .expect("release current token");
    assert_eq!(
        directory.release_mutator(stale, VmActorLifecycle::Parked),
        Err(VmActorDirectoryError::StaleMutator)
    );
}

#[test]
fn execution_without_queue_ownership_is_rejected() {
    let actor = pid(3);
    let mut directory = directory();
    directory.insert(actor, 1_u8).expect("insert actor");

    assert!(matches!(
        directory.acquire_mutator(actor, 1),
        Err(VmActorDirectoryError::InvalidTransition {
            from: VmActorLifecycle::Yielding,
            to: VmActorLifecycle::Executing
        })
    ));
    assert_eq!(
        directory.acquire_mutator(actor, 0),
        Err(VmActorDirectoryError::InvalidOwner(0))
    );
}

#[test]
fn migration_rejects_borrowed_state_and_exit_can_interrupt_transfer() {
    let actor = pid(4);
    let mut directory = directory();
    directory.insert(actor, 1_u8).expect("insert actor");
    directory.mark_queued(actor).expect("queue actor");
    let token = directory.acquire_mutator(actor, 1).expect("acquire actor");

    assert!(matches!(
        directory.begin_migration(actor),
        Err(VmActorDirectoryError::AlreadyOwned { .. })
    ));
    assert!(matches!(
        directory.release_mutator(token, VmActorLifecycle::Migrating),
        Err(VmActorDirectoryError::InvalidTransition { .. })
    ));

    let current = directory
        .acquire_mutator(actor, 1)
        .expect_err("token remains owned");
    assert!(matches!(
        current,
        VmActorDirectoryError::AlreadyOwned { .. }
    ));
}

#[test]
fn migration_and_exit_transition_after_mutator_release() {
    let actor = pid(5);
    let mut directory = directory();
    directory.insert(actor, 1_u8).expect("insert actor");
    directory.begin_migration(actor).expect("begin migration");
    directory.finish_migration(actor).expect("finish migration");
    directory.begin_migration(actor).expect("restart migration");
    directory
        .mark_exiting(actor)
        .expect("exit during migration");
    directory.mark_retired(actor).expect("retire actor");
    assert_eq!(directory.lifecycle(actor), Ok(VmActorLifecycle::Retired));
}

#[test]
fn queued_actor_cannot_be_retired_directly() {
    let actor = pid(6);
    let mut directory = directory();
    directory.insert(actor, 1_u8).expect("insert actor");
    directory.mark_queued(actor).expect("queue actor");

    assert_eq!(
        directory.mark_retired(actor),
        Err(VmActorDirectoryError::InvalidTransition {
            from: VmActorLifecycle::Queued,
            to: VmActorLifecycle::Retired,
        })
    );
}

#[test]
fn lookup_pin_blocks_reclamation_and_reused_slot_rejects_aba_handle() {
    let first = pid(7);
    let second = pid(8);
    let mut directory = directory();
    let stale = directory.insert(first, 11_u8).expect("insert first actor");
    let pinned = directory.pin_lookup(first).expect("pin actor");
    directory.mark_exiting(first).expect("exit actor");
    directory.mark_retired(first).expect("retire actor");

    assert_eq!(
        directory.reclaim(first),
        Err(VmActorDirectoryError::LookupPinned(1))
    );
    directory.unpin_lookup(pinned).expect("unpin actor");
    assert_eq!(directory.reclaim(first), Ok(11));

    let current = directory.insert(second, 22_u8).expect("reuse actor slot");
    assert_ne!(stale.actor_generation(), current.actor_generation());
    assert_eq!(
        directory.resolve_handle(stale),
        Err(VmActorDirectoryError::StaleHandle(stale))
    );
    assert_eq!(directory.resolve_handle(current), Ok(&22));
}

#[test]
fn stale_lookup_release_and_missing_actor_are_typed_failures() {
    let actor = pid(9);
    let missing = pid(99);
    let mut directory = directory();
    let handle = directory.insert(actor, 1_u8).expect("insert actor");

    assert_eq!(
        directory.pin_lookup(missing),
        Err(VmActorDirectoryError::MissingActor(missing))
    );
    assert_eq!(
        directory.unpin_lookup(handle),
        Err(VmActorDirectoryError::StaleHandle(handle))
    );
}

#[test]
fn corrupt_lifecycle_fails_stop_without_mutating_actor_data() {
    let actor = pid(10);
    let mut directory = directory();
    directory.insert(actor, 42_u8).expect("insert actor");
    directory.corrupt_state_for_test(actor, 0);

    assert_eq!(
        directory.lifecycle(actor),
        Err(VmActorDirectoryError::CorruptLifecycle(0))
    );
    assert_eq!(
        directory.acquire_mutator(actor, 1),
        Err(VmActorDirectoryError::CorruptLifecycle(0))
    );
    assert_eq!(directory.get(actor), Some(&42));
}

#[test]
fn invalid_release_state_does_not_consume_current_ownership() {
    let actor = pid(11);
    let mut directory = directory();
    directory.insert(actor, 5_u8).expect("insert actor");
    directory.mark_queued(actor).expect("queue actor");
    let token = directory.acquire_mutator(actor, 1).expect("acquire actor");
    let retained = token.duplicate_for_test();

    assert!(matches!(
        directory.release_mutator(token, VmActorLifecycle::Retired),
        Err(VmActorDirectoryError::InvalidTransition { .. })
    ));
    assert_eq!(directory.with_mutator(&retained, |value| *value), Ok(5));
}

#[test]
fn actor_generation_and_owner_limits_are_bounded() {
    let actor = pid(12);
    let mut directory = directory();
    directory.insert(actor, 1_u8).expect("insert actor");
    directory.mark_queued(actor).expect("queue actor");

    assert!(matches!(
        directory.acquire_mutator(actor, u64::MAX),
        Err(VmActorDirectoryError::InvalidOwner(u64::MAX))
    ));
}

#[test]
fn mailbox_publication_is_receiver_local_and_rejects_retired_generation() {
    let first = pid(13);
    let second = pid(14);
    let mut directory = directory();
    let first_handle = directory.insert(first, 1_u8).expect("insert first actor");
    let second_handle = directory.insert(second, 2_u8).expect("insert second actor");

    let (first_one, _) = directory
        .publish_fragment(first, ())
        .expect("publish first");
    let (first_two, _) = directory
        .publish_fragment(first, ())
        .expect("publish second");
    let (second_one, _) = directory
        .publish_fragment(second, ())
        .expect("publish peer");
    assert_eq!((first_one.handle, first_one.sequence), (first_handle, 1));
    assert_eq!((first_two.handle, first_two.sequence), (first_handle, 2));
    assert_eq!((second_one.handle, second_one.sequence), (second_handle, 1));

    directory.mark_exiting(first).expect("exit first actor");
    directory.mark_retired(first).expect("retire first actor");
    assert!(matches!(
        directory.publish_fragment(first, ()),
        Err(VmActorDirectoryError::InvalidTransition {
            from: VmActorLifecycle::Retired,
            to: VmActorLifecycle::Retired,
        })
    ));
}

#[test]
fn control_mutator_preserves_lifecycle_and_rejects_stale_generation() {
    let actor = pid(15);
    let mut directory = directory();
    directory.insert(actor, 10_u8).expect("insert actor");
    let token = directory
        .acquire_control_mutator(actor, 9)
        .expect("acquire control mutator");
    let stale = token.duplicate_for_test();

    assert_eq!(directory.lifecycle(actor), Ok(VmActorLifecycle::Yielding));
    assert!(matches!(
        directory.acquire_control_mutator(actor, 10),
        Err(VmActorDirectoryError::AlreadyOwned { owner: 9, .. })
    ));
    directory
        .with_mutator(&token, |value| *value = 11)
        .expect("mutate under control ownership");
    directory
        .release_control_mutator(token)
        .expect("release control mutator");

    assert_eq!(directory.resolve_handle(stale.handle()), Ok(&11));
    assert_eq!(
        directory.with_mutator(&stale, |value| *value),
        Err(VmActorDirectoryError::StaleMutator)
    );
    assert_eq!(directory.lifecycle(actor), Ok(VmActorLifecycle::Yielding));
    let events = directory.transition_events();
    assert_eq!(events.len(), 2);
    assert!(events
        .iter()
        .all(|event| event.from == event.to && event.owner == 9));
    assert_eq!(events[0].owner_generation, events[1].owner_generation);
}

#[test]
fn control_mutator_rejects_migration_and_invalid_owner() {
    let actor = pid(16);
    let mut directory = directory();
    directory.insert(actor, 1_u8).expect("insert actor");
    directory.begin_migration(actor).expect("begin migration");

    assert_eq!(
        directory.acquire_control_mutator(actor, 1),
        Err(VmActorDirectoryError::InvalidTransition {
            from: VmActorLifecycle::Migrating,
            to: VmActorLifecycle::Migrating,
        })
    );
    assert_eq!(
        directory.acquire_control_mutator(actor, 0),
        Err(VmActorDirectoryError::InvalidOwner(0))
    );
}

#[test]
fn publication_queue_integrates_only_under_owner_and_drops_retired_generation() {
    let first = pid(17);
    let second = pid(18);
    let mut directory: VmActorDirectory<Vec<u8>, u8> = VmActorDirectory::default();
    directory
        .insert(first, Vec::<u8>::new())
        .expect("insert actor");
    directory
        .publish_fragment(first, 1)
        .expect("publish fragment");
    let token = directory
        .acquire_control_mutator(first, 7)
        .expect("acquire receiver");
    directory
        .drain_publications(&token, |mailbox, _publication, value| mailbox.push(value))
        .expect("integrate under receiver ownership");
    directory
        .release_control_mutator(token)
        .expect("release receiver");
    directory.mark_exiting(first).expect("exit actor");
    directory.mark_retired(first).expect("retire actor");
    assert_eq!(directory.reclaim(first), Ok(vec![1]));
    directory
        .insert(second, Vec::<u8>::new())
        .expect("reuse actor slot");
    assert_eq!(
        directory.pending_publications(second),
        Ok(0),
        "reused slots cannot inherit retired mailbox fragments"
    );
}

#[test]
fn publication_during_execution_prevents_lost_wakeup_park() {
    let actor = pid(19);
    let mut directory: VmActorDirectory<u8, u8> = VmActorDirectory::default();
    directory.insert(actor, 0).expect("insert actor");
    directory.mark_queued(actor).expect("queue actor");
    let token = directory.acquire_mutator(actor, 1).expect("own actor");

    directory
        .publish_fragment(actor, 7)
        .expect("producer publishes without receiver mutation");
    assert_eq!(
        directory
            .release_mutator(token, VmActorLifecycle::Parked)
            .expect("release observes publication"),
        VmActorLifecycle::Yielding
    );
    directory.mark_queued(actor).expect("queue notified actor");
    let token = directory
        .acquire_mutator(actor, 1)
        .expect("reacquire actor");
    let mut received = None;
    assert_eq!(
        directory
            .drain_publications(&token, |_actor, _publication, value| received = Some(value))
            .expect("consume publication"),
        1
    );
    assert_eq!(received, Some(7));
    directory
        .release_mutator(token, VmActorLifecycle::Yielding)
        .expect("release receiver");
}

#[test]
fn actor_directory_accepts_cross_thread_producers_before_owned_integration() {
    let actor = pid(20);
    let mut directory: VmActorDirectory<u8, (u64, u64)> = VmActorDirectory::default();
    directory.insert(actor, 0).expect("insert actor");
    let directory = Arc::new(directory);
    let producers = (0_u64..4)
        .map(|sender| {
            let directory = Arc::clone(&directory);
            thread::spawn(move || {
                for ordinal in 0_u64..64 {
                    directory
                        .publish_fragment(actor, (sender, ordinal))
                        .expect("publish through shared actor directory");
                }
            })
        })
        .collect::<Vec<_>>();
    for producer in producers {
        producer.join().expect("producer remains healthy");
    }

    let mut directory = Arc::try_unwrap(directory).expect("all producers released directory");
    let token = directory
        .acquire_control_mutator(actor, 1)
        .expect("acquire receiver");
    let mut observed = vec![Vec::new(), Vec::new(), Vec::new(), Vec::new()];
    assert_eq!(
        directory
            .drain_publications(&token, |_actor, _publication, (sender, ordinal)| {
                observed[sender as usize].push(ordinal);
            })
            .expect("integrate all fragments"),
        256
    );
    for sender in observed {
        assert_eq!(sender, (0_u64..64).collect::<Vec<_>>());
    }
    directory
        .release_control_mutator(token)
        .expect("release receiver");
}

#[test]
fn terminal_lifecycle_rejects_late_publication_and_slot_reuse_starts_empty() {
    let retired = pid(21);
    let replacement = pid(22);
    let mut directory: VmActorDirectory<u8, u8> = VmActorDirectory::default();
    directory.insert(retired, 1).expect("insert retired actor");
    directory
        .publish_fragment(retired, 7)
        .expect("publication before exit remains valid");
    directory.mark_exiting(retired).expect("exit actor");
    assert!(matches!(
        directory.publish_fragment(retired, 8),
        Err(VmActorDirectoryError::InvalidTransition {
            from: VmActorLifecycle::Exiting,
            to: VmActorLifecycle::Exiting,
        })
    ));
    directory.mark_retired(retired).expect("retire actor");
    assert!(matches!(
        directory.publish_fragment(retired, 9),
        Err(VmActorDirectoryError::InvalidTransition {
            from: VmActorLifecycle::Retired,
            to: VmActorLifecycle::Retired,
        })
    ));
    assert_eq!(directory.reclaim(retired), Ok(1));
    directory
        .insert(replacement, 2)
        .expect("reuse reclaimed slot");
    assert_eq!(directory.pending_publications(replacement), Ok(0));
    assert!(matches!(
        directory.publish_fragment(retired, 10),
        Err(VmActorDirectoryError::MissingActor(missing)) if missing == retired
    ));
}

#[test]
fn process_control_mutator_releases_ownership_during_unwind() {
    let mut processes = VmProcessTable::default();
    let actor = processes.spawn_root(VmProcessSource::new("ownership", "panic", 0));

    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = processes.with_process_control_mutator(actor, |_process| -> () {
            panic!("simulated control mutation interruption");
        });
    }));
    assert!(panic.is_err());
    processes
        .with_process_control_mutator(actor, |process| process.block())
        .expect("actor ownership was released while unwinding");
    assert_eq!(
        processes.get(actor).expect("actor remains present").state,
        VmProcessState::Blocked
    );
}

#[test]
fn one_scheduler_uses_actor_ownership_without_changing_run_order() {
    let mut processes = VmProcessTable::default();
    let first = processes.spawn_root(VmProcessSource::new("ownership", "first", 0));
    let second = processes.spawn_root(VmProcessSource::new("ownership", "second", 0));
    let mut scheduler = VmScheduler::default();
    scheduler
        .enqueue_runnable(&processes, first)
        .expect("enqueue first actor");
    scheduler
        .enqueue_runnable(&processes, second)
        .expect("enqueue second actor");

    let first_run = scheduler
        .run_next(&mut processes, |_process, _slice| {
            VmSchedulerDecision::Yield { reductions: 1 }
        })
        .expect("run first actor");
    let second_run = scheduler
        .run_next(&mut processes, |_process, _slice| {
            VmSchedulerDecision::Yield { reductions: 1 }
        })
        .expect("run second actor");

    assert_eq!(first_run.pid, Some(first));
    assert_eq!(second_run.pid, Some(second));
    let events = processes.actor_transition_events();
    let first_events: Vec<_> = events
        .iter()
        .filter(|event| event.handle.pid() == first)
        .collect();
    assert_eq!(
        first_events
            .iter()
            .map(|event| event.to)
            .collect::<Vec<_>>(),
        vec![
            VmActorLifecycle::Queued,
            VmActorLifecycle::Executing,
            VmActorLifecycle::Yielding,
            VmActorLifecycle::Queued,
        ]
    );
    assert_eq!(first_events[1].owner, 1);
    assert_eq!(
        first_events[2].owner_generation,
        first_events[1].owner_generation
    );
}

#[test]
fn cancellation_requested_during_owned_slice_exits_at_safepoint() {
    let mut processes = VmProcessTable::default();
    let actor = processes.spawn_root(VmProcessSource::new("ownership", "cancel", 0));
    let mut scheduler = VmScheduler::default();
    scheduler
        .enqueue_runnable(&processes, actor)
        .expect("enqueue actor");

    let run = scheduler
        .run_next(&mut processes, |process, _slice| {
            process.request_cancellation();
            VmSchedulerDecision::Yield { reductions: 1 }
        })
        .expect("cancel actor at safepoint");

    assert_eq!(run.outcome, VmSchedulerOutcome::Cancelled(Vec::new()));
    assert!(matches!(
        processes.get(actor).expect("retired actor").state,
        VmProcessState::Exited(_)
    ));
    let events = processes.actor_transition_events();
    assert!(events.iter().any(|event| {
        event.handle.pid() == actor
            && event.from == VmActorLifecycle::Executing
            && event.to == VmActorLifecycle::Exiting
    }));
    assert!(events.iter().any(|event| {
        event.handle.pid() == actor
            && event.from == VmActorLifecycle::Exiting
            && event.to == VmActorLifecycle::Retired
    }));
}

fn _token_is_not_clone(_: VmActorMutatorToken) {}
