use std::collections::HashSet;

use crate::terlan_syntax::{
    SyntaxDeclarationOutput, SyntaxDeclarationPayload, SyntaxExprKind, SyntaxExprOutput,
    SyntaxFunctionClauseOutput, SyntaxModuleOutput, SyntaxParamOutput,
};

/// Local callable identity represented as source name and effective arity.
pub(crate) type CallableIdentity = (String, usize);

/// Infers conservative purity proofs for body-available module callables.
///
/// Inputs:
/// - `module`: parsed syntax output with function and receiver-method bodies.
///
/// Output:
/// - Function name/arity identities whose bodies are explicitly proven or can
///   be inferred pure without external interface knowledge.
///
/// Transformation:
/// - Starts body-available callables as pure candidates, then removes any
///   candidate that performs a structural effect or reaches an unproven call.
/// - Keeps mutually recursive components pure when every reachable expression
///   remains effect-free.
/// - Treats unknown/native calls conservatively, because interface
///   construction cannot inspect external implementations.
/// - Analyzes every callable with an available body regardless of unrelated
///   metadata annotations such as `@test`; annotations do not suppress
///   compiler-owned inference.
pub(crate) fn infer_body_available_pure_callables(
    module: &SyntaxModuleOutput,
) -> HashSet<CallableIdentity> {
    // `@pure` assertions are semantically validated by typechecking before a
    // module can be emitted. Keep that validated metadata available to this
    // syntax-level projection while independently inferring every other
    // body-available callable.
    let explicit = module
        .declarations
        .iter()
        .filter(|declaration| declaration_has_pure_annotation(declaration))
        .filter_map(syntax_declaration_callable_identity)
        .collect::<HashSet<_>>();
    let candidates = module
        .declarations
        .iter()
        .filter(|declaration| !declaration_has_pure_annotation(declaration))
        .filter(|declaration| callable_has_body(declaration))
        .filter(|declaration| compiler_intrinsic_is_inferably_pure(module, declaration))
        .filter_map(syntax_declaration_callable_identity)
        .collect::<HashSet<_>>();
    let mut proven = explicit;
    proven.extend(candidates.iter().cloned());

    loop {
        let rejected = module
            .declarations
            .iter()
            .filter_map(|declaration| {
                let identity = syntax_declaration_callable_identity(declaration)?;
                if !candidates.contains(&identity)
                    || !proven.contains(&identity)
                    || callable_is_pure_under(declaration, &proven)
                {
                    return None;
                }
                Some(identity)
            })
            .collect::<Vec<_>>();
        if rejected.is_empty() {
            return proven;
        }
        for identity in rejected {
            proven.remove(&identity);
        }
    }
}

/// Keeps compiler intrinsic placeholders from masquerading as source proofs.
fn compiler_intrinsic_is_inferably_pure(
    module: &SyntaxModuleOutput,
    declaration: &SyntaxDeclarationOutput,
) -> bool {
    if !declaration_has_annotation(declaration, &["compiler", "intrinsic"]) {
        return true;
    }
    let Some((function, arity)) = syntax_declaration_callable_identity(declaration) else {
        return false;
    };
    crate::terlan_typeck::core_intrinsic_is_pure(&module.module_name, &function, arity)
}

/// Extracts a direct local call identity from syntax output.
pub(crate) fn local_syntax_call_identity(expr: &SyntaxExprOutput) -> Option<(&str, usize)> {
    if expr.kind != SyntaxExprKind::Call || expr.remote.is_some() {
        return None;
    }
    let callee = expr.children.first()?;
    if callee.kind != SyntaxExprKind::Var {
        return None;
    }
    Some((callee.text.as_deref()?, expr.arity))
}

/// Reports whether evaluating an expression only constructs a deferred body.
///
/// Function bodies do not execute when a closure value is created. Purity
/// analysis must therefore classify the closure expression independently from
/// the effects that may occur when the resulting function value is invoked.
pub(crate) fn syntax_expression_defers_body(expr: &SyntaxExprOutput) -> bool {
    expr.kind == SyntaxExprKind::Fun
}

/// Extracts the effective local identity of a function or receiver method.
pub(crate) fn syntax_declaration_callable_identity(
    declaration: &SyntaxDeclarationOutput,
) -> Option<CallableIdentity> {
    match &declaration.payload {
        SyntaxDeclarationPayload::Function { name, params, .. } => {
            Some((name.clone(), params.len()))
        }
        SyntaxDeclarationPayload::Method { name, params, .. } => {
            Some((name.clone(), params.len() + 1))
        }
        _ => None,
    }
}

/// Reports whether a callable declaration exposes clauses for purity analysis.
fn callable_has_body(declaration: &SyntaxDeclarationOutput) -> bool {
    match &declaration.payload {
        SyntaxDeclarationPayload::Function { clauses, .. }
        | SyntaxDeclarationPayload::Method { clauses, .. } => !clauses.is_empty(),
        _ => false,
    }
}

/// Checks one callable under the current fixed-point set of proven identities.
fn callable_is_pure_under(
    declaration: &SyntaxDeclarationOutput,
    proven: &HashSet<CallableIdentity>,
) -> bool {
    let (params, clauses) = match &declaration.payload {
        SyntaxDeclarationPayload::Function {
            params, clauses, ..
        }
        | SyntaxDeclarationPayload::Method {
            params, clauses, ..
        } => (params, clauses),
        _ => return false,
    };

    params
        .iter()
        .all(|param| param_default_is_pure(param, proven))
        && clauses
            .iter()
            .all(|clause| clause_is_pure_under(clause, proven))
}

/// Checks an optional parameter default under the current purity proof set.
fn param_default_is_pure(param: &SyntaxParamOutput, proven: &HashSet<CallableIdentity>) -> bool {
    param
        .default
        .as_ref()
        .is_none_or(|default| expression_is_pure_under(default, proven))
}

/// Checks a callable clause guard and body under known purity proofs.
fn clause_is_pure_under(
    clause: &SyntaxFunctionClauseOutput,
    proven: &HashSet<CallableIdentity>,
) -> bool {
    clause
        .guard
        .as_ref()
        .is_none_or(|guard| expression_is_pure_under(guard, proven))
        && expression_is_pure_under(&clause.body, proven)
}

/// Conservatively proves one syntax expression free of observable effects.
fn expression_is_pure_under(expr: &SyntaxExprOutput, proven: &HashSet<CallableIdentity>) -> bool {
    if syntax_expression_defers_body(expr) {
        return true;
    }
    if matches!(
        expr.kind,
        SyntaxExprKind::IndexAssign
            | SyntaxExprKind::FunctionCall
            | SyntaxExprKind::Macro
            | SyntaxExprKind::RawMacro
            | SyntaxExprKind::HtmlBlock
            | SyntaxExprKind::TemplateInstantiate
    ) || (expr.kind == SyntaxExprKind::Var && expr.text.as_deref() == Some("native"))
    {
        return false;
    }
    if expr.kind == SyntaxExprKind::Call {
        let Some((name, arity)) = local_syntax_call_identity(expr) else {
            return false;
        };
        if !proven.contains(&(name.to_string(), arity)) {
            return false;
        }
    }

    expr.children
        .iter()
        .all(|child| expression_is_pure_under(child, proven))
        && expr
            .fields
            .iter()
            .all(|field| expression_is_pure_under(&field.value, proven))
        && expr.clauses.iter().all(|clause| {
            clause
                .guard
                .as_deref()
                .is_none_or(|guard| expression_is_pure_under(guard, proven))
                && expression_is_pure_under(&clause.body, proven)
        })
        && expr.catch_clauses.iter().all(|clause| {
            clause
                .guard
                .as_deref()
                .is_none_or(|guard| expression_is_pure_under(guard, proven))
                && expression_is_pure_under(&clause.body, proven)
        })
        && expr.try_after.as_ref().is_none_or(|after| {
            expression_is_pure_under(&after.trigger, proven)
                && expression_is_pure_under(&after.body, proven)
        })
}

/// Reports whether a declaration carries the compiler-validated `@pure`
/// metadata contract.
fn declaration_has_pure_annotation(declaration: &SyntaxDeclarationOutput) -> bool {
    declaration_has_annotation(declaration, &["pure"])
}

fn declaration_has_annotation(declaration: &SyntaxDeclarationOutput, path: &[&str]) -> bool {
    declaration.annotations.iter().any(|annotation| {
        annotation.path.len() == path.len()
            && annotation
                .path
                .iter()
                .map(String::as_str)
                .eq(path.iter().copied())
    })
}
