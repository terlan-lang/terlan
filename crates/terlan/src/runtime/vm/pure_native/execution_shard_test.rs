//! Tests for ordinary same-shard native actor dispatch.

use std::sync::Arc;

use crate::runtime::native_image::control::{TvmControlFrame, TvmTransitionOperation};
use crate::runtime::native_image::managed::{
    AllocationClass, ManagedTypeDescriptor, SemanticTypeId,
};
use crate::runtime::native_image::{TvmBoundaryType, TvmContinuationDescriptor};
use crate::runtime::vm::execution_shard_protocol::VmShardEpoch;
use crate::runtime::vm::execution_shard_supervisor::VmShardPhase;
use crate::runtime::vm::process::{
    VmExitReason, VmProcessResumeState, VmProcessSource, VmProcessState,
};
use crate::runtime::vm::ReplValue;

use super::{NativeShardDispatchEvent, PureNativeExecutionShard};
use crate::runtime::vm::pure_native::{
    NativeImageBackend, PureNativeBoundary, PureNativeExecution, PureNativeExecutionRuntime,
    PureNativeExportSpec, ResolvedPureArtifact, VmNativeGenerationReferenceClass,
};

impl PureNativeExecutionShard {
    /// Creates a shard around one admitted boundary.
    fn with_boundary(boundary: PureNativeBoundary) -> Self {
        let execution = PureNativeExecutionRuntime::runtime_default()
            .expect("default native execution limits must be valid");
        Self::with_boundary_and_execution(
            boundary,
            execution,
            crate::runtime::vm::execution_shard_protocol::VmExecutionShardId::new(
                "test-native-shard",
            )
            .expect("test shard identity"),
        )
        .expect("test shard admission")
    }

    /// Returns the ordered direct-dispatch trace for inspection and tests.
    fn dispatch_trace(&self) -> &[NativeShardDispatchEvent] {
        &self.trace
    }

    /// Returns the shard-owned actor runtime for VM inspection.
    fn actors(&self) -> &crate::runtime::vm::actor::VmActorRuntime {
        &self.actors
    }

    /// Returns mutable actor services to direct-shard lifecycle tests.
    fn actors_mut(&mut self) -> &mut crate::runtime::vm::actor::VmActorRuntime {
        &mut self.actors
    }

    /// Returns the number of materialized actor heaps retained by this shard.
    fn managed_actor_count(&self) -> usize {
        self.execution.managed_ref().actor_count()
    }

    /// Returns the active supervisor phase for lifecycle inspection.
    const fn lifecycle_phase(&self) -> VmShardPhase {
        self.supervisor.phase()
    }

    /// Returns the admitted supervisor epoch for lifecycle inspection.
    pub(crate) const fn lifecycle_epoch(&self) -> Option<VmShardEpoch> {
        self.supervisor.epoch()
    }

    /// Returns restart budget consumption for lifecycle inspection.
    const fn restart_count(&self) -> u32 {
        self.supervisor.restart_count()
    }

    /// Returns the admitted sealed-image identity for lifecycle inspection.
    fn lifecycle_image_identity(&self) -> Option<&str> {
        self.supervisor
            .image()
            .map(crate::runtime::vm::execution_shard_protocol::VmSealedShardImage::identity)
    }
}

/// In-process backend that exercises send, receive, yield, and resume without
/// any worker protocol.
#[derive(Debug, Default)]
struct LocalTransitionBackend;

impl NativeImageBackend for LocalTransitionBackend {
    fn call_frame(
        &mut self,
        context: &mut crate::runtime::vm::pure_native::PureNativeExecutionContext<'_>,
        request_id: u64,
        _export_id: u64,
        _args: &[ReplValue],
    ) -> Result<TvmControlFrame, String> {
        let owner_id = context.owner_id();
        Ok(TvmControlFrame::Transition {
            request_id,
            owner_id,
            continuation_id: 11,
            operation: TvmTransitionOperation::Send,
            arguments: vec![owner_id as i64, 42],
            values: Vec::new(),
        })
    }

    fn resume_frame(
        &mut self,
        context: &mut crate::runtime::vm::pure_native::PureNativeExecutionContext<'_>,
        request_id: u64,
        continuation_id: u64,
        values: Vec<i64>,
    ) -> Result<TvmControlFrame, String> {
        let owner_id = context.owner_id();
        match (continuation_id, values.as_slice()) {
            (11, []) => Ok(TvmControlFrame::Transition {
                request_id,
                owner_id,
                continuation_id: 12,
                operation: TvmTransitionOperation::Receive,
                arguments: Vec::new(),
                values: Vec::new(),
            }),
            (12, [42]) => Ok(TvmControlFrame::Transition {
                request_id,
                owner_id,
                continuation_id: 13,
                operation: TvmTransitionOperation::Yield,
                arguments: Vec::new(),
                values: Vec::new(),
            }),
            (13, []) => Ok(TvmControlFrame::Success {
                request_id,
                owner_id,
                value: 1,
            }),
            _ => Err("unexpected local transition resume".to_string()),
        }
    }

    fn shutdown(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn shutdown_owner(
        &mut self,
        context: &mut crate::runtime::vm::pure_native::PureNativeExecutionContext<'_>,
    ) -> Result<(), String> {
        let owner_id = context.owner_id();
        context.managed().release_owner(owner_id);
        Ok(())
    }

    fn fork_box(&self) -> Result<Box<dyn NativeImageBackend>, String> {
        Ok(Box::<Self>::default())
    }
}

/// Creates one admitted boundary around the in-process transition backend.
fn local_boundary() -> PureNativeBoundary {
    local_boundary_named("local-transition-image", 1)
}

/// Creates the local transition fixture with distinct sealed image metadata.
fn local_boundary_named(image_identity: &str, digest: u8) -> PureNativeBoundary {
    PureNativeBoundary {
        artifact: Some(ResolvedPureArtifact {
            image_identity: image_identity.to_string(),
            descriptor_digest: [digest; 32],
            exports: vec![PureNativeExportSpec {
                id: 7,
                module: "local.Shard".to_string(),
                function: "round_trip".to_string(),
                arity: 0,
                parameters: Vec::new(),
                result: TvmBoundaryType::Bool,
            }],
            continuations: vec![
                TvmContinuationDescriptor {
                    id: 11,
                    parameters: Vec::new(),
                    results: vec![TvmBoundaryType::Bool],
                },
                TvmContinuationDescriptor {
                    id: 12,
                    parameters: vec![TvmBoundaryType::Int],
                    results: vec![TvmBoundaryType::Bool],
                },
                TvmContinuationDescriptor {
                    id: 13,
                    parameters: Vec::new(),
                    results: vec![TvmBoundaryType::Bool],
                },
            ],
        }),
        backend: Some(Box::<LocalTransitionBackend>::default()),
    }
}

/// In-process backend that retains actor-owned state before failing a resume.
#[derive(Debug, Default)]
struct FailingResumeBackend;

impl NativeImageBackend for FailingResumeBackend {
    fn call_frame(
        &mut self,
        context: &mut crate::runtime::vm::pure_native::PureNativeExecutionContext<'_>,
        request_id: u64,
        _export_id: u64,
        _args: &[ReplValue],
    ) -> Result<TvmControlFrame, String> {
        let owner_id = context.owner_id();
        let descriptor = Arc::new(
            ManagedTypeDescriptor::new(
                SemanticTypeId::from_canonical("local.FailureState")
                    .expect("managed semantic identity"),
                8,
                8,
                Vec::new(),
                AllocationClass::Young,
            )
            .expect("managed failure-state descriptor"),
        );
        context
            .managed()
            .with_public_allocation(owner_id, |heap, _layouts| {
                heap.allocate::<u64>(descriptor, &[7; 8], &[])
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            })?;
        Ok(TvmControlFrame::Transition {
            request_id,
            owner_id,
            continuation_id: 21,
            operation: TvmTransitionOperation::Send,
            arguments: vec![owner_id as i64, 99],
            values: Vec::new(),
        })
    }

    fn resume_frame(
        &mut self,
        context: &mut crate::runtime::vm::pure_native::PureNativeExecutionContext<'_>,
        request_id: u64,
        continuation_id: u64,
        _values: Vec<i64>,
    ) -> Result<TvmControlFrame, String> {
        match continuation_id {
            21 => Ok(TvmControlFrame::Transition {
                request_id,
                owner_id: context.owner_id(),
                continuation_id: 22,
                operation: TvmTransitionOperation::Yield,
                arguments: Vec::new(),
                values: Vec::new(),
            }),
            22 => Err("forced direct resume failure".to_string()),
            _ => Err("unexpected failure-backend continuation".to_string()),
        }
    }

    fn shutdown(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn shutdown_owner(
        &mut self,
        context: &mut crate::runtime::vm::pure_native::PureNativeExecutionContext<'_>,
    ) -> Result<(), String> {
        let owner_id = context.owner_id();
        context.managed().release_owner(owner_id);
        Ok(())
    }

    fn fork_box(&self) -> Result<Box<dyn NativeImageBackend>, String> {
        Ok(Box::<Self>::default())
    }
}

/// Creates one admitted boundary whose second resume fails after retaining state.
fn failing_boundary() -> PureNativeBoundary {
    PureNativeBoundary {
        artifact: Some(ResolvedPureArtifact {
            image_identity: "failing-resume-image".to_string(),
            descriptor_digest: [2; 32],
            exports: vec![PureNativeExportSpec {
                id: 9,
                module: "local.Shard".to_string(),
                function: "fail_after_send".to_string(),
                arity: 0,
                parameters: Vec::new(),
                result: TvmBoundaryType::Bool,
            }],
            continuations: vec![
                TvmContinuationDescriptor {
                    id: 21,
                    parameters: Vec::new(),
                    results: vec![TvmBoundaryType::Bool],
                },
                TvmContinuationDescriptor {
                    id: 22,
                    parameters: Vec::new(),
                    results: vec![TvmBoundaryType::Bool],
                },
            ],
        }),
        backend: Some(Box::<FailingResumeBackend>::default()),
    }
}

/// Verifies ordinary entry and every resume remain inside one actor shard and
/// complete through VM-owned transition services.
#[test]
fn local_actor_entry_and_resume_never_use_worker_transport() {
    let mut shard = PureNativeExecutionShard::with_boundary(local_boundary());

    assert_eq!(
        shard
            .call("local.Shard.round_trip", &[])
            .expect("same-shard native call"),
        ReplValue::Bool(true)
    );
    assert_eq!(shard.completed_call_count(), 1);
    assert_eq!(shard.actors().pending_native_continuation_count(), 0);
    let owner = match shard.dispatch_trace()[0] {
        NativeShardDispatchEvent::Entry { owner } => owner,
        event => panic!("expected entry event, found {event:?}"),
    };
    assert_eq!(
        shard.dispatch_trace(),
        [
            NativeShardDispatchEvent::Entry { owner },
            NativeShardDispatchEvent::Resume {
                owner,
                continuation_id: 11,
            },
            NativeShardDispatchEvent::Resume {
                owner,
                continuation_id: 12,
            },
            NativeShardDispatchEvent::Resume {
                owner,
                continuation_id: 13,
            },
            NativeShardDispatchEvent::Complete { owner },
        ]
    );
    assert!(matches!(
        shard
            .actors()
            .processes()
            .get(owner)
            .expect("completed local actor")
            .state,
        VmProcessState::Exited(VmExitReason::Normal)
    ));
}

/// Verifies entry rejection exits the allocated actor without recording a
/// completed call.
#[test]
fn rejected_entry_releases_its_local_actor() {
    let mut shard = PureNativeExecutionShard::with_boundary(local_boundary());

    let error = shard
        .begin_call("local.Shard.missing", &[])
        .expect_err("missing entry must fail");
    assert!(error.contains("error[pure_native_export_missing]"));
    assert_eq!(shard.completed_call_count(), 0);
    let owner = match shard.dispatch_trace() {
        [NativeShardDispatchEvent::Entry { owner }] => *owner,
        trace => panic!("expected only a rejected entry event, found {trace:?}"),
    };
    assert!(matches!(
        shard
            .actors()
            .processes()
            .get(owner)
            .expect("rejected local actor")
            .state,
        VmProcessState::Exited(VmExitReason::Error(ref reason))
            if reason.contains("error[pure_native_export_missing]")
    ));
}

/// Verifies the shard rejects foreign resume authority before recording a
/// dispatch or mutating either actor's scheduler lease.
#[test]
fn foreign_resume_owner_cannot_consume_or_fail_another_actor() {
    let mut shard = PureNativeExecutionShard::with_boundary(local_boundary());
    let (owner, owner_execution) = shard
        .begin_call("local.Shard.round_trip", &[])
        .expect("owner entry");
    let PureNativeExecution::Suspended(owner_suspension) = owner_execution else {
        panic!("owner must suspend");
    };
    let (foreign, foreign_execution) = shard
        .begin_call("local.Shard.round_trip", &[])
        .expect("foreign entry");
    let PureNativeExecution::Suspended(_foreign_suspension) = foreign_execution else {
        panic!("foreign actor must suspend");
    };
    let trace_len = shard.dispatch_trace().len();

    assert_eq!(
        shard
            .resume_call(foreign, owner_suspension)
            .expect_err("foreign owner must be rejected"),
        format!(
            "error[pure_native_owner]: actor {} cannot resume owner {}",
            foreign.as_u64(),
            owner.as_u64()
        )
    );
    assert_eq!(shard.dispatch_trace().len(), trace_len);
    assert_eq!(shard.actors().pending_native_continuation_count(), 2);
    for process in [owner, foreign] {
        assert_eq!(
            shard
                .actors()
                .processes()
                .get(process)
                .expect("parked process")
                .state,
            VmProcessState::Suspended(VmProcessResumeState::Runnable)
        );
    }

    shard
        .finish_owner(owner, VmExitReason::Killed)
        .expect("release owner fixture");
    shard
        .finish_owner(foreign, VmExitReason::Killed)
        .expect("release foreign fixture");
    assert_eq!(shard.actors().pending_native_continuation_count(), 0);
}

/// Verifies two actors can interleave every direct continuation on one shard
/// without sharing request, mailbox, or completion state.
#[test]
fn actor_continuations_interleave_reentrantly_on_one_shard() {
    let mut shard = PureNativeExecutionShard::with_boundary(local_boundary());
    let (first_owner, first_send) = shard
        .begin_call("local.Shard.round_trip", &[])
        .expect("first actor entry");
    let PureNativeExecution::Suspended(first_send) = first_send else {
        panic!("first actor must suspend at send");
    };
    let (second_owner, second_send) = shard
        .begin_call("local.Shard.round_trip", &[])
        .expect("second actor entry");
    let PureNativeExecution::Suspended(second_send) = second_send else {
        panic!("second actor must suspend at send");
    };

    let PureNativeExecution::Suspended(second_receive) = shard
        .resume_call(second_owner, second_send)
        .expect("second actor send")
    else {
        panic!("second actor must suspend at receive");
    };
    let PureNativeExecution::Suspended(first_receive) = shard
        .resume_call(first_owner, first_send)
        .expect("first actor send")
    else {
        panic!("first actor must suspend at receive");
    };
    let PureNativeExecution::Suspended(first_yield) = shard
        .resume_call(first_owner, first_receive)
        .expect("first actor receive")
    else {
        panic!("first actor must suspend at yield");
    };
    let PureNativeExecution::Suspended(second_yield) = shard
        .resume_call(second_owner, second_receive)
        .expect("second actor receive")
    else {
        panic!("second actor must suspend at yield");
    };
    let PureNativeExecution::Complete(second_value) = shard
        .resume_call(second_owner, second_yield)
        .expect("second actor completion")
    else {
        panic!("second actor must complete");
    };
    let PureNativeExecution::Complete(first_value) = shard
        .resume_call(first_owner, first_yield)
        .expect("first actor completion")
    else {
        panic!("first actor must complete");
    };

    assert_eq!(first_value, ReplValue::Bool(true));
    assert_eq!(second_value, ReplValue::Bool(true));
    assert_eq!(shard.completed_call_count(), 2);
    assert_eq!(shard.actors().pending_native_continuation_count(), 0);
    shard
        .finish_owner(first_owner, VmExitReason::Normal)
        .expect("release first actor");
    shard
        .finish_owner(second_owner, VmExitReason::Normal)
        .expect("release second actor");
}

/// Verifies independently mutable shard forks can execute concurrently while
/// sharing only immutable admitted backend code.
#[test]
fn empty_shard_forks_execute_concurrently_without_shared_state() {
    let template = PureNativeExecutionShard::with_boundary(local_boundary());
    let mut first = template.fork_empty().expect("first empty shard");
    let mut second = template.fork_empty().expect("second empty shard");

    let first = std::thread::spawn(move || first.call("local.Shard.round_trip", &[]));
    let second = std::thread::spawn(move || second.call("local.Shard.round_trip", &[]));

    assert_eq!(
        first.join().expect("first shard thread"),
        Ok(ReplValue::Bool(true))
    );
    assert_eq!(
        second.join().expect("second shard thread"),
        Ok(ReplValue::Bool(true))
    );
    assert_eq!(template.completed_call_count(), 0);
}

/// Verifies active calls are admitted only while the supervised image is ready.
#[test]
fn active_shard_admission_and_shutdown_follow_supervisor_lifecycle() {
    let mut shard = PureNativeExecutionShard::with_boundary(local_boundary());
    assert_eq!(shard.lifecycle_phase(), VmShardPhase::Ready);
    assert_eq!(shard.lifecycle_epoch().map(VmShardEpoch::as_u64), Some(1));
    assert_eq!(
        shard.lifecycle_image_identity(),
        Some("local-transition-image")
    );
    let support = shard
        .native_support_bundle()
        .expect("capture native support bundle");
    assert_eq!(support.native_image.continuation_ids, [11, 12, 13]);
    assert_eq!(support.native_image.generation_epoch, 1);
    assert!(support.native_image.generation_quiescent);
    let support_json = String::from_utf8(
        support
            .serialized_bytes()
            .expect("serialize native support bundle"),
    )
    .expect("support bundle JSON");
    assert!(support_json.contains("local-transition-image"));
    assert!(!support_json.contains("CoreIR"));
    assert!(!support_json.contains("instructions"));
    shard.shutdown().expect("graceful supervised shutdown");
    assert_eq!(shard.lifecycle_phase(), VmShardPhase::Stopped);
    assert!(shard
        .call("local.Shard.round_trip", &[])
        .expect_err("stopped shard cannot execute")
        .contains("requires Ready, found Stopped"));
}

/// Verifies deliberate image replacement preserves shard supervision and advances epoch.
#[test]
fn active_shard_replacement_drains_and_publishes_the_next_epoch() {
    let mut shard = PureNativeExecutionShard::with_boundary(local_boundary());
    assert_eq!(
        shard
            .call("local.Shard.round_trip", &[])
            .expect("original image call"),
        ReplValue::Bool(true)
    );
    let replacement = shard
        .replace_components(
            local_boundary_named("local-transition-image-v2", 8),
            PureNativeExecutionRuntime::runtime_default().expect("replacement runtime"),
        )
        .expect("replace admitted image");
    assert_eq!(replacement.as_u64(), 2);
    assert_eq!(shard.lifecycle_phase(), VmShardPhase::Ready);
    assert_eq!(shard.restart_count(), 0);
    assert_eq!(
        shard.lifecycle_image_identity(),
        Some("local-transition-image-v2")
    );
    assert_eq!(
        shard
            .call("local.Shard.round_trip", &[])
            .expect("replacement image call"),
        ReplValue::Bool(true)
    );
}

#[test]
fn replacement_rejects_duplicate_image_generation_before_drain() {
    let mut shard = PureNativeExecutionShard::with_boundary(local_boundary());
    let error = shard
        .replace_components(
            local_boundary(),
            PureNativeExecutionRuntime::runtime_default().expect("candidate runtime"),
        )
        .expect_err("duplicate generation must fail");
    assert!(error.contains("execution_shard.duplicate_generation"));
    assert_eq!(shard.lifecycle_phase(), VmShardPhase::Ready);
    assert_eq!(
        shard.generation().expect("unchanged generation").as_u64(),
        1
    );
}

/// Draining rejects new routes while allowing an admitted continuation to resume.
#[test]
fn draining_generation_closes_entries_and_preserves_accepted_continuations() {
    let mut shard = PureNativeExecutionShard::with_boundary(local_boundary());
    let (owner, execution) = shard
        .begin_call("local.Shard.round_trip", &[])
        .expect("begin accepted call");
    let PureNativeExecution::Suspended(suspension) = execution else {
        panic!("round trip must suspend before completion");
    };
    let epoch = shard.lifecycle_epoch().expect("active epoch");
    shard.supervisor.begin_drain(epoch).expect("begin drain");

    assert!(shard
        .begin_call("local.Shard.round_trip", &[])
        .expect_err("draining generation rejects new entry")
        .contains("requires Ready, found Draining"));
    assert!(matches!(
        shard
            .resume_call(owner, suspension)
            .expect("accepted continuation resumes while draining"),
        PureNativeExecution::Suspended(_)
    ));
    let references = shard.generation_references();
    assert!(references.count(VmNativeGenerationReferenceClass::NativeFrame) > 0);
    assert!(references.count(VmNativeGenerationReferenceClass::ParkedContinuation) > 0);
}

/// Verifies crash recovery honors backoff and publishes a fresh image epoch.
#[test]
fn active_shard_crash_recovery_rejects_early_restart_and_stale_execution() {
    let mut shard = PureNativeExecutionShard::with_boundary(local_boundary());
    shard
        .report_crash("forced active shard crash", 100)
        .expect("record active crash");
    assert_eq!(shard.lifecycle_phase(), VmShardPhase::RestartBackoff);
    assert_eq!(shard.restart_count(), 1);
    let crash = shard
        .supervisor
        .last_crash()
        .expect("native crash metadata");
    let native_image = crash
        .native_image
        .as_ref()
        .expect("admitted image metadata");
    assert_eq!(native_image.image_identity, "local-transition-image");
    assert_eq!(native_image.continuation_ids, [11, 12, 13]);
    assert_eq!(native_image.generation_epoch, 1);
    assert!(shard
        .call("local.Shard.round_trip", &[])
        .expect_err("crashed shard cannot execute")
        .contains("requires Ready, found RestartBackoff"));
    assert!(shard
        .recover_components(
            local_boundary_named("too-early-recovery", 9),
            PureNativeExecutionRuntime::runtime_default().expect("early recovery runtime"),
            109,
        )
        .expect_err("restart before deadline must fail")
        .contains("RestartBackoffActive"));

    let recovered = shard
        .recover_components(
            local_boundary_named("recovered-transition-image", 10),
            PureNativeExecutionRuntime::runtime_default().expect("recovered runtime"),
            110,
        )
        .expect("recover at deadline");
    assert_eq!(recovered.as_u64(), 2);
    assert_eq!(shard.lifecycle_phase(), VmShardPhase::Ready);
    assert_eq!(
        shard.lifecycle_image_identity(),
        Some("recovered-transition-image")
    );
    assert_eq!(
        shard
            .call("local.Shard.round_trip", &[])
            .expect("recovered image call"),
        ReplValue::Bool(true)
    );
}

/// Verifies a post-entry direct-path failure uses the unified actor exit path
/// and releases every owner-scoped scheduler, mailbox, and managed-memory lease.
#[test]
fn resume_failure_propagates_and_releases_all_direct_path_ownership() {
    let mut shard = PureNativeExecutionShard::with_boundary(failing_boundary());
    let (owner, first) = shard
        .begin_call("local.Shard.fail_after_send", &[])
        .expect("failure fixture entry");
    let PureNativeExecution::Suspended(first) = first else {
        panic!("failure fixture must suspend at send");
    };
    assert_eq!(shard.managed_actor_count(), 1);

    let second = shard
        .resume_call(owner, first)
        .expect("self-send must reach second suspension");
    let PureNativeExecution::Suspended(second) = second else {
        panic!("failure fixture must suspend at yield");
    };
    let owner_process = shard
        .actors()
        .processes()
        .get(owner)
        .expect("owner process");
    assert_eq!(
        owner_process.state,
        VmProcessState::Suspended(VmProcessResumeState::Runnable)
    );
    assert_eq!(owner_process.mailbox_len(), 1);
    assert!(
        owner_process
            .mailbox_accounted_bytes()
            .expect("mailbox accounting")
            > 0
    );
    assert_eq!(shard.actors().pending_native_continuation_count(), 1);

    let linked = shard
        .actors_mut()
        .spawn_root(VmProcessSource::new("local.Shard", "linked", 0));
    let watcher = shard
        .actors_mut()
        .spawn_root(VmProcessSource::new("local.Shard", "watcher", 0));
    shard
        .actors_mut()
        .link_actors(owner, linked)
        .expect("link failure peer");
    shard
        .actors_mut()
        .monitor_actor(watcher, owner)
        .expect("monitor failure owner");

    let error = shard
        .resume_call(owner, second)
        .expect_err("forced backend failure must escape");
    assert_eq!(error, "forced direct resume failure");
    let reason = VmExitReason::Error(error);
    for exited in [owner, linked] {
        let process = shard
            .actors()
            .processes()
            .get(exited)
            .expect("exited process");
        assert_eq!(process.state, VmProcessState::Exited(reason.clone()));
    }
    let owner_process = shard
        .actors()
        .processes()
        .get(owner)
        .expect("owner process");
    assert_eq!(owner_process.mailbox_len(), 0);
    assert_eq!(owner_process.mailbox_accounted_bytes(), Ok(0));
    assert_eq!(
        shard
            .actors()
            .memory_metrics(owner)
            .expect("owner memory accounting")
            .current_bytes,
        0
    );
    assert_eq!(shard.actors().pending_native_continuation_count(), 0);
    assert_eq!(shard.managed_actor_count(), 0);
    let fatal = shard
        .actors()
        .latest_fatal_diagnostic()
        .expect("native fatal diagnostic");
    let native_image = fatal
        .native_image
        .as_ref()
        .expect("fatal diagnostic image metadata");
    assert_eq!(native_image.image_identity, "failing-resume-image");
    assert_eq!(native_image.continuation_ids, [21, 22]);
    assert_eq!(native_image.generation_epoch, 1);
    assert!(!native_image.generation_quiescent);
    let fatal_json = String::from_utf8(
        fatal
            .serialized_bytes()
            .expect("serialize native fatal diagnostic"),
    )
    .expect("fatal diagnostic JSON");
    assert!(!fatal_json.contains("CoreIR"));
    assert!(!fatal_json.contains("instructions"));
    assert_eq!(
        shard
            .actors()
            .processes()
            .get(watcher)
            .expect("watcher process")
            .mailbox_len(),
        1
    );
}
