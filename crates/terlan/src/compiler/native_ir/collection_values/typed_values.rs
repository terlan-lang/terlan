//! Checked aggregate values and contextual collection elements.

use super::*;

/// Attempts type-directed lowering for one concrete aggregate or scalar value.
///
/// A checked expression can have a collection-shaped result without itself
/// being a literal collection value (for example, a `case` resumed after an
/// asynchronous capability call). Callers that can lower general structured
/// expressions need to distinguish that case from an invalid concrete value.
pub(in crate::compiler::native_ir) fn try_lower_typed_value(
    value: &CoreExpr,
    expected: &CoreType,
    params: &HashMap<String, usize>,
    param_types: &HashMap<String, NativeType>,
    functions: &HashMap<(String, usize), usize>,
    function_types: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
) -> Result<Option<NativeExpr>, String> {
    if let Some(boxed) = super::super::effect_values::boxed_field(value, expected) {
        return lower_expr_with_constructors(
            &boxed,
            params,
            param_types,
            functions,
            function_types,
            constructors,
        )
        .map(Some);
    }
    if matches!(
        value,
        CoreExpr::Case { .. } | CoreExpr::If { .. } | CoreExpr::Try { .. }
    ) {
        return Ok(None);
    }
    if let Some(expr) = super::super::expression::collection_cast_source(
        value,
        expected,
        param_types,
        function_types,
        constructors,
    ) {
        return try_lower_typed_value(
            expr,
            expected,
            params,
            param_types,
            functions,
            function_types,
            constructors,
        );
    }
    if let (CoreExpr::Binary(value), CoreType::Binary) = (value, expected) {
        let value = super::super::expression::core_string_runtime_value(value)?;
        let encoded = encode_binary_literal(value.as_bytes())
            .map_err(|error| format!("error[native_ir.binary_literal]: {error}"))?;
        return Ok(Some(NativeExpr::ManagedLiteral {
            encoded: encoded.into(),
        }));
    }
    if let CoreExpr::Let { bindings, body } = value {
        let retained = super::super::escape::retained_managed_bindings(bindings, body);
        let mut locals = params.clone();
        let mut local_types = param_types.clone();
        let mut next_local = locals
            .values()
            .copied()
            .max()
            .map_or(0, |index| index.saturating_add(1));
        let mut lowered = Vec::with_capacity(bindings.len());
        for (binding, retained) in bindings.iter().zip(retained) {
            if !retained {
                continue;
            }
            if is_general_control_value(&binding.value) {
                return Ok(None);
            }
            let CorePattern::Var(name) = &binding.pattern else {
                return Err(
                    "error[native_ir.typed_let_pattern]: typed aggregate let requires variable bindings"
                        .to_string(),
                );
            };
            let (binding_type, binding_value) = if let CoreExpr::Cast { target_type, .. } =
                &binding.value
            {
                let binding_type = native_type(Some(target_type), &target_type.contract_text(), constructors)
                    .ok_or_else(|| {
                        format!(
                            "error[native_ir.typed_let_type]: cast prefix `{name}` has unsupported type `{}`",
                            target_type.contract_text()
                        )
                    })?;
                let binding_value = lower_typed_value(
                    &binding.value,
                    target_type,
                    &locals,
                    &local_types,
                    functions,
                    function_types,
                    constructors,
                )?;
                (binding_type, binding_value)
            } else {
                let binding_type = infer_native_type_with_constructors(
                    &binding.value,
                    &local_types,
                    function_types,
                    constructors,
                )
                .ok_or_else(|| {
                    format!(
                        "error[native_ir.typed_let_type]: cannot infer aggregate prefix `{name}`"
                    )
                })?;
                let binding_value = lower_expr_with_constructors(
                    &binding.value,
                    &locals,
                    &local_types,
                    functions,
                    function_types,
                    constructors,
                )?;
                (binding_type, binding_value)
            };
            lowered.push(binding_value);
            locals.insert(name.clone(), next_local);
            local_types.insert(name.clone(), binding_type);
            next_local = next_local.saturating_add(1);
        }
        let Some(body) = try_lower_typed_value(
            body,
            expected,
            &locals,
            &local_types,
            functions,
            function_types,
            constructors,
        )?
        else {
            return Ok(None);
        };
        return Ok(Some(if lowered.is_empty() {
            body
        } else {
            NativeExpr::Let {
                bindings: lowered,
                body: Box::new(body),
            }
        }));
    }
    let none_constructor =
        is_none_option_value(value, expected).then(|| CoreExpr::ConstructorCall {
            type_args: Vec::new(),
            constructor: "None".to_string(),
            constructor_identity: Some("std.core.Option.None".to_string()),
            args: Vec::new(),
        });
    let tagged_constructor = structural_tagged_tuple_constructor(value, expected);
    let structural_value = none_constructor
        .as_ref()
        .or(tagged_constructor.as_ref())
        .unwrap_or(value);
    if let Some(value) = super::super::constructors::lower_structural_constructor_call(
        structural_value,
        expected,
        |field, field_type| {
            let ty = native_type(Some(field_type), &field_type.contract_text(), constructors)
                .ok_or_else(|| {
                    format!(
                        "error[native_ir.collection_constructor_type]: `{}` is not a native field",
                        field_type.contract_text()
                    )
                })?;
            let lowered = if is_general_control_value(field) {
                let actual = infer_native_type_with_constructors(
                    field,
                    param_types,
                    function_types,
                    constructors,
                )
                .ok_or_else(|| {
                    format!(
                        "error[native_ir.collection_control_type]: cannot infer `{}` field value",
                        field_type.contract_text()
                    )
                })?;
                if actual != ty {
                    return Err(format!(
                        "error[native_ir.collection_control_type]: expected {ty:?}, found {actual:?}"
                    ));
                }
                lower_expr_with_constructors(
                    field,
                    params,
                    param_types,
                    functions,
                    function_types,
                    constructors,
                )?
            } else {
                lower_typed_value(
                    field,
                    field_type,
                    params,
                    param_types,
                    functions,
                    function_types,
                    constructors,
                )?
            };
            Ok((lowered, ty))
        },
    )? {
        return Ok(Some(value));
    }
    if let Some(value) = lower_boundary_collection_value(
        value,
        Some(expected),
        params,
        param_types,
        functions,
        function_types,
        constructors,
    )? {
        return Ok(Some(value));
    }
    let expected_native = native_type(Some(expected), &expected.contract_text(), constructors)
        .ok_or_else(|| {
            format!(
                "error[native_ir.collection_type]: `{}` is not a native collection field",
                expected.contract_text()
            )
        })?;
    let Some(actual) =
        infer_native_type_with_constructors(value, param_types, function_types, constructors)
    else {
        return Ok(None);
    };
    if actual != expected_native
        && !transparent_union_accepts_native(expected, actual, constructors)
    {
        return Err(format!(
            "error[native_ir.collection_value]: collection value type mismatch: expected {} as {expected_native:?}, found {actual:?} for {value:?}",
            expected.contract_text()
        ));
    }
    lower_expr_with_constructors(
        value,
        params,
        param_types,
        functions,
        function_types,
        constructors,
    )
    .map(Some)
}

/// Reports whether one concrete native value is a declared transparent-union
/// variant and therefore requires no representation-changing cast.
pub(super) fn transparent_union_accepts_native(
    expected: &CoreType,
    actual: NativeType,
    constructors: &NativeConstructorLayouts,
) -> bool {
    let CoreType::Union(variants) = expected else {
        return false;
    };
    variants.iter().any(|variant| {
        native_type(Some(variant), &variant.contract_text(), constructors)
            .is_some_and(|native| native == actual)
    })
}

/// Reports whether a value requires the general control-flow lowerer.
pub(super) fn is_general_control_value(value: &CoreExpr) -> bool {
    match value {
        CoreExpr::Case { .. } | CoreExpr::If { .. } | CoreExpr::Try { .. } => true,
        CoreExpr::Cast { expr, .. } => is_general_control_value(expr),
        _ => false,
    }
}

/// Restores a structural Option/Result constructor after transparent alias expansion.
pub(super) fn structural_tagged_tuple_constructor(
    value: &CoreExpr,
    expected: &CoreType,
) -> Option<CoreExpr> {
    let CoreType::Apply { constructor, .. } = expected else {
        return None;
    };
    if !matches!(constructor.rsplit('.').next(), Some("Option" | "Result")) {
        return None;
    }
    let CoreExpr::Tuple(items) = value else {
        return None;
    };
    let tag = checked_atom_literal(items.first()?)?;
    Some(CoreExpr::ConstructorCall {
        type_args: Vec::new(),
        constructor: tagged_variant_name(tag)?,
        constructor_identity: None,
        args: items.iter().skip(1).cloned().collect(),
    })
}
