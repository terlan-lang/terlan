//! Admission and evaluation-context extraction for bounded suspending calls.

use std::collections::HashSet;

use crate::terlan_typeck::{CoreExpr, CoreFunction, CoreLetBinding, CorePattern};

use super::{
    contains_process_yield, expr_calls_suspending, free_variables, is_process_transition,
    NativeContinuation, NativeExpr,
};

const MAX_COMPOSED_CALL_CONTINUATIONS: usize = 8;

pub(super) fn suspending_call_count(
    expr: &CoreExpr,
    suspending: &HashSet<(String, usize)>,
) -> usize {
    let own = match expr {
        CoreExpr::Call { function, args } => {
            usize::from(suspending.contains(&(function.clone(), args.len())))
        }
        _ => 0,
    };
    own + match expr {
        CoreExpr::Call { args, .. }
        | CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Intrinsic(crate::terlan_typeck::CoreIntrinsicCall { args, .. }) => args
            .iter()
            .map(|arg| suspending_call_count(arg, suspending))
            .sum(),
        CoreExpr::RecordConstruct { fields, .. } => fields
            .iter()
            .map(|field| suspending_call_count(&field.value, suspending))
            .sum(),
        CoreExpr::RecordUpdate { base, fields, .. } => {
            suspending_call_count(base, suspending)
                + fields
                    .iter()
                    .map(|field| suspending_call_count(&field.value, suspending))
                    .sum::<usize>()
        }
        CoreExpr::FieldAccess { base, .. } | CoreExpr::RecordAccess { base, .. } => {
            suspending_call_count(base, suspending)
        }
        CoreExpr::UnaryOp { operand, .. } => suspending_call_count(operand, suspending),
        CoreExpr::BinaryOp { left, right, .. } => {
            suspending_call_count(left, suspending) + suspending_call_count(right, suspending)
        }
        CoreExpr::Let { bindings, body } => {
            bindings
                .iter()
                .map(|binding| suspending_call_count(&binding.value, suspending))
                .sum::<usize>()
                + suspending_call_count(body, suspending)
        }
        CoreExpr::If { clauses } => clauses
            .iter()
            .map(|clause| {
                suspending_call_count(&clause.condition, suspending)
                    + suspending_call_count(&clause.body, suspending)
            })
            .sum(),
        _ => 0,
    }
}

#[derive(Clone)]
pub(super) struct ComposedCallProfile {
    pub(super) continuations: Vec<NativeContinuation>,
}

pub(super) struct CallRegion {
    pub(super) prefix: Vec<CoreLetBinding>,
    pub(super) function: String,
    pub(super) args: Vec<CoreExpr>,
    pub(super) resume: CoreExpr,
    pub(super) result_name: String,
}

fn unique_prefix_name(
    base: &str,
    region: &CallRegion,
    extra: &[CoreLetBinding],
    reserved: &HashSet<String>,
) -> String {
    let mut used = free_variables(&region.resume);
    used.extend(reserved.iter().cloned());
    used.extend(
        region
            .prefix
            .iter()
            .chain(extra)
            .filter_map(|binding| match &binding.pattern {
                CorePattern::Var(name) => Some(name.clone()),
                _ => None,
            }),
    );
    (0usize..)
        .map(|index| format!("{base}_{index}"))
        .find(|name| !used.contains(name))
        .expect("generated prefix name space is unbounded")
}

pub(super) fn composable_suspending_functions(
    functions: &[&CoreFunction],
    suspending: &HashSet<(String, usize)>,
) -> HashSet<(String, usize)> {
    functions
        .iter()
        .filter(|function| {
            function
                .clauses
                .first()
                .and_then(|clause| clause.body.core_expr.as_ref())
                .is_some_and(|body| {
                    let yield_count = process_yield_count(body);
                    (1..=MAX_COMPOSED_CALL_CONTINUATIONS).contains(&yield_count)
                        && !has_ambiguous_yield_branches(body)
                        && !expr_calls_suspending(body, suspending)
                })
        })
        .map(|function| (function.name.clone(), function.arity))
        .collect()
}

impl ComposedCallProfile {
    pub(super) fn new(
        function_body: &NativeExpr,
        continuations: &[NativeContinuation],
    ) -> Option<Self> {
        if continuations.is_empty()
            || continuations.len() > MAX_COMPOSED_CALL_CONTINUATIONS
            || direct_suspend_ids(function_body) != vec![continuations[0].id]
        {
            return None;
        }
        for (index, continuation) in continuations.iter().enumerate() {
            let ids = direct_suspend_ids(&continuation.body);
            if let Some(next) = continuations.get(index + 1) {
                if ids != vec![next.id] || !guarantees_suspension(&continuation.body) {
                    return None;
                }
            } else if !ids.is_empty() {
                return None;
            }
        }
        Some(Self {
            continuations: continuations.to_vec(),
        })
    }
}

pub(super) fn rewrite_linear_suspension(
    body: &NativeExpr,
    expected_callee_id: u64,
    wrapper_id: u64,
    caller_capture_start: usize,
    caller_capture_count: usize,
) -> Result<NativeExpr, String> {
    match body {
        NativeExpr::Suspend {
            operation,
            arguments,
            continuation_id,
            values,
        } if *continuation_id == expected_callee_id => {
            let mut wrapped_values = values.clone();
            wrapped_values.extend(
                (0..caller_capture_count)
                    .map(|index| NativeExpr::Param(caller_capture_start + index)),
            );
            Ok(NativeExpr::Suspend {
                operation: *operation,
                arguments: arguments.clone(),
                continuation_id: wrapper_id,
                values: wrapped_values,
            })
        }
        NativeExpr::Let { bindings, body } => Ok(NativeExpr::Let {
            bindings: bindings.clone(),
            body: Box::new(rewrite_linear_suspension(
                body,
                expected_callee_id,
                wrapper_id,
                caller_capture_start,
                caller_capture_count,
            )?),
        }),
        NativeExpr::If { clauses } => Ok(NativeExpr::If {
            clauses: clauses
                .iter()
                .map(|(condition, body)| {
                    Ok((
                        condition.clone(),
                        rewrite_linear_suspension(
                            body,
                            expected_callee_id,
                            wrapper_id,
                            caller_capture_start,
                            caller_capture_count,
                        )?,
                    ))
                })
                .collect::<Result<Vec<_>, String>>()?,
        }),
        _ => Err(
            "error[native_ir.call_chain]: linear continuation has a non-suspending path"
                .to_string(),
        ),
    }
}

pub(super) fn rebase_callee_locals(
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
        NativeExpr::Call { function, args } => NativeExpr::Call {
            function: *function,
            args: args
                .iter()
                .map(|arg| rebase_callee_locals(arg, callee_param_count, caller_param_count))
                .collect(),
        },
        NativeExpr::TailCall { function, args } => NativeExpr::TailCall {
            function: *function,
            args: args
                .iter()
                .map(|arg| rebase_callee_locals(arg, callee_param_count, caller_param_count))
                .collect(),
        },
        NativeExpr::CallThen {
            function,
            args,
            callee_continuation_id,
            callee_capture_count,
            continuation_id,
            values,
            resume,
        } => NativeExpr::CallThen {
            function: *function,
            args: args
                .iter()
                .map(|arg| rebase_callee_locals(arg, callee_param_count, caller_param_count))
                .collect(),
            callee_continuation_id: *callee_continuation_id,
            callee_capture_count: *callee_capture_count,
            continuation_id: *continuation_id,
            values: values
                .iter()
                .map(|value| rebase_callee_locals(value, callee_param_count, caller_param_count))
                .collect(),
            resume: Box::new(rebase_callee_locals(
                resume,
                callee_param_count,
                caller_param_count,
            )),
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

fn direct_suspend_ids(body: &NativeExpr) -> Vec<u64> {
    match body {
        NativeExpr::Suspend {
            continuation_id, ..
        } => vec![*continuation_id],
        NativeExpr::Let { body, .. } => direct_suspend_ids(body),
        NativeExpr::If { clauses } => clauses
            .iter()
            .flat_map(|(_, body)| direct_suspend_ids(body))
            .collect(),
        _ => Vec::new(),
    }
}

fn guarantees_suspension(body: &NativeExpr) -> bool {
    match body {
        NativeExpr::Suspend { .. } => true,
        NativeExpr::Let { body, .. } => guarantees_suspension(body),
        NativeExpr::If { clauses } => {
            !clauses.is_empty() && clauses.iter().all(|(_, body)| guarantees_suspension(body))
        }
        _ => false,
    }
}

fn process_yield_count(expr: &CoreExpr) -> usize {
    let own = usize::from(is_process_transition(expr));
    own + match expr {
        CoreExpr::Call { args, .. }
        | CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Intrinsic(crate::terlan_typeck::CoreIntrinsicCall { args, .. }) => {
            args.iter().map(process_yield_count).sum()
        }
        CoreExpr::RecordConstruct { fields, .. } => fields
            .iter()
            .map(|field| process_yield_count(&field.value))
            .sum(),
        CoreExpr::RecordUpdate { base, fields, .. } => {
            process_yield_count(base)
                + fields
                    .iter()
                    .map(|field| process_yield_count(&field.value))
                    .sum::<usize>()
        }
        CoreExpr::FieldAccess { base, .. } | CoreExpr::RecordAccess { base, .. } => {
            process_yield_count(base)
        }
        CoreExpr::UnaryOp { operand, .. } => process_yield_count(operand),
        CoreExpr::BinaryOp { left, right, .. } => {
            process_yield_count(left) + process_yield_count(right)
        }
        CoreExpr::Let { bindings, body } => {
            bindings
                .iter()
                .map(|binding| process_yield_count(&binding.value))
                .sum::<usize>()
                + process_yield_count(body)
        }
        CoreExpr::If { clauses } => clauses
            .iter()
            .map(|clause| {
                process_yield_count(&clause.condition) + process_yield_count(&clause.body)
            })
            .sum(),
        _ => 0,
    }
}

fn has_ambiguous_yield_branches(expr: &CoreExpr) -> bool {
    match expr {
        CoreExpr::If { clauses } => {
            clauses
                .iter()
                .filter(|clause| {
                    contains_process_yield(&clause.condition)
                        || contains_process_yield(&clause.body)
                })
                .count()
                > 1
                || clauses.iter().any(|clause| {
                    has_ambiguous_yield_branches(&clause.condition)
                        || has_ambiguous_yield_branches(&clause.body)
                })
        }
        CoreExpr::Call { args, .. }
        | CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Intrinsic(crate::terlan_typeck::CoreIntrinsicCall { args, .. }) => {
            args.iter().any(has_ambiguous_yield_branches)
        }
        CoreExpr::RecordConstruct { fields, .. } => fields
            .iter()
            .any(|field| has_ambiguous_yield_branches(&field.value)),
        CoreExpr::RecordUpdate { base, fields, .. } => {
            has_ambiguous_yield_branches(base)
                || fields
                    .iter()
                    .any(|field| has_ambiguous_yield_branches(&field.value))
        }
        CoreExpr::FieldAccess { base, .. } | CoreExpr::RecordAccess { base, .. } => {
            has_ambiguous_yield_branches(base)
        }
        CoreExpr::UnaryOp { operand, .. } => has_ambiguous_yield_branches(operand),
        CoreExpr::BinaryOp { left, right, .. } => {
            has_ambiguous_yield_branches(left) || has_ambiguous_yield_branches(right)
        }
        CoreExpr::Let { bindings, body } => {
            bindings
                .iter()
                .any(|binding| has_ambiguous_yield_branches(&binding.value))
                || has_ambiguous_yield_branches(body)
        }
        _ => false,
    }
}

pub(super) fn composed_call_region<F>(
    expr: &CoreExpr,
    suspending: &HashSet<(String, usize)>,
    is_composable: &F,
    reserved: &HashSet<String>,
) -> Option<CallRegion>
where
    F: Fn(&str, usize) -> bool,
{
    let result_name = "$native_call_result".to_string();
    composed_call_region_at(expr, suspending, is_composable, &result_name, reserved)
}

fn composed_call_region_at<F>(
    expr: &CoreExpr,
    suspending: &HashSet<(String, usize)>,
    is_composable: &F,
    result_name: &str,
    reserved: &HashSet<String>,
) -> Option<CallRegion>
where
    F: Fn(&str, usize) -> bool,
{
    match expr {
        CoreExpr::Call { function, args }
            if is_composable(function, args.len())
                && args.iter().all(|arg| {
                    !expr_calls_suspending(arg, suspending) && !contains_process_yield(arg)
                }) =>
        {
            Some(CallRegion {
                prefix: Vec::new(),
                function: function.clone(),
                args: args.clone(),
                resume: CoreExpr::Var(result_name.to_string()),
                result_name: result_name.to_string(),
            })
        }
        CoreExpr::Call { function, args }
            if !suspending.contains(&(function.clone(), args.len())) && !args.is_empty() =>
        {
            for (call_index, arg) in args.iter().enumerate() {
                let Some(mut region) =
                    composed_call_region_at(arg, suspending, is_composable, result_name, reserved)
                else {
                    if expr_calls_suspending(arg, suspending) || contains_process_yield(arg) {
                        return None;
                    }
                    continue;
                };
                let mut resumed_args = args.clone();
                let mut evaluated_prefix = Vec::with_capacity(call_index + region.prefix.len());
                for (index, earlier) in args[..call_index].iter().enumerate() {
                    let name = unique_prefix_name(
                        &format!("$native_call_arg_{index}"),
                        &region,
                        &evaluated_prefix,
                        reserved,
                    );
                    evaluated_prefix.push(CoreLetBinding {
                        pattern: CorePattern::Var(name.clone()),
                        value: earlier.clone(),
                    });
                    resumed_args[index] = CoreExpr::Var(name);
                }
                evaluated_prefix.append(&mut region.prefix);
                resumed_args[call_index] = region.resume;
                return Some(CallRegion {
                    prefix: evaluated_prefix,
                    function: region.function,
                    args: region.args,
                    resume: CoreExpr::Call {
                        function: function.clone(),
                        args: resumed_args,
                    },
                    result_name: region.result_name,
                });
            }
            None
        }
        CoreExpr::UnaryOp { operator, operand } => {
            let region =
                composed_call_region_at(operand, suspending, is_composable, result_name, reserved)?;
            Some(CallRegion {
                prefix: region.prefix,
                function: region.function,
                args: region.args,
                resume: CoreExpr::UnaryOp {
                    operator: operator.clone(),
                    operand: Box::new(region.resume),
                },
                result_name: region.result_name,
            })
        }
        CoreExpr::BinaryOp {
            operator,
            left,
            right,
        } => {
            if let Some(region) =
                composed_call_region_at(left, suspending, is_composable, result_name, reserved)
            {
                return Some(CallRegion {
                    prefix: region.prefix,
                    function: region.function,
                    args: region.args,
                    resume: CoreExpr::BinaryOp {
                        operator: operator.clone(),
                        left: Box::new(region.resume),
                        right: right.clone(),
                    },
                    result_name: region.result_name,
                });
            }
            if matches!(operator.as_str(), "and" | "&&" | "or" | "||")
                || expr_calls_suspending(left, suspending)
                || contains_process_yield(left)
            {
                return None;
            }
            let mut region =
                composed_call_region_at(right, suspending, is_composable, result_name, reserved)?;
            let left_name = unique_prefix_name("$native_call_left", &region, &[], reserved);
            region.prefix.insert(
                0,
                CoreLetBinding {
                    pattern: CorePattern::Var(left_name.clone()),
                    value: left.as_ref().clone(),
                },
            );
            Some(CallRegion {
                prefix: region.prefix,
                function: region.function,
                args: region.args,
                resume: CoreExpr::BinaryOp {
                    operator: operator.clone(),
                    left: Box::new(CoreExpr::Var(left_name)),
                    right: Box::new(region.resume),
                },
                result_name: region.result_name,
            })
        }
        CoreExpr::Let { bindings, body } if !bindings.is_empty() => {
            for (binding_index, binding) in bindings.iter().enumerate() {
                let Some(mut region) = composed_call_region_at(
                    &binding.value,
                    suspending,
                    is_composable,
                    result_name,
                    reserved,
                ) else {
                    if expr_calls_suspending(&binding.value, suspending)
                        || contains_process_yield(&binding.value)
                    {
                        return None;
                    }
                    continue;
                };
                let mut evaluated_prefix = bindings[..binding_index].to_vec();
                evaluated_prefix.append(&mut region.prefix);
                let mut resumed_bindings = bindings[binding_index..].to_vec();
                resumed_bindings[0].value = region.resume;
                return Some(CallRegion {
                    prefix: evaluated_prefix,
                    function: region.function,
                    args: region.args,
                    resume: CoreExpr::Let {
                        bindings: resumed_bindings,
                        body: body.clone(),
                    },
                    result_name: region.result_name,
                });
            }
            let mut region =
                composed_call_region_at(body, suspending, is_composable, result_name, reserved)?;
            let mut evaluated_prefix = bindings.clone();
            evaluated_prefix.append(&mut region.prefix);
            Some(CallRegion {
                prefix: evaluated_prefix,
                function: region.function,
                args: region.args,
                resume: region.resume,
                result_name: region.result_name,
            })
        }
        _ => None,
    }
}
