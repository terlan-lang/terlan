//! Ordered function clauses share the ordinary bounded case matcher.

use crate::terlan_typeck::{
    CoreCaseClause, CoreExpr, CoreFunction, CorePattern, CoreProofCoverage, CoreTupleTypeElem,
    CoreType,
};

use super::super::NativeIrResult;

pub(super) fn normalize(function: &mut CoreFunction) -> NativeIrResult<()> {
    if function.clauses.len() > super::MAX_SCALAR_CASE_CLAUSES {
        return Err(format!(
            "error[native_ir.function_head_budget]: `{}/{}` exceeds {} clauses",
            function.name,
            function.arity,
            super::MAX_SCALAR_CASE_CLAUSES,
        )
        .into());
    }
    let mut clauses = Vec::with_capacity(function.clauses.len());
    for clause in &function.clauses {
        if clause.core_patterns.len() != function.params.len() {
            return Err("error[native_ir.function_head_pattern]: parameter arity mismatch".into());
        }
        let patterns = clause
            .core_patterns
            .iter()
            .cloned()
            .collect::<Option<Vec<_>>>()
            .ok_or("error[native_ir.function_head_pattern]: missing typed head pattern")?;
        for (pattern, parameter) in patterns.iter().zip(&mut function.params) {
            if matches!(pattern, CorePattern::BinaryLayout { .. }) {
                parameter.ty = "BitString".to_string();
                parameter.core_ty = Some(CoreType::Named("BitString".to_string()));
            }
        }
        let pattern = match patterns.as_slice() {
            [] => CorePattern::Wildcard,
            [pattern] => pattern.clone(),
            _ => CorePattern::Tuple(patterns),
        };
        let guard = clause
            .guard
            .as_ref()
            .map(|guard| {
                guard
                    .core_expr
                    .clone()
                    .ok_or("error[native_ir.function_head_guard]: missing typed guard")
            })
            .transpose()?;
        let body = clause
            .body
            .core_expr
            .clone()
            .ok_or("error[native_ir.function_head_body]: missing typed body")?;
        clauses.push(CoreCaseClause {
            pattern,
            guard,
            body,
        });
    }
    let scrutinee = match function.params.as_slice() {
        [] => CoreExpr::Atom("Unit".to_string()),
        [parameter] => CoreExpr::Var(parameter.name.clone()),
        parameters => CoreExpr::Cast {
            expr: Box::new(CoreExpr::Tuple(
                parameters
                    .iter()
                    .map(|parameter| CoreExpr::Var(parameter.name.clone()))
                    .collect(),
            )),
            target_type: CoreType::Tuple(
                parameters
                    .iter()
                    .map(|parameter| {
                        parameter
                            .core_ty
                            .clone()
                            .map(CoreTupleTypeElem::Type)
                            .ok_or("error[native_ir.function_head_type]: missing typed parameter")
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            ),
        },
    };
    let clause = function
        .clauses
        .first_mut()
        .ok_or("error[native_ir.function_head_body]: function has no clauses")?;
    clause.patterns = function
        .params
        .iter()
        .map(|parameter| parameter.name.clone())
        .collect();
    clause.core_patterns = function
        .params
        .iter()
        .map(|parameter| Some(CorePattern::Var(parameter.name.clone())))
        .collect();
    clause.pattern_proof_coverage = vec![CoreProofCoverage::RuntimeBoundary; function.params.len()];
    clause.pattern_checked_preservation_evidence = vec![None; function.params.len()];
    clause.guard = None;
    clause.body.kind = "case".to_string();
    clause.body.core_expr = Some(CoreExpr::Case {
        scrutinee: Box::new(scrutinee),
        clauses,
    });
    clause.body.proof_coverage = CoreProofCoverage::RuntimeBoundary;
    clause.body.checked_preservation_evidence = None;
    function.clauses.truncate(1);
    Ok(())
}
