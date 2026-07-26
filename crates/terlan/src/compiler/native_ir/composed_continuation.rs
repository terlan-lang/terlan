//! Caller-owned wrappers for composed continuation graphs.

use std::collections::HashMap;

use super::NativeExpr;

/// Rehomes one callee continuation graph under caller-owned resume identities.
///
/// Yielding branches append the caller captures and target the corresponding
/// wrapper node. Branches that complete tail-call one shared completion
/// continuation, so a conditional continuation graph does not clone the
/// caller's remaining evaluation context into every terminal branch.
pub(super) fn wrap_composed_continuation(
    body: &NativeExpr,
    callee_capture_count: usize,
    caller_capture_count: usize,
    wrapper_ids: &HashMap<u64, u64>,
    completion_id: u64,
) -> Result<NativeExpr, String> {
    wrap_composed_continuation_at(
        body,
        callee_capture_count,
        caller_capture_count,
        callee_capture_count.saturating_add(caller_capture_count),
        wrapper_ids,
        completion_id,
    )
}

fn wrap_composed_continuation_at(
    body: &NativeExpr,
    callee_capture_count: usize,
    caller_capture_count: usize,
    local_count: usize,
    wrapper_ids: &HashMap<u64, u64>,
    completion_id: u64,
) -> Result<NativeExpr, String> {
    let caller_captures = || {
        (0..caller_capture_count)
            .map(|index| NativeExpr::Param(callee_capture_count.saturating_add(index)))
            .collect::<Vec<_>>()
    };
    match body {
        NativeExpr::Let { bindings, body } => Ok(NativeExpr::Let {
            bindings: bindings.clone(),
            body: Box::new(wrap_composed_continuation_at(
                body,
                callee_capture_count,
                caller_capture_count,
                local_count.saturating_add(bindings.len()),
                wrapper_ids,
                completion_id,
            )?),
        }),
        NativeExpr::If { clauses } => Ok(NativeExpr::If {
            clauses: clauses
                .iter()
                .map(|(condition, branch)| {
                    Ok((
                        condition.clone(),
                        wrap_composed_continuation_at(
                            branch,
                            callee_capture_count,
                            caller_capture_count,
                            local_count,
                            wrapper_ids,
                            completion_id,
                        )?,
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
            let continuation_id = wrapper_ids.get(continuation_id).copied().ok_or_else(|| {
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
            callee_continuation_id,
            callee_capture_count: nested_capture_count,
            continuation_id,
            completion_continuation_id,
            completion_function: _,
            values,
        } => {
            let continuation_id = wrapper_ids.get(continuation_id).copied().ok_or_else(|| {
                format!(
                    "error[native_ir.call_then]: callee continuation {continuation_id} is outside its composed profile"
                )
            })?;
            let completion_continuation_id = wrapper_ids
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
                callee_continuation_id: *callee_continuation_id,
                callee_capture_count: *nested_capture_count,
                continuation_id,
                completion_continuation_id,
                completion_function: None,
                values,
            })
        }
        NativeExpr::ContinuationTailCall {
            continuation_id,
            args,
        } => {
            let continuation_id = wrapper_ids.get(continuation_id).copied().ok_or_else(|| {
                format!(
                    "error[native_ir.call_then]: callee continuation {continuation_id} is outside its composed profile"
                )
            })?;
            let mut args = args.clone();
            args.extend(caller_captures());
            Ok(NativeExpr::ContinuationTailCall {
                continuation_id,
                args,
            })
        }
        NativeExpr::TailCall { .. } => Err(
            "error[native_ir.call_then]: a composed continuation cannot hide a suspending tail call"
                .to_string(),
        ),
        completed => {
            let mut args = caller_captures();
            args.push(NativeExpr::Param(local_count));
            Ok(NativeExpr::Let {
                bindings: vec![completed.clone()],
                body: Box::new(NativeExpr::ContinuationTailCall {
                    continuation_id: completion_id,
                    args,
                }),
            })
        }
    }
}
