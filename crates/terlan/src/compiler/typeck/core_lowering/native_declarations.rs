//! Native declarations acquire a typed implementation directly in CoreIR.

use super::*;

pub(super) fn materialize(function: &mut CoreFunction) {
    let Some(operation) = &function.native_operation else {
        return;
    };
    // Retain compatibility with older source declarations carrying a body.
    if !function.clauses.is_empty() {
        return;
    }
    let Some(parameter_types) = function
        .params
        .iter()
        .map(|param| param.core_ty.clone())
        .collect::<Option<Vec<_>>>()
    else {
        return;
    };
    let Some(return_type) = function.core_return_type.clone() else {
        return;
    };
    let names = function
        .params
        .iter()
        .map(|param| param.name.clone())
        .collect::<Vec<_>>();
    function.clauses.push(CoreFunctionClause {
        patterns: names.clone(),
        core_patterns: names
            .iter()
            .map(|name| Some(CorePattern::Var(name.clone())))
            .collect(),
        pattern_proof_coverage: vec![CoreProofCoverage::LeanCovered; names.len()],
        pattern_checked_preservation_evidence: vec![None; names.len()],
        guard: None,
        body: CoreExprSummary {
            kind: "NativeOperation".into(),
            core_expr: Some(CoreExpr::Intrinsic(CoreIntrinsicCall {
                id: CoreIntrinsicId::NativeOperation {
                    operation: operation.clone(),
                    parameter_types,
                },
                args: names.into_iter().map(CoreExpr::Var).collect(),
                return_type,
                effects: CoreEffectSet {
                    effects: vec!["native-package".into()],
                },
                span: Span::new(0, 0),
            })),
            checked_preservation_evidence: None,
            proof_coverage: CoreProofCoverage::ProofModelRequired,
            text: None,
            remote: None,
            operator: None,
            arity: function.arity,
            children: Vec::new(),
        },
    });
}
