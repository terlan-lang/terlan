use super::super::super::bitstring::VmBitString;
use super::super::super::process::{
    VmExitReason, VmProcessId, VmProcessResumeState, VmProcessSource, VmProcessState,
};
use super::super::super::scheduler::{VmSchedulerDecision, VmSchedulerOutcome};
use super::super::super::ReplValue;
use super::super::{VmActorReceive, VmActorRuntime};
use std::sync::Arc;

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.Main", name, 0)
}

#[test]
fn actor_runtime_suspension_removes_queued_process_without_running_it() {
    let mut runtime = VmActorRuntime::default();
    let pid = runtime.spawn_root(source("suspended"));

    runtime.suspend(pid).expect("actor should suspend");
    let run = runtime
        .run_next(|_, _| panic!("suspended actor must not execute"))
        .expect("empty scheduler should report idle");

    assert_eq!(runtime.scheduled_len(), 0);
    assert_eq!(run.outcome, VmSchedulerOutcome::Idle);
    assert_eq!(
        runtime
            .processes()
            .get(pid)
            .expect("actor should exist")
            .state,
        VmProcessState::Suspended(VmProcessResumeState::Runnable)
    );
}

#[test]
fn actor_runtime_suspension_resume_requeues_and_runs_actor() {
    let mut runtime = VmActorRuntime::default();
    let pid = runtime.spawn_root(source("resumed"));
    runtime.suspend(pid).expect("actor should suspend");

    runtime.resume(pid).expect("actor should resume");
    let run = runtime
        .run_next(|_, _| VmSchedulerDecision::Yield { reductions: 1 })
        .expect("resumed actor should run");

    assert_eq!(run.pid, Some(pid));
    assert_eq!(run.outcome, VmSchedulerOutcome::Ran);
    assert_eq!(
        runtime
            .processes()
            .get(pid)
            .expect("actor should exist")
            .state,
        VmProcessState::Runnable
    );
}

#[test]
fn actor_runtime_suspension_reports_missing_exited_and_invalid_resume() {
    let mut runtime = VmActorRuntime::default();
    let live = runtime.spawn_root(source("live"));
    let exited = runtime.spawn_root(source("exited"));
    let missing = VmProcessId::from_raw_for_test(404);
    runtime
        .exit_actor(exited, VmExitReason::Normal)
        .expect("actor should exit");

    assert_eq!(
        runtime
            .suspend(missing)
            .expect_err("missing actor cannot suspend"),
        "cannot suspend missing process 404"
    );
    assert_eq!(
        runtime
            .resume(missing)
            .expect_err("missing actor cannot resume"),
        "cannot resume missing process 404"
    );
    assert_eq!(
        runtime
            .suspend(exited)
            .expect_err("exited actor cannot suspend"),
        "cannot suspend exited process 2"
    );
    assert_eq!(
        runtime
            .resume(exited)
            .expect_err("exited actor cannot resume"),
        "cannot resume exited process 2"
    );
    assert_eq!(
        runtime
            .resume(live)
            .expect_err("runnable actor is not suspended"),
        "cannot resume process 1: process is not suspended"
    );
}

#[test]
fn actor_runtime_suspension_message_delivery_does_not_resume_actor() {
    let mut runtime = VmActorRuntime::default();
    let sender = runtime.spawn_root(source("sender"));
    let recipient = runtime.spawn_root(source("recipient"));
    runtime
        .suspend(recipient)
        .expect("recipient should suspend");

    runtime
        .send(sender, recipient, ReplValue::Int(17))
        .expect("message should queue");

    let process = runtime
        .processes()
        .get(recipient)
        .expect("recipient should exist");
    assert_eq!(
        process.state,
        VmProcessState::Suspended(VmProcessResumeState::Runnable)
    );
    assert_eq!(process.mailbox_len(), 1);
    assert_eq!(runtime.scheduled_len(), 1);
}

#[test]
fn actor_runtime_suspension_resume_receives_message_queued_while_suspended() {
    let mut runtime = VmActorRuntime::default();
    let sender = runtime.spawn_root(source("sender"));
    let recipient = runtime.spawn_root(source("recipient"));
    assert_eq!(
        runtime
            .receive_next_or_block(recipient)
            .expect("empty receive should block"),
        VmActorReceive::Blocked
    );
    runtime
        .suspend(recipient)
        .expect("blocked actor should suspend");
    assert_eq!(
        runtime
            .processes()
            .get(recipient)
            .expect("recipient should exist")
            .state,
        VmProcessState::Suspended(VmProcessResumeState::Blocked)
    );

    runtime
        .send(sender, recipient, ReplValue::Int(23))
        .expect("message should queue");
    assert_eq!(
        runtime
            .processes()
            .get(recipient)
            .expect("recipient should exist")
            .state,
        VmProcessState::Suspended(VmProcessResumeState::Runnable)
    );
    runtime.resume(recipient).expect("recipient should resume");

    let received = runtime
        .receive_next_or_block(recipient)
        .expect("queued message should be received");
    assert!(matches!(
        received,
        VmActorReceive::Message(message) if message.payload == ReplValue::Int(23)
    ));
}

#[test]
fn actor_runtime_suspension_resume_without_message_restores_blocked_state() {
    let mut runtime = VmActorRuntime::default();
    let pid = runtime.spawn_root(source("blocked"));
    runtime
        .receive_next_or_block(pid)
        .expect("empty receive should block");
    runtime.suspend(pid).expect("blocked actor should suspend");

    runtime.resume(pid).expect("actor should resume as blocked");

    assert_eq!(
        runtime
            .processes()
            .get(pid)
            .expect("actor should exist")
            .state,
        VmProcessState::Blocked
    );
    assert_eq!(runtime.scheduled_len(), 0);
}

#[test]
fn actor_runtime_suspension_does_not_block_later_runnable_actor() {
    let mut runtime = VmActorRuntime::default();
    let suspended = runtime.spawn_root(source("suspended"));
    let runnable = runtime.spawn_root(source("runnable"));
    runtime
        .suspend(suspended)
        .expect("first actor should suspend");

    let run = runtime
        .run_next(|process, _| {
            assert_eq!(process.pid, runnable);
            VmSchedulerDecision::Yield { reductions: 1 }
        })
        .expect("later runnable actor should run");

    assert_eq!(run.pid, Some(runnable));
    assert_eq!(run.outcome, VmSchedulerOutcome::Ran);
}

#[test]
fn native_continuation_resume_requires_exact_process_request_and_continuation() {
    let mut runtime = VmActorRuntime::default();
    let owner = runtime.spawn_root(source("native-owner"));
    let other = runtime.spawn_root(source("other-owner"));

    runtime
        .park_native_continuation(owner.as_u64(), 17, 23)
        .expect("native continuation should park its owner");
    assert_eq!(runtime.pending_native_continuation_count(), 1);
    assert_eq!(
        runtime.processes().get(owner).expect("owner exists").state,
        VmProcessState::Suspended(VmProcessResumeState::Runnable)
    );
    assert_eq!(
        runtime
            .resume_native_continuation(other.as_u64(), 17, 23)
            .expect_err("another process must not claim the continuation"),
        "native continuation 17/23 is owned by process 1, not process 2"
    );
    assert_eq!(
        runtime
            .resume_native_continuation(owner.as_u64(), 17, 24)
            .expect_err("another continuation identity must be stale"),
        "stale native continuation 17/24"
    );
    assert_eq!(runtime.pending_native_continuation_count(), 1);

    runtime
        .resume_native_continuation(owner.as_u64(), 17, 23)
        .expect("exact owner and identity should resume");
    assert_eq!(runtime.pending_native_continuation_count(), 0);
    assert_eq!(
        runtime.processes().get(owner).expect("owner exists").state,
        VmProcessState::Runnable
    );
}

#[test]
fn native_continuation_parking_rejects_duplicate_and_zero_identities() {
    let mut runtime = VmActorRuntime::default();
    let first = runtime.spawn_root(source("first-native-owner"));
    let second = runtime.spawn_root(source("second-native-owner"));

    assert_eq!(
        runtime
            .park_native_continuation(0, 1, 1)
            .expect_err("zero owner must fail"),
        "native continuation owner identity must be nonzero"
    );
    assert_eq!(
        runtime
            .park_native_continuation(first.as_u64(), 0, 1)
            .expect_err("zero request must fail"),
        "native continuation request identity must be nonzero"
    );
    assert_eq!(
        runtime
            .park_native_continuation(first.as_u64(), 1, 0)
            .expect_err("zero continuation must fail"),
        "native continuation identity must be nonzero"
    );
    runtime
        .park_native_continuation(first.as_u64(), 11, 13)
        .expect("first identity should park");
    assert_eq!(
        runtime
            .park_native_continuation(first.as_u64(), 12, 14)
            .expect_err("one owner cannot park twice"),
        "process 1 already owns native continuation 11/13"
    );
    assert_eq!(
        runtime
            .park_native_continuation(second.as_u64(), 11, 13)
            .expect_err("one identity cannot have two owners"),
        "native continuation 11/13 is already owned by process 1"
    );
    runtime
        .suspend(second)
        .expect("second actor should suspend");
    assert_eq!(
        runtime
            .park_native_continuation(second.as_u64(), 12, 14)
            .expect_err("an independently suspended actor cannot claim a continuation"),
        "cannot park native continuation for non-runnable process 2"
    );
}

#[test]
fn actor_exit_releases_native_continuation_ownership() {
    let mut runtime = VmActorRuntime::default();
    let exiting = runtime.spawn_root(source("exiting-native-owner"));
    let replacement = runtime.spawn_root(source("replacement-native-owner"));
    runtime
        .park_native_continuation(exiting.as_u64(), 29, 31)
        .expect("continuation should park");

    runtime
        .exit_actor(exiting, VmExitReason::Killed)
        .expect("parked owner should exit");
    assert_eq!(runtime.pending_native_continuation_count(), 0);
    runtime
        .park_native_continuation(replacement.as_u64(), 29, 31)
        .expect("released identity should be reusable");
}

#[test]
fn native_send_transition_delivers_before_exact_owner_resume() {
    let mut runtime = VmActorRuntime::default();
    let owner = runtime.spawn_root(source("native-sender"));
    let recipient = runtime.spawn_root(source("native-recipient"));
    runtime
        .park_native_continuation(owner.as_u64(), 41, 43)
        .expect("native send continuation should park");

    let message_id = runtime
        .service_native_send(
            owner.as_u64(),
            41,
            43,
            recipient.as_u64(),
            ReplValue::Int(47),
        )
        .expect("native send should deliver and resume");

    assert_ne!(message_id, 0);
    assert_eq!(runtime.pending_native_continuation_count(), 0);
    assert_eq!(
        runtime.processes().get(owner).expect("owner exists").state,
        VmProcessState::Runnable
    );
    assert!(matches!(
        runtime
            .receive_next_or_block(recipient)
            .expect("recipient receive should succeed"),
        VmActorReceive::Message(message)
            if message.sender == owner && message.payload == ReplValue::Int(47)
    ));
}

#[test]
fn native_send_transition_rejects_invalid_ownership_without_mailbox_mutation() {
    let mut runtime = VmActorRuntime::default();
    let owner = runtime.spawn_root(source("native-sender"));
    let foreign = runtime.spawn_root(source("foreign-sender"));
    let recipient = runtime.spawn_root(source("native-recipient"));
    runtime
        .park_native_continuation(owner.as_u64(), 53, 59)
        .expect("native send continuation should park");

    assert_eq!(
        runtime
            .service_native_send(
                foreign.as_u64(),
                53,
                59,
                recipient.as_u64(),
                ReplValue::Int(61),
            )
            .expect_err("foreign owner must fail"),
        "native continuation 53/59 is owned by process 1, not process 2"
    );
    assert_eq!(
        runtime
            .service_native_send(
                owner.as_u64(),
                53,
                60,
                recipient.as_u64(),
                ReplValue::Int(61),
            )
            .expect_err("stale continuation must fail"),
        "stale native continuation 53/60"
    );
    assert_eq!(
        runtime
            .service_native_send(owner.as_u64(), 53, 59, 404, ReplValue::Int(61))
            .expect_err("missing recipient must fail"),
        "missing recipient process 404"
    );
    assert_eq!(
        runtime
            .service_native_send(owner.as_u64(), 53, 59, 0, ReplValue::Int(61))
            .expect_err("zero recipient must fail"),
        "native send recipient identity must be nonzero"
    );

    assert_eq!(runtime.pending_native_continuation_count(), 1);
    assert_eq!(
        runtime
            .processes()
            .get(recipient)
            .expect("recipient exists")
            .mailbox_len(),
        0
    );
    assert_eq!(
        runtime.processes().get(owner).expect("owner exists").state,
        VmProcessState::Suspended(VmProcessResumeState::Runnable)
    );
}

#[test]
fn native_receive_transition_consumes_typed_mailbox_value_before_resume() {
    let mut runtime = VmActorRuntime::default();
    let owner = runtime.spawn_root(source("native-receiver"));
    runtime
        .send(owner, owner, ReplValue::Int(67))
        .expect("queue native receive payload");
    runtime
        .park_native_continuation(owner.as_u64(), 71, 73)
        .expect("native receive continuation should park");

    assert_eq!(
        runtime
            .service_native_receive_int(owner.as_u64(), 71, 73)
            .expect("native receive should resume"),
        Some(67)
    );
    assert_eq!(runtime.pending_native_continuation_count(), 0);
    assert_eq!(
        runtime.processes().get(owner).expect("owner exists").state,
        VmProcessState::Runnable
    );
    assert_eq!(
        runtime
            .processes()
            .get(owner)
            .expect("owner exists")
            .mailbox_len(),
        0
    );
}

#[test]
fn native_receive_transition_retains_lease_and_nonmatching_mailbox_values() {
    let mut runtime = VmActorRuntime::default();
    let owner = runtime.spawn_root(source("native-receiver"));
    let foreign = runtime.spawn_root(source("foreign-receiver"));
    runtime
        .send(owner, owner, ReplValue::Bool(true))
        .expect("queue nonmatching payload");
    runtime
        .park_native_continuation(owner.as_u64(), 79, 83)
        .expect("native receive continuation should park");

    assert_eq!(
        runtime
            .service_native_receive_int(owner.as_u64(), 79, 83)
            .expect("empty typed receive should remain parked"),
        None
    );
    assert!(runtime
        .service_native_receive_int(foreign.as_u64(), 79, 83)
        .expect_err("foreign receive owner must fail")
        .contains("is owned by process"));
    assert_eq!(runtime.pending_native_continuation_count(), 1);
    assert_eq!(
        runtime
            .processes()
            .get(owner)
            .expect("owner exists")
            .mailbox_len(),
        1
    );
    assert_eq!(
        runtime.processes().get(owner).expect("owner exists").state,
        VmProcessState::Suspended(VmProcessResumeState::Runnable)
    );
}

#[test]
fn typed_native_mailbox_preserves_identity_and_commits_after_receiver_encoding() {
    let mut runtime = VmActorRuntime::default();
    let owner = runtime.spawn_root(source("typed-native-owner"));
    runtime
        .park_native_continuation(owner.as_u64(), 89, 97)
        .expect("typed send continuation should park");
    runtime
        .service_native_send_typed(
            owner.as_u64(),
            89,
            97,
            owner.as_u64(),
            ReplValue::String("owned message".to_string()),
            crate::runtime::native_image::TvmBoundaryType::String,
        )
        .expect("typed send should publish an owned value");
    runtime
        .park_native_continuation(owner.as_u64(), 101, 103)
        .expect("typed receive continuation should park");
    let encoded = runtime
        .service_native_receive_typed(
            owner.as_u64(),
            101,
            103,
            &crate::runtime::native_image::TvmBoundaryType::String,
            |payload| match payload {
                ReplValue::String(value) if value == "owned message" => Ok(107),
                other => Err(format!("unexpected typed payload {other:?}")),
            },
        )
        .expect("receiver conversion should commit");
    assert_eq!(encoded, Some(107));
    assert_eq!(runtime.pending_native_continuation_count(), 0);
    assert_eq!(
        runtime
            .processes()
            .get(owner)
            .expect("owner exists")
            .mailbox_len(),
        0
    );
}

#[test]
fn typed_native_receive_rolls_back_conversion_failure_and_skips_other_types() {
    let mut runtime = VmActorRuntime::default();
    let owner = runtime.spawn_root(source("typed-native-rollback"));
    runtime
        .send(owner, owner, ReplValue::String("untyped".to_string()))
        .expect("queue untyped lookalike");
    runtime
        .send_typed(
            owner,
            owner,
            ReplValue::String("typed".to_string()),
            crate::runtime::native_image::TvmBoundaryType::String,
        )
        .expect("queue typed payload");
    runtime
        .park_native_continuation(owner.as_u64(), 109, 113)
        .expect("typed receive continuation should park");

    assert_eq!(
        runtime
            .service_native_receive_typed(
                owner.as_u64(),
                109,
                113,
                &crate::runtime::native_image::TvmBoundaryType::String,
                |_| Err("receiver heap exhausted".to_string()),
            )
            .expect_err("failed receiver allocation must roll back"),
        "receiver heap exhausted"
    );
    assert_eq!(runtime.pending_native_continuation_count(), 1);
    assert_eq!(
        runtime
            .processes()
            .get(owner)
            .expect("owner exists")
            .mailbox_len(),
        2
    );
    assert_eq!(
        runtime
            .service_native_receive_typed(
                owner.as_u64(),
                109,
                113,
                &crate::runtime::native_image::TvmBoundaryType::String,
                |payload| match payload {
                    ReplValue::String(value) if value == "typed" => Ok(127),
                    other => Err(format!("unexpected typed payload {other:?}")),
                },
            )
            .expect("retry should consume the same typed message"),
        Some(127)
    );
    assert_eq!(
        runtime
            .processes()
            .get(owner)
            .expect("owner exists")
            .mailbox_len(),
        1
    );
}

#[test]
fn typed_native_receive_distinguishes_bytes_binary_and_atom_sidecars() {
    use crate::runtime::native_image::TvmBoundaryType;

    let mut runtime = VmActorRuntime::default();
    let owner = runtime.spawn_root(source("typed-native-families"));
    runtime
        .send_typed(
            owner,
            owner,
            ReplValue::Bytes(Arc::from(&b"same"[..])),
            TvmBoundaryType::Bytes,
        )
        .expect("queue typed Bytes");
    runtime
        .send_typed(
            owner,
            owner,
            ReplValue::BitString(VmBitString::from_bytes(b"same", 32).expect("aligned Binary")),
            TvmBoundaryType::Binary,
        )
        .expect("queue typed Binary");
    runtime
        .send_typed(
            owner,
            owner,
            ReplValue::Atom("ready".to_owned()),
            TvmBoundaryType::Atom,
        )
        .expect("queue typed Atom");

    for (request, continuation, boundary_type, expected) in [
        (131, 137, TvmBoundaryType::Binary, "binary"),
        (139, 149, TvmBoundaryType::Atom, "atom"),
        (151, 157, TvmBoundaryType::Bytes, "bytes"),
    ] {
        runtime
            .park_native_continuation(owner.as_u64(), request, continuation)
            .expect("typed receive continuation should park");
        let encoded = runtime
            .service_native_receive_typed(
                owner.as_u64(),
                request,
                continuation,
                &boundary_type,
                |payload| match (expected, payload) {
                    ("binary", ReplValue::BitString(value)) if value.bit_len() == 32 => Ok(1),
                    ("atom", ReplValue::Atom(value)) if value == "ready" => Ok(2),
                    ("bytes", ReplValue::Bytes(value)) if value.as_ref() == b"same" => Ok(3),
                    _ => Err(format!("unexpected {expected} payload {payload:?}")),
                },
            )
            .expect("exact typed receive should succeed");
        assert!(encoded.is_some(), "{expected} receive must match");
    }

    assert_eq!(runtime.pending_native_continuation_count(), 0);
    assert_eq!(
        runtime
            .processes()
            .get(owner)
            .expect("owner exists")
            .mailbox_len(),
        0
    );
}

#[test]
fn native_spawn_transition_creates_scheduled_child_before_parent_resume() {
    let mut runtime = VmActorRuntime::default();
    let owner = runtime.spawn_root(source("native-spawner"));
    runtime
        .park_native_continuation(owner.as_u64(), 89, 97)
        .expect("native spawn continuation should park");

    let child_id = runtime
        .service_native_spawn(owner.as_u64(), 89, 97, 101)
        .expect("native spawn should create child and resume");
    let child = VmProcessId::from_raw_for_test(child_id);

    assert_eq!(runtime.pending_native_continuation_count(), 0);
    assert!(runtime.is_alive(child));
    let child_process = runtime
        .processes()
        .get(child)
        .expect("spawned child exists");
    assert_eq!(child_process.parent, Some(owner));
    assert_eq!(child_process.source.module, "native.Image");
    assert_eq!(child_process.source.function, "entry_101");
    assert_eq!(child_process.state, VmProcessState::Runnable);
    assert_eq!(
        runtime.processes().get(owner).expect("owner exists").state,
        VmProcessState::Runnable
    );
}

#[test]
fn native_spawn_transition_rejects_invalid_ownership_without_child_creation() {
    let mut runtime = VmActorRuntime::default();
    let owner = runtime.spawn_root(source("native-spawner"));
    let foreign = runtime.spawn_root(source("foreign-spawner"));
    runtime
        .park_native_continuation(owner.as_u64(), 103, 107)
        .expect("native spawn continuation should park");
    let live_before = runtime.live_process_ids();

    assert!(runtime
        .service_native_spawn(foreign.as_u64(), 103, 107, 109)
        .expect_err("foreign spawn owner must fail")
        .contains("is owned by process"));
    assert_eq!(
        runtime
            .service_native_spawn(owner.as_u64(), 103, 107, 0)
            .expect_err("zero entry must fail"),
        "native spawn entry identity must be nonzero"
    );
    assert_eq!(runtime.live_process_ids(), live_before);
    assert_eq!(runtime.pending_native_continuation_count(), 1);
    assert_eq!(
        runtime.processes().get(owner).expect("owner exists").state,
        VmProcessState::Suspended(VmProcessResumeState::Runnable)
    );
}

#[test]
fn native_timer_transition_fires_before_exact_owner_resume() {
    let mut runtime = VmActorRuntime::default();
    let owner = runtime.spawn_root(source("native-sleeper"));
    runtime
        .park_native_continuation(owner.as_u64(), 113, 127)
        .expect("native timer continuation should park");

    let wait = runtime
        .begin_native_timer(owner.as_u64(), 113, 127, 3)
        .expect("native timer should start");
    assert!(wait.timer_id.as_u64() > 0);
    assert_eq!(wait.deadline_tick, 3);
    assert_eq!(runtime.pending_native_continuation_count(), 1);
    assert_eq!(
        runtime.processes().get(owner).expect("owner exists").state,
        VmProcessState::Suspended(VmProcessResumeState::Runnable)
    );

    runtime
        .complete_native_timer(owner.as_u64(), 113, 127, wait)
        .expect("deadline should fire before native resume");
    assert_eq!(runtime.pending_native_continuation_count(), 0);
    assert_eq!(
        runtime.processes().get(owner).expect("owner exists").state,
        VmProcessState::Runnable
    );
}

#[test]
fn native_timer_transition_rejects_invalid_ownership_without_wakeup() {
    let mut runtime = VmActorRuntime::default();
    let owner = runtime.spawn_root(source("native-sleeper"));
    let foreign = runtime.spawn_root(source("foreign-sleeper"));
    runtime
        .park_native_continuation(owner.as_u64(), 131, 137)
        .expect("native timer continuation should park");

    assert!(runtime
        .begin_native_timer(foreign.as_u64(), 131, 137, 3)
        .expect_err("foreign timer owner must fail")
        .contains("is owned by process"));
    assert_eq!(
        runtime
            .begin_native_timer(owner.as_u64(), 131, 137, 0)
            .expect_err("zero delay must fail"),
        "native timer delay must be positive"
    );
    let wait = runtime
        .begin_native_timer(owner.as_u64(), 131, 137, 3)
        .expect("valid timer should start");
    assert!(runtime
        .complete_native_timer(foreign.as_u64(), 131, 137, wait)
        .expect_err("foreign completion must fail")
        .contains("is owned by process"));
    assert_eq!(runtime.pending_native_continuation_count(), 1);
    assert_eq!(
        runtime.processes().get(owner).expect("owner exists").state,
        VmProcessState::Suspended(VmProcessResumeState::Runnable)
    );
    runtime
        .complete_native_timer(owner.as_u64(), 131, 137, wait)
        .expect("valid owner should complete timer");
}

#[test]
fn native_link_transition_creates_failure_relationship_before_owner_resume() {
    let mut runtime = VmActorRuntime::default();
    let owner = runtime.spawn_root(source("native-linker"));
    let peer = runtime.spawn_root(source("native-peer"));
    runtime
        .park_native_continuation(owner.as_u64(), 139, 149)
        .expect("native link continuation should park");

    assert!(runtime
        .service_native_link(owner.as_u64(), 139, 149, peer.as_u64())
        .expect("native link should be created"));
    assert_eq!(runtime.pending_native_continuation_count(), 0);
    assert_eq!(
        runtime
            .failure_snapshot(owner)
            .expect("owner relationships")
            .links,
        [peer]
    );
    assert_eq!(
        runtime.processes().get(owner).expect("owner exists").state,
        VmProcessState::Runnable
    );

    runtime
        .exit_actor(peer, VmExitReason::Killed)
        .expect("peer abnormal exit should propagate");
    assert_eq!(
        runtime.processes().get(owner).expect("owner exists").state,
        VmProcessState::Exited(VmExitReason::Killed)
    );
}

#[test]
fn native_link_transition_rejects_invalid_ownership_without_relationship_mutation() {
    let mut runtime = VmActorRuntime::default();
    let owner = runtime.spawn_root(source("native-linker"));
    let peer = runtime.spawn_root(source("native-peer"));
    let foreign = runtime.spawn_root(source("foreign-linker"));
    runtime
        .park_native_continuation(owner.as_u64(), 151, 157)
        .expect("native link continuation should park");

    assert!(runtime
        .service_native_link(foreign.as_u64(), 151, 157, peer.as_u64())
        .expect_err("foreign link owner must fail")
        .contains("is owned by process"));
    assert_eq!(
        runtime
            .service_native_link(owner.as_u64(), 151, 157, owner.as_u64())
            .expect_err("self link must fail"),
        format!("cannot link process {} to itself", owner.as_u64())
    );
    assert_eq!(
        runtime
            .service_native_link(owner.as_u64(), 151, 157, 404)
            .expect_err("missing peer must fail"),
        "cannot link missing process 404"
    );
    assert!(runtime
        .failure_snapshot(owner)
        .expect("owner relationships")
        .links
        .is_empty());
    assert_eq!(runtime.pending_native_continuation_count(), 1);
    assert_eq!(
        runtime.processes().get(owner).expect("owner exists").state,
        VmProcessState::Suspended(VmProcessResumeState::Runnable)
    );
}

#[test]
fn native_monitor_transition_allocates_reference_before_down_delivery() {
    let mut runtime = VmActorRuntime::default();
    let owner = runtime.spawn_root(source("native-monitor"));
    let target = runtime.spawn_root(source("native-target"));
    runtime
        .park_native_continuation(owner.as_u64(), 163, 167)
        .expect("native monitor continuation should park");

    let monitor_ref = runtime
        .service_native_monitor(owner.as_u64(), 163, 167, target.as_u64())
        .expect("native monitor should be created");
    assert_eq!(monitor_ref, 1);
    assert_eq!(runtime.pending_native_continuation_count(), 0);
    let snapshot = runtime
        .failure_snapshot(owner)
        .expect("owner relationships");
    assert_eq!(snapshot.monitoring.len(), 1);
    assert_eq!(snapshot.monitoring[0].monitor_ref.as_u64(), monitor_ref);
    assert_eq!(snapshot.monitoring[0].peer, target);

    runtime
        .exit_actor(target, VmExitReason::Killed)
        .expect("target exit should deliver DOWN");
    let VmActorReceive::Message(message) = runtime
        .receive_next_or_block(owner)
        .expect("receive native monitor completion")
    else {
        panic!("native monitor must deliver a DOWN message");
    };
    assert_eq!(
        message.payload,
        ReplValue::Tuple(vec![
            ReplValue::Atom("down".to_string()),
            ReplValue::Int(monitor_ref as i64),
            ReplValue::Int(target.as_u64() as i64),
            ReplValue::Atom("killed".to_string()),
        ])
    );
}

#[test]
fn native_monitor_transition_rejects_missing_targets_before_reference_allocation() {
    let mut runtime = VmActorRuntime::default();
    let owner = runtime.spawn_root(source("native-monitor"));
    let target = runtime.spawn_root(source("native-target"));
    let foreign = runtime.spawn_root(source("foreign-monitor"));
    runtime
        .park_native_continuation(owner.as_u64(), 173, 179)
        .expect("native monitor continuation should park");

    assert!(runtime
        .service_native_monitor(foreign.as_u64(), 173, 179, target.as_u64())
        .expect_err("foreign monitor owner must fail")
        .contains("is owned by process"));
    assert_eq!(
        runtime
            .service_native_monitor(owner.as_u64(), 173, 179, 404)
            .expect_err("missing monitor target must fail"),
        "cannot monitor missing process 404"
    );
    assert!(runtime
        .failure_snapshot(owner)
        .expect("owner relationships")
        .monitoring
        .is_empty());
    assert_eq!(runtime.pending_native_continuation_count(), 1);
    assert_eq!(
        runtime
            .service_native_monitor(owner.as_u64(), 173, 179, target.as_u64())
            .expect("first valid monitor should allocate first reference"),
        1
    );
}

#[test]
fn native_resource_transition_registers_owned_handle_and_cleans_up_on_exit() {
    let mut runtime = VmActorRuntime::default();
    let owner = runtime.spawn_root(source("native-resource"));
    runtime
        .park_native_continuation(owner.as_u64(), 181, 191)
        .expect("native resource continuation should park");

    let resource_id = runtime
        .service_native_resource(owner.as_u64(), 181, 191, 7)
        .expect("native resource should register");
    assert_eq!(resource_id, 1);
    assert_eq!(runtime.pending_native_continuation_count(), 0);
    let snapshots = runtime.resource_snapshots();
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].id.as_u64(), resource_id);
    assert_eq!(snapshots[0].owner, owner);
    assert_eq!(snapshots[0].kind, "native.scalar");
    assert_eq!(snapshots[0].label, "tag_7");
    assert_eq!(
        snapshots[0].transfer_policy,
        crate::runtime::vm::resource::VmResourceTransferPolicy::OwnerOnly
    );
    assert_eq!(
        runtime
            .processes()
            .get(owner)
            .expect("resource owner")
            .resource_handles,
        ["resource:1"]
    );

    assert_eq!(
        runtime
            .exit_actor(owner, VmExitReason::Normal)
            .expect("resource owner exit"),
        ["resource:1"]
    );
    assert!(runtime.resource_snapshots().is_empty());
}

#[test]
fn native_resource_transition_rejects_invalid_authority_before_allocation() {
    let mut runtime = VmActorRuntime::default();
    let owner = runtime.spawn_root(source("native-resource"));
    let foreign = runtime.spawn_root(source("foreign-resource"));
    runtime
        .park_native_continuation(owner.as_u64(), 193, 197)
        .expect("native resource continuation should park");

    assert!(runtime
        .service_native_resource(foreign.as_u64(), 193, 197, 7)
        .expect_err("foreign resource owner must fail")
        .contains("is owned by process"));
    assert_eq!(
        runtime
            .service_native_resource(owner.as_u64(), 193, 197, 0)
            .expect_err("zero resource kind must fail"),
        "native resource kind tag must be positive"
    );
    assert!(runtime.resource_snapshots().is_empty());
    assert_eq!(runtime.pending_native_continuation_count(), 1);
    assert_eq!(
        runtime
            .service_native_resource(owner.as_u64(), 193, 197, 7)
            .expect("first valid resource should allocate first identity"),
        1
    );
}

#[test]
fn native_cancellation_records_target_before_resuming_exact_owner() {
    let mut runtime = VmActorRuntime::default();
    let owner = runtime.spawn_root(source("native-canceller"));
    let target = runtime.spawn_root(source("native-target"));
    runtime
        .park_native_continuation(owner.as_u64(), 199, 211)
        .expect("native cancellation continuation should park");

    runtime
        .service_native_cancellation(owner.as_u64(), 199, 211, target.as_u64())
        .expect("native cancellation should be recorded");
    assert_eq!(runtime.pending_native_continuation_count(), 0);
    assert!(
        runtime
            .processes()
            .get(target)
            .expect("cancellation target")
            .cancellation_requested
    );
    let cancelled = runtime
        .run_next(|_, _| panic!("cancelled target must not run a slice"))
        .expect("scheduler should apply cancellation");
    assert_eq!(cancelled.pid, Some(target));
    assert_eq!(cancelled.outcome, VmSchedulerOutcome::Cancelled(Vec::new()));
    assert_eq!(
        runtime
            .processes()
            .get(target)
            .expect("cancelled target")
            .state,
        VmProcessState::Exited(VmExitReason::Killed)
    );
}

#[test]
fn native_cancellation_rejects_invalid_authority_before_target_mutation() {
    let mut runtime = VmActorRuntime::default();
    let owner = runtime.spawn_root(source("native-canceller"));
    let foreign = runtime.spawn_root(source("foreign-canceller"));
    let target = runtime.spawn_root(source("native-target"));
    runtime
        .park_native_continuation(owner.as_u64(), 223, 227)
        .expect("native cancellation continuation should park");

    assert!(runtime
        .service_native_cancellation(foreign.as_u64(), 223, 227, target.as_u64())
        .expect_err("foreign cancellation owner must fail")
        .contains("is owned by process"));
    assert!(
        !runtime
            .processes()
            .get(target)
            .expect("unchanged cancellation target")
            .cancellation_requested
    );
    assert_eq!(
        runtime
            .service_native_cancellation(owner.as_u64(), 223, 227, 9999)
            .expect_err("missing cancellation target must fail"),
        "cannot cancel missing process 9999"
    );
    assert_eq!(runtime.pending_native_continuation_count(), 1);
    assert!(
        !runtime
            .processes()
            .get(target)
            .expect("unchanged cancellation target")
            .cancellation_requested
    );
}

#[test]
fn native_self_cancellation_wins_before_resume_boundary() {
    let mut runtime = VmActorRuntime::default();
    let owner = runtime.spawn_root(source("native-self-canceller"));
    runtime
        .park_native_continuation(owner.as_u64(), 229, 233)
        .expect("native cancellation continuation should park");

    runtime
        .service_native_cancellation(owner.as_u64(), 229, 233, owner.as_u64())
        .expect("self cancellation should be recorded");
    assert!(runtime
        .enforce_native_cancellation_boundary(owner.as_u64())
        .expect("native cancellation boundary should apply"));
    assert_eq!(runtime.pending_native_continuation_count(), 0);
    assert_eq!(
        runtime
            .processes()
            .get(owner)
            .expect("cancelled owner")
            .state,
        VmProcessState::Exited(VmExitReason::Killed)
    );
}
