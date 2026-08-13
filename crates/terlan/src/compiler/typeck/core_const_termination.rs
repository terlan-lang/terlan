use std::collections::HashMap;

use super::core_proof::{core_function_clause_summary, function_value_parameter_names};
use super::*;

/// Builds recomputable CoreIR totality evidence for compile-time const functions.
///
/// Const functions do not enter executable CoreIR, so this proof-only view
/// gives their typed parameters and expression bodies the same termination
/// analyzer used by runtime functions without making them runtime callables.
pub(crate) fn core_const_function_termination_evidence(
    module: &SyntaxModuleOutput,
) -> CoreTerminationEvidence {
    let receiver_methods = HashMap::new();
    let template_prop_order = HashMap::new();
    let functions = module
        .declarations
        .iter()
        .filter_map(|declaration| {
            let SyntaxDeclarationPayload::ConstFunction {
                name,
                params,
                return_type,
                body,
                is_public,
            } = &declaration.payload
            else {
                return None;
            };
            let function_value_locals = function_value_parameter_names(params);
            let clause = crate::terlan_syntax::SyntaxFunctionClauseOutput {
                patterns: params
                    .iter()
                    .map(|param| crate::terlan_syntax::SyntaxPatternOutput {
                        kind: crate::terlan_syntax::SyntaxPatternKind::Var,
                        arity: 0,
                        text: Some(param.name.clone()),
                        children: Vec::new(),
                        fields: Vec::new(),
                    })
                    .collect(),
                guard: None,
                body: body.clone(),
                has_guard: false,
                span: declaration.span,
            };
            Some(CoreFunction {
                name: name.clone(),
                arity: params.len(),
                public: *is_public,
                generic_params: Vec::new(),
                native_operation: None,
                params: params
                    .iter()
                    .map(|param| CoreParam {
                        name: param.name.clone(),
                        ty: param.annotation.text.clone(),
                        core_ty: core_type_from_text(&param.annotation.text),
                    })
                    .collect(),
                return_type: return_type.text.clone(),
                core_return_type: core_type_from_text(&return_type.text),
                clauses: vec![core_function_clause_summary(
                    &clause,
                    &receiver_methods,
                    &template_prop_order,
                    &function_value_locals,
                )],
            })
        })
        .collect::<Vec<_>>();
    analyze_core_function_termination(&functions)
}
