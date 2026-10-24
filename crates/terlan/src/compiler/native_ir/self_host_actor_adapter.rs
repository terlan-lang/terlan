//! Materializes admitted self-hosted actor metadata into backend-neutral NativeIR.

use crate::self_host_actor_ir::{
    RuntimeActorContinuation, RuntimeActorPlan, RuntimeActorResumeEdge,
    RuntimeActorSuspensionSite,
};

use super::{
    NativeCallResume, NativeContinuation, NativeDynamicCallResume, NativeExpr, NativeType,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct NativeActorSuspensionSite {
    pub(super) source_function: String,
    pub(super) source_arity: usize,
    pub(super) native_call_index: usize,
    pub(super) block_label: String,
    pub(super) result_name: String,
    pub(super) callee_module: String,
    pub(super) callee_function: String,
    pub(super) callee_arity: usize,
    pub(super) value_names: Vec<String>,
    pub(super) direct_resumes: Vec<NativeCallResume>,
    pub(super) dynamic_resumes: Vec<NativeDynamicCallResume>,
    pub(super) completion_continuation_id: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct NativeActorPlan {
    pub(super) continuations: Vec<NativeContinuation>,
    pub(super) suspension_sites: Vec<NativeActorSuspensionSite>,
}

/// Replaces one admitted raw call with its VM-owned continuation form.
pub(super) fn apply_suspension_site(
    expression: NativeExpr,
    site: &NativeActorSuspensionSite,
    values: Vec<NativeExpr>,
) -> Result<NativeExpr, String> {
    match expression {
        NativeExpr::Call { function, args } => {
            if !site.dynamic_resumes.is_empty() {
                return Err(format!(
                    "error[self-host.actor.direct-resume-kind]: fixed call `{}.{}` carries dynamic resume targets",
                    site.callee_module, site.callee_function
                ));
            }
            Ok(NativeExpr::CallThen {
                function,
                args,
                resumes: site.direct_resumes.clone(),
                completion_continuation_id: site.completion_continuation_id,
                completion_function: None,
                values,
            })
        }
        NativeExpr::InvokeClosure {
            callee,
            args,
            parameter_types,
            result_type,
        } => {
            if !site.direct_resumes.is_empty() {
                return Err(format!(
                    "error[self-host.actor.dynamic-resume-kind]: closure call `{}.{}` carries fixed resume targets",
                    site.callee_module, site.callee_function
                ));
            }
            Ok(NativeExpr::InvokeClosureThen {
                callee,
                args,
                parameter_types,
                result_type,
                resumes: site.dynamic_resumes.clone(),
                completion_continuation_id: site.completion_continuation_id,
                completion_function: None,
                values,
            })
        }
        NativeExpr::CallThen { .. } | NativeExpr::InvokeClosureThen { .. } => Err(format!(
            "error[self-host.actor.duplicate-rewrite]: call `{}.{}` already owns continuation metadata",
            site.callee_module, site.callee_function
        )),
        _ => Err(format!(
            "error[self-host.actor.site-expression]: suspension site `{}:{}` does not identify a call expression",
            site.source_function, site.block_label
        )),
    }
}

fn materialize_resume(
    value: RuntimeActorResumeEdge,
    direct: &mut Vec<NativeCallResume>,
    dynamic: &mut Vec<NativeDynamicCallResume>,
) -> Result<(), String> {
    if let Some(callee_export_id) = value.callee_export_id {
        if value.caller_value_start != 0 {
            return Err(format!(
                "error[self-host.actor.dynamic-caller-offset]: dynamic resume {} has unsupported caller value offset {}",
                value.callee_continuation_id, value.caller_value_start
            ));
        }
        dynamic.push(NativeDynamicCallResume {
            callee_export_id,
            callee_continuation_id: value.callee_continuation_id,
            callee_capture_count: value.callee_capture_count,
            continuation_id: value.continuation_id,
        });
    } else {
        direct.push(NativeCallResume {
            callee_continuation_id: value.callee_continuation_id,
            callee_capture_count: value.callee_capture_count,
            continuation_id: value.continuation_id,
            caller_value_start: value.caller_value_start,
        });
    }
    Ok(())
}

fn materialize_site(value: RuntimeActorSuspensionSite) -> Result<NativeActorSuspensionSite, String> {
    let mut direct_resumes = Vec::new();
    let mut dynamic_resumes = Vec::new();
    for resume in value.resumes {
        materialize_resume(resume, &mut direct_resumes, &mut dynamic_resumes)?;
    }
    Ok(NativeActorSuspensionSite {
        source_function: value.source_function,
        source_arity: value.source_arity,
        native_call_index: value.native_call_index,
        block_label: value.block_label,
        result_name: value.result_name,
        callee_module: value.callee_module,
        callee_function: value.callee_function,
        callee_arity: value.callee_arity,
        value_names: value.value_names,
        direct_resumes,
        dynamic_resumes,
        completion_continuation_id: value.completion_continuation_id,
    })
}

/// Resolves symbolic types and continuation entry blocks only after actor ABI admission.
pub(super) fn materialize_actor_plan<TypeResolver, BodyResolver>(
    plan: RuntimeActorPlan,
    mut resolve_type: TypeResolver,
    mut resolve_body: BodyResolver,
) -> Result<NativeActorPlan, String>
where
    TypeResolver: FnMut(&str) -> Result<NativeType, String>,
    BodyResolver: FnMut(
        &RuntimeActorContinuation,
        &[NativeActorSuspensionSite],
    ) -> Result<NativeExpr, String>,
{
    let suspension_sites = plan
        .suspension_sites
        .into_iter()
        .map(materialize_site)
        .collect::<Result<Vec<_>, _>>()?;
    let continuations = plan
        .continuations
        .into_iter()
        .map(|value| {
            let params = value
                .parameter_types
                .iter()
                .map(|type_name| resolve_type(type_name))
                .collect::<Result<Vec<_>, _>>()?;
            let return_type = resolve_type(&value.return_type)?;
            let body = resolve_body(&value, &suspension_sites)?;
            Ok(NativeContinuation {
                id: value.id,
                source_module: value.source_module,
                source_function: value.source_function,
                source_arity: value.source_arity,
                source_span: None,
                capture_names: value.capture_names,
                params,
                return_type,
                body,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(NativeActorPlan {
        continuations,
        suspension_sites,
    })
}
