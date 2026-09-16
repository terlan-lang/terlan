use super::*;

#[path = "empty_mutation.rs"]
mod empty_mutation;

/// Pushes a retained aggregate witness into its fields before projections can
/// remove the aggregate. Empty payloads must keep their checked element types.
pub(super) fn specialize_cast_contents(
    expr: &mut Box<CoreExpr>,
    target_type: &CoreType,
    variables: &HashMap<String, CoreType>,
    functions: &FunctionTypes,
    module: &str,
) {
    specialize_expected_collection_new(expr, target_type, functions, module);
    while matches!(expr.as_ref(), CoreExpr::Cast { target_type: inner, .. } if inner == target_type)
    {
        let CoreExpr::Cast { expr: inner, .. } =
            std::mem::replace(expr.as_mut(), CoreExpr::Atom("Unit".to_string()))
        else {
            unreachable!("matched cast")
        };
        *expr = inner;
    }
    if let CoreExpr::List(items) = expr.as_mut() {
        // The enclosing annotation already owns this list's schema.
        specialize_elements(items, variables, functions, module);
    } else if let CoreExpr::RecordConstruct { fields, .. } = expr.as_mut() {
        for field in fields {
            specialize_expr(&mut field.value, variables, functions, module);
        }
    } else {
        specialize_expr(expr, variables, functions, module);
    }
}

/// Only resolved parameter types may replace an argument's checked witness.
/// Generic parameters are contextualized by the monomorphizer after unification;
/// copying their declaration here would erase explicit types such as List[Binary].
pub(super) fn specialize_parameter_arguments(
    args: &mut [CoreExpr],
    signature: &FunctionSignature,
    functions: &FunctionTypes,
    module: &str,
) {
    for (argument, expected) in args.iter_mut().zip(&signature.params) {
        if super::super::generic_specialization::contains_generic_parameter(
            expected,
            &signature.generic_params,
        ) {
            continue;
        }
        specialize_expected_collection_new(argument, expected, functions, module);
        annotate_expected_structural_constructors(argument, expected);
    }
}

pub(super) fn specialize_expected_collection_new(
    expr: &mut CoreExpr,
    expected: &CoreType,
    functions: &FunctionTypes,
    module: &str,
) {
    let resolved = nominal_type(functions, module, expected);
    let expected = resolved.as_deref().unwrap_or(expected);
    match expr {
        CoreExpr::Binary(_) if matches!(expected, CoreType::Binary | CoreType::String) => {
            let literal = std::mem::replace(expr, CoreExpr::Binary("\"\"".to_string()));
            *expr = CoreExpr::Cast {
                expr: Box::new(literal),
                target_type: expected.clone(),
            };
        }
        CoreExpr::List(items) if list_element(expected).is_some() => {
            let element = list_element(expected).expect("guard requires list element");
            for item in items.iter_mut() {
                specialize_expected_collection_new(item, element, functions, module);
                annotate_expected_structural_constructors(item, element);
            }
            let list = std::mem::replace(expr, CoreExpr::List(Vec::new()));
            *expr = CoreExpr::Cast {
                expr: Box::new(list),
                target_type: expected.clone(),
            };
        }
        CoreExpr::ListCons { head, tail } if list_element(expected).is_some() => {
            let element = list_element(expected).expect("guard requires list element");
            specialize_expected_collection_new(head, element, functions, module);
            annotate_expected_structural_constructors(head, element);
            specialize_expected_collection_new(tail, expected, functions, module);
        }
        CoreExpr::ConstructorCall {
            type_args: _,
            constructor,
            constructor_identity,
            args,
        } if is_std_list_constructor(constructor, constructor_identity.as_deref())
            && list_element(expected).is_some() =>
        {
            let element = list_element(expected).expect("guard requires list element");
            for item in args {
                specialize_expected_collection_new(item, element, functions, module);
                annotate_expected_structural_constructors(item, element);
            }
        }
        CoreExpr::ConstructorCall {
            type_args: _,
            constructor,
            constructor_identity,
            args,
        } if matches!(expected, CoreType::Struct { name, fields }
            if fields.len() == args.len()
                && constructor_identity.as_deref().map_or_else(
                    || constructor == name || format!("{module}.{constructor}") == *name,
                    |identity| identity == name || format!("{module}.{identity}") == *name)) =>
        {
            if let CoreType::Struct { fields, .. } = expected {
                // Specialize before call composition lifts earlier arguments
                // into locals across a later suspending constructor argument.
                for (argument, field) in args.iter_mut().zip(fields) {
                    specialize_expected_collection_new(argument, &field.ty, functions, module);
                    annotate_expected_structural_constructors(argument, &field.ty);
                }
            }
        }
        CoreExpr::Tuple(items) => {
            let element_types = contextual_tuple_elements(items, expected);
            if let Some(element_types) = element_types {
                for (item, element) in items.iter_mut().zip(element_types) {
                    specialize_expected_collection_new(item, element, functions, module);
                    annotate_expected_structural_constructors(item, element);
                }
                let tuple = std::mem::replace(expr, CoreExpr::Tuple(Vec::new()));
                *expr = CoreExpr::Cast {
                    expr: Box::new(tuple),
                    target_type: expected.clone(),
                };
            }
        }
        CoreExpr::Call {
            function,
            args,
            type_args,
        } => {
            if let Some(signature) = function_signature(functions, module, function, args.len()) {
                contextualize_call_arguments(
                    args, type_args, signature, expected, functions, module,
                );
            }
        }
        CoreExpr::RemoteCall {
            module: owner,
            function,
            args,
            type_args,
        } => {
            if let Some(signature) = function_signature(functions, owner, function, args.len()) {
                contextualize_call_arguments(
                    args, type_args, signature, expected, functions, module,
                );
            }
        }
        CoreExpr::Intrinsic(call) => match call.id {
            CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::MapNew)
                if map_elements(expected).is_some() =>
            {
                call.return_type = expected.clone();
            }
            CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::ListNew)
                if list_element(expected).is_some() =>
            {
                call.return_type = expected.clone();
            }
            CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::SetNew)
                if set_element(expected).is_some() =>
            {
                call.return_type = expected.clone();
            }
            _ => {}
        },
        CoreExpr::RecordConstruct { fields, .. } | CoreExpr::TemplateInstantiate { fields, .. } => {
            for field in fields {
                if let Some(expected_field) =
                    named_field_type_with_nominals(expected, &field.key, functions, module)
                {
                    specialize_expected_collection_new(
                        &mut field.value,
                        &expected_field,
                        functions,
                        module,
                    );
                    annotate_expected_structural_constructors(&mut field.value, &expected_field);
                }
            }
        }
        CoreExpr::Cast { expr, target_type } => {
            if matches!(
                expected,
                CoreType::List(_)
                    | CoreType::Apply { .. }
                    | CoreType::Tuple(_)
                    | CoreType::Union(_)
            ) || (matches!(expected, CoreType::String | CoreType::Binary)
                && contextual_text_literal(expr))
            {
                *target_type = expected.clone();
            }
            specialize_expected_collection_new(expr, expected, functions, module);
            // This pass runs before and after instantiation. Reapplying the
            // same checked context must not grow a stack of identical casts.
            while matches!(expr.as_ref(), CoreExpr::Cast { target_type: inner, .. } if inner == target_type)
            {
                let CoreExpr::Cast { expr: inner, .. } =
                    std::mem::replace(expr.as_mut(), CoreExpr::Atom("Unit".to_string()))
                else {
                    unreachable!("matched cast")
                };
                *expr = inner;
            }
        }
        CoreExpr::Let { body, .. } => {
            specialize_expected_collection_new(body, expected, functions, module)
        }
        CoreExpr::If { clauses } => {
            for clause in clauses {
                specialize_expected_collection_new(&mut clause.body, expected, functions, module);
            }
        }
        CoreExpr::Case { clauses, .. } => {
            for clause in clauses {
                specialize_expected_collection_new(&mut clause.body, expected, functions, module);
            }
        }
        _ => {}
    }
}

/// Text literals can be materialized directly in their checked String/Binary
/// context. Existing values must retain their representation instead.
fn contextual_text_literal(expr: &CoreExpr) -> bool {
    match expr {
        CoreExpr::Binary(_) => true,
        CoreExpr::Cast {
            expr,
            target_type: CoreType::String | CoreType::Binary,
        } => contextual_text_literal(expr),
        _ => false,
    }
}

#[cfg(test)]
#[path = "expected_new_test.rs"]
mod tests;

fn contextualize_call_arguments(
    args: &mut [CoreExpr],
    type_args: &mut Vec<CoreType>,
    signature: &FunctionSignature,
    expected: &CoreType,
    functions: &FunctionTypes,
    module: &str,
) {
    let mut values = HashMap::new();
    if !match_context_type(
        &signature.result,
        expected,
        &signature.generic_params,
        &mut values,
    ) {
        return;
    }
    // Return-only parameters have no runtime argument witness. Retain their
    // checked context in declaration order, including symbolic contexts that
    // are made concrete when an enclosing generic function is instantiated.
    let return_only = signature.generic_params.iter().any(|parameter| {
        !signature.params.iter().any(|ty| {
            super::super::generic_specialization::contains_generic_parameter(
                ty,
                std::slice::from_ref(parameter),
            )
        })
    });
    // Argument-inferable parameters must be solved from their argument values,
    // not from still-symbolic parameter names in an enclosing callee signature.
    if type_args.is_empty() && return_only {
        if let Some(inferred) = signature
            .generic_params
            .iter()
            .map(|name| values.get(name).cloned())
            .collect::<Option<Vec<_>>>()
        {
            *type_args = inferred;
        }
    }
    for (argument, parameter) in args.iter_mut().zip(&signature.params) {
        let parameter = substitute_context_type(parameter, &values);
        specialize_expected_collection_new(argument, &parameter, functions, module);
        annotate_expected_structural_constructors(argument, &parameter);
    }
}

fn contextual_tuple_elements<'a>(
    items: &[CoreExpr],
    expected: &'a CoreType,
) -> Option<Vec<&'a CoreType>> {
    let elements = match expected {
        CoreType::Tuple(elements) if elements.len() == items.len() => elements,
        CoreType::Union(variants) => {
            let CoreExpr::Atom(tag) = items.first()? else {
                return None;
            };
            variants.iter().find_map(|variant| {
                let CoreType::Tuple(elements) = variant else {
                    return None;
                };
                matches!(
                    elements.first().map(tuple_element_type),
                    Some(CoreType::AtomLiteral(candidate)) if candidate == tag
                )
                .then_some(elements)
            })?
        }
        _ => return None,
    };
    Some(elements.iter().map(tuple_element_type).collect())
}

fn tuple_element_type(element: &crate::terlan_typeck::CoreTupleTypeElem) -> &CoreType {
    match element {
        crate::terlan_typeck::CoreTupleTypeElem::Type(ty)
        | crate::terlan_typeck::CoreTupleTypeElem::Field { ty, .. } => ty,
    }
}

fn match_context_type(
    template: &CoreType,
    concrete: &CoreType,
    generic_params: &[String],
    values: &mut HashMap<String, CoreType>,
) -> bool {
    if let CoreType::Named(name) = template {
        if generic_params.contains(name) {
            return values.get(name).is_none_or(|prior| prior == concrete) && {
                values.insert(name.clone(), concrete.clone());
                true
            };
        }
    }
    match (template, concrete) {
        (CoreType::List(left), CoreType::List(right)) => {
            match_context_type(left, right, generic_params, values)
        }
        (CoreType::Union(left), CoreType::Union(right)) if left.len() == right.len() => left
            .iter()
            .zip(right)
            .all(|(left, right)| match_context_type(left, right, generic_params, values)),
        (
            CoreType::Apply {
                constructor: left,
                args: left_args,
            },
            CoreType::Apply {
                constructor: right,
                args: right_args,
            },
        ) if left.rsplit('.').next() == right.rsplit('.').next()
            && left_args.len() == right_args.len() =>
        {
            left_args
                .iter()
                .zip(right_args)
                .all(|(left, right)| match_context_type(left, right, generic_params, values))
        }
        (CoreType::Tuple(left), CoreType::Tuple(right)) if left.len() == right.len() => {
            left.iter().zip(right).all(|(left, right)| {
                match_context_type(
                    tuple_element_type(left),
                    tuple_element_type(right),
                    generic_params,
                    values,
                )
            })
        }
        _ => template == concrete,
    }
}

fn substitute_context_type(ty: &CoreType, values: &HashMap<String, CoreType>) -> CoreType {
    match ty {
        CoreType::Named(name) => values.get(name).cloned().unwrap_or_else(|| ty.clone()),
        CoreType::List(element) => {
            CoreType::List(Box::new(substitute_context_type(element, values)))
        }
        CoreType::Union(variants) => CoreType::Union(
            variants
                .iter()
                .map(|ty| substitute_context_type(ty, values))
                .collect(),
        ),
        CoreType::Apply { constructor, args } => CoreType::Apply {
            constructor: constructor.clone(),
            args: args
                .iter()
                .map(|arg| substitute_context_type(arg, values))
                .collect(),
        },
        CoreType::Tuple(elements) => CoreType::Tuple(
            elements
                .iter()
                .map(|element| match element {
                    crate::terlan_typeck::CoreTupleTypeElem::Type(ty) => {
                        crate::terlan_typeck::CoreTupleTypeElem::Type(substitute_context_type(
                            ty, values,
                        ))
                    }
                    crate::terlan_typeck::CoreTupleTypeElem::Field { name, ty } => {
                        crate::terlan_typeck::CoreTupleTypeElem::Field {
                            name: name.clone(),
                            ty: substitute_context_type(ty, values),
                        }
                    }
                })
                .collect(),
        ),
        _ => ty.clone(),
    }
}

pub(super) fn specialize_collection_new_bindings(
    bindings: &mut [CoreLetBinding],
    body: &CoreExpr,
    variables: &HashMap<String, CoreType>,
    functions: &FunctionTypes,
    module: &str,
) {
    let mut variables = variables.clone();
    for index in 0..bindings.len() {
        let inferred = empty_mutation::infer_binding_use(
            &bindings[index],
            &bindings[index + 1..],
            body,
            &variables,
            functions,
            module,
        );
        let binding = &mut bindings[index];
        if let Some(inferred) = inferred {
            specialize_expected_collection_new(&mut binding.value, &inferred, functions, module);
            annotate_expected_structural_constructors(&mut binding.value, &inferred);
        }
        let ty = specialize_expr(&mut binding.value.clone(), &variables, functions, module);
        for name in
            super::super::expression::free_variable_analysis::pattern_bound_names(&binding.pattern)
        {
            variables.remove(&name);
        }
        if let Some(ty) = ty {
            bind_pattern(&binding.pattern, &ty, &mut variables);
        }
    }
}

fn expected_call_argument_type(
    name: &str,
    expr: &CoreExpr,
    functions: &FunctionTypes,
    module: &str,
) -> Option<CoreType> {
    let (signature, args) = match expr {
        CoreExpr::Call { function, args, .. } => (
            function_signature(functions, module, function, args.len()),
            args,
        ),
        CoreExpr::RemoteCall {
            module: owner,
            function,
            args,
            ..
        } => (
            functions.get(&(owner.clone(), function.clone(), args.len())),
            args,
        ),
        CoreExpr::Cast { expr, .. } => {
            return expected_call_argument_type(name, expr, functions, module);
        }
        _ => return None,
    };
    let signature = signature?;
    args.iter()
        .zip(&signature.params)
        .find_map(|(argument, expected)| {
            (matches!(argument, CoreExpr::Var(argument) if argument == name)
                && !super::super::generic_specialization::contains_generic_parameter(
                    expected,
                    &signature.generic_params,
                ))
            .then(|| expected.clone())
        })
}
