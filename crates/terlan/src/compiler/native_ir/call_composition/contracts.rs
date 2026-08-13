//! Closure and validation of composed-call continuation contracts.

use std::collections::HashMap;

use super::{walk_native_expr, ComposedCallProfile};
use crate::compiler::native_ir::NativeExpr;

impl ComposedCallProfile {
    /// Merges another member's converged graph into one recursive component.
    pub(in crate::compiler::native_ir) fn merge_recursive_component_profile(
        &mut self,
        other: &Self,
    ) {
        self.entries.extend(other.entries.iter().copied());
        self.entries.sort_unstable();
        self.entries.dedup();
        for continuation in &other.continuations {
            if let Some(existing) = self
                .continuations
                .iter_mut()
                .find(|existing| existing.id == continuation.id)
            {
                // Completion is a profile-local role. An identity exposed by
                // any member is outward in the merged component.
                existing.completion_result &= continuation.completion_result;
                merge_expr_contracts(&mut existing.body, &continuation.body);
            } else {
                self.continuations.push(continuation.clone());
            }
        }
        for (target, entries) in &other.tail_entries {
            let merged = self.tail_entries.entry(*target).or_default();
            merged.extend(entries.iter().copied());
            merged.sort_unstable();
            merged.dedup();
        }
    }

    /// Makes every recursive edge accept every outward component yield.
    pub(in crate::compiler::native_ir) fn refresh_recursive_component_contract(
        &mut self,
        functions: &[usize],
    ) -> Vec<(u64, usize)> {
        let by_id = self
            .continuations
            .iter()
            .map(|continuation| (continuation.id, continuation.params.len()))
            .collect::<HashMap<_, _>>();
        let entries = self
            .entries
            .iter()
            .filter_map(|id| by_id.get(id).map(|count| (*id, *count)))
            .collect::<Vec<_>>();
        if entries.is_empty() {
            return entries;
        }
        for function in functions {
            self.tail_entries.insert(*function, entries.clone());
        }
        for continuation in &mut self.continuations {
            for function in functions {
                refresh_recursive_call_contract(&mut continuation.body, *function, &entries);
            }
        }
        entries
    }
}

/// Monotonically combines resume tables from two generations of one stable
/// continuation body.
fn merge_expr_contracts(existing: &mut NativeExpr, other: &NativeExpr) {
    match (existing, other) {
        (
            NativeExpr::CallThen {
                function: existing_function,
                args: existing_args,
                resumes: existing_resumes,
                values: existing_values,
                ..
            },
            NativeExpr::CallThen {
                function: other_function,
                args: other_args,
                resumes: other_resumes,
                values: other_values,
                ..
            },
        ) if existing_function == other_function => {
            for resume in other_resumes {
                if !existing_resumes.iter().any(|existing| {
                    existing.callee_continuation_id == resume.callee_continuation_id
                }) {
                    existing_resumes.push(*resume);
                }
            }
            merge_expr_slices(existing_args, other_args);
            merge_expr_slices(existing_values, other_values);
        }
        (
            NativeExpr::InvokeClosureThen {
                callee: existing_callee,
                args: existing_args,
                resumes: existing_resumes,
                values: existing_values,
                ..
            },
            NativeExpr::InvokeClosureThen {
                callee: other_callee,
                args: other_args,
                resumes: other_resumes,
                values: other_values,
                ..
            },
        ) => {
            for resume in other_resumes {
                if !existing_resumes.iter().any(|existing| {
                    existing.callee_export_id == resume.callee_export_id
                        && existing.callee_continuation_id == resume.callee_continuation_id
                }) {
                    existing_resumes.push(*resume);
                }
            }
            merge_expr_contracts(existing_callee, other_callee);
            merge_expr_slices(existing_args, other_args);
            merge_expr_slices(existing_values, other_values);
        }
        (
            NativeExpr::ManagedOperation { args: existing, .. }
            | NativeExpr::Call { args: existing, .. }
            | NativeExpr::TailCall { args: existing, .. }
            | NativeExpr::ContinuationTailCall { args: existing, .. },
            NativeExpr::ManagedOperation { args: other, .. }
            | NativeExpr::Call { args: other, .. }
            | NativeExpr::TailCall { args: other, .. }
            | NativeExpr::ContinuationTailCall { args: other, .. },
        ) => merge_expr_slices(existing, other),
        (
            NativeExpr::MakeClosure {
                captures: existing, ..
            },
            NativeExpr::MakeClosure {
                captures: other, ..
            },
        )
        | (
            NativeExpr::Construct {
                fields: existing, ..
            },
            NativeExpr::Construct { fields: other, .. },
        ) => merge_expr_slices(existing, other),
        (
            NativeExpr::InvokeClosure {
                callee: existing_callee,
                args: existing_args,
                ..
            },
            NativeExpr::InvokeClosure {
                callee: other_callee,
                args: other_args,
                ..
            },
        ) => {
            merge_expr_contracts(existing_callee, other_callee);
            merge_expr_slices(existing_args, other_args);
        }
        (
            NativeExpr::Suspend {
                arguments: existing_arguments,
                values: existing_values,
                ..
            },
            NativeExpr::Suspend {
                arguments: other_arguments,
                values: other_values,
                ..
            },
        ) => {
            merge_expr_slices(existing_arguments, other_arguments);
            merge_expr_slices(existing_values, other_values);
        }
        (NativeExpr::Neg(existing), NativeExpr::Neg(other))
        | (NativeExpr::FloatNeg(existing), NativeExpr::FloatNeg(other))
        | (NativeExpr::FloatFloor(existing), NativeExpr::FloatFloor(other))
        | (NativeExpr::FloatCeil(existing), NativeExpr::FloatCeil(other))
        | (NativeExpr::IntToFloat(existing), NativeExpr::IntToFloat(other))
        | (NativeExpr::Not(existing), NativeExpr::Not(other)) => {
            merge_expr_contracts(existing, other);
        }
        (
            NativeExpr::Binary {
                left: existing_left,
                right: existing_right,
                ..
            },
            NativeExpr::Binary {
                left: other_left,
                right: other_right,
                ..
            },
        ) => {
            merge_expr_contracts(existing_left, other_left);
            merge_expr_contracts(existing_right, other_right);
        }
        (
            NativeExpr::Let {
                bindings: existing_bindings,
                body: existing_body,
            },
            NativeExpr::Let {
                bindings: other_bindings,
                body: other_body,
            },
        ) => {
            merge_expr_slices(existing_bindings, other_bindings);
            merge_expr_contracts(existing_body, other_body);
        }
        (NativeExpr::If { clauses: existing }, NativeExpr::If { clauses: other }) => {
            for ((existing_condition, existing_body), (other_condition, other_body)) in
                existing.iter_mut().zip(other)
            {
                merge_expr_contracts(existing_condition, other_condition);
                merge_expr_contracts(existing_body, other_body);
            }
        }
        (
            NativeExpr::Try {
                protected: existing_protected,
                success: existing_success,
                failure: existing_failure,
                cleanup: existing_cleanup,
            },
            NativeExpr::Try {
                protected: other_protected,
                success: other_success,
                failure: other_failure,
                cleanup: other_cleanup,
            },
        ) => {
            merge_expr_contracts(existing_protected, other_protected);
            merge_expr_contracts(existing_success, other_success);
            merge_expr_contracts(existing_failure, other_failure);
            merge_expr_slices(existing_cleanup, other_cleanup);
        }
        _ => {}
    }
}

fn merge_expr_slices(existing: &mut [NativeExpr], other: &[NativeExpr]) {
    for (existing, other) in existing.iter_mut().zip(other) {
        merge_expr_contracts(existing, other);
    }
}

pub(in crate::compiler::native_ir) fn refresh_recursive_call_contract(
    expr: &mut NativeExpr,
    function: usize,
    entries: &[(u64, usize)],
) {
    if let NativeExpr::CallThen {
        function: target,
        resumes,
        values,
        ..
    } = expr
    {
        // A tail-recursive component contributes no caller-owned frame, so a
        // deeper component yield can retain its existing identity. A non-tail
        // recursive call requires a real wrapper; silently forwarding it
        // would discard an unbounded chain of caller frames.
        if *target == function && values.is_empty() {
            for (callee_continuation_id, callee_capture_count) in entries {
                if resumes
                    .iter()
                    .any(|resume| resume.callee_continuation_id == *callee_continuation_id)
                {
                    continue;
                }
                resumes.push(crate::compiler::native_ir::NativeCallResume {
                    callee_continuation_id: *callee_continuation_id,
                    callee_capture_count: *callee_capture_count,
                    continuation_id: *callee_continuation_id,
                    caller_value_start: values.len(),
                });
            }
        }
    }
    match expr {
        NativeExpr::ManagedOperation { args, .. }
        | NativeExpr::Call { args, .. }
        | NativeExpr::TailCall { args, .. }
        | NativeExpr::ContinuationTailCall { args, .. } => {
            for arg in args {
                refresh_recursive_call_contract(arg, function, entries);
            }
        }
        NativeExpr::MakeClosure { captures, .. } => {
            for capture in captures {
                refresh_recursive_call_contract(capture, function, entries);
            }
        }
        NativeExpr::Construct { fields, .. } => {
            for field in fields {
                refresh_recursive_call_contract(field, function, entries);
            }
        }
        NativeExpr::InvokeClosure { callee, args, .. } => {
            refresh_recursive_call_contract(callee, function, entries);
            for arg in args {
                refresh_recursive_call_contract(arg, function, entries);
            }
        }
        NativeExpr::InvokeClosureThen {
            callee,
            args,
            values,
            ..
        } => {
            refresh_recursive_call_contract(callee, function, entries);
            for value in args.iter_mut().chain(values) {
                refresh_recursive_call_contract(value, function, entries);
            }
        }
        NativeExpr::CallThen { args, values, .. }
        | NativeExpr::Suspend {
            arguments: args,
            values,
            ..
        } => {
            for value in args.iter_mut().chain(values) {
                refresh_recursive_call_contract(value, function, entries);
            }
        }
        NativeExpr::Neg(value)
        | NativeExpr::FloatNeg(value)
        | NativeExpr::FloatFloor(value)
        | NativeExpr::FloatCeil(value)
        | NativeExpr::IntToFloat(value)
        | NativeExpr::Not(value) => refresh_recursive_call_contract(value, function, entries),
        NativeExpr::Binary { left, right, .. } => {
            refresh_recursive_call_contract(left, function, entries);
            refresh_recursive_call_contract(right, function, entries);
        }
        NativeExpr::Let { bindings, body } => {
            for binding in bindings {
                refresh_recursive_call_contract(binding, function, entries);
            }
            refresh_recursive_call_contract(body, function, entries);
        }
        NativeExpr::If { clauses } => {
            for (condition, body) in clauses {
                refresh_recursive_call_contract(condition, function, entries);
                refresh_recursive_call_contract(body, function, entries);
            }
        }
        NativeExpr::Try {
            protected,
            success,
            failure,
            cleanup,
        } => {
            refresh_recursive_call_contract(protected, function, entries);
            refresh_recursive_call_contract(success, function, entries);
            refresh_recursive_call_contract(failure, function, entries);
            for value in cleanup {
                refresh_recursive_call_contract(value, function, entries);
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

/// Proves that every direct composed call accepts its callee's full contract.
#[cfg(test)]
pub(in crate::compiler::native_ir) fn validate_call_then_contracts(
    body: &NativeExpr,
    profiles: &HashMap<usize, ComposedCallProfile>,
    function_labels: &HashMap<usize, String>,
) -> super::super::NativeIrResult<()> {
    let destination_capture_counts = profiles
        .values()
        .flat_map(|profile| {
            profile
                .continuations
                .iter()
                .map(|continuation| (continuation.id, continuation.params.len()))
        })
        .collect::<HashMap<_, _>>();
    validate_call_then_contracts_with_destinations(
        body,
        profiles,
        function_labels,
        &destination_capture_counts,
    )
}

/// Validates one emitted body against the complete application continuation
/// table.
///
/// A top-level test or command can own local wrappers without itself exposing a
/// reusable suspension profile. Its `CallThen` destinations are therefore
/// present in the emitted module, but intentionally absent from `profiles`.
/// Final application admission supplies those local destinations here while
/// still using profiles to validate every callee entry contract.
pub(in crate::compiler::native_ir) fn validate_call_then_contracts_with_destinations(
    body: &NativeExpr,
    profiles: &HashMap<usize, ComposedCallProfile>,
    function_labels: &HashMap<usize, String>,
    destination_capture_counts: &HashMap<u64, usize>,
) -> super::super::NativeIrResult<()> {
    let mut failure = None;
    walk_native_expr(body, &mut |expr| {
        if failure.is_some() {
            return;
        }
        let NativeExpr::CallThen {
            function,
            resumes,
            values,
            ..
        } = expr
        else {
            return;
        };
        let label = function_labels.get(function).map_or("", String::as_str);
        let Some(profile) = profiles.get(function) else {
            failure = Some(format!(
                "call-then target {function} ({label}) has no converged suspension profile"
            ));
            return;
        };
        let capture_counts = profile
            .continuations
            .iter()
            .map(|continuation| (continuation.id, continuation.params.len()))
            .collect::<HashMap<_, _>>();
        for entry in &profile.entries {
            let Some(expected_capture_count) = capture_counts.get(entry) else {
                failure = Some(format!(
                    "callee {function} ({label}) advertises continuation {entry} without a body"
                ));
                return;
            };
            match resumes
                .iter()
                .find(|resume| resume.callee_continuation_id == *entry)
            {
                Some(resume) if resume.callee_capture_count == *expected_capture_count => {}
                Some(resume) => {
                    failure = Some(format!(
                        "call-then target {function} ({label}) continuation {entry} expects {expected_capture_count} captures but records {}",
                        resume.callee_capture_count
                    ));
                    return;
                }
                None => {
                    failure = Some(format!(
                        "call-then target {function} ({label}) omits continuation {entry} with {expected_capture_count} captures"
                    ));
                    return;
                }
            }
        }
        for resume in resumes {
            // A tail-recursive reduction edge forwards the callee-owned
            // parked frame under its existing identity. It does not create a
            // caller completion frame and therefore retains the callee's
            // capture shape verbatim.
            if resume.continuation_id == resume.callee_continuation_id {
                match destination_capture_counts.get(&resume.continuation_id) {
                    Some(actual) if *actual == resume.callee_capture_count => continue,
                    Some(actual) => {
                        failure = Some(format!(
                            "call-then target {function} ({label}) forwards callee continuation {} with {} captures to the same identity with {actual} parameters",
                            resume.callee_continuation_id, resume.callee_capture_count
                        ));
                        return;
                    }
                    None => {
                        failure = Some(format!(
                            "call-then target {function} ({label}) forwards callee continuation {} to an absent identity",
                            resume.callee_continuation_id
                        ));
                        return;
                    }
                }
            }
            if resume.caller_value_start > values.len() {
                failure = Some(format!(
                    "call-then target {function} ({label}) caller value offset {} exceeds frame width {}",
                    resume.caller_value_start,
                    values.len()
                ));
                return;
            }
            let appended_value_count = values.len().saturating_sub(resume.caller_value_start);
            // The destination is a VM-owned completion frame: caller
            // captures followed by the callee result.  Callee captures remain
            // owned by the original continuation and are not flattened into
            // the destination signature.
            let expected_destination_count = appended_value_count.saturating_add(1);
            match destination_capture_counts.get(&resume.continuation_id) {
                Some(actual) if *actual == expected_destination_count => {}
                Some(actual) => {
                    failure = Some(format!(
                        "call-then target {function} ({label}) maps callee continuation {} with {} captures to completion {} with {actual} parameters, but {} caller values plus the result require {expected_destination_count}",
                        resume.callee_continuation_id,
                        resume.callee_capture_count,
                        resume.continuation_id,
                        appended_value_count
                    ));
                    return;
                }
                None => {
                    failure = Some(format!(
                        "call-then target {function} ({label}) maps callee continuation {} to absent continuation {}",
                        resume.callee_continuation_id, resume.continuation_id
                    ));
                    return;
                }
            }
        }
    });
    failure.map_or(Ok(()), |error| Err(error.into()))
}
