//! Scheduler-visible execution state for direct native image calls.

use super::{
    native_status_error, validate_owner_id, validate_request_id, NativeDecodedResult,
    NativeResultProjection, PreparedNativeCall, PureNativeBoundary, PureNativeExecutionContext,
    PureNativeExportSpec, PureNativeIoWake, PureNativeSuspension,
};
use crate::runtime::native_image::control::{
    TvmControlFrame, TvmTransitionOperation, TVM_SQL_CAPABILITY_PREFIX_WORDS,
    TVM_SQL_CAPABILITY_TAG,
};
use crate::runtime::native_image::TvmBoundaryType;
use crate::runtime::vm::actor::{VmActorRuntime, VmNativeTraceCall};
use crate::runtime::vm::process::{VmExitReason, VmProcessId, VmProcessSource, VmProcessState};
use crate::runtime::vm::scheduler::VmSchedulerClass;
use crate::runtime::vm::ReplValue;
use crate::terlan_native_boundary::term::NativeBoundaryTerm;

#[path = "execution/entry.rs"]
mod entry;
#[path = "execution/reply.rs"]
mod reply;
#[path = "execution/support.rs"]
mod support;
#[path = "execution/transition_validation.rs"]
mod transition_validation;

use reply::handle_reply;
use support::{
    capability_identity, native_actor_exit_error, repl_value_to_boundary_term,
    transition_capture_types, validate_capability_arguments, validate_transition_continuation,
};
pub(crate) use transition_validation::validate_transition_arguments;

const MAX_NATIVE_RESUME_COUNT: usize = 1_048_576;

/// Owned capability RPC prepared from one generated suspension.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PureNativeCapabilityRequest {
    pub(crate) capability: String,
    pub(crate) operation: String,
    pub(crate) arguments: Vec<NativeBoundaryTerm>,
    /// Typed package arguments decoded directly from the actor-owned heap.
    pub(crate) package_arguments: Option<Vec<ReplValue>>,
    pub(crate) result_type: TvmBoundaryType,
}

/// One scheduler-visible result from starting or resuming native execution.
#[derive(Debug)]
pub(crate) enum PureNativeExecution {
    Complete(ReplValue),
    HttpResponse(crate::runtime::vm::VmAotHttpResponse),
    Suspended(Box<PureNativeSuspension>),
}

impl PureNativeBoundary {
    /// Materializes one parked capture through its actor-owned heap.
    pub(crate) fn debugger_decode_capture(
        &self,
        context: &PureNativeExecutionContext<'_>,
        boundary_type: &TvmBoundaryType,
        value: i64,
    ) -> Result<ReplValue, String> {
        self.backend
            .as_deref()
            .ok_or_else(|| {
                "error[pure_native_backend_missing]: no active native execution backend".to_string()
            })?
            .decode_transition_value(context, boundary_type, value)
    }

    /// Resumes a stopped failure/debug continuation through a typed debugger restart.
    pub(crate) fn resume_debug_restart_for_actor(
        &mut self,
        actors: &mut VmActorRuntime,
        context: &mut PureNativeExecutionContext<'_>,
        suspension: PureNativeSuspension,
    ) -> Result<PureNativeExecution, String> {
        if suspension.owner_id() != context.owner_id() {
            return Err("error[pure_native_debug_restart_owner]: foreign suspension owner".into());
        }
        if !matches!(
            suspension.operation(),
            TvmTransitionOperation::Failure | TvmTransitionOperation::Debug
        ) {
            return Err(format!(
                "error[pure_native_debug_restart_operation]: cannot bypass {:?}",
                suspension.operation()
            ));
        }
        actors.resume_native_continuation(
            suspension.owner_id(),
            suspension.request_id(),
            suspension.continuation_id(),
        )?;
        self.finish_transition_resume(actors, context, suspension, Vec::new(), None, None)
    }

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
        let result_type = TvmBoundaryType::from_transition_words(&arguments[1..4])?;
        let backend = self.backend.as_deref().ok_or_else(|| {
            "error[pure_native_backend_missing]: no active native execution backend".to_string()
        })?;
        if arguments[0] == 7 {
            let operation = backend
                .decode_transition_value(context, &TvmBoundaryType::String, arguments[4])
                .and_then(|value| match value {
                    ReplValue::String(value) => Ok(value),
                    _ => Err(
                        "error[pure_native_capability_argument]: expected package operation String"
                            .to_string(),
                    ),
                })?;
            let argument_count = usize::try_from(arguments[5]).map_err(|_| {
                "error[pure_native_capability_arguments]: package argument count must be nonnegative"
                    .to_string()
            })?;
            let package_arguments = (0..argument_count)
                .map(|index| {
                    let offset = 6 + index * 4;
                    let boundary_type =
                        TvmBoundaryType::from_transition_words(&arguments[offset..offset + 3])?;
                    backend
                        .decode_transition_value(context, &boundary_type, arguments[offset + 3])
                        .map_err(|error| {
                            format!(
                                "error[pure_native_capability_argument]: operation `{operation}` argument {index} ({boundary_type:?}): {error}"
                            )
                        })
                })
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(PureNativeCapabilityRequest {
                capability: "package-native".to_string(),
                operation,
                arguments: Vec::new(),
                package_arguments: Some(package_arguments),
                result_type,
            });
        }
        if arguments[0] == TVM_SQL_CAPABILITY_TAG {
            let decode_string = |index: usize, label: &str| {
                backend
                    .decode_transition_value(context, &TvmBoundaryType::String, arguments[index])
                    .and_then(|value| match value {
                        ReplValue::String(value) => Ok(NativeBoundaryTerm::Text(value)),
                        _ => Err(format!(
                            "error[pure_native_capability_argument]: expected SQL {label} String"
                        )),
                    })
            };
            let mut sql_arguments = (4..=8)
                .zip([
                    "row type",
                    "statement",
                    "query kind",
                    "transaction requirement",
                    "cardinality",
                ])
                .map(|(index, label)| decode_string(index, label))
                .collect::<Result<Vec<_>, _>>()?;
            let projection_type = TvmBoundaryType::Managed(
                crate::runtime::native_image::managed::SemanticTypeId::from_canonical(
                    "List(String)",
                )
                .map_err(|error| format!("error[pure_native_capability_argument]: {error}"))?
                .bytes(),
            );
            sql_arguments.push(
                backend
                    .decode_transition_value(context, &projection_type, arguments[9])
                    .and_then(|value| repl_value_to_boundary_term(value).map_err(String::from))?,
            );
            let parameter_count = usize::try_from(arguments[10]).map_err(|_| {
                "error[pure_native_capability_arguments]: SQL parameter count must be nonnegative"
                    .to_string()
            })?;
            for index in 0..parameter_count {
                let offset = TVM_SQL_CAPABILITY_PREFIX_WORDS + index * 4;
                let boundary_type =
                    TvmBoundaryType::from_transition_words(&arguments[offset..offset + 3])?;
                sql_arguments.push(
                    backend
                        .decode_transition_value(
                            context,
                            &boundary_type,
                            arguments[offset + 3],
                        )
                        .and_then(|value| repl_value_to_boundary_term(value).map_err(String::from))
                        .map_err(|error| {
                            format!(
                                "error[pure_native_capability_argument]: SQL parameter {} ({boundary_type:?}): {error}",
                                index + 1
                            )
                        })?,
                );
            }
            return Ok(PureNativeCapabilityRequest {
                capability: "postgres".to_string(),
                operation: "std.db.sql.query".to_string(),
                arguments: sql_arguments,
                package_arguments: None,
                result_type,
            });
        }
        let (capability, operation) = capability_identity(arguments[0])?;
        let boundary_arguments = arguments[4..]
            .iter()
            .enumerate()
            .map(|(index, value)| {
                let boundary_type = match (arguments[0], index) {
                    (9, 0) => TvmBoundaryType::Int,
                    (41, 1) | (41, 2) => TvmBoundaryType::Int,
                    (50, 1) => TvmBoundaryType::Bool,
                    (21, 4) | (21, 5) => TvmBoundaryType::Int,
                    (22, 0) => TvmBoundaryType::Managed(
                        crate::runtime::native_image::managed::SemanticTypeId::from_canonical(
                            "std.system.Process.Command",
                        )
                        .map_err(|error| {
                            format!("error[pure_native_capability_argument]: {error}")
                        })?
                        .bytes(),
                    ),
                    (31, 0) => TvmBoundaryType::Managed(
                        crate::runtime::native_image::managed::SemanticTypeId::from_canonical(
                            "std.system.Process.BatchRequest",
                        )
                        .map_err(|error| {
                            format!("error[pure_native_capability_argument]: {error}")
                        })?
                        .bytes(),
                    ),
                    (48, 0) => TvmBoundaryType::Managed(
                        crate::runtime::native_image::managed::SemanticTypeId::from_canonical(
                            "std.system.Process.FramedRequest",
                        )
                        .map_err(|error| {
                            format!("error[pure_native_capability_argument]: {error}")
                        })?
                        .bytes(),
                    ),
                    (54, 0) => TvmBoundaryType::Managed(
                        crate::runtime::native_image::managed::SemanticTypeId::from_canonical(
                            "List(Struct(std.io.File.CopyPlan;source:String,destination:String))",
                        )
                        .map_err(|error| {
                            format!("error[pure_native_capability_argument]: {error}")
                        })?
                        .bytes(),
                    ),
                    (55 | 58, 0) => TvmBoundaryType::Managed(
                        crate::runtime::native_image::managed::SemanticTypeId::from_canonical(
                            "List(Struct(std.crypto.Hash.LabeledFile;path:String,label:String))",
                        )
                        .map_err(|error| {
                            format!("error[pure_native_capability_argument]: {error}")
                        })?
                        .bytes(),
                    ),
                    (56, 0) => TvmBoundaryType::Managed(
                        crate::runtime::native_image::managed::SemanticTypeId::from_canonical(
                            "List(Struct(std.crypto.Hash.LabeledFile;path:String,label:String))",
                        )
                        .map_err(|error| {
                            format!("error[pure_native_capability_argument]: {error}")
                        })?
                        .bytes(),
                    ),
                    (57, 1) => TvmBoundaryType::Managed(
                        crate::runtime::native_image::managed::SemanticTypeId::from_canonical(
                            "List(Struct(std.crypto.Hash.LabeledFilePattern;id:String,pattern:String))",
                        )
                        .map_err(|error| {
                            format!("error[pure_native_capability_argument]: {error}")
                        })?
                        .bytes(),
                    ),
                    (17, 1) | (24, 2) | (39, 2) | (47, 1) | (56, 1) | (57, 2) => TvmBoundaryType::Managed(
                        crate::runtime::native_image::managed::SemanticTypeId::from_canonical(
                            "List(String)",
                        )
                        .map_err(|error| {
                            format!("error[pure_native_capability_argument]: {error}")
                        })?
                        .bytes(),
                    ),
                    (18, 0) => TvmBoundaryType::Managed(
                        crate::runtime::native_image::managed::SemanticTypeId::from_canonical(
                            "List(String)",
                        )
                        .map_err(|error| {
                            format!("error[pure_native_capability_argument]: {error}")
                        })?
                        .bytes(),
                    ),
                    (20, 1) | (21, 1) | (21, 2) | (21, 3) => TvmBoundaryType::Managed(
                        crate::runtime::native_image::managed::SemanticTypeId::from_canonical(
                            "List(String)",
                        )
                        .map_err(|error| {
                            format!("error[pure_native_capability_argument]: {error}")
                        })?
                        .bytes(),
                    ),
                    _ => TvmBoundaryType::String,
                };
                backend
                    .decode_transition_value(context, &boundary_type, *value)
                    .and_then(|value| repl_value_to_boundary_term(value).map_err(String::from))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(PureNativeCapabilityRequest {
            capability,
            operation,
            arguments: boundary_arguments,
            package_arguments: None,
            result_type,
        })
    }

    /// Resumes a decoded capability without rereading relocated request arguments.
    pub(crate) fn resume_capability_value_for_actor(
        &mut self,
        actors: &mut VmActorRuntime,
        context: &mut PureNativeExecutionContext<'_>,
        suspension: PureNativeSuspension,
        result_type: &TvmBoundaryType,
        result: &ReplValue,
    ) -> Result<PureNativeExecution, String> {
        let backend = self.backend.as_deref_mut().ok_or_else(|| {
            "error[pure_native_backend_missing]: no active native execution backend".to_string()
        })?;
        let encoded = backend.encode_transition_value(context, result_type, result)?;
        actors.resume_native_continuation(
            suspension.owner_id(),
            suspension.request_id(),
            suspension.continuation_id(),
        )?;
        self.finish_transition_resume(actors, context, suspension, vec![encoded], None, None)
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
        let resident_recipient = matches!(operation, TvmTransitionOperation::Send)
            .then(|| arguments.first().copied())
            .flatten()
            .and_then(|value| u64::try_from(value).ok());
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
            return Ok(PureNativeExecution::Suspended(Box::new(suspension)));
        };
        let execution = self.finish_transition_resume(
            actors,
            context,
            suspension,
            resume_values,
            consumed_mailbox_fragment,
            spawn_export,
        )?;
        if let Some(recipient) = resident_recipient {
            self.drive_resident_actor(actors, context, recipient)?;
        }
        Ok(execution)
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
            resume_state.resume_count,
            resume_state.trace_call,
            reply,
        )
    }

    /// Drives the step API synchronously for callers that only need a final value.
    #[cfg(test)]
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
                    self.resume_transition_for_actor(actors, context, *suspension)?
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
        let result: Result<bool, String> = (|| {
            let mut child_context = context.reborrow(child);
            let mut execution =
                boundary.begin_call_for_actor(actors, &mut child_context, &function, &[])?;
            loop {
                execution = match execution {
                    PureNativeExecution::Complete(_) => return Ok(true),
                    PureNativeExecution::HttpResponse(_) => {
                        return Err("error[pure_native.result_projection]: spawned child returned an HTTP-only result".to_string())
                    }
                    PureNativeExecution::Suspended(suspension) => {
                        let parked_identity = (
                            suspension.request_id(),
                            suspension.continuation_id(),
                        );
                        match boundary.resume_transition_for_actor(
                            actors,
                            &mut child_context,
                            *suspension,
                        )? {
                            PureNativeExecution::Suspended(next)
                                if (next.request_id(), next.continuation_id())
                                    == parked_identity =>
                            {
                                child_context.park_resident_suspension(*next)?;
                                return Ok(false);
                            }
                            next => next,
                        }
                    }
                };
            }
        })();
        let completed = result.as_ref().copied().unwrap_or(true);
        let release = if completed {
            let mut child_context = context.reborrow(child);
            boundary.release_owner(&mut child_context)
        } else {
            Ok(())
        };
        let shutdown = boundary.shutdown();
        let failure = result
            .err()
            .or_else(|| release.err())
            .or_else(|| shutdown.err());
        if completed || failure.is_some() {
            let reason = failure.map_or(VmExitReason::Normal, VmExitReason::Error);
            actors.exit_actor(child, reason)?;
        }
        Ok(())
    }

    /// Runs a woken spawned actor until it blocks again or exits.
    fn drive_resident_actor(
        &mut self,
        actors: &mut VmActorRuntime,
        context: &mut PureNativeExecutionContext<'_>,
        owner_id: u64,
    ) -> Result<(), String> {
        let Some(suspension) = context.take_resident_suspension(owner_id) else {
            return Ok(());
        };
        let owner = VmProcessId::from_native_owner(owner_id)?;
        self.drive_resident_execution(
            actors,
            context,
            owner,
            PureNativeExecution::Suspended(Box::new(suspension)),
        )?;
        Ok(())
    }

    /// Drives an externally resumed resident actor to its next park point or exit.
    pub(crate) fn drive_resident_execution(
        &mut self,
        actors: &mut VmActorRuntime,
        context: &mut PureNativeExecutionContext<'_>,
        owner: VmProcessId,
        mut execution: PureNativeExecution,
    ) -> Result<bool, String> {
        loop {
            match execution {
                PureNativeExecution::Complete(_) => {
                    let mut resident_context = context.reborrow(owner);
                    self.release_owner(&mut resident_context)?;
                    actors.exit_actor(owner, VmExitReason::Normal)?;
                    return Ok(true);
                }
                PureNativeExecution::HttpResponse(_) => {
                    let error = "error[pure_native.resident_result_projection]: spawned actor returned an HTTP-only result".to_string();
                    let mut resident_context = context.reborrow(owner);
                    self.release_owner(&mut resident_context)?;
                    actors.exit_actor(owner, VmExitReason::Error(error.clone()))?;
                    return Ok(true);
                }
                PureNativeExecution::Suspended(suspension) => {
                    let parked_identity = (suspension.request_id(), suspension.continuation_id());
                    let resumed = {
                        let mut resident_context = context.reborrow(owner);
                        self.resume_transition_for_actor(actors, &mut resident_context, *suspension)
                    };
                    execution = match resumed {
                        Ok(PureNativeExecution::Suspended(next))
                            if (next.request_id(), next.continuation_id()) == parked_identity =>
                        {
                            context.park_resident_suspension(*next)?;
                            return Ok(false);
                        }
                        Ok(next) => next,
                        Err(error) => {
                            let mut resident_context = context.reborrow(owner);
                            let release = self.release_owner(&mut resident_context);
                            let exit = actors.exit_actor(owner, VmExitReason::Error(error.clone()));
                            release?;
                            exit?;
                            // Spawned actors are isolation boundaries. Their
                            // terminal failure must not escape into the actor
                            // which happened to wake the continuation.
                            return Ok(true);
                        }
                    };
                }
            }
        }
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
        TvmTransitionOperation::Debug => actors
            .resume_native_continuation(owner_id, request_id, continuation_id)
            .map(|()| Some(Vec::new())),
        TvmTransitionOperation::Identity => actors
            .resume_native_continuation(owner_id, request_id, continuation_id)
            .and_then(|()| {
                i64::try_from(owner_id)
                    .map(|identity| Some(vec![identity]))
                    .map_err(|_| {
                        "error[pure_native_identity_result]: process identity exceeds native Int"
                            .to_string()
                    })
            }),
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
