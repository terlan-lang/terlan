//! Retains concrete nominal struct layouts used by monomorphized applications.

use super::super::{
    specialization_budget::{SpecializationBudget, SpecializationKind},
    NativeIrResult,
};
use std::collections::{HashMap, HashSet};

use crate::terlan_typeck::{
    visit_core_expr_mut, CoreExpr, CoreModule, CoreTupleTypeElem, CoreType,
};

/// Instantiates checked declarations using the same substitution as functions.
/// The application type remains nominal; each argument vector owns a distinct
/// managed identity, even when two instantiations have identical physical kinds.
pub(super) fn retain(
    cores: &mut [CoreModule],
    budget: &mut SpecializationBudget,
) -> NativeIrResult<()> {
    let templates = cores
        .iter()
        .enumerate()
        .flat_map(|(owner, core)| {
            core.types
                .iter()
                .filter(|declaration| {
                    matches!(declaration.core_body, Some(CoreType::Struct { .. }))
                        && !declaration.params.is_empty()
                })
                .map(move |declaration| {
                    (
                        format!("{}.{}", core.module, declaration.name),
                        (owner, declaration.clone()),
                    )
                })
        })
        .collect::<HashMap<_, _>>();
    if templates.is_empty() {
        return Ok(());
    }
    let mut pending = Vec::new();
    for core in cores.iter_mut() {
        for function in &mut core.functions {
            for ty in function
                .params
                .iter()
                .filter_map(|parameter| parameter.core_ty.as_ref())
                .chain(function.core_return_type.iter())
            {
                collect(ty, &mut pending);
            }
            for clause in &mut function.clauses {
                for expression in clause
                    .guard
                    .iter_mut()
                    .filter_map(|guard| guard.core_expr.as_mut())
                    .chain(clause.body.core_expr.as_mut())
                {
                    visit_core_expr_mut(expression, &mut |expr| match expr {
                        CoreExpr::Cast { target_type, .. } => collect(target_type, &mut pending),
                        CoreExpr::Intrinsic(call) => collect(&call.return_type, &mut pending),
                        CoreExpr::Lam {
                            parameter_types, ..
                        } => {
                            for ty in parameter_types.iter().flatten() {
                                collect(ty, &mut pending);
                            }
                        }
                        _ => {}
                    });
                }
            }
        }
    }
    let mut seen = HashSet::new();
    while let Some(application) = pending.pop() {
        let CoreType::Apply { constructor, args } = &application else {
            continue;
        };
        let Some((owner, template)) = templates.get(constructor) else {
            continue;
        };
        // An open generic signature may remain in a template catalog long
        // enough to be visited here. It describes no runtime identity and
        // must never become an admitted managed layout. Only applications
        // whose arguments no longer mention this declaration's parameters
        // are concrete struct instances.
        if args
            .iter()
            .any(|argument| contains_parameter(argument, &template.params))
        {
            continue;
        }
        let canonical = super::super::expression::managed_semantic_contract(&application);
        if args.len() != template.params.len() || !seen.insert(canonical.clone()) {
            continue;
        }
        budget.reserve(SpecializationKind::Generic, &cores[*owner].module, 1)?;
        let values = template
            .params
            .iter()
            .cloned()
            .zip(args.iter().cloned())
            .collect();
        let mut declaration = template.clone();
        let Some(body) = &template.core_body else {
            continue;
        };
        let mut body =
            super::super::generic_specialization::substitute(body, &template.params, &values);
        let CoreType::Struct { name, fields } = &mut body else {
            continue;
        };
        *name = canonical.clone();
        for field in fields {
            collect(&field.ty, &mut pending);
        }
        declaration.name = canonical;
        declaration.params.clear();
        declaration.core_body = Some(body);
        cores[*owner].types.push(declaration);
    }
    Ok(())
}

fn contains_parameter(ty: &CoreType, parameters: &[String]) -> bool {
    match ty {
        CoreType::Named(name) => parameters.contains(name),
        CoreType::Apply { args, .. } | CoreType::Union(args) => {
            args.iter().any(|ty| contains_parameter(ty, parameters))
        }
        CoreType::List(element) => contains_parameter(element, parameters),
        CoreType::Tuple(elements) => elements.iter().any(|element| {
            let (CoreTupleTypeElem::Type(ty) | CoreTupleTypeElem::Field { ty, .. }) = element;
            contains_parameter(ty, parameters)
        }),
        CoreType::Struct { fields, .. } => fields
            .iter()
            .any(|field| contains_parameter(&field.ty, parameters)),
        CoreType::Map(fields) => fields
            .iter()
            .any(|field| contains_parameter(&field.value, parameters)),
        CoreType::Arrow {
            params,
            return_type,
        } => {
            params.iter().any(|ty| contains_parameter(ty, parameters))
                || contains_parameter(return_type, parameters)
        }
        CoreType::Int
        | CoreType::Float
        | CoreType::Number
        | CoreType::String
        | CoreType::Binary
        | CoreType::Atom
        | CoreType::Bool
        | CoreType::Term
        | CoreType::Dynamic
        | CoreType::Never
        | CoreType::AtomLiteral(_) => false,
    }
}

fn collect(ty: &CoreType, pending: &mut Vec<CoreType>) {
    match ty {
        CoreType::Apply { args, .. } => {
            pending.push(ty.clone());
            for arg in args {
                collect(arg, pending);
            }
        }
        CoreType::List(element) => collect(element, pending),
        CoreType::Tuple(elements) => {
            for element in elements {
                let (CoreTupleTypeElem::Type(ty) | CoreTupleTypeElem::Field { ty, .. }) = element;
                collect(ty, pending);
            }
        }
        CoreType::Struct { fields, .. } => {
            for field in fields {
                collect(&field.ty, pending);
            }
        }
        CoreType::Map(fields) => {
            for field in fields {
                collect(&field.value, pending);
            }
        }
        CoreType::Arrow {
            params,
            return_type,
        } => {
            for parameter in params {
                collect(parameter, pending);
            }
            collect(return_type, pending);
        }
        CoreType::Union(variants) => {
            for variant in variants {
                collect(variant, pending);
            }
        }
        _ => {}
    }
}
