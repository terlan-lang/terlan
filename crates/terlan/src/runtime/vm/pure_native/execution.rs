//! Scheduler-visible execution state for direct native image calls.

#![allow(dead_code)] // The step API is consumed by terlan-vm, not every binary embedding this module.

use std::collections::HashSet;

use super::{
    native_status_error, validate_continuation, validate_owner_id, validate_request_id,
    NativeDecodedResult, NativeResultProjection, PreparedNativeCall, PureNativeBoundary,
    PureNativeExecutionContext, PureNativeExportSpec, PureNativeIoWake, PureNativeSuspension,
};
use crate::runtime::native_image::control::{TvmControlFrame, TvmTransitionOperation};
use crate::runtime::native_image::TvmBoundaryType;
use crate::runtime::vm::actor::{VmActorRuntime, VmNativeTraceCall};
use crate::runtime::vm::process::{VmExitReason, VmProcessId, VmProcessSource, VmProcessState};
use crate::runtime::vm::scheduler::VmSchedulerClass;
use crate::runtime::vm::ReplValue;
use crate::terlan_native_boundary::term::NativeBoundaryTerm;

/// Owned capability RPC prepared from one generated suspension.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PureNativeCapabilityRequest {
    pub(crate) capability: &'static str,
    pub(crate) operation: &'static str,
    pub(crate) arguments: Vec<NativeBoundaryTerm>,
    pub(crate) result_type: TvmBoundaryType,
}

/// One scheduler-visible result from starting or resuming native execution.
#[derive(Debug)]
pub(crate) enum PureNativeExecution {
    Complete(ReplValue),
    HttpResponse(crate::runtime::vm::VmAotHttpResponse),
    Suspended(PureNativeSuspension),
}

impl PureNativeBoundary {
    /// Decodes one generated capability suspension without exposing heap words
    /// or worker transport identity outside the owning execution shard.
    pub(crate) fn capability_request_for_actor(
        &self,
        context: &PureNativeExecutionContext<'_>,
        suspension: &PureNativeSuspension,
    ) -> Result<PureNativeCapabilityRequest, String> {
        if suspension.owner_id() != context.owner_id() {
            return Err("error[pure_native_capability_owner]: foreign suspension owner".into());
        }
        if suspension.operation() != TvmTransitionOperation::Capability {
            return Err(format!(
                "error[pure_native_capability_operation]: expected Capability, found {:?}",
                suspension.operation()
            ));
        }
        validate_capability_arguments(suspension.arguments())?;
        let arguments = suspension.arguments();
        let (capability, operation) = capability_identity(arguments[0])?;
        let result_type = TvmBoundaryType::from_transition_words(&arguments[1..4])?;
        let backend = self.backend.as_deref().ok_or_else(|| {
            "error[pure_native_backend_missing]: no active native execution backend".to_string()
        })?;
        let arguments = arguments[4..]
            .iter()
            .map(|value| {
                backend
                    .decode_transition_value(context, &TvmBoundaryType::String, *value)
                    .and_then(|value| match value {
                        ReplValue::String(value) => Ok(NativeBoundaryTerm::Text(value)),
                        _ => Err(
                            "error[pure_native_capability_argument]: expected String payload"
                                .to_string(),
                        ),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(PureNativeCapabilityRequest {
            capability,
            operation,
            arguments,
            result_type,
        })
    }

    /// Injects one worker-owned capability completion into its exact parked continuation.
    pub(crate) fn resume_capability_for_actor(
        &mut self,
        actors: &mut VmActorRuntime,
        context: &mut PureNativeExecutionContext<'_>,
        suspension: PureNativeSuspension,
        result: &ReplValue,
    ) -> Result<PureNativeExecution, String> {
        let request = self.capability_request_for_actor(context, &suspension)?;
        let backend = self.backend.as_deref_mut().ok_or_else(|| {
            "error[pure_native_backend_missing]: no active native execution backend".to_string()
        })?;
        let encoded = backend.encode_transition_value(context, &request.result_type, result)?;
        actors.resume_native_continuation(
            suspension.owner_id(),
            suspension.request_id(),
            suspension.continuation_id(),
        )?;
        self.finish_transition_resume(actors, context, suspension, vec![encoded], None, None)
    }

    /// Starts a native call and returns while its actor remains parked on Yield.
    pub(crate) fn begin_call_for_actor(
        &mut self,
        actors: &mut VmActorRuntime,
        context: &mut PureNativeExecutionContext<'_>,
        function: &str,
        args: &[ReplValue],
    ) -> Result<PureNativeExecution, String> {
        self.begin_call_for_actor_with_projection(
            actors,
            context,
            function,
            args,
            NativeResultProjection::PublicValue,
        )
    }

    /// Starts a direct HTTP call whose non-file Response may bypass generic
    /// public-value materialization.
    pub(crate) fn begin_http_response_call_for_actor(
        &mut self,
        actors: &mut VmActorRuntime,
        context: &mut PureNativeExecutionContext<'_>,
        function: &str,
        args: &[ReplValue],
    ) -> Result<PureNativeExecution, String> {
        self.begin_call_for_actor_with_projection(
            actors,
            context,
            function,
            args,
            NativeResultProjection::HttpResponse,
        )
    }

    fn begin_call_for_actor_with_projection(
        &mut self,
        actors: &mut VmActorRuntime,
        context: &mut PureNativeExecutionContext<'_>,
        function: &str,
        args: &[ReplValue],
        result_projection: NativeResultProjection,
    ) -> Result<PureNativeExecution, String> {
        let owner = context.actor();
        let mut prepared = self.prepare_call(
            context,
            function,
            args,
            actors.native_trace_enabled(),
            result_projection,
        )?;
        let trace_call =
            actors.begin_optional_native_trace_call(owner, prepared.trace_source.take())?;
        let reply = match self
            .backend
            .as_mut()
            .expect("prepare_call initializes the native backend")
            .call_frame(context, prepared.request_id, prepared.export_id, args)
        {
            Ok(reply) => reply,
            Err(error) => {
                let _ = actors.fail_native_trace_call(owner, trace_call, error.clone());
                return Err(error);
            }
        };
        if matches!(reply, TvmControlFrame::Transition { .. }) {
            prepared.continuations = Some(
                self.call_cache
                    .as_ref()
                    .expect("resolved export installs its continuation cache")
                    .continuations
                    .as_ref()
                    .to_vec(),
            );
        }
        let backend = self
            .backend
            .as_deref()
            .expect("prepared native backend remains available");
        handle_reply(
            backend,
            actors,
            context,
            prepared,
            HashSet::new(),
            trace_call,
            reply,
        )
    }

    /// Services one VM-owned transition and advances its exact native continuation.
    pub(crate) fn resume_transition_for_actor(
        &mut self,
        actors: &mut VmActorRuntime,
        context: &mut PureNativeExecutionContext<'_>,
        suspension: PureNativeSuspension,
    ) -> Result<PureNativeExecution, String> {
        let owner = context.actor();
        let request_id = suspension.request_id();
        let owner_id = suspension.owner_id();
        let continuation_id = suspension.continuation_id();
        let operation = suspension.operation();
        let arguments = suspension.arguments();
        if owner_id != owner.as_u64() {
            return Err(format!(
                "error[pure_native_owner]: actor {} cannot resume owner {}",
                owner.as_u64(),
                owner_id
            ));
        }
        let spawn_export = if matches!(operation, TvmTransitionOperation::Spawn) {
            Some(self.resolve_spawn_export(arguments[0])?)
        } else {
            None
        };
        let typed_transition = matches!(
            (&operation, arguments.len()),
            (TvmTransitionOperation::Send, 5) | (TvmTransitionOperation::Receive, 3)
        );
        let mut consumed_mailbox_fragment = None;
        let transition_result = if typed_transition {
            let boundary_type = match operation {
                TvmTransitionOperation::Send => {
                    TvmBoundaryType::from_transition_words(&arguments[1..4])?
                }
                TvmTransitionOperation::Receive => {
                    TvmBoundaryType::from_transition_words(arguments)?
                }
                _ => unreachable!("typed transition shape was checked"),
            };
            match operation {
                TvmTransitionOperation::Send => {
                    let recipient = u64::try_from(arguments[0]).map_err(|_| {
                        "error[pure_native_transition_arguments]: Send recipient must be a positive process identity".to_string()
                    })?;
                    let backend = self.backend.as_deref_mut().ok_or_else(|| {
                        "error[pure_native_backend_missing]: no active native execution backend"
                            .to_string()
                    })?;
                    if let Some(fragment) = backend.copy_transition_value(
                        context,
                        recipient,
                        &boundary_type,
                        arguments[4],
                    )? {
                        let rollback = fragment;
                        if let Err(error) = actors.service_native_send_managed(
                            owner_id,
                            request_id,
                            continuation_id,
                            recipient,
                            fragment,
                            boundary_type,
                        ) {
                            backend.rollback_transition_value(context, rollback)?;
                            return Err(error);
                        }
                    } else {
                        let payload = backend.decode_transition_value(
                            context,
                            &boundary_type,
                            arguments[4],
                        )?;
                        actors.service_native_send_typed(
                            owner_id,
                            request_id,
                            continuation_id,
                            recipient,
                            payload,
                            boundary_type,
                        )?;
                    }
                    Some(Vec::new())
                }
                TvmTransitionOperation::Receive => {
                    let backend = self.backend.as_deref_mut().ok_or_else(|| {
                        "error[pure_native_backend_missing]: no active native execution backend"
                            .to_string()
                    })?;
                    let mut received_fragment = None;
                    let received = actors.service_native_receive_typed_message(
                        owner_id,
                        request_id,
                        continuation_id,
                        &boundary_type,
                        |message| {
                            received_fragment = message.managed_fragment;
                            backend.encode_transition_message(context, &boundary_type, message)
                        },
                    )?;
                    if received.is_some() {
                        consumed_mailbox_fragment = received_fragment;
                    }
                    received.map(|value| vec![value])
                }
                _ => unreachable!("typed transition shape was checked"),
            }
        } else {
            dispatch_transition_operation(
                actors,
                owner_id,
                request_id,
                continuation_id,
                &operation,
                arguments,
            )?
        };
        let Some(resume_values) = transition_result else {
            return Ok(PureNativeExecution::Suspended(suspension));
        };
        self.finish_transition_resume(
            actors,
            context,
            suspension,
            resume_values,
            consumed_mailbox_fragment,
            spawn_export,
        )
    }

    /// Injects one typed VM I/O wake and advances its exact native continuation.
    pub(crate) fn resume_io_for_actor(
        &mut self,
        actors: &mut VmActorRuntime,
        context: &mut PureNativeExecutionContext<'_>,
        suspension: PureNativeSuspension,
        wake: PureNativeIoWake,
    ) -> Result<PureNativeExecution, String> {
        let request_id = suspension.request_id();
        let owner_id = suspension.owner_id();
        let continuation_id = suspension.continuation_id();
        let boundary_type = wake.wait().boundary_type().clone();
        let backend = self.backend.as_deref_mut().ok_or_else(|| {
            "error[pure_native_backend_missing]: no active native execution backend".to_string()
        })?;
        let encoded = backend.encode_transition_value(context, &boundary_type, wake.value())?;
        actors.resume_native_continuation(owner_id, request_id, continuation_id)?;
        self.finish_transition_resume(actors, context, suspension, vec![encoded], None, None)
    }

    /// Advances a generated timer continuation after scheduler-owned delivery.
    pub(crate) fn resume_timer_for_actor(
        &mut self,
        actors: &mut VmActorRuntime,
        context: &mut PureNativeExecutionContext<'_>,
        suspension: PureNativeSuspension,
    ) -> Result<PureNativeExecution, String> {
        if suspension.owner_id() != context.owner_id() {
            return Err("error[pure_native_timer_owner]: foreign suspension owner".to_string());
        }
        if suspension.operation() != TvmTransitionOperation::Timer {
            return Err(format!(
                "error[pure_native_timer_operation]: expected Timer, found {:?}",
                suspension.operation()
            ));
        }
        self.finish_transition_resume(actors, context, suspension, Vec::new(), None, None)
    }

    /// Restores generated continuation state after one VM transition completes.
    pub(super) fn finish_transition_resume(
        &mut self,
        actors: &mut VmActorRuntime,
        context: &mut PureNativeExecutionContext<'_>,
        suspension: PureNativeSuspension,
        mut resume_values: Vec<i64>,
        consumed_mailbox_fragment: Option<crate::runtime::vm::process::VmManagedMailboxToken>,
        spawn_export: Option<PureNativeExportSpec>,
    ) -> Result<PureNativeExecution, String> {
        let request_id = suspension.request_id();
        let owner_id = suspension.owner_id();
        let continuation_id = suspension.continuation_id();
        let resume_state = suspension.into_resume_state();
        if let Some(export) = spawn_export {
            let child_id = *resume_values.first().ok_or_else(|| {
                "error[pure_native_spawn_result]: Spawn did not return a child identity".to_string()
            })?;
            let child_id = u64::try_from(child_id).map_err(|_| {
                "error[pure_native_spawn_result]: Spawn returned an invalid child identity"
                    .to_string()
            })?;
            actors.attach_native_spawn_entry(
                child_id,
                VmProcessSource::new(&export.module, &export.function, export.arity),
            )?;
            self.execute_spawned_child(
                actors,
                context,
                VmProcessId::from_native_owner(child_id)?,
                &export,
            )?;
        }
        if actors.enforce_native_cancellation_boundary(owner_id)? {
            if let Some(fragment) = consumed_mailbox_fragment {
                self.backend
                    .as_deref_mut()
                    .expect("suspended execution retains its backend")
                    .consume_transition_value(context, fragment)?;
            }
            return Err(format!(
                "error[pure_native_cancelled]: native actor {} was cancelled before resume",
                owner_id
            ));
        }
        let owner = VmProcessId::from_native_owner(owner_id)?;
        if let Some(VmProcessState::Exited(reason)) =
            actors.processes().get(owner).map(|process| &process.state)
        {
            if let Some(fragment) = consumed_mailbox_fragment {
                self.backend
                    .as_deref_mut()
                    .expect("suspended execution retains its backend")
                    .consume_transition_value(context, fragment)?;
            }
            return Err(native_actor_exit_error(owner_id, reason));
        }
        resume_values.extend(resume_state.values);
        let backend = self.backend.as_mut().ok_or_else(|| {
            "error[pure_native_backend_missing]: no active native execution backend".to_string()
        })?;
        let reply = backend.resume_frame(context, request_id, continuation_id, resume_values);
        if let Some(fragment) = consumed_mailbox_fragment {
            backend.consume_transition_value(context, fragment)?;
        }
        let reply = match reply {
            Ok(reply) => reply,
            Err(error) => {
                let _ =
                    actors.fail_native_trace_call(owner, resume_state.trace_call, error.clone());
                return Err(error);
            }
        };
        let prepared = PreparedNativeCall {
            request_id: resume_state.request_id,
            owner_id: resume_state.owner_id,
            export_id: 0,
            result_type: resume_state.result_type,
            continuations: resume_state.continuations.into(),
            trace_source: None,
            result_projection: resume_state.result_projection,
        };
        let backend = self
            .backend
            .as_deref()
            .expect("resumed native backend remains available");
        handle_reply(
            backend,
            actors,
            context,
            prepared,
            resume_state.observed_continuations,
            resume_state.trace_call,
            reply,
        )
    }

    /// Drives the step API synchronously for callers that only need a final value.
    pub(crate) fn call_for_actor(
        &mut self,
        actors: &mut VmActorRuntime,
        context: &mut PureNativeExecutionContext<'_>,
        function: &str,
        args: &[ReplValue],
    ) -> Result<ReplValue, String> {
        let mut execution = self.begin_call_for_actor(actors, context, function, args)?;
        loop {
            execution = match execution {
                PureNativeExecution::Complete(value) => return Ok(value),
                PureNativeExecution::HttpResponse(_) => {
                    return Err("error[pure_native.result_projection]: typed HTTP response returned through a public-value call".to_string())
                }
                PureNativeExecution::Suspended(suspension) => {
                    self.resume_transition_for_actor(actors, context, suspension)?
                }
            };
        }
    }

    fn resolve_spawn_export(&self, tag: i64) -> Result<PureNativeExportSpec, String> {
        if tag <= 0 {
            return Err(
                "error[pure_native_spawn_entry]: Spawn entry tag must be positive".to_string(),
            );
        }
        let function = format!("spawn_{tag}");
        let matches = self
            .artifact
            .as_ref()
            .into_iter()
            .flat_map(|artifact| artifact.exports.iter())
            .filter(|export| export.function == function && export.arity == 0)
            .cloned()
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [export] => Ok(export.clone()),
            [] => Err(format!(
                "error[pure_native_spawn_entry]: image has no zero-arity native export `{function}`"
            )),
            _ => Err(format!(
                "error[pure_native_spawn_entry]: image has multiple zero-arity native exports `{function}`"
            )),
        }
    }

    fn execute_spawned_child(
        &self,
        actors: &mut VmActorRuntime,
        context: &mut PureNativeExecutionContext<'_>,
        child: VmProcessId,
        export: &PureNativeExportSpec,
    ) -> Result<(), String> {
        let backend = self
            .backend
            .as_ref()
            .ok_or_else(|| {
                "error[pure_native_backend_missing]: no backend available for spawned actor"
                    .to_string()
            })?
            .fork_box()?;
        let mut boundary = PureNativeBoundary {
            artifact: self.artifact.clone(),
            backend: Some(backend),
            call_cache: None,
        };
        let function = format!("{}.{}", export.module, export.function);
        let result: Result<(), String> = (|| {
            let mut child_context = context.reborrow(child);
            let mut execution =
                boundary.begin_call_for_actor(actors, &mut child_context, &function, &[])?;
            loop {
                execution = match execution {
                    PureNativeExecution::Complete(_) => return Ok(()),
                    PureNativeExecution::HttpResponse(_) => {
                        return Err("error[pure_native.result_projection]: spawned child returned an HTTP-only result".to_string())
                    }
                    PureNativeExecution::Suspended(suspension) => boundary
                        .resume_transition_for_actor(actors, &mut child_context, suspension)?,
                };
            }
        })();
        let release = {
            let mut child_context = context.reborrow(child);
            boundary.release_owner(&mut child_context)
        };
        let shutdown = boundary.shutdown();
        let failure = result
            .err()
            .or_else(|| release.err())
            .or_else(|| shutdown.err());
        let reason = failure.map_or(VmExitReason::Normal, VmExitReason::Error);
        actors.exit_actor(child, reason)?;
        Ok(())
    }
}

fn handle_reply(
    backend: &dyn super::NativeImageBackend,
    actors: &mut VmActorRuntime,
    context: &PureNativeExecutionContext<'_>,
    prepared: PreparedNativeCall,
    mut observed_continuations: HashSet<u64>,
    trace_call: VmNativeTraceCall,
    reply: TvmControlFrame,
) -> Result<PureNativeExecution, String> {
    match reply {
        TvmControlFrame::Success {
            request_id,
            owner_id,
            value,
        } => {
            validate_request_id(request_id, prepared.request_id)?;
            validate_owner_id(owner_id, prepared.owner_id)?;
            match backend.decode_result(
                context,
                &prepared.result_type,
                value,
                prepared.result_projection,
            ) {
                Ok(NativeDecodedResult::Value(value)) => {
                    actors.complete_native_trace_call(
                        VmProcessId::from_native_owner(owner_id)?,
                        trace_call,
                    )?;
                    Ok(PureNativeExecution::Complete(value))
                }
                Ok(NativeDecodedResult::HttpResponse(response)) => {
                    actors.complete_native_trace_call(
                        VmProcessId::from_native_owner(owner_id)?,
                        trace_call,
                    )?;
                    Ok(PureNativeExecution::HttpResponse(response))
                }
                Err(error) => {
                    let _ = actors.fail_native_trace_call(
                        VmProcessId::from_native_owner(owner_id)?,
                        trace_call,
                        error.clone(),
                    );
                    Err(error)
                }
            }
        }
        TvmControlFrame::Failure {
            request_id,
            owner_id,
            status,
        } => {
            validate_request_id(request_id, prepared.request_id)?;
            validate_owner_id(owner_id, prepared.owner_id)?;
            let error = native_status_error(status);
            let _ = actors.fail_native_trace_call(
                VmProcessId::from_native_owner(owner_id)?,
                trace_call,
                error.clone(),
            );
            Err(error)
        }
        TvmControlFrame::Transition {
            request_id,
            owner_id,
            continuation_id,
            operation,
            arguments,
            values,
        } => {
            validate_request_id(request_id, prepared.request_id)?;
            validate_owner_id(owner_id, prepared.owner_id)?;
            validate_transition_arguments(&operation, &arguments)?;
            if !observed_continuations.insert(continuation_id) {
                return Err(format!(
                    "error[pure_native_continuation_cycle]: continuation {continuation_id} was yielded more than once"
                ));
            }
            let continuations = prepared.continuations.as_ref().ok_or_else(|| {
                "error[pure_native_continuation_metadata]: transition has no admitted continuation table"
                    .to_string()
            })?;
            let continuation = continuations
                .iter()
                .find(|entry| entry.id == continuation_id)
                .ok_or_else(|| {
                    format!(
                        "error[pure_native_continuation_unknown]: image yielded undeclared continuation {continuation_id}"
                    )
                })?;
            validate_transition_continuation(
                continuation,
                &prepared.result_type,
                &operation,
                &arguments,
                &values,
            )?;
            actors.park_native_continuation(owner_id, request_id, continuation_id)?;
            Ok(PureNativeExecution::Suspended(PureNativeSuspension::new(
                request_id,
                owner_id,
                continuation_id,
                operation,
                arguments,
                values,
                prepared.result_type,
                prepared.result_projection,
                continuations.clone(),
                observed_continuations,
                trace_call,
            )))
        }
        _ => Err("error[pure_native_continuation_reply]: unexpected control frame".to_string()),
    }
}

pub(crate) fn dispatch_transition_operation(
    actors: &mut VmActorRuntime,
    owner_id: u64,
    request_id: u64,
    continuation_id: u64,
    operation: &TvmTransitionOperation,
    arguments: &[i64],
) -> Result<Option<Vec<i64>>, String> {
    validate_transition_arguments(operation, arguments)?;
    match operation {
        TvmTransitionOperation::Yield => actors
            .resume_native_continuation(owner_id, request_id, continuation_id)
            .map(|()| Some(Vec::new())),
        TvmTransitionOperation::Send => {
            let recipient = u64::try_from(arguments[0]).map_err(|_| {
                "error[pure_native_transition_arguments]: Send recipient must be a positive process identity"
                    .to_string()
            })?;
            actors
                .service_native_send(
                    owner_id,
                    request_id,
                    continuation_id,
                    recipient,
                    ReplValue::Int(arguments[1]),
                )
                .map(|_| Some(Vec::new()))
        }
        TvmTransitionOperation::Receive => actors
            .service_native_receive_int(owner_id, request_id, continuation_id)
            .map(|payload| payload.map(|value| vec![value])),
        TvmTransitionOperation::Spawn => {
            let entry_id = u64::try_from(arguments[0]).map_err(|_| {
                "error[pure_native_transition_arguments]: Spawn entry must be a positive native identity"
                    .to_string()
            })?;
            actors
                .service_native_spawn(owner_id, request_id, continuation_id, entry_id)
                .map(|child| Some(vec![child as i64]))
        }
        TvmTransitionOperation::Timer => {
            let delay_ticks = u64::try_from(arguments[0]).map_err(|_| {
                "error[pure_native_transition_arguments]: Timer delay must be positive".to_string()
            })?;
            actors
                .service_native_timer(owner_id, request_id, continuation_id, delay_ticks)
                .map(|()| Some(Vec::new()))
        }
        TvmTransitionOperation::Link => {
            let peer_id = u64::try_from(arguments[0]).map_err(|_| {
                "error[pure_native_transition_arguments]: Link peer must be a positive process identity"
                    .to_string()
            })?;
            actors
                .service_native_link(owner_id, request_id, continuation_id, peer_id)
                .map(|_| Some(Vec::new()))
        }
        TvmTransitionOperation::Monitor => {
            let target_id = u64::try_from(arguments[0]).map_err(|_| {
                "error[pure_native_transition_arguments]: Monitor target must be a positive process identity"
                    .to_string()
            })?;
            actors
                .service_native_monitor(owner_id, request_id, continuation_id, target_id)
                .and_then(|monitor_ref| {
                    i64::try_from(monitor_ref)
                        .map(|value| Some(vec![value]))
                        .map_err(|_| {
                            "error[pure_native_monitor_result]: monitor reference exceeds native Int"
                                .to_string()
                        })
                })
        }
        TvmTransitionOperation::Resource => {
            let kind_tag = u64::try_from(arguments[0]).map_err(|_| {
                "error[pure_native_transition_arguments]: Resource kind tag must be positive"
                    .to_string()
            })?;
            actors
                .service_native_resource(owner_id, request_id, continuation_id, kind_tag)
                .and_then(|resource_id| {
                    i64::try_from(resource_id)
                        .map(|value| Some(vec![value]))
                        .map_err(|_| {
                            "error[pure_native_resource_result]: resource identity exceeds native Int"
                                .to_string()
                        })
                })
        }
        TvmTransitionOperation::Cancellation => {
            let target_id = u64::try_from(arguments[0]).map_err(|_| {
                "error[pure_native_transition_arguments]: Cancellation target must be a positive process identity"
                    .to_string()
            })?;
            actors
                .service_native_cancellation(owner_id, request_id, continuation_id, target_id)
                .map(|()| Some(Vec::new()))
        }
        TvmTransitionOperation::Failure => {
            let failure_code = u64::try_from(arguments[0]).map_err(|_| {
                "error[pure_native_transition_arguments]: Failure code must be positive".to_string()
            })?;
            actors
                .service_native_failure(owner_id, request_id, continuation_id, failure_code)
                .map(|_| Some(Vec::new()))
        }
        TvmTransitionOperation::Scheduling => {
            let class = match arguments[0] {
                1 => VmSchedulerClass::Priority,
                2 => VmSchedulerClass::Normal,
                3 => VmSchedulerClass::Background,
                _ => unreachable!("Scheduling arguments were validated before dispatch"),
            };
            actors
                .service_native_scheduling(owner_id, request_id, continuation_id, class)
                .map(|()| Some(Vec::new()))
        }
        TvmTransitionOperation::Capability => Ok(None),
    }
}

pub(crate) fn validate_transition_arguments(
    operation: &TvmTransitionOperation,
    arguments: &[i64],
) -> Result<(), String> {
    match operation {
        TvmTransitionOperation::Yield if arguments.is_empty() => Ok(()),
        TvmTransitionOperation::Yield => Err(
            "error[pure_native_transition_arguments]: Yield transition must not carry operation arguments"
                .to_string(),
        ),
        TvmTransitionOperation::Send if !matches!(arguments.len(), 2 | 5) => Err(format!(
            "error[pure_native_transition_arguments]: Send transition requires 2 scalar or 5 typed arguments, received {} arguments",
            arguments.len()
        )),
        TvmTransitionOperation::Send if arguments[0] <= 0 => Err(
            "error[pure_native_transition_arguments]: Send recipient must be a positive process identity"
                .to_string(),
        ),
        TvmTransitionOperation::Send if arguments.len() == 5 => {
            TvmBoundaryType::from_transition_words(&arguments[1..4]).map(|_| ())
        }
        TvmTransitionOperation::Send => Ok(()),
        TvmTransitionOperation::Receive if arguments.is_empty() => Ok(()),
        TvmTransitionOperation::Receive if arguments.len() == 3 => {
            TvmBoundaryType::from_transition_words(arguments).map(|_| ())
        }
        TvmTransitionOperation::Receive => Err(format!(
            "error[pure_native_transition_arguments]: Receive transition requires 0 scalar or 3 typed arguments, received {} arguments",
            arguments.len()
        )),
        TvmTransitionOperation::Spawn if arguments.len() != 1 => Err(format!(
            "error[pure_native_transition_arguments]: Spawn transition requires one native entry identity, received {} arguments",
            arguments.len()
        )),
        TvmTransitionOperation::Spawn if arguments[0] <= 0 => Err(
            "error[pure_native_transition_arguments]: Spawn entry must be a positive native identity"
                .to_string(),
        ),
        TvmTransitionOperation::Spawn => Ok(()),
        TvmTransitionOperation::Timer if arguments.len() != 1 => Err(format!(
            "error[pure_native_transition_arguments]: Timer transition requires one positive delay, received {} arguments",
            arguments.len()
        )),
        TvmTransitionOperation::Timer if arguments[0] <= 0 => Err(
            "error[pure_native_transition_arguments]: Timer delay must be positive".to_string(),
        ),
        TvmTransitionOperation::Timer => Ok(()),
        TvmTransitionOperation::Link if arguments.len() != 1 => Err(format!(
            "error[pure_native_transition_arguments]: Link transition requires one positive peer identity, received {} arguments",
            arguments.len()
        )),
        TvmTransitionOperation::Link if arguments[0] <= 0 => Err(
            "error[pure_native_transition_arguments]: Link peer must be a positive process identity"
                .to_string(),
        ),
        TvmTransitionOperation::Link => Ok(()),
        TvmTransitionOperation::Monitor if arguments.len() != 1 => Err(format!(
            "error[pure_native_transition_arguments]: Monitor transition requires one positive target identity, received {} arguments",
            arguments.len()
        )),
        TvmTransitionOperation::Monitor if arguments[0] <= 0 => Err(
            "error[pure_native_transition_arguments]: Monitor target must be a positive process identity"
                .to_string(),
        ),
        TvmTransitionOperation::Monitor => Ok(()),
        TvmTransitionOperation::Resource if arguments.len() != 1 => Err(format!(
            "error[pure_native_transition_arguments]: Resource transition requires one positive kind tag, received {} arguments",
            arguments.len()
        )),
        TvmTransitionOperation::Resource if arguments[0] <= 0 => Err(
            "error[pure_native_transition_arguments]: Resource kind tag must be positive"
                .to_string(),
        ),
        TvmTransitionOperation::Resource => Ok(()),
        TvmTransitionOperation::Cancellation if arguments.len() != 1 => Err(format!(
            "error[pure_native_transition_arguments]: Cancellation transition requires one positive target identity, received {} arguments",
            arguments.len()
        )),
        TvmTransitionOperation::Cancellation if arguments[0] <= 0 => Err(
            "error[pure_native_transition_arguments]: Cancellation target must be a positive process identity"
                .to_string(),
        ),
        TvmTransitionOperation::Cancellation => Ok(()),
        TvmTransitionOperation::Failure if arguments.len() != 1 => Err(format!(
            "error[pure_native_transition_arguments]: Failure transition requires one positive failure code, received {} arguments",
            arguments.len()
        )),
        TvmTransitionOperation::Failure if arguments[0] <= 0 => Err(
            "error[pure_native_transition_arguments]: Failure code must be positive".to_string(),
        ),
        TvmTransitionOperation::Failure => Ok(()),
        TvmTransitionOperation::Scheduling if arguments.len() != 1 => Err(format!(
            "error[pure_native_transition_arguments]: Scheduling transition requires one class tag, received {} arguments",
            arguments.len()
        )),
        TvmTransitionOperation::Scheduling if !(1..=3).contains(&arguments[0]) => Err(
            "error[pure_native_transition_arguments]: Scheduling class tag must be 1, 2, or 3"
                .to_string(),
        ),
        TvmTransitionOperation::Scheduling => Ok(()),
        TvmTransitionOperation::Capability => validate_capability_arguments(arguments),
    }
}

fn native_actor_exit_error(owner_id: u64, reason: &VmExitReason) -> String {
    match reason {
        VmExitReason::Error(message) => {
            format!("error[pure_native_failure]: native actor {owner_id} failed: {message}")
        }
        other => format!(
            "error[pure_native_failure]: native actor {owner_id} exited before resume: {other:?}"
        ),
    }
}

fn validate_transition_continuation(
    continuation: &crate::runtime::native_image::TvmContinuationDescriptor,
    result_type: &crate::runtime::native_image::TvmBoundaryType,
    operation: &TvmTransitionOperation,
    arguments: &[i64],
    values: &[i64],
) -> Result<(), String> {
    if !matches!(
        operation,
        TvmTransitionOperation::Receive
            | TvmTransitionOperation::Spawn
            | TvmTransitionOperation::Monitor
            | TvmTransitionOperation::Resource
            | TvmTransitionOperation::Capability
    ) {
        return validate_continuation(continuation, result_type, values);
    }
    let injected_type =
        if matches!(operation, TvmTransitionOperation::Receive) && arguments.len() == 3 {
            TvmBoundaryType::from_transition_words(arguments)?
        } else if matches!(operation, TvmTransitionOperation::Capability) {
            TvmBoundaryType::from_transition_words(&arguments[1..4])?
        } else {
            TvmBoundaryType::Int
        };
    if continuation.parameters.first() != Some(&injected_type) {
        return Err(format!(
            "error[pure_native_continuation_type]: {operation:?} continuation {} must accept a {injected_type:?} result first", continuation.id
        ));
    }
    let mut captures = continuation.clone();
    captures.parameters.remove(0);
    validate_continuation(&captures, result_type, values)
}

fn validate_capability_arguments(arguments: &[i64]) -> Result<(), String> {
    let expected = match arguments.first().copied() {
        Some(1 | 2 | 3 | 6) => 5,
        Some(4 | 5) => 6,
        Some(tag) => {
            return Err(format!(
                "error[pure_native_capability_arguments]: unknown capability tag {tag}"
            ));
        }
        None => {
            return Err(
                "error[pure_native_capability_arguments]: missing capability tag".to_string(),
            );
        }
    };
    if arguments.len() == expected {
        Ok(())
    } else {
        Err(format!(
            "error[pure_native_capability_arguments]: capability tag {} requires {} payload words, received {}",
            arguments[0],
            expected - 4,
            arguments.len() - 1
        ))
    }
}

fn capability_identity(tag: i64) -> Result<(&'static str, &'static str), String> {
    match tag {
        1 => Ok(("stdio", "std.io.console.println")),
        2 => Ok(("filesystem", "std.io.file.exists")),
        3 => Ok(("filesystem", "std.io.file.read_text")),
        4 => Ok(("filesystem", "std.io.file.write_text")),
        5 => Ok(("filesystem", "std.io.file.append_text")),
        6 => Ok(("filesystem", "std.io.file.delete")),
        _ => Err(format!(
            "error[pure_native_capability_arguments]: unknown capability tag {tag}"
        )),
    }
}
