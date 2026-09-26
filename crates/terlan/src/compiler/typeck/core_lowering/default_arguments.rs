//! Declaration-order materialization of named and default call arguments.

use std::collections::HashMap;

use crate::terlan_syntax::{
    SyntaxDeclarationPayload, SyntaxExprKind, SyntaxExprOutput, SyntaxModuleOutput,
};

use super::super::{CoreConstructorDecl, ResolvedModule};

#[derive(Clone)]
struct DefaultParameter {
    name: String,
    default: Option<SyntaxExprOutput>,
    variadic: bool,
}

type SignatureMap = HashMap<(Option<String>, String), Vec<Vec<DefaultParameter>>>;

/// Expands callable arguments before parser-owned names disappear at CoreIR.
///
/// Imported function calls have already been canonicalized to their provider
/// module when this pass runs. Each accepted call is rewritten to positional
/// declaration order and every omitted slot is populated from the checked
/// structured default stored in HIR.
pub(super) fn materialize_default_call_arguments(
    module: &mut SyntaxModuleOutput,
    resolved: &ResolvedModule,
    constructors: &[CoreConstructorDecl],
) {
    let mut signatures = collect_signatures(module, resolved);
    let initializers = signatures.clone();
    for (provider, interface) in std::iter::once((None, &resolved.interface)).chain(
        resolved
            .interface_map
            .iter()
            .map(|(name, interface)| (Some(name.clone()), interface)),
    ) {
        for (name, declarations) in &interface.constructors {
            signatures.insert(
                (provider.clone(), name.clone()),
                declarations
                    .iter()
                    .map(|declaration| {
                        declaration
                            .params
                            .iter()
                            .map(|param| DefaultParameter {
                                name: param.name.clone(),
                                default: param.default.clone(),
                                variadic: false,
                            })
                            .chain(declaration.vararg.iter().map(|param| DefaultParameter {
                                name: param.name.clone(),
                                default: None,
                                variadic: true,
                            }))
                            .collect()
                    })
                    .collect(),
            );
        }
    }
    for declaration in &mut module.declarations {
        let active_initializer = match &declaration.payload {
            SyntaxDeclarationPayload::Function { name, .. } => constructors
                .iter()
                .find(|constructor| {
                    constructor
                        .implementation
                        .as_ref()
                        .is_some_and(|implementation| implementation.function == *name)
                })
                .map(|constructor| (None, constructor.name.clone())),
            _ => None,
        };
        let mut scoped = None;
        if let Some(key) = active_initializer {
            if let Some(initializer) = initializers.get(&key) {
                let mut local = signatures.clone();
                local.insert(key, initializer.clone());
                scoped = Some(local);
            }
        }
        let signatures = scoped.as_ref().unwrap_or(&signatures);
        let clauses = match &mut declaration.payload {
            SyntaxDeclarationPayload::Function {
                params, clauses, ..
            }
            | SyntaxDeclarationPayload::Method {
                params, clauses, ..
            } => {
                for param in params {
                    if let Some(default) = &mut param.default {
                        materialize_expression(default, signatures);
                    }
                }
                clauses
            }
            _ => continue,
        };
        for clause in clauses {
            if let Some(guard) = &mut clause.guard {
                materialize_expression(guard, signatures);
            }
            materialize_expression(&mut clause.body, signatures);
        }
    }
}

fn collect_signatures(module: &SyntaxModuleOutput, resolved: &ResolvedModule) -> SignatureMap {
    let mut signatures = SignatureMap::new();
    for declaration in &module.declarations {
        let (name, parameters) = match &declaration.payload {
            SyntaxDeclarationPayload::Function { name, params, .. } => (
                name,
                params
                    .iter()
                    .map(|param| DefaultParameter {
                        name: param.name.clone(),
                        default: param.default.clone(),
                        variadic: false,
                    })
                    .collect(),
            ),
            SyntaxDeclarationPayload::Struct { name, fields, .. } => (
                name,
                fields
                    .iter()
                    .map(|field| DefaultParameter {
                        name: field.name.clone(),
                        default: field.default.clone(),
                        variadic: false,
                    })
                    .collect(),
            ),
            _ => continue,
        };
        signatures
            .entry((None, name.clone()))
            .or_default()
            .push(parameters);
    }
    for (module_name, interface) in &resolved.interface_map {
        for signature in interface
            .functions
            .values()
            .chain(interface.function_overloads.values().flatten())
        {
            let candidate = signature
                .params
                .iter()
                .map(|param| DefaultParameter {
                    name: param.name.clone(),
                    default: param.default.clone(),
                    variadic: false,
                })
                .collect::<Vec<_>>();
            let entry = signatures
                .entry((Some(module_name.clone()), signature.name.clone()))
                .or_default();
            if !entry
                .iter()
                .any(|existing| same_signature(existing, &candidate))
            {
                entry.push(candidate);
            }
        }
    }
    signatures
}

fn same_signature(left: &[DefaultParameter], right: &[DefaultParameter]) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            left.name == right.name
                && left.default == right.default
                && left.variadic == right.variadic
        })
}

fn materialize_expression(expr: &mut SyntaxExprOutput, signatures: &SignatureMap) {
    for child in &mut expr.children {
        materialize_expression(child, signatures);
    }
    for guard in expr.let_guards.iter_mut().flatten() {
        materialize_expression(guard, signatures);
    }
    for field in &mut expr.fields {
        materialize_expression(&mut field.value, signatures);
    }
    for clause in expr.clauses.iter_mut().chain(&mut expr.catch_clauses) {
        if let Some(guard) = &mut clause.guard {
            materialize_expression(guard, signatures);
        }
        materialize_expression(&mut clause.body, signatures);
    }
    if let Some(after) = &mut expr.try_after {
        materialize_expression(&mut after.trigger, signatures);
        materialize_expression(&mut after.body, signatures);
    }

    if expr.kind != SyntaxExprKind::Call {
        return;
    }
    let Some((callee, args)) = expr.children.split_first() else {
        return;
    };
    if !matches!(callee.kind, SyntaxExprKind::Var | SyntaxExprKind::Atom) {
        return;
    }
    let Some(function) = callee.text.as_ref() else {
        return;
    };
    let key = (expr.remote.clone(), function.clone());
    let Some(candidates) = signatures.get(&key) else {
        return;
    };
    let expanded = candidates
        .iter()
        .filter_map(|candidate| expand_arguments(args, &expr.arg_names, candidate))
        .collect::<Vec<_>>();
    let Some(first) = expanded.first() else {
        return;
    };
    if expanded.iter().skip(1).any(|candidate| candidate != first) {
        return;
    }
    let callee = callee.clone();
    expr.children = std::iter::once(callee)
        .chain(first.iter().cloned())
        .collect();
    expr.arg_names = vec![None; first.len()];
    expr.arity = first.len();
}

fn expand_arguments(
    args: &[SyntaxExprOutput],
    arg_names: &[Option<String>],
    params: &[DefaultParameter],
) -> Option<Vec<SyntaxExprOutput>> {
    let variadic = params.last().is_some_and(|param| param.variadic);
    let params = if variadic {
        &params[..params.len() - 1]
    } else {
        params
    };
    if !variadic && args.len() > params.len() {
        return None;
    }
    let mut slots = vec![None; params.len()];
    let mut positional = 0usize;
    let mut tail = Vec::new();
    for (index, argument) in args.iter().enumerate() {
        let target = match arg_names.get(index).and_then(Option::as_ref) {
            Some(name) => params.iter().position(|param| &param.name == name)?,
            None => {
                let target = positional;
                positional = positional.saturating_add(1);
                target
            }
        };
        if variadic && target >= slots.len() {
            tail.push(argument.clone());
            continue;
        }
        if target >= slots.len() || slots[target].is_some() {
            return None;
        }
        slots[target] = Some(argument.clone());
    }
    let mut arguments = slots
        .into_iter()
        .zip(params)
        .map(|(argument, param)| argument.or_else(|| param.default.clone()))
        .collect::<Option<Vec<_>>>()?;
    arguments.extend(tail);
    Some(arguments)
}

#[cfg(test)]
#[path = "default_arguments_test.rs"]
mod test;
