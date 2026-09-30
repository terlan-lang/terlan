//! Provider-owned default adapters for type-directed receiver calls.

use crate::terlan_syntax::{
    SyntaxDeclarationPayload, SyntaxExprKind, SyntaxExprOutput, SyntaxFunctionClauseOutput,
    SyntaxModuleOutput, SyntaxPatternKind, SyntaxPatternOutput,
};

/// Retain shortened receiver arities as ordinary source callables. Defaults are
/// evaluated in their provider's scope, not guessed from a method name at a call.
pub(super) fn materialize(module: &mut SyntaxModuleOutput) {
    let mut generated = Vec::new();
    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Method {
            receiver,
            name,
            params,
            ..
        } = &declaration.payload
        else {
            continue;
        };
        let required = params
            .iter()
            .rposition(|param| param.default.is_none())
            .map_or(0, |index| index + 1);
        for count in required..params.len() {
            let mut wrapper = declaration.clone();
            wrapper.annotations.clear();
            let SyntaxDeclarationPayload::Method {
                params: wrapper_params,
                clauses,
                ..
            } = &mut wrapper.payload
            else {
                unreachable!("cloned method");
            };
            wrapper_params.truncate(count);
            for param in wrapper_params.iter_mut() {
                param.default = None;
                param.default_text = None;
                param.has_default = false;
                param.pattern_text = None;
            }
            let mut body = expression(SyntaxExprKind::Call, None);
            body.span = declaration.span;
            body.children
                .push(expression(SyntaxExprKind::Var, Some(name.clone())));
            body.children
                .push(expression(SyntaxExprKind::Var, Some(receiver.name.clone())));
            body.children
                .extend(params.iter().enumerate().map(|(index, param)| {
                    if index < count {
                        expression(SyntaxExprKind::Var, Some(param.name.clone()))
                    } else {
                        param.default.clone().expect("trailing checked default")
                    }
                }));
            body.arity = params.len() + 1;
            body.arg_names = vec![None; body.arity];
            *clauses = vec![SyntaxFunctionClauseOutput {
                patterns: wrapper_params
                    .iter()
                    .map(|param| SyntaxPatternOutput {
                        kind: SyntaxPatternKind::Var,
                        arity: 0,
                        text: Some(param.name.clone()),
                        children: Vec::new(),
                        fields: Vec::new(),
                    })
                    .collect(),
                guard: None,
                has_guard: false,
                body,
                span: declaration.span,
            }];
            generated.push(wrapper);
        }
    }
    module.declarations.extend(generated);
}

fn expression(kind: SyntaxExprKind, text: Option<String>) -> SyntaxExprOutput {
    SyntaxExprOutput {
        kind,
        text,
        arity: 0,
        span: Default::default(),
        raw: None,
        comprehension_lift: None,
        type_args: Vec::new(),
        operator: None,
        remote: None,
        arg_names: Vec::new(),
        children: Vec::new(),
        patterns: Vec::new(),
        let_guards: Vec::new(),
        fields: Vec::new(),
        clauses: Vec::new(),
        catch_clauses: Vec::new(),
        try_after: None,
        html_nodes: Vec::new(),
    }
}

#[cfg(test)]
#[path = "receiver_defaults_test.rs"]
mod tests;
