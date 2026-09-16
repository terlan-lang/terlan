//! Retain checked constructor bodies as ordinary Core-boundary callables.

use super::*;
use crate::terlan_syntax::syntax_output::SyntaxConstructorParamOutput;
use crate::terlan_syntax::{
    SyntaxDeclarationOutput, SyntaxParamOutput, SyntaxPatternKind, SyntaxPatternOutput,
    SyntaxTypeOutput,
};
use crate::terlan_typeck::core_ir::CoreFunctionSource;

/// Retains checked constructor bodies and constant defaults as provider-local functions.
pub(super) fn materialize(
    module: &mut SyntaxModuleOutput,
    constructors: &mut Vec<CoreConstructorDecl>,
) {
    let mut generated = Vec::new();
    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Constructor {
            name,
            params: generic_params,
            is_public,
            clauses,
        } = &declaration.payload
        else {
            continue;
        };
        constructors.retain(|constructor| constructor.name != *name);
        for (index, clause) in clauses.iter().enumerate() {
            let function = format!("$constructor_{name}_{index}");
            let params = clause
                .params
                .iter()
                .map(callable_parameter)
                .collect::<Vec<_>>();
            let mut defaults = Vec::new();
            for (index, parameter) in clause
                .params
                .iter()
                .take_while(|parameter| !parameter.is_varargs)
                .enumerate()
            {
                let default = parameter.default.as_ref().map(|body| {
                    let default_name = format!("{function}_default_{index}");
                    generated.push(callable(
                        declaration,
                        &default_name,
                        generic_params,
                        params[..index].to_vec(),
                        parameter.annotation.clone(),
                        body.clone(),
                    ));
                    default_name
                });
                defaults.push(default);
            }
            constructors.push(CoreConstructorDecl {
                implementation: Some(CoreConstructorImplementation {
                    function: function.clone(),
                    defaults,
                }),
                name: name.clone(),
                public: *is_public,
                min_arity: clause
                    .params
                    .iter()
                    .filter(|parameter| !parameter.is_varargs && !parameter.has_default)
                    .count(),
                params: clause
                    .params
                    .iter()
                    .filter(|parameter| !parameter.is_varargs)
                    .map(constructor_parameter)
                    .collect(),
                vararg: clause
                    .params
                    .iter()
                    .find(|parameter| parameter.is_varargs)
                    .map(constructor_parameter),
                return_type: clause.return_type.text.clone(),
                core_return_type: core_type_from_text(&clause.return_type.text),
            });
            generated.push(callable(
                declaration,
                &function,
                generic_params,
                params,
                clause.return_type.clone(),
                clause.body.clone(),
            ));
        }
    }
    module.declarations.extend(generated);
}

/// Attributes constructor bodies and default helpers to their original clause.
pub(super) fn retain_sources(core: &mut CoreModule) {
    let mut sources = std::collections::HashMap::new();
    for constructor in &core.constructors {
        let Some(implementation) = &constructor.implementation else {
            continue;
        };
        let source = CoreFunctionSource {
            module: core.module.clone(),
            function: constructor.name.clone(),
            arity: constructor.params.len() + usize::from(constructor.vararg.is_some()),
        };
        sources.insert(implementation.function.as_str(), source.clone());
        for default in implementation.defaults.iter().flatten() {
            sources.insert(default.as_str(), source.clone());
        }
    }
    for function in &mut core.functions {
        if let Some(source) = sources.get(function.name.as_str()) {
            function.source = Some(source.clone());
        }
    }
}

fn constructor_parameter(parameter: &SyntaxConstructorParamOutput) -> CoreParam {
    CoreParam {
        name: parameter.name.clone(),
        ty: parameter.annotation.text.clone(),
        core_ty: core_type_from_text(&parameter.annotation.text),
    }
}

fn callable_parameter(parameter: &SyntaxConstructorParamOutput) -> SyntaxParamOutput {
    let mut annotation = parameter.annotation.clone();
    if parameter.is_varargs {
        annotation.text = format!("List[{}]", annotation.text);
    }
    SyntaxParamOutput {
        name: parameter.name.clone(),
        annotation,
        is_mutable: false,
        has_default: false,
        default: None,
        default_text: None,
        pattern_text: None,
        span: parameter.span,
    }
}

fn callable(
    declaration: &SyntaxDeclarationOutput,
    name: &str,
    generic_params: &[String],
    params: Vec<SyntaxParamOutput>,
    return_type: SyntaxTypeOutput,
    body: SyntaxExprOutput,
) -> SyntaxDeclarationOutput {
    let patterns = params
        .iter()
        .map(|parameter| SyntaxPatternOutput {
            kind: SyntaxPatternKind::Var,
            arity: 0,
            text: Some(parameter.name.clone()),
            children: Vec::new(),
            fields: Vec::new(),
        })
        .collect();
    SyntaxDeclarationOutput {
        index: declaration.index,
        class: "function".to_string(),
        span: declaration.span,
        docs: Vec::new(),
        annotations: Vec::new(),
        payload: SyntaxDeclarationPayload::Function {
            name: name.to_string(),
            generic_params: generic_params.to_vec(),
            params,
            return_type,
            is_public: false,
            is_macro: false,
            generic_bounds: Vec::new(),
            clauses: vec![crate::terlan_syntax::SyntaxFunctionClauseOutput {
                patterns,
                guard: None,
                body,
                has_guard: false,
                span: declaration.span,
            }],
        },
    }
}
