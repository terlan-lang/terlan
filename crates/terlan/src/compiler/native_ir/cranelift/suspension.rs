//! Fixed-point analysis for native suspension and transition-frame sizing.

use super::{NativeExpr, NativeModule};
use crate::runtime::native_image::TVM_INDIRECT_TRANSITION_WORD_CAPACITY;

/// Computes which functions suspend and their maximum transition value count.
pub(super) fn suspension_profile(native: &NativeModule) -> (Vec<bool>, Vec<usize>) {
    let mut suspending = vec![false; native.functions.len()];
    loop {
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
    let mut transition_counts = vec![0; native.functions.len()];
    loop {
        let next = native
            .functions
            .iter()
            .map(|function| suspension_value_count(&function.body, &transition_counts))
            .collect::<Vec<_>>();
        if next == transition_counts {
            return (suspending, transition_counts);
        }
        transition_counts = next;
    }
}

/// Returns the largest transition frame required by one expression tree.
pub(super) fn suspension_value_count(body: &NativeExpr, function_counts: &[usize]) -> usize {
    match body {
        NativeExpr::Suspend {
            arguments, values, ..
        } => arguments.len().saturating_add(values.len()),
        NativeExpr::TailCall { function, .. } => {
            function_counts.get(*function).copied().unwrap_or(0)
        }
        NativeExpr::CallThen {
            callee_capture_count,
            values,
            resume,
            ..
        } => callee_capture_count
            .saturating_add(values.len())
            .max(suspension_value_count(resume, function_counts)),
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

/// Returns whether an expression can park its actor or call one that can.
pub(super) fn is_suspending(body: &NativeExpr, function_suspending: &[bool]) -> bool {
    match body {
        NativeExpr::Suspend { .. } => true,
        NativeExpr::TailCall { function, .. } => {
            function_suspending.get(*function).copied().unwrap_or(false)
        }
        NativeExpr::CallThen { .. } => true,
        NativeExpr::InvokeClosure { .. } => true,
        NativeExpr::Let { body, .. } => is_suspending(body, function_suspending),
        NativeExpr::If { clauses } => clauses
            .iter()
            .any(|(_, body)| is_suspending(body, function_suspending)),
        NativeExpr::Try { .. } => false,
        _ => false,
    }
}
