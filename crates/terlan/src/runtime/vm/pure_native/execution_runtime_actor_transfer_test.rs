use super::actor_transfer::PureNativeActorExecutionTransfer;
use super::*;
use crate::runtime::native_image::TvmBoundaryType;

#[test]
fn parked_execution_moves_between_runtimes_and_preserves_linear_claim() {
    let mut source = PureNativeExecutionRuntime::runtime_default().expect("source runtime");
    source
        .park_continuation(7, 11, 13, None, None)
        .expect("park continuation");
    let transfer = source
        .detach_actor_execution(7)
        .expect("detach parked execution");
    assert_eq!(transfer.owner_id(), 7);
    assert_eq!(transfer.request_id(), 11);
    assert_eq!(transfer.continuation_id(), 13);
    assert_eq!(source.pending_continuation_count(), 0);

    let mut destination = PureNativeExecutionRuntime::runtime_default().expect("destination");
    destination
        .import_actor_execution(transfer)
        .expect("import parked execution");
    let claim = destination
        .claim_continuation(7, 11, 13)
        .expect("claim imported continuation");
    assert_eq!(claim.owner_id(), 7);
    assert!(destination.claim_continuation(7, 11, 13).is_err());
}

#[test]
fn failed_destination_import_returns_state_for_source_rollback() {
    let mut source = PureNativeExecutionRuntime::runtime_default().expect("source runtime");
    source
        .park_continuation(9, 17, 19, None, None)
        .expect("source continuation");
    let transfer = source
        .detach_actor_execution(9)
        .expect("detach source execution");
    let mut destination = PureNativeExecutionRuntime::runtime_default().expect("destination");
    destination
        .park_continuation(9, 23, 29, None, None)
        .expect("destination collision");

    let failure = destination
        .import_actor_execution(transfer)
        .expect_err("collision must retain transfer");
    assert!(failure.reason().contains("transfer_collision"));
    source
        .import_actor_execution(failure.into_transfer())
        .expect("restore source execution");
    source
        .claim_continuation(9, 17, 19)
        .expect("restored continuation remains exact");
}

#[test]
fn actor_execution_transfer_is_send_without_native_stack_borrows() {
    fn assert_send<T: Send>() {}
    assert_send::<PureNativeActorExecutionTransfer>();
}

#[test]
fn managed_heap_and_mailbox_roots_move_with_parked_continuation() {
    let mut source = PureNativeExecutionRuntime::runtime_default().expect("source runtime");
    let value = source
        .managed
        .allocate_string_value(31, "migrating")
        .expect("allocate actor string");
    let fragment = source
        .managed
        .copy_mailbox_value(31, 31, &TvmBoundaryType::String, value)
        .expect("retain mailbox graph");
    source
        .park_continuation(31, 37, 41, None, None)
        .expect("park continuation");

    let transfer = source
        .detach_actor_execution(31)
        .expect("detach managed actor");
    assert!(transfer.managed().heap_usage().is_some());
    assert_eq!(transfer.managed().mailbox_fragment_count(), 1);
    assert_eq!(source.managed.actor_count(), 0);
    let mut destination = PureNativeExecutionRuntime::runtime_default().expect("destination");
    destination
        .import_actor_execution(transfer)
        .expect("import managed actor");
    let imported = destination
        .managed
        .mailbox_value_word(fragment.fragment_id(), 31, &TvmBoundaryType::String)
        .expect("resolve imported root");
    assert_eq!(
        destination
            .managed
            .materialize_string_value(31, imported)
            .expect("materialize imported string"),
        "migrating"
    );
}

#[test]
fn parked_collection_relocates_top_and_nested_completion_roots() {
    let owner = 37;
    let mut runtime = PureNativeExecutionRuntime::runtime_default().expect("managed runtime");
    let top = runtime
        .managed
        .allocate_string_value(owner, "top")
        .expect("top continuation root");
    let completion = runtime
        .managed
        .allocate_string_value(owner, "completion")
        .expect("completion root");
    let garbage = "unreachable-managed-allocation";
    for _ in 0..40_000 {
        runtime
            .managed
            .allocate_string_value(owner, garbage)
            .expect("garbage below the hard heap limit");
    }
    let (_, top_pending) = runtime
        .managed
        .park_continuation_captures(owner, 41, &[TvmBoundaryType::String], &[top])
        .expect("park top root");
    let (_, completion_pending) = runtime
        .managed
        .park_continuation_captures(owner, 43, &[TvmBoundaryType::String], &[completion])
        .expect("park completion root");
    runtime
        .park_continuation_with_completions(
            owner,
            47,
            41,
            None,
            top_pending,
            vec![PendingNativeCompletionFrame {
                continuation_id: 43,
                scalar_captures: Vec::new(),
                managed: completion_pending,
            }],
        )
        .expect("park complete continuation stack");

    runtime
        .collect_parked_owner_at_safepoint(owner)
        .expect("collect precise parked roots");
    let claim = runtime
        .claim_continuation(owner, 47, 41)
        .expect("claim relocated continuation");
    let (_, top_pending, mut completions) = claim.into_resume_state_with_completions();
    let restored_top = runtime
        .managed
        .restore_continuation_captures(owner, 41, &[TvmBoundaryType::String], &[], top_pending)
        .expect("restore relocated top root");
    let completion = completions.pop().expect("completion frame");
    let restored_completion = runtime
        .managed
        .restore_continuation_captures(
            owner,
            completion.continuation_id,
            &[TvmBoundaryType::String],
            &completion.scalar_captures,
            completion.managed,
        )
        .expect("restore relocated completion root");

    assert_eq!(
        runtime
            .managed
            .materialize_string_value(owner, restored_top[0])
            .expect("materialize top root"),
        "top"
    );
    assert_eq!(
        runtime
            .managed
            .materialize_string_value(owner, restored_completion[0])
            .expect("materialize completion root"),
        "completion"
    );
    assert_eq!(
        runtime
            .managed
            .heap_usage(owner)
            .map(|(_, objects)| objects),
        Some(2)
    );
}

#[test]
fn managed_destination_collision_returns_the_complete_actor_for_rollback() {
    let mut source = PureNativeExecutionRuntime::runtime_default().expect("source runtime");
    let source_value = source
        .managed
        .allocate_string_value(43, "source")
        .expect("allocate source heap");
    source
        .park_continuation(43, 47, 53, None, None)
        .expect("park source continuation");
    let transfer = source
        .detach_actor_execution(43)
        .expect("detach source actor");

    let mut destination = PureNativeExecutionRuntime::runtime_default().expect("destination");
    destination
        .managed
        .allocate_string_value(43, "destination")
        .expect("create destination collision");
    let failure = destination
        .import_actor_execution(transfer)
        .expect_err("managed owner collision must reject the whole transfer");
    assert!(failure.reason().contains("transfer_collision"));
    source
        .import_actor_execution(failure.into_transfer())
        .expect("restore source actor");
    source
        .claim_continuation(43, 47, 53)
        .expect("restore exact continuation authority");
    assert_eq!(
        source
            .managed
            .materialize_string_value(43, source_value)
            .expect("restore exact managed reference"),
        "source"
    );
}
