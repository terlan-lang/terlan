//! Closed native-function admission diagnostics.

use crate::terlan_typeck::{CoreExprSummary, CoreFunction, CorePattern};

use super::super::{
    expr_is_native_control, native_return_type_with_constructors, native_type,
    scalar_replacement::scalar_replace_fixed_aggregates,
};

/// Collects syntax-output node kinds that did not preserve executable CoreIR.
fn missing_core_kinds(summary: &CoreExprSummary, missing: &mut Vec<String>) {
    if summary.core_expr.is_none() {
        let identity = format!(
            "{}[text={},remote={}]",
            summary.kind,
            summary.text.as_deref().unwrap_or("-"),
            summary.remote.as_deref().unwrap_or("-"),
        );
        if !missing.contains(&identity) {
            missing.push(identity);
        }
    }
    for child in &summary.children {
        missing_core_kinds(child, missing);
    }
}

/// Explains which closed native-function admission invariant rejected a declaration.
pub(super) fn candidate_admission_summary(
    function: &CoreFunction,
    constructors: &super::super::constructors::NativeConstructorLayouts,
) -> String {
    let parameters = function
        .params
        .iter()
        .all(|param| native_type(param.core_ty.as_ref(), &param.ty).is_some());
    let clause = matches!(function.clauses.as_slice(), [clause]
    if clause.guard.is_none()
        && clause.core_patterns.len() == function.params.len()
        && clause.core_patterns.iter().zip(&function.params).all(|(pattern, param)| {
            matches!(pattern, Some(CorePattern::Var(name)) if name == &param.name)
        }));
    let normalized_body = function
        .clauses
        .first()
        .and_then(|clause| clause.body.core_expr.as_ref())
        .map(|body| scalar_replace_fixed_aggregates(body, constructors));
    let body = normalized_body.as_ref().is_some_and(expr_is_native_control);
    let body_gap = if body {
        "none".to_string()
    } else {
        normalized_body
            .as_ref()
            .map(|body| body.contract_text())
            .unwrap_or_else(|| "missing".to_string())
    };
    let mut missing = Vec::new();
    if let Some(clause) = function.clauses.first() {
        missing_core_kinds(&clause.body, &mut missing);
    }
    format!(
        "native-operation={}, parameters={parameters}, result={}, clause={clause}, body={body}, body-gap={body_gap}, missing-core={}",
        function.native_operation.is_none(),
        native_return_type_with_constructors(function, constructors).is_some(),
        if missing.is_empty() {
            "none".to_string()
        } else {
            missing.join(",")
        },
    )
}
