use crate::terlan_typeck::{CoreCaseClause, CoreExpr, CoreExprSummary};

use super::super::std_runtime::{
    target_profile_supports_vm_intrinsic, target_profile_supports_vm_mutable_receiver_call,
    target_profile_supports_vm_std_remote_call,
};
use super::super::TargetProfile;

/// Returns whether a summary is executable by the CoreV0 VM lane.
pub(super) fn summary_allows_vm_owned_expr(
    profile: TargetProfile,
    summary: &CoreExprSummary,
) -> bool {
    let Some(expr) = summary.core_expr.as_ref() else {
        return false;
    };
    let contains_vm_runtime = expr_contains_vm_supported_std_runtime(profile, expr);
    let supported_boundary = summary.remote.is_some() && contains_vm_runtime;
    matches!(profile, TargetProfile::CoreV0)
        && (profile.allows_expr_coverage(summary.proof_coverage) || contains_vm_runtime)
        && (!profile.requires_checked_preservation_evidence()
            || summary.checked_preservation_evidence.is_some()
            || contains_vm_runtime)
        && profile.allows_expr_shape(expr)
        && (summary.remote.is_none() || supported_boundary)
}

/// Recursively detects a standard runtime operation implemented by the VM profile.
fn expr_contains_vm_supported_std_runtime(profile: TargetProfile, expr: &CoreExpr) -> bool {
    match expr {
        CoreExpr::RemoteCall {
            module,
            function,
            args,
        } => {
            target_profile_supports_vm_std_remote_call(profile, module, function, args.len())
                || args
                    .iter()
                    .any(|arg| expr_contains_vm_supported_std_runtime(profile, arg))
        }
        CoreExpr::Intrinsic(call) => {
            target_profile_supports_vm_intrinsic(profile, call)
                || call
                    .args
                    .iter()
                    .any(|arg| expr_contains_vm_supported_std_runtime(profile, arg))
        }
        CoreExpr::Tuple(values) | CoreExpr::List(values) => values
            .iter()
            .any(|value| expr_contains_vm_supported_std_runtime(profile, value)),
        CoreExpr::ListCons { head, tail } => {
            expr_contains_vm_supported_std_runtime(profile, head)
                || expr_contains_vm_supported_std_runtime(profile, tail)
        }
        CoreExpr::ConstructorCall { args, .. } | CoreExpr::Call { args, .. } => args
            .iter()
            .any(|arg| expr_contains_vm_supported_std_runtime(profile, arg)),
        CoreExpr::FunctionCall { callee, args } => {
            expr_contains_vm_supported_std_runtime(profile, callee)
                || args
                    .iter()
                    .any(|arg| expr_contains_vm_supported_std_runtime(profile, arg))
        }
        CoreExpr::Cast { expr, .. }
        | CoreExpr::FieldAccess { base: expr, .. }
        | CoreExpr::UnaryOp { operand: expr, .. } => {
            expr_contains_vm_supported_std_runtime(profile, expr)
        }
        CoreExpr::Case { scrutinee, clauses } => {
            expr_contains_vm_supported_std_runtime(profile, scrutinee)
                || clauses
                    .iter()
                    .any(|clause| case_clause_contains_vm_std_remote_call(profile, clause))
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            expr_contains_vm_supported_std_runtime(profile, body)
                || of_clauses
                    .iter()
                    .any(|clause| case_clause_contains_vm_std_remote_call(profile, clause))
                || catch_clauses
                    .iter()
                    .any(|clause| case_clause_contains_vm_std_remote_call(profile, clause))
                || after_clause.as_ref().is_some_and(|after| {
                    expr_contains_vm_supported_std_runtime(profile, &after.body)
                })
        }
        CoreExpr::If { clauses } => clauses.iter().any(|clause| {
            expr_contains_vm_supported_std_runtime(profile, &clause.condition)
                || expr_contains_vm_supported_std_runtime(profile, &clause.body)
        }),
        CoreExpr::Lam { body, .. } => expr_contains_vm_supported_std_runtime(profile, body),
        CoreExpr::BinaryOp { left, right, .. }
        | CoreExpr::Index {
            base: left,
            index: right,
        } => {
            expr_contains_vm_supported_std_runtime(profile, left)
                || expr_contains_vm_supported_std_runtime(profile, right)
        }
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            expr_contains_vm_supported_std_runtime(profile, expr)
                || generators.iter().any(|generator| {
                    expr_contains_vm_supported_std_runtime(profile, &generator.source)
                })
                || guards
                    .iter()
                    .any(|guard| expr_contains_vm_supported_std_runtime(profile, guard))
        }
        CoreExpr::Let { bindings, body } => {
            bindings
                .iter()
                .any(|binding| expr_contains_vm_supported_std_runtime(profile, &binding.value))
                || expr_contains_vm_supported_std_runtime(profile, body)
        }
        CoreExpr::Map(fields) => fields
            .iter()
            .any(|field| expr_contains_vm_supported_std_runtime(profile, &field.value)),
        CoreExpr::RecordConstruct { fields, .. }
        | CoreExpr::RecordUpdate { fields, .. }
        | CoreExpr::TemplateInstantiate { fields, .. } => fields
            .iter()
            .any(|field| expr_contains_vm_supported_std_runtime(profile, &field.value)),
        CoreExpr::RecordAccess { base, .. } => {
            expr_contains_vm_supported_std_runtime(profile, base)
        }
        CoreExpr::ConstructorChain { args, record, .. } => {
            args.iter()
                .any(|arg| expr_contains_vm_supported_std_runtime(profile, arg))
                || expr_contains_vm_supported_std_runtime(profile, record)
        }
        CoreExpr::MutableReceiverCall {
            receiver,
            method,
            args,
            ..
        } => {
            target_profile_supports_vm_mutable_receiver_call(profile, method, args.len())
                || expr_contains_vm_supported_std_runtime(profile, receiver)
                || args
                    .iter()
                    .any(|arg| expr_contains_vm_supported_std_runtime(profile, arg))
        }
        CoreExpr::SqlQuery { parameters, .. } => parameters
            .iter()
            .any(|parameter| expr_contains_vm_supported_std_runtime(profile, parameter)),
        CoreExpr::Int(_)
        | CoreExpr::Float(_)
        | CoreExpr::Binary(_)
        | CoreExpr::Atom(_)
        | CoreExpr::Var(_)
        | CoreExpr::FixedArray(_)
        | CoreExpr::RemoteFunRef { .. } => false,
    }
}

/// Checks one case clause for a VM-supported standard runtime call.
fn case_clause_contains_vm_std_remote_call(
    profile: TargetProfile,
    clause: &CoreCaseClause,
) -> bool {
    clause
        .guard
        .as_ref()
        .is_some_and(|guard| expr_contains_vm_supported_std_runtime(profile, guard))
        || expr_contains_vm_supported_std_runtime(profile, &clause.body)
}
