use crate::terlan_syntax::{SyntaxExprKind, SyntaxExprOutput};

/// Returns whether a test name describes broad property-style behavior.
pub(super) fn is_property_candidate_name(name: &str) -> bool {
    let direct_markers = [
        "roundtrip",
        "round_trip",
        "parse_render",
        "render_parse",
        "serialize",
        "deserialize",
        "serialization",
        "invariant",
    ];
    direct_markers.iter().any(|marker| name.contains(marker))
        || (name.contains("parse") && name.contains("to_string"))
        || (name.contains("ordering")
            && ["law", "transitive", "antisymmetric", "reflexive", "total"]
                .iter()
                .any(|marker| name.contains(marker)))
}

/// Returns whether an expression tree uses the property-test runner surface.
pub(super) fn has_property_runner_call(expr: &SyntaxExprOutput) -> bool {
    is_property_runner_call(expr)
        || expr.children.iter().any(has_property_runner_call)
        || expr
            .fields
            .iter()
            .any(|field| has_property_runner_call(&field.value))
        || expr.clauses.iter().any(|clause| {
            clause
                .guard
                .as_ref()
                .is_some_and(|guard| has_property_runner_call(guard))
                || has_property_runner_call(&clause.body)
        })
        || expr.catch_clauses.iter().any(|clause| {
            clause
                .guard
                .as_ref()
                .is_some_and(|guard| has_property_runner_call(guard))
                || has_property_runner_call(&clause.body)
        })
        || expr.try_after.as_ref().is_some_and(|after| {
            has_property_runner_call(&after.trigger) || has_property_runner_call(&after.body)
        })
}

/// Returns whether one call expression invokes a property-test runner.
fn is_property_runner_call(expr: &SyntaxExprOutput) -> bool {
    if expr.kind != SyntaxExprKind::Call {
        return false;
    }
    let Some(callee) = expr.children.first() else {
        return false;
    };
    let Some(name) = callee.text.as_deref() else {
        return false;
    };
    let property_runners = [
        "for_all",
        "for_all_limit",
        "report",
        "run_report",
        "step_report",
        "shrink_failure",
        "shrink_report",
    ];
    if !property_runners.contains(&name) {
        return false;
    }
    expr.remote
        .as_deref()
        .is_none_or(|remote| remote == "std.test.Gen")
}
