use super::*;

pub(super) fn specialize_expected_collection_new(
    expr: &mut CoreExpr,
    expected: &CoreType,
    functions: &FunctionTypes,
    module: &str,
) {
    let expected = nominal_type(functions, module, expected).unwrap_or(expected);
    match expr {
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
        CoreExpr::Cast { expr, .. } => {
            specialize_expected_collection_new(expr, expected, functions, module)
        }
        _ => {}
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
