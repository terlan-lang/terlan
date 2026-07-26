//! Admission and evaluation-context extraction for bounded suspending calls.

use std::collections::HashSet;

use crate::terlan_typeck::{CoreExpr, CoreLetBinding, CorePattern};

use super::{
    contains_process_yield, expr_calls_suspending, free_variables, is_process_transition,
    NativeContinuation, NativeExpr, NativeType,
};

const MAX_COMPOSED_CALL_CONTINUATIONS: usize = 1_024;

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
    pub(super) continuations: Vec<ComposedContinuationProfile>,
}

/// The bounded ABI metadata needed to call a shared continuation body.
#[derive(Clone)]
pub(super) struct ComposedContinuationProfile {
    pub(super) id: u64,
    pub(super) params: Vec<NativeType>,
    pub(super) body: NativeExpr,
}

#[derive(Clone, Debug)]
pub(super) struct CallRegion {
    pub(super) prefix: Vec<CoreLetBinding>,
    pub(super) function: String,
    pub(super) args: Vec<CoreExpr>,
    pub(super) resume: CoreExpr,
    pub(super) result_name: String,
    pub(super) gates: Vec<CallGate>,
    pub(super) join: Option<CallJoin>,
}

#[derive(Clone, Debug)]
pub(super) struct CallJoin {
    pub(super) result_name: String,
    pub(super) resume: CoreExpr,
}

/// One short-circuit decision that must be evaluated before entering a call.
#[derive(Clone, Debug)]
pub(super) struct CallGate {
    pub(super) condition: CoreExpr,
    pub(super) call_when_true: bool,
    pub(super) prefix: Vec<CoreLetBinding>,
    pub(super) bypass_resume: CoreExpr,
}

fn unique_prefix_name(
    base: &str,
    region: &CallRegion,
    extra: &[CoreLetBinding],
    reserved: &HashSet<String>,
) -> String {
    let mut used = free_variables(&region.resume);
    for gate in &region.gates {
        used.extend(free_variables(&gate.condition));
        used.extend(free_variables(&gate.bypass_resume));
    }
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

pub(super) fn is_composable_suspending_body(
    body: &CoreExpr,
    suspending: &HashSet<(String, usize)>,
    composable: &HashSet<(String, usize)>,
) -> bool {
    let continuation_count =
        process_yield_count(body).saturating_add(suspending_call_count(body, suspending));
    (1..=MAX_COMPOSED_CALL_CONTINUATIONS).contains(&continuation_count)
        && !has_ambiguous_yield_branches(body)
        && suspending_calls_are_composable(body, suspending, composable)
}

fn suspending_calls_are_composable(
    expr: &CoreExpr,
    suspending: &HashSet<(String, usize)>,
    composable: &HashSet<(String, usize)>,
) -> bool {
    if let CoreExpr::Call { function, args } = expr {
        let identity = (function.clone(), args.len());
        if suspending.contains(&identity) && !composable.contains(&identity) {
            return false;
        }
    }
    match expr {
        CoreExpr::Call { args, .. }
        | CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Intrinsic(crate::terlan_typeck::CoreIntrinsicCall { args, .. }) => args
            .iter()
            .all(|arg| suspending_calls_are_composable(arg, suspending, composable)),
        CoreExpr::RecordConstruct { fields, .. } => fields
            .iter()
            .all(|field| suspending_calls_are_composable(&field.value, suspending, composable)),
        CoreExpr::RecordUpdate { base, fields, .. } => {
            suspending_calls_are_composable(base, suspending, composable)
                && fields.iter().all(|field| {
                    suspending_calls_are_composable(&field.value, suspending, composable)
                })
        }
        CoreExpr::FieldAccess { base, .. } | CoreExpr::RecordAccess { base, .. } => {
            suspending_calls_are_composable(base, suspending, composable)
        }
        CoreExpr::UnaryOp { operand, .. } => {
            suspending_calls_are_composable(operand, suspending, composable)
        }
        CoreExpr::BinaryOp { left, right, .. } => {
            suspending_calls_are_composable(left, suspending, composable)
                && suspending_calls_are_composable(right, suspending, composable)
        }
        CoreExpr::Let { bindings, body } => {
            bindings.iter().all(|binding| {
                suspending_calls_are_composable(&binding.value, suspending, composable)
            }) && suspending_calls_are_composable(body, suspending, composable)
        }
        CoreExpr::If { clauses } => clauses.iter().all(|clause| {
            suspending_calls_are_composable(&clause.condition, suspending, composable)
                && suspending_calls_are_composable(&clause.body, suspending, composable)
        }),
        _ => true,
    }
}

impl ComposedCallProfile {
    pub(super) fn new(
        function_body: &NativeExpr,
        continuations: &[NativeContinuation],
    ) -> Option<Self> {
        if continuations.is_empty() || continuations.len() > MAX_COMPOSED_CALL_CONTINUATIONS {
            return None;
        }

        let by_id = continuations
            .iter()
            .map(|continuation| (continuation.id, continuation))
            .collect::<std::collections::HashMap<_, _>>();
        if by_id.len() != continuations.len() {
            return None;
        }
        let mut next_ids = unique_direct_resume_ids(function_body);
        if next_ids.len() != 1 {
            return None;
        }

        let mut ordered = Vec::with_capacity(continuations.len());
        let mut visited = HashSet::with_capacity(continuations.len());
        while let Some(id) = next_ids.pop() {
            if !visited.insert(id) {
                continue;
            }
            let continuation = by_id.get(&id).copied()?;
            ordered.push(ComposedContinuationProfile {
                id,
                params: continuation.params.clone(),
                body: continuation.body.clone(),
            });
            next_ids.extend(unique_direct_resume_ids(&continuation.body));
        }
        if visited.len() != continuations.len() {
            return None;
        }
        Some(Self {
            continuations: ordered,
        })
    }
}

fn unique_direct_resume_ids(body: &NativeExpr) -> Vec<u64> {
    let mut ids = direct_suspend_ids(body);
    ids.sort_unstable();
    ids.dedup();
    ids
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
            callee_continuation_id,
            callee_capture_count,
            continuation_id,
            completion_continuation_id,
            completion_function,
            values,
        } => NativeExpr::CallThen {
            function: *function,
            args: args
                .iter()
                .map(|arg| rebase_callee_locals(arg, callee_param_count, caller_param_count))
                .collect(),
            callee_continuation_id: *callee_continuation_id,
            callee_capture_count: *callee_capture_count,
            continuation_id: *continuation_id,
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

fn direct_suspend_ids(body: &NativeExpr) -> Vec<u64> {
    match body {
        NativeExpr::Suspend {
            continuation_id, ..
        } => vec![*continuation_id],
        NativeExpr::CallThen {
            continuation_id, ..
        }
        | NativeExpr::ContinuationTailCall {
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
                gates: Vec::new(),
                join: None,
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
                resumed_args[call_index] = region.resume.clone();
                region.prefix = evaluated_prefix;
                return Some(map_region_resumes(region, |resume| {
                    let mut args = resumed_args.clone();
                    args[call_index] = resume;
                    CoreExpr::Call {
                        function: function.clone(),
                        args,
                    }
                }));
            }
            None
        }
        CoreExpr::Intrinsic(call) if !is_process_transition(expr) && !call.args.is_empty() => {
            for (call_index, arg) in call.args.iter().enumerate() {
                let Some(mut region) =
                    composed_call_region_at(arg, suspending, is_composable, result_name, reserved)
                else {
                    if expr_calls_suspending(arg, suspending) || contains_process_yield(arg) {
                        return None;
                    }
                    continue;
                };
                let mut resumed_args = call.args.clone();
                let mut evaluated_prefix = Vec::with_capacity(call_index + region.prefix.len());
                for (index, earlier) in call.args[..call_index].iter().enumerate() {
                    let name = unique_prefix_name(
                        &format!("$native_intrinsic_arg_{index}"),
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
                resumed_args[call_index] = region.resume.clone();
                region.prefix = evaluated_prefix;
                return Some(map_region_resumes(region, |resume| {
                    let mut resumed = call.clone();
                    let mut args = resumed_args.clone();
                    args[call_index] = resume;
                    resumed.args = args;
                    CoreExpr::Intrinsic(resumed)
                }));
            }
            None
        }
        CoreExpr::UnaryOp { operator, operand } => {
            let region =
                composed_call_region_at(operand, suspending, is_composable, result_name, reserved)?;
            Some(map_region_resumes(region, |resume| CoreExpr::UnaryOp {
                operator: operator.clone(),
                operand: Box::new(resume),
            }))
        }
        CoreExpr::ListCons { head, tail } => {
            if let Some(region) =
                composed_call_region_at(head, suspending, is_composable, result_name, reserved)
            {
                return Some(map_region_resumes(region, |resume| CoreExpr::ListCons {
                    head: Box::new(resume),
                    tail: tail.clone(),
                }));
            }
            if expr_calls_suspending(head, suspending) || contains_process_yield(head) {
                return None;
            }
            let mut region =
                composed_call_region_at(tail, suspending, is_composable, result_name, reserved)?;
            let head_name = unique_prefix_name("$native_list_head", &region, &[], reserved);
            region.prefix.insert(
                0,
                CoreLetBinding {
                    pattern: CorePattern::Var(head_name.clone()),
                    value: head.as_ref().clone(),
                },
            );
            Some(map_region_resumes(region, |resume| CoreExpr::ListCons {
                head: Box::new(CoreExpr::Var(head_name.clone())),
                tail: Box::new(resume),
            }))
        }
        CoreExpr::BinaryOp {
            operator,
            left,
            right,
        } => {
            if let Some(region) =
                composed_call_region_at(left, suspending, is_composable, result_name, reserved)
            {
                return Some(map_region_resumes(region, |resume| CoreExpr::BinaryOp {
                    operator: operator.clone(),
                    left: Box::new(resume),
                    right: right.clone(),
                }));
            }
            if expr_calls_suspending(left, suspending) || contains_process_yield(left) {
                return None;
            }
            let mut region =
                composed_call_region_at(right, suspending, is_composable, result_name, reserved)?;
            if matches!(operator.as_str(), "and" | "&&" | "or" | "||") {
                let gated_prefix = std::mem::take(&mut region.prefix);
                let call_when_true = matches!(operator.as_str(), "and" | "&&");
                if gated_prefix.is_empty()
                    && region
                        .gates
                        .first()
                        .is_some_and(|gate| gate.call_when_true == call_when_true)
                {
                    let gate = &mut region.gates[0];
                    gate.condition = CoreExpr::BinaryOp {
                        operator: operator.clone(),
                        left: left.clone(),
                        right: Box::new(gate.condition.clone()),
                    };
                    return Some(region);
                }
                region.gates.insert(
                    0,
                    CallGate {
                        condition: left.as_ref().clone(),
                        call_when_true,
                        prefix: gated_prefix,
                        bypass_resume: CoreExpr::Atom(
                            if matches!(operator.as_str(), "or" | "||") {
                                "true"
                            } else {
                                "false"
                            }
                            .to_string(),
                        ),
                    },
                );
                return Some(region);
            }
            let left_name = unique_prefix_name("$native_call_left", &region, &[], reserved);
            let left_binding = CoreLetBinding {
                pattern: CorePattern::Var(left_name.clone()),
                value: left.as_ref().clone(),
            };
            region.prefix.insert(0, left_binding);
            Some(map_region_resumes(region, |resume| CoreExpr::BinaryOp {
                operator: operator.clone(),
                left: Box::new(CoreExpr::Var(left_name.clone())),
                right: Box::new(resume),
            }))
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
                resumed_bindings[0].value = region.resume.clone();
                region.prefix = evaluated_prefix;
                return Some(map_region_resumes(region, |resume| {
                    let mut bindings = resumed_bindings.clone();
                    bindings[0].value = resume;
                    CoreExpr::Let {
                        bindings,
                        body: body.clone(),
                    }
                }));
            }
            let mut region =
                composed_call_region_at(body, suspending, is_composable, result_name, reserved)?;
            let mut evaluated_prefix = bindings.clone();
            evaluated_prefix.append(&mut region.prefix);
            region.prefix = evaluated_prefix;
            Some(region)
        }
        _ => None,
    }
}

/// Applies one surrounding evaluation context to both the call result and its
/// short-circuit bypass result.
fn map_region_resumes(
    mut region: CallRegion,
    mut map: impl FnMut(CoreExpr) -> CoreExpr,
) -> CallRegion {
    if let Some(join) = &mut region.join {
        join.resume = map(join.resume.clone());
    } else if region.gates.is_empty() {
        region.resume = map(region.resume);
    } else {
        let result_name = unique_prefix_name("$native_gate_result", &region, &[], &HashSet::new());
        region.join = Some(CallJoin {
            result_name: result_name.clone(),
            resume: map(CoreExpr::Var(result_name)),
        });
    }
    region
}
