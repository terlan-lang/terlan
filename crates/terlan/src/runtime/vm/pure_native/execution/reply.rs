use super::*;

pub(super) fn handle_reply(
    backend: &dyn super::super::NativeImageBackend,
    actors: &mut VmActorRuntime,
    context: &PureNativeExecutionContext<'_>,
    prepared: PreparedNativeCall,
    mut resume_count: usize,
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
            resume_count = resume_count.saturating_add(1);
            if resume_count > MAX_NATIVE_RESUME_COUNT {
                return Err(format!("error[pure_native_resume_budget]: native call exceeded {MAX_NATIVE_RESUME_COUNT} continuation resumes at {continuation_id}"));
            }
            let continuations = prepared.continuations.as_ref().ok_or_else(|| "error[pure_native_continuation_metadata]: transition has no admitted continuation table".to_string())?;
            let continuation = continuations.iter().find(|entry| entry.id == continuation_id).ok_or_else(|| format!("error[pure_native_continuation_unknown]: image yielded undeclared continuation {continuation_id}"))?;
            validate_transition_continuation(
                continuation,
                &prepared.result_type,
                &operation,
                &arguments,
                &values,
            )?;
            actors.park_native_continuation(owner_id, request_id, continuation_id)?;
            Ok(PureNativeExecution::Suspended(Box::new(
                PureNativeSuspension::new(
                    super::super::thread_neutral::NativeContinuationIdentity {
                        request_id,
                        owner_id,
                        continuation_id,
                    },
                    super::super::thread_neutral::OwnedNativeTransition {
                        capture_types: transition_capture_types(
                            continuation,
                            &operation,
                            &arguments,
                        )?,
                        operation,
                        arguments,
                        values,
                    },
                    super::super::thread_neutral::OwnedNativeResumeProgram {
                        result_type: prepared.result_type,
                        result_projection: prepared.result_projection,
                        continuations: continuations.clone(),
                        resume_count,
                    },
                    trace_call,
                ),
            )))
        }
        _ => Err("error[pure_native_continuation_reply]: unexpected control frame".to_string()),
    }
}
