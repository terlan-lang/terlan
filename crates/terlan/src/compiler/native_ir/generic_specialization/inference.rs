//! Contextual expression inference for generic specialization.

use std::collections::{HashMap, HashSet};

use crate::terlan_typeck::{CoreExpr, CoreFunction, CoreTupleTypeElem, CoreType};

use super::{
    callable_templates, collect_implicit_generic_params, contains_generic_parameter, substitute,
    unify, CallableTemplates,
};

pub(super) fn infer_type(
    expr: &CoreExpr,
    variables: &HashMap<String, CoreType>,
    templates: &CallableTemplates,
    module: &str,
) -> Option<CoreType> {
    match expr {
        CoreExpr::Int(_) => Some(CoreType::Int),
        CoreExpr::Float(_) => Some(CoreType::Float),
        CoreExpr::Binary(_) => Some(CoreType::String),
        CoreExpr::Atom(value) if matches!(value.as_str(), "true" | "false") => Some(CoreType::Bool),
        CoreExpr::Atom(value) if value == "Unit" => Some(CoreType::Named("Unit".into())),
        CoreExpr::Atom(value) => Some(CoreType::AtomLiteral(value.clone())),
        CoreExpr::Var(name) if matches!(name.as_str(), "true" | "false") => Some(CoreType::Bool),
        CoreExpr::Var(name) if name == "Unit" => Some(CoreType::Named("Unit".into())),
        CoreExpr::Var(name) => variables.get(name).cloned(),
        CoreExpr::List(items) if !items.is_empty() => {
            let first = infer_type(&items[0], variables, templates, module)?;
            items[1..]
                .iter()
                .all(|item| infer_type(item, variables, templates, module) == Some(first.clone()))
                .then(|| CoreType::List(Box::new(first)))
        }
        CoreExpr::Tuple(items) => items
            .iter()
            .map(|item| infer_type(item, variables, templates, module).map(CoreTupleTypeElem::Type))
            .collect::<Option<Vec<_>>>()
            .map(CoreType::Tuple),
        CoreExpr::RecordConstruct { name, .. } => Some(CoreType::Named(name.clone())),
        CoreExpr::Intrinsic(call) => Some(call.return_type.clone()),
        CoreExpr::UnaryOp { operator, .. } if matches!(operator.as_str(), "not" | "!") => {
            Some(CoreType::Bool)
        }
        CoreExpr::UnaryOp { operator, operand } if operator == "-" => {
            infer_type(operand, variables, templates, module)
        }
        CoreExpr::BinaryOp { operator, .. }
            if matches!(
                operator.as_str(),
                "==" | "!=" | "<" | "<=" | ">" | ">=" | "and" | "or"
            ) =>
        {
            Some(CoreType::Bool)
        }
        CoreExpr::BinaryOp { left, .. } => infer_type(left, variables, templates, module),
        CoreExpr::FieldAccess { base, field } | CoreExpr::RecordAccess { base, field, .. } => {
            let base_type = infer_type(base, variables, templates, module)?;
            named_field_type(&base_type, field).cloned()
        }
        CoreExpr::Cast { expr, target_type } if contains_implicit_generic_type(target_type) => {
            infer_type(expr, variables, templates, module)
        }
        CoreExpr::Cast { target_type, .. } => Some(target_type.clone()),
        CoreExpr::Call { function, args }
            if function.rsplit('.').next() == Some("unwrap") && args.len() == 1 =>
        {
            match infer_type(&args[0], variables, templates, module)? {
                CoreType::Apply { constructor, args }
                    if constructor.rsplit('.').next() == Some("Message") && args.len() == 1 =>
                {
                    args.into_iter().next()
                }
                _ => None,
            }
        }
        CoreExpr::Call { function, args }
            if matches!(variables.get(function), Some(CoreType::Arrow { .. })) =>
        {
            let CoreType::Arrow {
                params,
                return_type,
            } = variables.get(function)?
            else {
                unreachable!("guard requires an arrow type")
            };
            (params.len() == args.len()).then(|| return_type.as_ref().clone())
        }
        CoreExpr::Call { function, args } => {
            let candidates = callable_templates(templates, module, function, args.len())?;
            let argument_types = args
                .iter()
                .map(|argument| infer_type(argument, variables, templates, module))
                .collect::<Option<Vec<_>>>();
            let Some(argument_types) = argument_types else {
                return common_concrete_return_type(candidates);
            };
            let mut matched_return = None;
            for template in candidates {
                let mut values = HashMap::new();
                let matches =
                    template
                        .params
                        .iter()
                        .zip(&argument_types)
                        .all(|(parameter, argument)| {
                            parameter.core_ty.as_ref().is_some_and(|expected| {
                                unify(expected, argument, &template.generic_params, &mut values)
                                    .is_ok()
                            })
                        });
                if !matches {
                    continue;
                }
                let result = substitute(
                    template.core_return_type.as_ref()?,
                    &template.generic_params,
                    &values,
                );
                if matched_return
                    .as_ref()
                    .is_some_and(|prior| prior != &result)
                {
                    return None;
                }
                matched_return = Some(result);
            }
            matched_return.or_else(|| common_concrete_return_type(candidates))
        }
        _ => None,
    }
}

fn named_field_type<'a>(ty: &'a CoreType, name: &str) -> Option<&'a CoreType> {
    match ty {
        CoreType::Tuple(elements) => elements.iter().find_map(|element| match element {
            CoreTupleTypeElem::Field { name: field, ty } if field == name => Some(ty),
            _ => None,
        }),
        CoreType::Struct { fields, .. } => fields
            .iter()
            .find(|field| field.name == name)
            .map(|field| &field.ty),
        CoreType::Map(fields) => fields
            .iter()
            .find(|field| field.key == name)
            .map(|field| &field.value),
        _ => None,
    }
}

pub(super) fn common_concrete_return_type(candidates: &[CoreFunction]) -> Option<CoreType> {
    let first = candidates.first()?.core_return_type.as_ref()?;
    if contains_generic_parameter(first, &candidates[0].generic_params)
        || candidates.iter().skip(1).any(|candidate| {
            candidate.core_return_type.as_ref() != Some(first)
                || contains_generic_parameter(
                    candidate
                        .core_return_type
                        .as_ref()
                        .expect("return type equality checked first"),
                    &candidate.generic_params,
                )
        })
    {
        return None;
    }
    Some(first.clone())
}

pub(super) fn common_concrete_parameter_types(
    candidates: &[CoreFunction],
) -> Option<Vec<CoreType>> {
    let first = candidates.first()?;
    let parameters = first
        .params
        .iter()
        .map(|parameter| parameter.core_ty.clone())
        .collect::<Option<Vec<_>>>()?;
    if parameters
        .iter()
        .any(|ty| contains_generic_parameter(ty, &first.generic_params))
        || candidates.iter().skip(1).any(|candidate| {
            candidate.params.len() != parameters.len()
                || candidate
                    .params
                    .iter()
                    .zip(&parameters)
                    .any(|(parameter, expected)| {
                        parameter.core_ty.as_ref() != Some(expected)
                            || contains_generic_parameter(
                                parameter
                                    .core_ty
                                    .as_ref()
                                    .expect("parameter equality checked first"),
                                &candidate.generic_params,
                            )
                    })
        })
    {
        return None;
    }
    Some(parameters)
}

pub(super) fn needs_contextual_type(expr: &CoreExpr) -> bool {
    matches!(
        expr,
        CoreExpr::List(_)
            | CoreExpr::Tuple(_)
            | CoreExpr::Map(_)
            | CoreExpr::ConstructorCall { .. }
    )
}

fn contains_implicit_generic_type(ty: &CoreType) -> bool {
    let mut names = HashSet::new();
    collect_implicit_generic_params(ty, &mut names);
    !names.is_empty()
}
