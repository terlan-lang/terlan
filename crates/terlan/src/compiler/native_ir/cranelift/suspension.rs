//! Fixed-point analysis for native suspension and transition-frame sizing.

use super::{NativeExpr, NativeModule};
use crate::runtime::native_image::TVM_INDIRECT_TRANSITION_WORD_CAPACITY;

/// Computes which functions suspend and their maximum transition value count.
pub(crate) fn suspension_profile(
    native: &NativeModule,
) -> super::NativeIrResult<(Vec<bool>, Vec<usize>)> {
    let mut suspending = vec![false; native.functions.len()];
    for _ in 0..=native.functions.len() {
        let next = native
            .functions
            .iter()
            .map(|function| is_suspending(&function.body, &suspending))
            .collect::<Vec<_>>();
        if next == suspending {
            break;
        }
        suspending = next;
    }
    let suspension_converged = native
        .functions
        .iter()
        .zip(&suspending)
        .all(|(function, expected)| is_suspending(&function.body, &suspending) == *expected);
    if !suspension_converged {
        return Err(
            "error[cranelift.suspension_fixed_point]: suspension analysis did not converge"
                .to_string()
                .into(),
        );
    }
    let mut transition_counts = vec![0; native.functions.len()];
    for _ in 0..=native.functions.len() {
        let next = native
            .functions
            .iter()
            .map(|function| suspension_value_count(&function.body, &transition_counts))
            .collect::<Vec<_>>();
        if next == transition_counts {
            return Ok((suspending, transition_counts));
        }
        transition_counts = next;
    }
    let next = native
        .functions
        .iter()
        .map(|function| suspension_value_count(&function.body, &transition_counts))
        .collect::<Vec<_>>();
    let growing = native
        .functions
        .iter()
        .zip(transition_counts.iter().zip(next))
        .filter(|(_function, (before, after))| after > *before)
        .map(|(function, (before, after))| {
            format!(
                "{}.{}/{} ({before}->{after})",
                native.name, function.name, function.arity
            )
        })
        .collect::<Vec<_>>();
    Err(format!(
        "error[cranelift.unbounded_completion_stack]: suspension frame sizing did not converge; a non-tail recursive call retains an unbounded caller stack: {}",
        growing.join(", ")
    ).into())
}

/// Gives every fused tail-component entry the capacity required by its widest
/// member while rejecting a component that cannot share one suspension ABI.
pub(super) fn normalize_tail_component_profiles(
    suspending: &[bool],
    transition_counts: &mut [usize],
    components: &[Vec<usize>],
) -> super::NativeIrResult<()> {
    for component in components {
        let Some(first) = component.first().copied() else {
            continue;
        };
        let expected = suspending.get(first).copied().ok_or_else(|| {
            format!("error[cranelift.tail_component]: function {first} is unavailable")
        })?;
        if component
            .iter()
            .any(|member| suspending.get(*member).copied() != Some(expected))
        {
            return Err(format!(
                "error[cranelift.tail_component_suspension]: component {component:?} mixes suspending and non-suspending functions"
            ).into());
        }
        let capacity = component
            .iter()
            .filter_map(|member| transition_counts.get(*member).copied())
            .max()
            .unwrap_or_default();
        for member in component {
            let count = transition_counts.get_mut(*member).ok_or_else(|| {
                format!("error[cranelift.tail_component]: function {member} is unavailable")
            })?;
            *count = capacity;
        }
    }
    Ok(())
}

/// Returns the largest transition frame required by one expression tree.
pub(super) fn suspension_value_count(body: &NativeExpr, function_counts: &[usize]) -> usize {
    match body {
        NativeExpr::Suspend {
            arguments, values, ..
        } => arguments.len().saturating_add(values.len()),
        NativeExpr::TailCall {
            function,
            args,
            yield_continuation_id,
        } => yield_continuation_id.map_or_else(
            || function_counts.get(*function).copied().unwrap_or(0),
            |_| args.len(),
        ),
        NativeExpr::ContinuationTailCall { .. } => 0,
        NativeExpr::CallThen {
            function,
            resumes,
            completion_function,
            values,
            ..
        } => resumes
            .iter()
            .map(|resume| {
                resume
                    .callee_capture_count
                    .saturating_add(values.len().saturating_sub(resume.caller_value_start))
                    .saturating_add(2)
            })
            .max()
            .unwrap_or(0)
            .max(
                completion_function
                    .and_then(|function| function_counts.get(function).copied())
                    .unwrap_or(0),
            )
            .max(
                function_counts
                    .get(*function)
                    .copied()
                    .unwrap_or(0)
                    .saturating_add(values.len())
                    .saturating_add(2),
            ),
        NativeExpr::InvokeClosureThen {
            resumes,
            completion_function,
            values,
            ..
        } => resumes
            .iter()
            .map(|resume| resume.callee_capture_count)
            .max()
            .unwrap_or(0)
            .saturating_add(values.len())
            .saturating_add(2)
            .max(
                completion_function
                    .and_then(|function| function_counts.get(function).copied())
                    .unwrap_or(0),
            )
            .max(TVM_INDIRECT_TRANSITION_WORD_CAPACITY),
        NativeExpr::InvokeClosure { .. } => TVM_INDIRECT_TRANSITION_WORD_CAPACITY,
        NativeExpr::Let { body, .. } => suspension_value_count(body, function_counts),
        NativeExpr::If { clauses } => clauses
            .iter()
            .map(|(_, body)| suspension_value_count(body, function_counts))
            .max()
            .unwrap_or(0),
        NativeExpr::Try { .. } => 0,
        _ => 0,
    }
}

#[cfg(test)]
#[path = "suspension_test.rs"]
mod tests;

/// Returns whether an expression can park its actor or call one that can.
pub(super) fn is_suspending(body: &NativeExpr, function_suspending: &[bool]) -> bool {
    match body {
        NativeExpr::Suspend { .. } => true,
        NativeExpr::TailCall {
            function,
            yield_continuation_id,
            ..
        } => {
            yield_continuation_id.is_some()
                || function_suspending.get(*function).copied().unwrap_or(false)
        }
        NativeExpr::ContinuationTailCall { .. } => false,
        NativeExpr::CallThen { .. } | NativeExpr::InvokeClosureThen { .. } => true,
        NativeExpr::InvokeClosure { .. } => true,
        NativeExpr::Let { body, .. } => is_suspending(body, function_suspending),
        NativeExpr::If { clauses } => clauses
            .iter()
            .any(|(_, body)| is_suspending(body, function_suspending)),
        NativeExpr::Try { .. } => false,
        _ => false,
    }
}

/// Reports whether this body owns a compiler-inserted reduction-yield edge.
pub(super) fn has_reduction_yield(body: &NativeExpr) -> bool {
    match body {
        NativeExpr::TailCall {
            yield_continuation_id,
            ..
        } => yield_continuation_id.is_some(),
        NativeExpr::Let { body, .. } => has_reduction_yield(body),
        NativeExpr::If { clauses } => clauses.iter().any(|(_, body)| has_reduction_yield(body)),
        NativeExpr::Try {
            success,
            failure,
            cleanup,
            ..
        } if cleanup.is_empty() => has_reduction_yield(success) || has_reduction_yield(failure),
        _ => false,
    }
}
