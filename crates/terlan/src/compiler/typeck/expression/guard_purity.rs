use std::collections::{HashMap, HashSet};

use crate::terlan_purity::{local_syntax_call_identity, syntax_expression_defers_body};

use super::calls::infer_receiver_method_candidate;
use super::{
    apply_subst, expand_type_aliases, infer_syntax_expr, primitive_receiver_method_scheme,
    ExprInferContext, SyntaxExprKind, SyntaxExprOutput, TemplateScheme, Type, TypeVarId,
};

#[derive(Clone, Copy, Default)]
pub(crate) struct EffectfulCallFacts<'a> {
    pub(crate) local: Option<&'a HashSet<(String, usize)>>,
    pub(crate) conditional_local: Option<&'a HashSet<(String, usize)>>,
    pub(crate) function_values: Option<&'a HashSet<String>>,
    /// Function-valued parameters whose effects are inherited by the current
    /// callable rather than performed independently by it.
    pub(crate) conditional_function_values: Option<&'a HashSet<String>>,
    pub(crate) imported: Option<&'a HashSet<(String, usize)>>,
    pub(crate) qualified: Option<&'a HashSet<(String, String, usize)>>,
    pub(crate) imported_receiver: Option<&'a HashSet<(String, usize)>>,
    pub(crate) trait_qualified: Option<&'a HashSet<(String, String, usize)>>,
    pub(crate) trait_receiver: Option<&'a HashSet<(String, usize)>>,
    pub(crate) module_aliases: Option<&'a HashSet<String>>,
    pub(crate) proven_pure_receiver_calls: Option<&'a [usize]>,
}

/// Appends a stable diagnostic when a guard contains an effect-only expression.
///
/// Inputs:
/// - `guard`: syntax-output expression used as a clause guard.
/// - `label`: user-facing guard context, for example `case guard`.
/// - `templates`: local template declarations whose generated calls render.
/// - `facts`: resolved local, imported, receiver, and trait effect identities.
/// - `errors`: mutable typecheck error sink.
///
/// Output:
/// - No direct return value; `errors` receives the first impure expression
///   diagnostic when one is found.
///
/// Transformation:
/// - Applies the same structural and resolved-call classifier used by `@pure`
///   bodies so guards cannot bypass imported or transitively local effects.
pub(crate) fn check_clause_guard_purity(
    guard: &SyntaxExprOutput,
    label: &str,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext<'_>,
    subst: &HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) {
    let mut proven_pure_receiver_calls = Vec::new();
    collect_proven_pure_receiver_calls(guard, locals, ctx, subst, &mut proven_pure_receiver_calls);
    let facts = EffectfulCallFacts {
        proven_pure_receiver_calls: Some(&proven_pure_receiver_calls),
        ..ctx.effectful_calls
    };
    check_pure_expression_effects_with_call_facts(guard, label, ctx.templates, &facts, errors);
}

/// Collects receiver calls that normal dispatch proves are pure primitives.
///
/// Primitive dispatch runs before imported and trait receiver dispatch. Recording
/// the exact syntax node prevents nested calls with coarse shared spans from
/// proving one another pure while preserving conservative treatment for
/// user-defined receivers.
pub(in crate::compiler::typeck) fn collect_proven_pure_receiver_calls(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext<'_>,
    subst: &HashMap<TypeVarId, Type>,
    calls: &mut Vec<usize>,
) {
    collect_proven_pure_module_call(expr, locals, ctx, subst, calls);
    if let Some((receiver, method)) = receiver_method_call_parts(expr) {
        let mut trial_subst = subst.clone();
        let mut trial_errors = Vec::new();
        let receiver_type = apply_subst(
            &infer_syntax_expr(receiver, locals, ctx, &mut trial_subst, &mut trial_errors),
            &trial_subst,
        );
        let primitive = is_primitive_dispatch_receiver(&receiver_type)
            && primitive_receiver_method_scheme(&receiver_type, method, expr.arity).is_some();
        let imported = if !matches!(receiver_type, Type::Dynamic) {
            let mut argument_errors = Vec::new();
            let argument_types = expr
                .children
                .iter()
                .skip(1)
                .map(|argument| {
                    infer_syntax_expr(
                        argument,
                        locals,
                        ctx,
                        &mut trial_subst,
                        &mut argument_errors,
                    )
                })
                .collect::<Vec<_>>();
            if argument_types
                .iter()
                .all(|argument| !matches!(argument, Type::Dynamic))
            {
                let matching = ctx
                    .receiver_methods
                    .get(&(method.to_string(), expr.arity))
                    .into_iter()
                    .flatten()
                    .filter(|candidate| {
                        infer_receiver_method_candidate(
                            candidate,
                            Some(method),
                            &receiver_type,
                            &argument_types,
                            ctx,
                            &mut trial_subst.clone(),
                        )
                        .is_ok()
                    })
                    .collect::<Vec<_>>();
                !matching.is_empty() && matching.iter().all(|candidate| candidate.pure)
            } else {
                false
            }
        } else {
            false
        };
        let callbacks_are_pure = matching_receiver_callbacks_are_pure(
            method,
            expr,
            &receiver_type,
            locals,
            ctx,
            &trial_subst,
        );
        if primitive || (imported && callbacks_are_pure) {
            calls.push(call_site_id(expr));
        }
    }

    expr.children
        .iter()
        .for_each(|child| collect_proven_pure_receiver_calls(child, locals, ctx, subst, calls));
    expr.fields.iter().for_each(|field| {
        collect_proven_pure_receiver_calls(&field.value, locals, ctx, subst, calls)
    });
    expr.clauses.iter().for_each(|clause| {
        if let Some(guard) = clause.guard.as_deref() {
            collect_proven_pure_receiver_calls(guard, locals, ctx, subst, calls);
        }
        collect_proven_pure_receiver_calls(&clause.body, locals, ctx, subst, calls);
    });
    expr.catch_clauses.iter().for_each(|clause| {
        if let Some(guard) = clause.guard.as_deref() {
            collect_proven_pure_receiver_calls(guard, locals, ctx, subst, calls);
        }
        collect_proven_pure_receiver_calls(&clause.body, locals, ctx, subst, calls);
    });
    if let Some(after) = expr.try_after.as_ref() {
        collect_proven_pure_receiver_calls(&after.trigger, locals, ctx, subst, calls);
        collect_proven_pure_receiver_calls(&after.body, locals, ctx, subst, calls);
    }
}

/// Records selected and module-qualified calls whose concrete callback
/// arguments satisfy a conditional purity contract.
fn collect_proven_pure_module_call(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext<'_>,
    subst: &HashMap<TypeVarId, Type>,
    calls: &mut Vec<usize>,
) {
    let signatures = if let Some((module_alias, function)) = module_member_call_identity(expr) {
        let Some(module) = ctx.module_aliases.get(module_alias) else {
            return;
        };
        let Some(interface) = ctx.interface_map.get(module) else {
            return;
        };
        interface
            .function_overloads
            .get(&(function.to_string(), expr.arity))
    } else if let Some((function, arity)) = local_syntax_call_identity(expr) {
        let identity = (function.to_string(), arity);
        if ctx
            .effectful_calls
            .conditional_local
            .is_some_and(|calls| calls.contains(&identity))
        {
            let Some(schemes) = ctx.signatures.get(&identity) else {
                return;
            };
            if !schemes.is_empty()
                && schemes
                    .iter()
                    .all(|scheme| scheme_callbacks_are_pure(scheme, expr, locals, ctx, subst))
            {
                calls.push(call_site_id(expr));
            }
            return;
        }
        let Some(targets) = ctx.function_imports.get(function) else {
            return;
        };
        let mut imported = Vec::new();
        for target in targets {
            let module = ctx
                .module_aliases
                .get(&target.module)
                .map(String::as_str)
                .unwrap_or(target.module.as_str());
            let Some(interface) = ctx.interface_map.get(module) else {
                return;
            };
            let Some(found) = interface
                .function_overloads
                .get(&(target.function.clone(), arity))
            else {
                return;
            };
            imported.extend(found.iter());
        }
        if imported.is_empty()
            || !imported
                .iter()
                .all(|signature| signature.public && signature.pure)
        {
            return;
        }
        if imported
            .iter()
            .all(|signature| signature_callbacks_are_pure(signature, expr, locals, ctx, subst))
        {
            calls.push(call_site_id(expr));
        }
        return;
    } else {
        return;
    };

    let Some(signatures) = signatures else {
        return;
    };
    let public = signatures
        .iter()
        .filter(|signature| signature.public)
        .collect::<Vec<_>>();
    if !public.is_empty()
        && public.iter().all(|signature| {
            signature.pure && signature_callbacks_are_pure(signature, expr, locals, ctx, subst)
        })
    {
        calls.push(call_site_id(expr));
    }
}

fn scheme_callbacks_are_pure(
    scheme: &super::super::FunctionScheme,
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext<'_>,
    subst: &HashMap<TypeVarId, Type>,
) -> bool {
    scheme
        .params
        .iter()
        .zip(expr.children.iter().skip(1))
        .all(|(parameter, argument)| {
            let Type::Function { params, .. } = expand_type_aliases(parameter, ctx.aliases) else {
                return true;
            };
            callback_is_proven_pure(argument, params.len(), ctx, &ctx.effectful_calls) && {
                let mut trial_subst = subst.clone();
                let mut errors = Vec::new();
                let argument_type =
                    infer_syntax_expr(argument, locals, ctx, &mut trial_subst, &mut errors);
                errors.is_empty()
                    && matches!(
                        expand_type_aliases(&argument_type, ctx.aliases),
                        Type::Function { .. }
                    )
            }
        })
}

fn signature_callbacks_are_pure(
    signature: &crate::terlan_hir::FunctionSignature,
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext<'_>,
    subst: &HashMap<TypeVarId, Type>,
) -> bool {
    let arguments = expr.children.iter().skip(1).collect::<Vec<_>>();
    signature
        .params
        .iter()
        .zip(arguments)
        .all(|(parameter, argument)| {
            if !parameter.annotation.contains("->") {
                return true;
            }
            let mut trial_subst = subst.clone();
            let mut errors = Vec::new();
            let callback_type = apply_subst(
                &infer_syntax_expr(argument, locals, ctx, &mut trial_subst, &mut errors),
                &trial_subst,
            );
            let Type::Function { params, .. } = expand_type_aliases(&callback_type, ctx.aliases)
            else {
                return false;
            };
            errors.is_empty()
                && callback_is_proven_pure(argument, params.len(), ctx, &ctx.effectful_calls)
        })
}

/// Proves the callback arguments of every matching pure receiver candidate.
///
/// A receiver combinator is conditionally pure: calling the method adds no
/// effect of its own, while invoking an effectful callback still makes the
/// complete call effectful. Candidates without callback parameters are pure
/// immediately.
fn matching_receiver_callbacks_are_pure(
    method: &str,
    expr: &SyntaxExprOutput,
    receiver_type: &Type,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext<'_>,
    subst: &HashMap<TypeVarId, Type>,
) -> bool {
    let Some(candidates) = ctx.receiver_methods.get(&(method.to_string(), expr.arity)) else {
        return false;
    };

    let arguments = expr.children.iter().skip(1).collect::<Vec<_>>();
    let matching = candidates
        .iter()
        .filter(|candidate| candidate.pure)
        .filter(|candidate| {
            let mut trial_subst = subst.clone();
            let mut errors = Vec::new();
            let argument_types = arguments
                .iter()
                .map(|argument| {
                    infer_syntax_expr(argument, locals, ctx, &mut trial_subst, &mut errors)
                })
                .collect::<Vec<_>>();
            errors.is_empty()
                && infer_receiver_method_candidate(
                    candidate,
                    Some(method),
                    receiver_type,
                    &argument_types,
                    ctx,
                    &mut trial_subst,
                )
                .is_ok()
        })
        .collect::<Vec<_>>();

    !matching.is_empty()
        && matching.iter().all(|candidate| {
            candidate
                .scheme
                .params
                .iter()
                .zip(arguments.iter())
                .all(|(parameter, argument)| {
                    let parameter = expand_type_aliases(parameter, ctx.aliases);
                    match parameter {
                        Type::Function { params, .. } => callback_is_proven_pure(
                            argument,
                            params.len(),
                            ctx,
                            &ctx.effectful_calls,
                        ),
                        _ => true,
                    }
                })
        })
}

/// Proves that invoking a callback expression cannot perform an effect.
fn callback_is_proven_pure(
    callback: &SyntaxExprOutput,
    arity: usize,
    ctx: &ExprInferContext<'_>,
    facts: &EffectfulCallFacts<'_>,
) -> bool {
    match callback.kind {
        SyntaxExprKind::Fun => callback.clauses.iter().all(|clause| {
            clause
                .guard
                .as_deref()
                .is_none_or(|guard| first_impure_expr(guard, ctx.templates, facts).is_none())
                && first_impure_expr(&clause.body, ctx.templates, facts).is_none()
        }),
        SyntaxExprKind::Var => {
            let Some(name) = callback.text.as_deref() else {
                return false;
            };
            if facts
                .conditional_function_values
                .is_some_and(|values| values.contains(name))
            {
                return true;
            }
            let identity = (name.to_string(), arity);
            if ctx.signatures.contains_key(&identity) {
                return !facts.local.is_some_and(|calls| calls.contains(&identity));
            }
            ctx.function_imports.get(name).is_some_and(|targets| {
                !targets.is_empty()
                    && targets.iter().all(|target| {
                        let module = ctx
                            .module_aliases
                            .get(&target.module)
                            .map(String::as_str)
                            .unwrap_or(target.module.as_str());
                        ctx.interface_map.get(module).is_some_and(|interface| {
                            interface
                                .function_overloads
                                .get(&(target.function.clone(), arity))
                                .is_some_and(|signatures| {
                                    !signatures.is_empty()
                                        && signatures
                                            .iter()
                                            .filter(|signature| signature.public)
                                            .all(|signature| signature.pure)
                                })
                        })
                    })
            })
        }
        SyntaxExprKind::FieldAccess => {
            let Some(module_alias) = callback
                .children
                .first()
                .and_then(|receiver| receiver.text.as_deref())
            else {
                return false;
            };
            let Some(member) = callback.text.as_deref() else {
                return false;
            };
            let Some(module) = ctx.module_aliases.get(module_alias) else {
                return false;
            };
            ctx.interface_map.get(module).is_some_and(|interface| {
                interface
                    .function_overloads
                    .get(&(member.to_string(), arity))
                    .is_some_and(|signatures| {
                        !signatures.is_empty()
                            && signatures
                                .iter()
                                .filter(|signature| signature.public)
                                .all(|signature| signature.pure)
                    })
            })
        }
        _ => false,
    }
}

/// Reports whether receiver dispatch resolves through primitive pure methods.
fn is_primitive_dispatch_receiver(receiver_type: &Type) -> bool {
    matches!(
        receiver_type,
        Type::Binary | Type::Int | Type::LiteralInt(_) | Type::Float | Type::Dynamic
    )
}

/// Extracts receiver and method name from a local receiver-call expression.
fn receiver_method_call_parts(expr: &SyntaxExprOutput) -> Option<(&SyntaxExprOutput, &str)> {
    if expr.kind != SyntaxExprKind::Call || expr.remote.is_some() {
        return None;
    }
    let callee = expr.children.first()?;
    if callee.kind != SyntaxExprKind::FieldAccess {
        return None;
    }
    Some((callee.children.first()?, callee.text.as_deref()?))
}

/// Returns the in-memory identity of one syntax expression during a purity pass.
///
/// Collection and validation traverse the same immutable syntax tree, so this
/// identity distinguishes nested calls even when a parser assigns them the same
/// coarse source span.
fn call_site_id(expr: &SyntaxExprOutput) -> usize {
    std::ptr::from_ref(expr).addr()
}

/// Appends a stable diagnostic when a pure context calls known effectful locals.
///
/// Inputs:
/// - `expr`: syntax-output expression used inside a pure context.
/// - `label`: user-facing context, for example `function foo annotated @pure`.
/// - `templates`: local template declarations whose generated calls render.
/// - `facts`: resolved local, imported, receiver, and trait effect identities.
/// - `errors`: mutable typecheck error sink.
///
/// Output:
/// - No direct return value; `errors` receives the first impure expression
///   diagnostic when one is found.
///
/// Transformation:
/// - Extends the structural purity walk with a first same-module call graph
///   check, so an `@pure` function cannot hide effects behind a local helper.
pub(crate) fn check_pure_expression_effects_with_call_facts(
    expr: &SyntaxExprOutput,
    label: &str,
    templates: &HashMap<String, TemplateScheme>,
    facts: &EffectfulCallFacts<'_>,
    errors: &mut Vec<String>,
) {
    if let Some(impure) = first_impure_expr(expr, templates, facts) {
        errors.push(format!(
            "{label} must be pure; found {}",
            impure_guard_expr_label(impure)
        ));
    }
}

/// Returns whether an expression contains a direct effect or a known effectful local call.
///
/// Inputs:
/// - `expr`: syntax-output expression subtree.
/// - `templates`: local template declarations whose generated calls render.
/// - `facts`: resolved local, imported, receiver, and trait effect identities.
///
/// Output:
/// - `true` when the expression contains an effect-only syntax form or calls a
///   known effectful local helper.
///
/// Transformation:
/// - Reuses the same classifier as `@pure` validation so the local purity
///   inference pre-pass can be iterated to a fixed point instead of stopping at
///   direct structural effects.
pub(crate) fn expression_has_effects_with_call_facts(
    expr: &SyntaxExprOutput,
    templates: &HashMap<String, TemplateScheme>,
    facts: &EffectfulCallFacts<'_>,
) -> bool {
    first_impure_expr(expr, templates, facts).is_some()
}

/// Returns the first expression classified as impure for a pure context.
///
/// Inputs:
/// - `expr`: syntax-output expression subtree.
/// - `templates`: local template declarations whose generated calls render.
/// - `facts`: resolved local, imported, receiver, and trait effect identities.
///
/// Output:
/// - `Some(impure)` for the first effectful expression or effectful local call;
///   otherwise `None`.
///
/// Transformation:
/// - Walks the expression tree and checks both direct syntax effects and the
///   first local call graph purity facts available to the typechecker.
fn first_impure_expr(
    expr: &SyntaxExprOutput,
    templates: &HashMap<String, TemplateScheme>,
    facts: &EffectfulCallFacts<'_>,
) -> Option<ImpureGuardExpr> {
    if syntax_expression_defers_body(expr) {
        return None;
    }
    if guard_expr_kind_is_impure(expr.kind) {
        return Some(ImpureGuardExpr::Kind(expr.kind));
    }
    if syntax_expr_is_template_call(expr, templates) {
        return Some(ImpureGuardExpr::TemplateCall);
    }
    if let Some((function_name, arity)) = local_syntax_call_identity(expr) {
        if facts
            .conditional_function_values
            .is_some_and(|names| names.contains(function_name))
        {
            return expr
                .children
                .iter()
                .skip(1)
                .find_map(|child| first_impure_expr(child, templates, facts));
        }
        if facts
            .function_values
            .is_some_and(|names| names.contains(function_name))
        {
            return Some(ImpureGuardExpr::UnprovenFunctionValueCall);
        }
        if facts
            .local
            .is_some_and(|calls| calls.contains(&(function_name.to_string(), arity)))
        {
            return Some(ImpureGuardExpr::EffectfulLocalCall);
        }
        let resolved_as_conditionally_pure = facts
            .proven_pure_receiver_calls
            .is_some_and(|calls| calls.contains(&call_site_id(expr)));
        if !resolved_as_conditionally_pure
            && facts
                .conditional_local
                .is_some_and(|calls| calls.contains(&(function_name.to_string(), arity)))
        {
            return Some(ImpureGuardExpr::EffectfulLocalCall);
        }
        if !resolved_as_conditionally_pure
            && facts
                .imported
                .is_some_and(|calls| calls.contains(&(function_name.to_string(), arity)))
        {
            return Some(ImpureGuardExpr::EffectfulImportedCall);
        }
    }
    if let Some((module_alias, function_name)) = module_member_call_identity(expr) {
        let identity = (
            module_alias.to_string(),
            function_name.to_string(),
            expr.arity,
        );
        let resolved_as_conditionally_pure = facts
            .proven_pure_receiver_calls
            .is_some_and(|calls| calls.contains(&call_site_id(expr)));
        if !resolved_as_conditionally_pure
            && facts
                .qualified
                .is_some_and(|calls| calls.contains(&identity))
        {
            return Some(ImpureGuardExpr::EffectfulImportedCall);
        }
        if !facts
            .module_aliases
            .is_some_and(|aliases| aliases.contains(module_alias))
            && facts
                .trait_qualified
                .is_some_and(|calls| calls.contains(&identity))
        {
            return Some(ImpureGuardExpr::EffectfulTraitCall);
        }
    }
    let empty_aliases = HashSet::new();
    let module_aliases = facts.module_aliases.unwrap_or(&empty_aliases);
    if let Some((method, arity)) = receiver_method_call_identity(expr, module_aliases) {
        let identity = (method.to_string(), arity);
        let resolved_as_pure_primitive = facts
            .proven_pure_receiver_calls
            .is_some_and(|calls| calls.contains(&call_site_id(expr)));
        if !resolved_as_pure_primitive
            && facts
                .imported_receiver
                .is_some_and(|calls| calls.contains(&identity))
        {
            return Some(ImpureGuardExpr::EffectfulImportedReceiverCall);
        }
        if !resolved_as_pure_primitive
            && facts
                .trait_receiver
                .is_some_and(|calls| calls.contains(&identity))
        {
            return Some(ImpureGuardExpr::EffectfulTraitReceiverCall);
        }
    }

    expr.children
        .iter()
        .find_map(|child| first_impure_expr(child, templates, facts))
        .or_else(|| {
            expr.fields
                .iter()
                .find_map(|field| first_impure_expr(&field.value, templates, facts))
        })
        .or_else(|| {
            expr.clauses.iter().find_map(|clause| {
                clause
                    .guard
                    .as_deref()
                    .and_then(|guard| first_impure_expr(guard, templates, facts))
                    .or_else(|| first_impure_expr(&clause.body, templates, facts))
            })
        })
        .or_else(|| {
            expr.catch_clauses.iter().find_map(|clause| {
                clause
                    .guard
                    .as_deref()
                    .and_then(|guard| first_impure_expr(guard, templates, facts))
                    .or_else(|| first_impure_expr(&clause.body, templates, facts))
            })
        })
        .or_else(|| {
            expr.try_after.as_ref().and_then(|after| {
                first_impure_expr(&after.trigger, templates, facts)
                    .or_else(|| first_impure_expr(&after.body, templates, facts))
            })
        })
}

/// Extracts a module-member call identity from `Module.member(...)`.
///
/// Inputs:
/// - `expr`: syntax-output expression subtree.
///
/// Output:
/// - `Some((module_alias, member))` for module-shaped member calls.
/// - `None` for local calls, receiver calls, or malformed call expressions.
///
/// Transformation:
/// - Reads syntax-output call metadata only; semantic import resolution remains
///   in the normal typechecker path.
fn module_member_call_identity(expr: &SyntaxExprOutput) -> Option<(&str, &str)> {
    if expr.kind != SyntaxExprKind::Call {
        return None;
    }
    if let Some(remote) = expr.remote.as_deref() {
        return Some((remote, expr.children.first()?.text.as_deref()?));
    }
    let callee = expr.children.first()?;
    if callee.kind != SyntaxExprKind::FieldAccess {
        return None;
    }
    let receiver = callee.children.first()?;
    match receiver.kind {
        SyntaxExprKind::Atom | SyntaxExprKind::Var => {
            Some((receiver.text.as_deref()?, callee.text.as_deref()?))
        }
        _ => None,
    }
}

fn receiver_method_call_identity<'a>(
    expr: &'a SyntaxExprOutput,
    imported_module_aliases: &HashSet<String>,
) -> Option<(&'a str, usize)> {
    let (receiver, method) = receiver_method_call_parts(expr)?;
    if matches!(receiver.kind, SyntaxExprKind::Atom | SyntaxExprKind::Var)
        && receiver
            .text
            .as_ref()
            .is_some_and(|name| imported_module_aliases.contains(name))
    {
        return None;
    }
    Some((method, expr.arity))
}

/// Returns whether an expression kind is categorically invalid inside guards.
///
/// Inputs:
/// - `kind`: syntax-output expression kind.
///
/// Output:
/// - `true` when the expression form implies mutation, raw side effects, or
///   rendering/runtime work rather than pure guard selection.
///
/// Transformation:
/// - Keeps the first guard-purity rule explicit and auditable until full
///   inferred purity and effect typing are available.
fn guard_expr_kind_is_impure(kind: SyntaxExprKind) -> bool {
    matches!(
        kind,
        SyntaxExprKind::IndexAssign
            | SyntaxExprKind::FunctionCall
            | SyntaxExprKind::RawMacro
            | SyntaxExprKind::HtmlBlock
            | SyntaxExprKind::TemplateInstantiate
    )
}

/// Returns whether an expression is a direct generated template function call.
///
/// Inputs:
/// - `expr`: syntax-output expression.
/// - `templates`: template declarations visible in the current module.
///
/// Output:
/// - `true` when the expression is `Page(...)` for a declared local template.
///
/// Transformation:
/// - Mirrors template-call normalization early enough for guard and `@pure`
///   validation, before expression inference rewrites the call to an explicit
///   template-instantiation node.
fn syntax_expr_is_template_call(
    expr: &SyntaxExprOutput,
    templates: &HashMap<String, TemplateScheme>,
) -> bool {
    if expr.kind != SyntaxExprKind::Call || expr.remote.is_some() {
        return false;
    }

    expr.children.first().is_some_and(|callee| {
        callee.kind == SyntaxExprKind::Var
            && callee
                .text
                .as_deref()
                .is_some_and(|name| templates.contains_key(name))
    })
}

/// One expression form classified as effectful for guards and pure contexts.
#[derive(Clone, Copy)]
enum ImpureGuardExpr {
    Kind(SyntaxExprKind),
    UnprovenFunctionValueCall,
    TemplateCall,
    EffectfulLocalCall,
    EffectfulImportedCall,
    EffectfulImportedReceiverCall,
    EffectfulTraitCall,
    EffectfulTraitReceiverCall,
}

/// Returns the diagnostic label for one impure guard expression kind.
///
/// Inputs:
/// - `kind`: expression kind already classified as impure for guards.
///
/// Output:
/// - Stable, source-facing expression label.
///
/// Transformation:
/// - Avoids leaking Rust enum names into diagnostics.
fn impure_guard_expr_label(impure: ImpureGuardExpr) -> &'static str {
    match impure {
        ImpureGuardExpr::TemplateCall => "template instantiation",
        ImpureGuardExpr::UnprovenFunctionValueCall => "unproven function-value call",
        ImpureGuardExpr::EffectfulLocalCall => "effectful local function call",
        ImpureGuardExpr::EffectfulImportedCall => "effectful imported function call",
        ImpureGuardExpr::EffectfulImportedReceiverCall => "effectful imported receiver method call",
        ImpureGuardExpr::EffectfulTraitCall => "effectful trait call without a purity contract",
        ImpureGuardExpr::EffectfulTraitReceiverCall => {
            "effectful receiver-style trait call without a purity contract"
        }
        ImpureGuardExpr::Kind(SyntaxExprKind::IndexAssign) => "indexed assignment",
        ImpureGuardExpr::Kind(SyntaxExprKind::FunctionCall) => "unproven function-value call",
        ImpureGuardExpr::Kind(SyntaxExprKind::RawMacro) => "raw macro",
        ImpureGuardExpr::Kind(SyntaxExprKind::HtmlBlock) => "html block",
        ImpureGuardExpr::Kind(SyntaxExprKind::TemplateInstantiate) => "template instantiation",
        _ => "impure expression",
    }
}
