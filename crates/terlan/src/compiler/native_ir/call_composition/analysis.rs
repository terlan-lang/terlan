//! Native expression suspension and continuation analysis.

use super::*;

pub(in crate::compiler::native_ir) fn has_uncomposed_suspending_call(
    expr: &NativeExpr,
    suspending: &HashSet<usize>,
) -> bool {
    match expr {
        NativeExpr::Call { function, args } => {
            suspending.contains(function)
                || args
                    .iter()
                    .any(|arg| has_uncomposed_suspending_call(arg, suspending))
        }
        NativeExpr::Construct { fields, .. } => fields
            .iter()
            .any(|field| has_uncomposed_suspending_call(field, suspending)),
        NativeExpr::ManagedOperation { args, .. }
        | NativeExpr::ContinuationTailCall { args, .. }
        | NativeExpr::TailCall { args, .. } => args
            .iter()
            .any(|arg| has_uncomposed_suspending_call(arg, suspending)),
        NativeExpr::MakeClosure { captures, .. } => captures
            .iter()
            .any(|capture| has_uncomposed_suspending_call(capture, suspending)),
        NativeExpr::InvokeClosure { callee, args, .. } => {
            has_uncomposed_suspending_call(callee, suspending)
                || args
                    .iter()
                    .any(|arg| has_uncomposed_suspending_call(arg, suspending))
        }
        NativeExpr::InvokeClosureThen {
            callee,
            args,
            values,
            ..
        } => {
            has_uncomposed_suspending_call(callee, suspending)
                || args
                    .iter()
                    .chain(values)
                    .any(|arg| has_uncomposed_suspending_call(arg, suspending))
        }
        NativeExpr::CallThen { args, values, .. }
        | NativeExpr::Suspend {
            arguments: args,
            values,
            ..
        } => args
            .iter()
            .chain(values)
            .any(|value| has_uncomposed_suspending_call(value, suspending)),
        NativeExpr::Neg(value)
        | NativeExpr::FloatNeg(value)
        | NativeExpr::FloatFloor(value)
        | NativeExpr::FloatCeil(value)
        | NativeExpr::IntToFloat(value)
        | NativeExpr::Not(value) => has_uncomposed_suspending_call(value, suspending),
        NativeExpr::Binary { left, right, .. } => {
            has_uncomposed_suspending_call(left, suspending)
                || has_uncomposed_suspending_call(right, suspending)
        }
        NativeExpr::Let { bindings, body } => {
            bindings
                .iter()
                .any(|binding| has_uncomposed_suspending_call(binding, suspending))
                || has_uncomposed_suspending_call(body, suspending)
        }
        NativeExpr::If { clauses } => clauses.iter().any(|(condition, body)| {
            has_uncomposed_suspending_call(condition, suspending)
                || has_uncomposed_suspending_call(body, suspending)
        }),
        NativeExpr::Try {
            protected,
            success,
            failure,
            cleanup,
        } => {
            has_uncomposed_suspending_call(protected, suspending)
                || has_uncomposed_suspending_call(success, suspending)
                || has_uncomposed_suspending_call(failure, suspending)
                || cleanup
                    .iter()
                    .any(|value| has_uncomposed_suspending_call(value, suspending))
        }
        NativeExpr::Unit
        | NativeExpr::Int(_)
        | NativeExpr::Float(_)
        | NativeExpr::Bool(_)
        | NativeExpr::AtomLiteral(_)
        | NativeExpr::StringLiteral { .. }
        | NativeExpr::Param(_) => false,
    }
}

/// Proves that a lowered body cannot expose a scheduler transition.
///
/// A missing composed profile is not evidence of purity: it can also mean a
/// callee profile has not reached the current fixed-point phase. Only bodies
/// with no transition node, indirect invocation, or call into the known
/// suspending set may receive an explicit empty dynamic-call profile.
pub(in crate::compiler::native_ir) fn is_definitely_non_suspending(
    body: &NativeExpr,
    suspending: &HashSet<usize>,
) -> bool {
    let mut non_suspending = true;
    walk_native_expr(body, &mut |expr| match expr {
        NativeExpr::Suspend { .. }
        | NativeExpr::CallThen { .. }
        | NativeExpr::InvokeClosure { .. }
        | NativeExpr::InvokeClosureThen { .. }
        | NativeExpr::ContinuationTailCall { .. } => non_suspending = false,
        NativeExpr::Call { function, .. } | NativeExpr::TailCall { function, .. }
            if suspending.contains(function) =>
        {
            non_suspending = false;
        }
        _ => {}
    });
    non_suspending
}

pub(in crate::compiler::native_ir) fn rebase_callee_locals(
    body: &NativeExpr,
    callee_param_count: usize,
    caller_param_count: usize,
) -> NativeExpr {
    match body {
        NativeExpr::Param(index) if *index >= callee_param_count => {
            NativeExpr::Param(index.saturating_add(caller_param_count))
        }
        NativeExpr::Construct {
            descriptor,
            encoded_layout,
            fields,
        } => NativeExpr::Construct {
            descriptor: descriptor.clone(),
            encoded_layout: encoded_layout.clone(),
            fields: fields
                .iter()
                .map(|field| rebase_callee_locals(field, callee_param_count, caller_param_count))
                .collect(),
        },
        NativeExpr::ManagedOperation { encoded, args } => NativeExpr::ManagedOperation {
            encoded: encoded.clone(),
            args: args
                .iter()
                .map(|arg| rebase_callee_locals(arg, callee_param_count, caller_param_count))
                .collect(),
        },
        NativeExpr::MakeClosure { encoded, captures } => NativeExpr::MakeClosure {
            encoded: encoded.clone(),
            captures: captures
                .iter()
                .map(|capture| {
                    rebase_callee_locals(capture, callee_param_count, caller_param_count)
                })
                .collect(),
        },
        NativeExpr::InvokeClosure {
            callee,
            args,
            parameter_types,
            result_type,
        } => NativeExpr::InvokeClosure {
            callee: Box::new(rebase_callee_locals(
                callee,
                callee_param_count,
                caller_param_count,
            )),
            args: args
                .iter()
                .map(|arg| rebase_callee_locals(arg, callee_param_count, caller_param_count))
                .collect(),
            parameter_types: parameter_types.clone(),
            result_type: *result_type,
        },
        NativeExpr::InvokeClosureThen {
            callee,
            args,
            parameter_types,
            result_type,
            resumes,
            completion_continuation_id,
            completion_function,
            values,
        } => NativeExpr::InvokeClosureThen {
            callee: Box::new(rebase_callee_locals(
                callee,
                callee_param_count,
                caller_param_count,
            )),
            args: args
                .iter()
                .map(|arg| rebase_callee_locals(arg, callee_param_count, caller_param_count))
                .collect(),
            parameter_types: parameter_types.clone(),
            result_type: *result_type,
            resumes: resumes.clone(),
            completion_continuation_id: *completion_continuation_id,
            completion_function: *completion_function,
            values: values
                .iter()
                .map(|value| rebase_callee_locals(value, callee_param_count, caller_param_count))
                .collect(),
        },
        NativeExpr::Call { function, args } => NativeExpr::Call {
            function: *function,
            args: args
                .iter()
                .map(|arg| rebase_callee_locals(arg, callee_param_count, caller_param_count))
                .collect(),
        },
        NativeExpr::TailCall {
            function,
            args,
            yield_continuation_id,
        } => NativeExpr::TailCall {
            function: *function,
            args: args
                .iter()
                .map(|arg| rebase_callee_locals(arg, callee_param_count, caller_param_count))
                .collect(),
            yield_continuation_id: *yield_continuation_id,
        },
        NativeExpr::ContinuationTailCall {
            continuation_id,
            args,
        } => NativeExpr::ContinuationTailCall {
            continuation_id: *continuation_id,
            args: args
                .iter()
                .map(|arg| rebase_callee_locals(arg, callee_param_count, caller_param_count))
                .collect(),
        },
        NativeExpr::CallThen {
            function,
            args,
            resumes,
            completion_continuation_id,
            completion_function,
            values,
        } => NativeExpr::CallThen {
            function: *function,
            args: args
                .iter()
                .map(|arg| rebase_callee_locals(arg, callee_param_count, caller_param_count))
                .collect(),
            resumes: resumes.clone(),
            completion_continuation_id: *completion_continuation_id,
            completion_function: *completion_function,
            values: values
                .iter()
                .map(|value| rebase_callee_locals(value, callee_param_count, caller_param_count))
                .collect(),
        },
        NativeExpr::Neg(operand) => NativeExpr::Neg(Box::new(rebase_callee_locals(
            operand,
            callee_param_count,
            caller_param_count,
        ))),
        NativeExpr::FloatNeg(operand) => NativeExpr::FloatNeg(Box::new(rebase_callee_locals(
            operand,
            callee_param_count,
            caller_param_count,
        ))),
        NativeExpr::FloatFloor(operand) => NativeExpr::FloatFloor(Box::new(rebase_callee_locals(
            operand,
            callee_param_count,
            caller_param_count,
        ))),
        NativeExpr::FloatCeil(operand) => NativeExpr::FloatCeil(Box::new(rebase_callee_locals(
            operand,
            callee_param_count,
            caller_param_count,
        ))),
        NativeExpr::IntToFloat(operand) => NativeExpr::IntToFloat(Box::new(rebase_callee_locals(
            operand,
            callee_param_count,
            caller_param_count,
        ))),
        NativeExpr::Not(operand) => NativeExpr::Not(Box::new(rebase_callee_locals(
            operand,
            callee_param_count,
            caller_param_count,
        ))),
        NativeExpr::Binary {
            operator,
            operand_type,
            left,
            right,
        } => NativeExpr::Binary {
            operator: *operator,
            operand_type: *operand_type,
            left: Box::new(rebase_callee_locals(
                left,
                callee_param_count,
                caller_param_count,
            )),
            right: Box::new(rebase_callee_locals(
                right,
                callee_param_count,
                caller_param_count,
            )),
        },
        NativeExpr::Let { bindings, body } => NativeExpr::Let {
            bindings: bindings
                .iter()
                .map(|binding| {
                    rebase_callee_locals(binding, callee_param_count, caller_param_count)
                })
                .collect(),
            body: Box::new(rebase_callee_locals(
                body,
                callee_param_count,
                caller_param_count,
            )),
        },
        NativeExpr::If { clauses } => NativeExpr::If {
            clauses: clauses
                .iter()
                .map(|(condition, body)| {
                    (
                        rebase_callee_locals(condition, callee_param_count, caller_param_count),
                        rebase_callee_locals(body, callee_param_count, caller_param_count),
                    )
                })
                .collect(),
        },
        NativeExpr::Try {
            protected,
            success,
            failure,
            cleanup,
        } => NativeExpr::Try {
            protected: Box::new(rebase_callee_locals(
                protected,
                callee_param_count,
                caller_param_count,
            )),
            success: Box::new(rebase_callee_locals(
                success,
                callee_param_count,
                caller_param_count,
            )),
            failure: Box::new(rebase_callee_locals(
                failure,
                callee_param_count,
                caller_param_count,
            )),
            cleanup: cleanup
                .iter()
                .map(|value| rebase_callee_locals(value, callee_param_count, caller_param_count))
                .collect(),
        },
        NativeExpr::Suspend {
            operation,
            arguments,
            continuation_id,
            values,
        } => NativeExpr::Suspend {
            operation: *operation,
            arguments: arguments
                .iter()
                .map(|value| rebase_callee_locals(value, callee_param_count, caller_param_count))
                .collect(),
            continuation_id: *continuation_id,
            values: values
                .iter()
                .map(|value| rebase_callee_locals(value, callee_param_count, caller_param_count))
                .collect(),
        },
        _ => body.clone(),
    }
}

pub(in crate::compiler::native_ir) fn direct_suspend_ids(body: &NativeExpr) -> Vec<u64> {
    let mut ids = Vec::new();
    walk_native_expr(body, &mut |expr| match expr {
        NativeExpr::Suspend {
            continuation_id, ..
        } => ids.push(*continuation_id),
        NativeExpr::CallThen { resumes, .. } => {
            ids.extend(resumes.iter().map(|resume| resume.callee_continuation_id));
        }
        NativeExpr::InvokeClosureThen { resumes, .. } => {
            ids.extend(resumes.iter().map(|resume| resume.callee_continuation_id));
        }
        _ => {}
    });
    ids
}

pub(in crate::compiler::native_ir) fn direct_tail_continuation_ids(body: &NativeExpr) -> Vec<u64> {
    let mut ids = Vec::new();
    walk_native_expr(body, &mut |expr| {
        if let NativeExpr::ContinuationTailCall {
            continuation_id, ..
        } = expr
        {
            ids.push(*continuation_id);
        }
    });
    ids
}

pub(in crate::compiler::native_ir) fn direct_completion_ids(body: &NativeExpr) -> Vec<u64> {
    let mut ids = Vec::new();
    walk_native_expr(body, &mut |expr| match expr {
        NativeExpr::CallThen {
            completion_continuation_id,
            ..
        }
        | NativeExpr::InvokeClosureThen {
            completion_continuation_id,
            ..
        } => ids.push(*completion_continuation_id),
        _ => {}
    });
    ids
}

pub(in crate::compiler::native_ir) fn walk_native_expr(
    expr: &NativeExpr,
    visit: &mut impl FnMut(&NativeExpr),
) {
    visit(expr);
    match expr {
        NativeExpr::ManagedOperation { args, .. }
        | NativeExpr::Call { args, .. }
        | NativeExpr::TailCall { args, .. }
        | NativeExpr::ContinuationTailCall { args, .. } => {
            for arg in args {
                walk_native_expr(arg, visit);
            }
        }
        NativeExpr::MakeClosure { captures, .. } => {
            for capture in captures {
                walk_native_expr(capture, visit);
            }
        }
        NativeExpr::Construct { fields, .. } => {
            for field in fields {
                walk_native_expr(field, visit);
            }
        }
        NativeExpr::InvokeClosure { callee, args, .. } => {
            walk_native_expr(callee, visit);
            for arg in args {
                walk_native_expr(arg, visit);
            }
        }
        NativeExpr::InvokeClosureThen {
            callee,
            args,
            values,
            ..
        } => {
            walk_native_expr(callee, visit);
            for value in args.iter().chain(values) {
                walk_native_expr(value, visit);
            }
        }
        NativeExpr::CallThen { args, values, .. }
        | NativeExpr::Suspend {
            arguments: args,
            values,
            ..
        } => {
            for value in args.iter().chain(values) {
                walk_native_expr(value, visit);
            }
        }
        NativeExpr::Neg(value)
        | NativeExpr::FloatNeg(value)
        | NativeExpr::FloatFloor(value)
        | NativeExpr::FloatCeil(value)
        | NativeExpr::IntToFloat(value)
        | NativeExpr::Not(value) => walk_native_expr(value, visit),
        NativeExpr::Binary { left, right, .. } => {
            walk_native_expr(left, visit);
            walk_native_expr(right, visit);
        }
        NativeExpr::Let { bindings, body } => {
            for binding in bindings {
                walk_native_expr(binding, visit);
            }
            walk_native_expr(body, visit);
        }
        NativeExpr::If { clauses } => {
            for (condition, body) in clauses {
                walk_native_expr(condition, visit);
                walk_native_expr(body, visit);
            }
        }
        NativeExpr::Try {
            protected,
            success,
            failure,
            cleanup,
        } => {
            walk_native_expr(protected, visit);
            walk_native_expr(success, visit);
            walk_native_expr(failure, visit);
            for value in cleanup {
                walk_native_expr(value, visit);
            }
        }
        NativeExpr::Unit
        | NativeExpr::Int(_)
        | NativeExpr::Float(_)
        | NativeExpr::Bool(_)
        | NativeExpr::AtomLiteral(_)
        | NativeExpr::StringLiteral { .. }
        | NativeExpr::Param(_) => {}
    }
}
