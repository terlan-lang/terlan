//! Contextual expression inference for generic specialization.

use std::collections::{HashMap, HashSet};

use crate::terlan_typeck::{
    CoreExpr, CoreFunction, CoreIntrinsicId, CoreMapTypeField, CorePrimitiveIntrinsic,
    CoreTupleTypeElem, CoreType,
};

use super::{
    callable_templates, collect_implicit_generic_params, contains_generic_parameter,
    contextual_literal_type, substitute, unify, CallableTemplates,
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
        CoreExpr::Map(fields) => fields
            .iter()
            .map(|field| {
                infer_type(&field.value, variables, templates, module).map(|value| {
                    CoreMapTypeField {
                        key: field.key.clone(),
                        operator: ":".to_string(),
                        value,
                    }
                })
            })
            .collect::<Option<Vec<_>>>()
            .map(CoreType::Map),
        CoreExpr::RecordConstruct { name, .. } => Some(CoreType::Named(name.clone())),
        CoreExpr::Intrinsic(call)
            if matches!(
                call.id,
                CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::ListIterator)
            ) =>
        {
            let receiver = infer_type(call.args.first()?, variables, templates, module)?;
            let element = list_element_type(&receiver)?.clone();
            Some(CoreType::Apply {
                constructor: "Iterator".to_string(),
                args: vec![element],
            })
        }
        CoreExpr::Intrinsic(call)
            if matches!(
                call.id,
                CoreIntrinsicId::Primitive(
                    CorePrimitiveIntrinsic::ListConcat
                        | CorePrimitiveIntrinsic::ListSubtract
                        | CorePrimitiveIntrinsic::ListPush
                        | CorePrimitiveIntrinsic::ListClear
                )
            ) =>
        {
            infer_type(call.args.first()?, variables, templates, module)
        }
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
            let mut matched_return = None;
            for template in candidates {
                let mut values = HashMap::new();
                let Ok(argument_types) =
                    infer_generic_argument_types(template, args, variables, templates, module)
                else {
                    continue;
                };
                if template
                    .params
                    .iter()
                    .zip(&argument_types)
                    .any(|(parameter, argument)| {
                        parameter.core_ty.as_ref().is_none_or(|expected| {
                            unify(expected, argument, &template.generic_params, &mut values)
                                .is_err()
                        })
                    })
                {
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

/// Resolves a checked bare function value from its contextual arrow arity.
pub(super) fn named_callable_type(
    argument: &CoreExpr,
    expected: &CoreType,
    templates: &CallableTemplates,
    module: &str,
) -> Option<CoreType> {
    let CoreExpr::Var(function) = argument else {
        return None;
    };
    let CoreType::Arrow { params, .. } = expected else {
        return None;
    };
    let candidates = callable_templates(templates, module, function, params.len())?;
    let mut signatures = candidates.iter().filter_map(|candidate| {
        let params = candidate
            .params
            .iter()
            .map(|parameter| parameter.core_ty.clone())
            .collect::<Option<Vec<_>>>()?;
        let return_type = candidate.core_return_type.clone()?;
        (!contains_generic_parameter(&return_type, &candidate.generic_params)
            && params
                .iter()
                .all(|ty| !contains_generic_parameter(ty, &candidate.generic_params)))
        .then(|| CoreType::Arrow {
            params,
            return_type: Box::new(return_type),
        })
    });
    let signature = signatures.next()?;
    signatures
        .all(|candidate| candidate == signature)
        .then_some(signature)
}

fn contextual_lambda_type(
    argument: &CoreExpr,
    expected: &CoreType,
    generic_params: &[String],
    variables: &HashMap<String, CoreType>,
    templates: &CallableTemplates,
    module: &str,
) -> Option<CoreType> {
    let CoreExpr::Lam { params, body } = argument else {
        return None;
    };
    let CoreType::Arrow {
        params: parameter_types,
        return_type,
    } = expected
    else {
        return None;
    };
    if params.len() != parameter_types.len()
        || parameter_types
            .iter()
            .any(|ty| contains_generic_parameter(ty, generic_params))
    {
        return None;
    }
    let mut locals = variables.clone();
    for (pattern, ty) in params.iter().zip(parameter_types) {
        super::pattern_types::bind_pattern_types(pattern, ty, &mut locals);
    }
    let result = contextual_literal_type(body, return_type)
        .or_else(|| infer_type(body, &locals, templates, module))
        .or_else(|| {
            (!contains_generic_parameter(return_type, generic_params))
                .then(|| return_type.as_ref().clone())
        })?;
    Some(CoreType::Arrow {
        params: parameter_types.clone(),
        return_type: Box::new(result),
    })
}

fn infer_generic_argument_type(
    argument: &CoreExpr,
    generic_params: &[String],
    variables: &HashMap<String, CoreType>,
    templates: &CallableTemplates,
    module: &str,
) -> Option<CoreType> {
    match argument {
        CoreExpr::Cast { expr, target_type }
            if contains_generic_parameter(target_type, generic_params) =>
        {
            infer_generic_argument_type(expr, generic_params, variables, templates, module)
        }
        CoreExpr::List(items) if !items.is_empty() => {
            let first = infer_generic_argument_type(
                &items[0],
                generic_params,
                variables,
                templates,
                module,
            )?;
            items[1..]
                .iter()
                .all(|item| {
                    infer_generic_argument_type(item, generic_params, variables, templates, module)
                        == Some(first.clone())
                })
                .then(|| CoreType::List(Box::new(first)))
        }
        CoreExpr::Tuple(items) => items
            .iter()
            .map(|item| {
                infer_generic_argument_type(item, generic_params, variables, templates, module)
                    .map(CoreTupleTypeElem::Type)
            })
            .collect::<Option<Vec<_>>>()
            .map(CoreType::Tuple),
        _ => infer_type(argument, variables, templates, module),
    }
}

pub(super) fn infer_generic_argument_types(
    template: &CoreFunction,
    arguments: &[CoreExpr],
    variables: &HashMap<String, CoreType>,
    templates: &CallableTemplates,
    module: &str,
) -> Result<Vec<CoreType>, String> {
    let mut substitution = HashMap::new();
    let mut concrete = vec![None; arguments.len()];

    // Function values carry more type information than literals and
    // aggregates. Apply their checked signatures first so generic inference
    // does not depend on source argument order or narrow a union to the one
    // constructor visible in a literal row.
    for (index, (parameter, argument)) in template.params.iter().zip(arguments).enumerate() {
        let expected = parameter.core_ty.as_ref().ok_or_else(|| {
            "error[native_ir.generic_signature]: generic parameter type is absent".to_string()
        })?;
        let Some(inferred) = named_callable_type(argument, expected, templates, module) else {
            continue;
        };
        unify(
            expected,
            &inferred,
            &template.generic_params,
            &mut substitution,
        )?;
        concrete[index] = Some(inferred);
    }
    for (index, (parameter, argument)) in template.params.iter().zip(arguments).enumerate() {
        let expected = parameter.core_ty.as_ref().ok_or_else(|| {
            "error[native_ir.generic_signature]: generic parameter type is absent".to_string()
        })?;
        let contextual_expected = substitute(expected, &template.generic_params, &substitution);
        let inferred = concrete[index]
            .clone()
            .or_else(|| {
                contextual_lambda_type(
                    argument,
                    &contextual_expected,
                    &template.generic_params,
                    variables,
                    templates,
                    module,
                )
            })
            .or_else(|| contextual_literal_type(argument, &contextual_expected))
            .or_else(|| {
                (needs_contextual_type(argument)
                    && !contains_generic_parameter(
                        &contextual_expected,
                        &template.generic_params,
                    ))
                .then_some(contextual_expected.clone())
            })
            .or_else(|| {
                infer_generic_argument_type(
                    argument,
                    &template.generic_params,
                    variables,
                    templates,
                    module,
                )
            })
            .or_else(|| {
                (!contains_generic_parameter(&contextual_expected, &template.generic_params))
                    .then_some(contextual_expected.clone())
            })
            .ok_or_else(|| {
                format!(
                    "error[native_ir.generic_argument]: cannot infer argument {} for `{}/{}` from `{}` against contextual type `{}`",
                    index + 1,
                    template.name,
                    template.arity,
                    argument.contract_text(),
                    contextual_expected.contract_text(),
                )
            })?;
        unify(
            expected,
            &inferred,
            &template.generic_params,
            &mut substitution,
        )
        .map_err(|error| {
            format!(
                "{error}; while specializing argument {} of `{}/{}` from `{}`",
                index + 1,
                template.name,
                template.arity,
                argument.contract_text()
            )
        })?;
        concrete[index] = Some(inferred);
    }
    concrete
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| "error[native_ir.generic_argument]: incomplete inference".to_string())
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

fn list_element_type(ty: &CoreType) -> Option<&CoreType> {
    match ty {
        CoreType::List(element) => Some(element),
        CoreType::Apply { constructor, args }
            if constructor.rsplit('.').next() == Some("List") && args.len() == 1 =>
        {
            Some(&args[0])
        }
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

pub(super) fn contains_implicit_generic_type(ty: &CoreType) -> bool {
    let mut names = HashSet::new();
    collect_implicit_generic_params(ty, &mut names);
    !names.is_empty()
}
