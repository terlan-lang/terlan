use super::*;

pub(super) fn specialize_expected_collection_new(
    expr: &mut CoreExpr,
    expected: &CoreType,
    functions: &FunctionTypes,
    module: &str,
) {
    let expected = nominal_type(functions, module, expected).unwrap_or(expected);
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
        CoreExpr::Call { function, args } => {
            if let Some(signature) = function_signature(functions, module, function, args.len()) {
                contextualize_call_arguments(args, signature, expected, functions, module);
            }
        }
        CoreExpr::RemoteCall {
            module: owner,
            function,
            args,
        } => {
            if let Some(signature) = function_signature(functions, owner, function, args.len()) {
                contextualize_call_arguments(args, signature, expected, functions, module);
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
                        expected_field,
                        functions,
                        module,
                    );
                    annotate_expected_structural_constructors(&mut field.value, expected_field);
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
            ) {
                *target_type = expected.clone();
            }
            specialize_expected_collection_new(expr, expected, functions, module)
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

fn contextualize_call_arguments(
    args: &mut [CoreExpr],
    signature: &FunctionSignature,
    expected: &CoreType,
    functions: &FunctionTypes,
    module: &str,
) {
    let mut values = HashMap::new();
    if !match_context_type(&signature.result, expected, &mut values) {
        return;
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
    values: &mut HashMap<String, CoreType>,
) -> bool {
    if let CoreType::Named(name) = template {
        if name.len() == 1 && name.as_bytes()[0].is_ascii_uppercase() {
            return values.get(name).is_none_or(|prior| prior == concrete) && {
                values.insert(name.clone(), concrete.clone());
                true
            };
        }
    }
    match (template, concrete) {
        (CoreType::List(left), CoreType::List(right)) => match_context_type(left, right, values),
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
                .all(|(left, right)| match_context_type(left, right, values))
        }
        (CoreType::Tuple(left), CoreType::Tuple(right)) if left.len() == right.len() => {
            left.iter().zip(right).all(|(left, right)| {
                match_context_type(tuple_element_type(left), tuple_element_type(right), values)
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
    functions: &FunctionTypes,
    module: &str,
) {
    for index in 0..bindings.len() {
        let CorePattern::Var(name) = &bindings[index].pattern else {
            continue;
        };
        let name = name.clone();
        let inferred = bindings[index + 1..]
            .iter()
            .find_map(|binding| {
                expected_call_argument_type(&name, &binding.value, functions, module)
                    .or_else(|| infer_map_put(&name, &binding.value))
                    .or_else(|| infer_set_add(&name, &binding.value))
            })
            .or_else(|| expected_call_argument_type(&name, body, functions, module))
            .or_else(|| infer_map_put(&name, body))
            .or_else(|| infer_set_add(&name, body));
        let Some(inferred) = inferred else {
            continue;
        };
        specialize_expected_collection_new(
            &mut bindings[index].value,
            &inferred,
            functions,
            module,
        );
        annotate_expected_structural_constructors(&mut bindings[index].value, &inferred);
    }
}

fn expected_call_argument_type(
    name: &str,
    expr: &CoreExpr,
    functions: &FunctionTypes,
    module: &str,
) -> Option<CoreType> {
    let (signature, args) = match expr {
        CoreExpr::Call { function, args } => (
            function_signature(functions, module, function, args.len()),
            args,
        ),
        CoreExpr::RemoteCall {
            module: owner,
            function,
            args,
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
            matches!(argument, CoreExpr::Var(argument) if argument == name)
                .then(|| expected.clone())
        })
}
