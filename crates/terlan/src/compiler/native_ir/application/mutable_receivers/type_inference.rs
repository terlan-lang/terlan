//! Contextual type inference used while resolving mutable receiver calls.

use std::collections::HashMap;

use crate::terlan_typeck::{CoreExpr, CorePattern, CoreType};

use super::{callable_identity, receiver_target, NominalTypes, ReceiverTargets};

pub(super) fn infer_core_type(
    expr: &CoreExpr,
    variables: &HashMap<String, CoreType>,
    functions: &HashMap<(String, usize), CoreType>,
    targets: &ReceiverTargets,
    types: &NominalTypes,
    module: &str,
) -> Option<CoreType> {
    match expr {
        CoreExpr::Var(name) if matches!(name.as_str(), "true" | "false") => Some(CoreType::Bool),
        CoreExpr::Var(name) => variables.get(name).cloned(),
        CoreExpr::Atom(value) if matches!(value.as_str(), "true" | "false") => Some(CoreType::Bool),
        CoreExpr::Atom(value) => Some(CoreType::AtomLiteral(value.clone())),
        CoreExpr::Int(_) => Some(CoreType::Int),
        CoreExpr::Float(_) => Some(CoreType::Float),
        CoreExpr::Binary(_) => Some(CoreType::String),
        CoreExpr::Intrinsic(call) => Some(call.return_type.clone()),
        CoreExpr::RecordConstruct { name, .. } => Some(CoreType::Named(name.clone())),
        CoreExpr::RecordUpdate { base, .. } => {
            infer_core_type(base, variables, functions, targets, types, module)
        }
        CoreExpr::FieldAccess { base, field } | CoreExpr::RecordAccess { base, field, .. } => {
            let base = infer_core_type(base, variables, functions, targets, types, module)?;
            nominal_field_type(&base, field, types)
        }
        CoreExpr::Call { function, args, .. } => {
            specialized_call_result(function, args, variables, functions, targets, types, module)
                .or_else(|| functions.get(&(function.clone(), args.len())).cloned())
        }
        CoreExpr::RemoteCall {
            module: owner,
            function,
            args,
            ..
        } => {
            let qualified = format!("{owner}.{function}");
            specialized_call_result(
                &qualified, args, variables, functions, targets, types, module,
            )
            .or_else(|| functions.get(&(qualified, args.len())).cloned())
        }
        CoreExpr::Cast { target_type, .. } => Some(target_type.clone()),
        CoreExpr::List(items) => items
            .first()
            .and_then(|item| infer_core_type(item, variables, functions, targets, types, module))
            .map(|item| CoreType::List(Box::new(item))),
        CoreExpr::ListCons { tail, .. } => {
            infer_core_type(tail, variables, functions, targets, types, module)
        }
        CoreExpr::Tuple(items) => Some(CoreType::Tuple(
            items
                .iter()
                .map(|item| {
                    infer_core_type(item, variables, functions, targets, types, module)
                        .map(crate::terlan_typeck::CoreTupleTypeElem::Type)
                })
                .collect::<Option<Vec<_>>>()?,
        )),
        CoreExpr::BinaryOp {
            operator,
            left,
            right,
        } => super::super::super::expression::infer_core_binary_type(
            operator,
            infer_core_type(left, variables, functions, targets, types, module),
            infer_core_type(right, variables, functions, targets, types, module),
        ),
        CoreExpr::Let { bindings, body } => {
            let mut locals = variables.clone();
            for binding in bindings {
                if let (CorePattern::Var(name), Some(ty)) = (
                    &binding.pattern,
                    infer_core_type(&binding.value, &locals, functions, targets, types, module),
                ) {
                    locals.insert(name.clone(), ty);
                }
            }
            infer_core_type(body, &locals, functions, targets, types, module)
        }
        CoreExpr::If { clauses } => common_branch_type(clauses.iter().map(|clause| {
            infer_core_type(&clause.body, variables, functions, targets, types, module)
        })),
        CoreExpr::Case { clauses, .. } => common_branch_type(clauses.iter().map(|clause| {
            infer_core_type(&clause.body, variables, functions, targets, types, module)
        })),
        _ => None,
    }
}

fn nominal_field_type(base: &CoreType, field: &str, types: &NominalTypes) -> Option<CoreType> {
    match base {
        CoreType::Struct { fields, .. } => fields
            .iter()
            .find(|candidate| candidate.name == field)
            .map(|candidate| candidate.ty.clone()),
        CoreType::Map(fields) => fields
            .iter()
            .find(|candidate| candidate.key == field)
            .map(|candidate| candidate.value.clone()),
        CoreType::Tuple(elements) => elements.iter().find_map(|element| match element {
            crate::terlan_typeck::CoreTupleTypeElem::Field { name, ty } if name == field => {
                Some(ty.clone())
            }
            _ => None,
        }),
        CoreType::Named(name) => nominal_field_type(&types.get(name)?.body, field, types),
        CoreType::Apply { constructor, args } => {
            let declaration = types.get(constructor)?;
            let substitution = declaration
                .params
                .iter()
                .cloned()
                .zip(args.iter().cloned())
                .collect::<HashMap<_, _>>();
            let body = super::super::super::generic_specialization::substitute(
                &declaration.body,
                &declaration.params,
                &substitution,
            );
            nominal_field_type(&body, field, types)
        }
        _ => None,
    }
}

/// Specializes a receiver method's result from its concrete receiver.
fn specialized_call_result(
    function: &str,
    args: &[CoreExpr],
    variables: &HashMap<String, CoreType>,
    functions: &HashMap<(String, usize), CoreType>,
    targets: &ReceiverTargets,
    types: &NominalTypes,
    module: &str,
) -> Option<CoreType> {
    let receiver = infer_core_type(args.first()?, variables, functions, targets, types, module)?;
    let method = function.rsplit('.').next()?;
    let target = receiver_target(method, args.len(), &receiver, module, targets)?;
    if callable_identity(target, module) != function {
        return None;
    }
    let result = target.result.as_ref()?;
    if target.generic_params.is_empty() {
        return Some(result.clone());
    }
    let mut substitution = HashMap::new();
    super::super::super::generic_specialization::unify(
        &target.receiver,
        &receiver,
        &target.generic_params,
        &mut substitution,
    )
    .ok()?;
    Some(super::super::super::generic_specialization::substitute(
        result,
        &target.generic_params,
        &substitution,
    ))
}

fn common_branch_type(types: impl Iterator<Item = Option<CoreType>>) -> Option<CoreType> {
    let mut types = types;
    let first = types.next()??;
    types
        .all(|candidate| candidate.as_ref() == Some(&first))
        .then_some(first)
}
