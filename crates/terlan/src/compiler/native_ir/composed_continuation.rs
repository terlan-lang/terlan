//! Caller-owned wrappers for composed continuation graphs.

use std::collections::{HashMap, HashSet};

use super::NativeExpr;

pub(super) struct ComposedContinuationContext<'a> {
    pub(super) caller_capture_start: usize,
    pub(super) caller_capture_count: usize,
    pub(super) wrapper_ids: &'a HashMap<u64, u64>,
    pub(super) completion_result_ids: &'a HashSet<u64>,
    pub(super) tail_entries: &'a HashMap<usize, Vec<(u64, usize)>>,
    pub(super) completion_id: u64,
}

/// Rehomes one callee continuation graph under caller-owned resume identities.
///
/// Yielding branches append the caller captures and target the corresponding
/// wrapper node. Branches that complete tail-call one shared completion
/// continuation, so a conditional continuation graph does not clone the
/// caller's remaining evaluation context into every terminal branch.
pub(super) fn wrap_composed_continuation(
    body: &NativeExpr,
    local_count: usize,
    context: &ComposedContinuationContext<'_>,
) -> Result<NativeExpr, String> {
    wrap_composed_continuation_at(body, local_count, context)
}

fn wrap_composed_continuation_at(
    body: &NativeExpr,
    local_count: usize,
    context: &ComposedContinuationContext<'_>,
) -> Result<NativeExpr, String> {
    let caller_captures = || {
        (0..context.caller_capture_count)
            .map(|index| NativeExpr::Param(context.caller_capture_start.saturating_add(index)))
            .collect::<Vec<_>>()
    };
    match body {
        NativeExpr::Let { bindings, body } => Ok(NativeExpr::Let {
            bindings: bindings.clone(),
            body: Box::new(wrap_composed_continuation_at(
                body,
                local_count.saturating_add(bindings.len()),
                context,
            )?),
        }),
        NativeExpr::If { clauses } => Ok(NativeExpr::If {
            clauses: clauses
                .iter()
                .map(|(condition, branch)| {
                    Ok((
                        condition.clone(),
                        wrap_composed_continuation_at(branch, local_count, context)?,
                    ))
                })
                .collect::<Result<Vec<_>, String>>()?,
        }),
        NativeExpr::Suspend {
            operation,
            arguments,
            continuation_id,
            values,
        } => {
            let continuation_id = context
                .wrapper_ids
                .get(continuation_id)
                .copied()
                .ok_or_else(|| {
                    format!(
                        "error[native_ir.call_then]: callee continuation {continuation_id} is outside its composed profile"
                    )
                })?;
            let mut values = values.clone();
            values.extend(caller_captures());
            Ok(NativeExpr::Suspend {
                operation: *operation,
                arguments: arguments.clone(),
                continuation_id,
                values,
            })
        }
        NativeExpr::CallThen {
            function,
            args,
            resumes,
            completion_continuation_id,
            completion_function: _,
            values,
        } => {
            let mut resumes = resumes
                .iter()
                .map(|resume| {
                    let continuation_id = context
                        .wrapper_ids
                        .get(&resume.continuation_id)
                        .copied()
                        .ok_or_else(|| {
                            format!(
                                "error[native_ir.call_then]: callee continuation {} is outside its composed profile",
                                resume.continuation_id
                            )
                        })?;
                    Ok(super::NativeCallResume {
                        callee_continuation_id: resume.callee_continuation_id,
                        callee_capture_count: resume.callee_capture_count,
                        continuation_id,
                        caller_value_start: resume.caller_value_start,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            if let Some(entries) = context.tail_entries.get(function) {
                for (callee_continuation_id, callee_capture_count) in entries {
                    if resumes
                        .iter()
                        .any(|resume| resume.callee_continuation_id == *callee_continuation_id)
                    {
                        continue;
                    }
                    let continuation_id = context
                        .wrapper_ids
                        .get(callee_continuation_id)
                        .copied()
                        .ok_or_else(|| {
                            format!(
                                "error[native_ir.call_then]: tail continuation {callee_continuation_id} is outside its composed profile"
                            )
                        })?;
                    resumes.push(super::NativeCallResume {
                        callee_continuation_id: *callee_continuation_id,
                        callee_capture_count: *callee_capture_count,
                        continuation_id,
                        caller_value_start: values.len(),
                    });
                }
            }
            let completion_continuation_id = context
                .wrapper_ids
                .get(completion_continuation_id)
                .copied()
                .ok_or_else(|| {
                    format!(
                        "error[native_ir.call_then]: completion continuation {completion_continuation_id} is outside its composed profile"
                    )
                })?;
            let mut values = values.clone();
            values.extend(caller_captures());
            Ok(NativeExpr::CallThen {
                function: *function,
                args: args.clone(),
                resumes,
                completion_continuation_id,
                completion_function: None,
                values,
            })
        }
        NativeExpr::InvokeClosureThen {
            callee,
            args,
            parameter_types,
            result_type,
            resumes,
            completion_continuation_id,
            completion_function: _,
            values,
        } => {
            let resumes = resumes
                .iter()
                .map(|resume| {
                    let continuation_id = context
                        .wrapper_ids
                        .get(&resume.continuation_id)
                        .copied()
                        .ok_or_else(|| {
                            format!(
                                "error[native_ir.call_then]: dynamic callee continuation {} is outside its composed profile",
                                resume.continuation_id
                            )
                        })?;
                    Ok(super::NativeDynamicCallResume {
                        callee_export_id: resume.callee_export_id,
                        callee_continuation_id: resume.callee_continuation_id,
                        callee_capture_count: resume.callee_capture_count,
                        continuation_id,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            let completion_continuation_id = context
                .wrapper_ids
                .get(completion_continuation_id)
                .copied()
                .ok_or_else(|| {
                    format!(
                        "error[native_ir.call_then]: completion continuation {completion_continuation_id} is outside its composed profile"
                    )
                })?;
            let mut values = values.clone();
            values.extend(caller_captures());
            Ok(NativeExpr::InvokeClosureThen {
                callee: callee.clone(),
                args: args.clone(),
                parameter_types: parameter_types.clone(),
                result_type: *result_type,
                resumes,
                completion_continuation_id,
                completion_function: None,
                values,
            })
        }
        NativeExpr::ContinuationTailCall {
            continuation_id,
            args,
        } => {
            let completion_result = context.completion_result_ids.contains(continuation_id);
            let continuation_id = context
                .wrapper_ids
                .get(continuation_id)
                .copied()
                .ok_or_else(|| {
                    format!(
                        "error[native_ir.call_then]: callee continuation {continuation_id} is outside its composed profile"
                    )
                })?;
            let mut args = args.clone();
            let caller_captures = caller_captures();
            if completion_result {
                let result = args.pop().ok_or_else(|| {
                    format!(
                        "error[native_ir.call_then]: completion continuation {continuation_id} has no result argument"
                    )
                })?;
                args.extend(caller_captures);
                args.push(result);
            } else {
                args.extend(caller_captures);
            }
            Ok(NativeExpr::ContinuationTailCall {
                continuation_id,
                args,
            })
        }
        NativeExpr::TailCall { function, args, .. } => {
            let entries = context.tail_entries.get(function).ok_or_else(|| {
                format!(
                    "error[native_ir.call_then]: suspending tail target {function} has no composed profile"
                )
            })?;
            let resumes = entries
                .iter()
                .map(|(callee_continuation_id, callee_capture_count)| {
                    let continuation_id = context
                        .wrapper_ids
                        .get(callee_continuation_id)
                        .copied()
                        .ok_or_else(|| {
                            format!(
                                "error[native_ir.call_then]: tail continuation {callee_continuation_id} is outside its composed profile"
                            )
                        })?;
                    Ok(super::NativeCallResume {
                        callee_continuation_id: *callee_continuation_id,
                        callee_capture_count: *callee_capture_count,
                        continuation_id,
                        caller_value_start: 0,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(NativeExpr::CallThen {
                function: *function,
                args: args.clone(),
                resumes,
                completion_continuation_id: context.completion_id,
                completion_function: None,
                values: caller_captures(),
            })
        }
        completed => {
            let mut args = caller_captures();
            args.push(NativeExpr::Param(local_count));
            Ok(NativeExpr::Let {
                bindings: vec![completed.clone()],
                body: Box::new(NativeExpr::ContinuationTailCall {
                    continuation_id: context.completion_id,
                    args,
                }),
            })
        }
    }
}
