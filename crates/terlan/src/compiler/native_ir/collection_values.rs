//! Type-directed lowering of concrete persistent List and Map values.

use std::collections::HashMap;
use std::sync::Arc;

use crate::runtime::native_image::managed::{
    encode_aggregate_layout, encode_list_from_elements_operation, encode_list_prepend_operation,
    encode_map_from_entries_operation, ManagedAggregateDescriptor, SemanticTypeId,
};
use crate::terlan_typeck::{CoreExpr, CoreTupleTypeElem, CoreType};

use super::{
    infer_native_type_with_constructors, lower_expr_with_constructors, native_type,
    NativeConstructorLayouts, NativeExpr, NativeType,
};

/// Lowers one collection-valued native function body from its checked result type.
#[allow(clippy::too_many_arguments)]
pub(super) fn lower_boundary_collection_value(
    body: &CoreExpr,
    expected: Option<&CoreType>,
    params: &HashMap<String, usize>,
    param_types: &HashMap<String, NativeType>,
    functions: &HashMap<(String, usize), usize>,
    function_types: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
) -> Result<Option<NativeExpr>, String> {
    let Some(expected) = expected else {
        return Ok(None);
    };
    if let (CoreExpr::Tuple(items), CoreType::Tuple(elements)) = (body, expected) {
        if items.len() != elements.len() {
            return Err("error[native_ir.tuple_value]: tuple value arity mismatch".to_string());
        }
        let element_types = elements
            .iter()
            .map(|element| match element {
                CoreTupleTypeElem::Type(ty) | CoreTupleTypeElem::Field { ty, .. } => ty,
            })
            .collect::<Vec<_>>();
        let fields = element_types
            .iter()
            .map(|ty| {
                native_type(Some(ty), &ty.contract_text())
                    .ok_or_else(|| {
                        format!(
                            "error[native_ir.tuple_value_type]: unsupported tuple field `{}`",
                            ty.contract_text()
                        )
                    })
                    .and_then(super::constructors::managed_field_type)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let descriptor = Arc::new(
            ManagedAggregateDescriptor::tuple(&expected.contract_text(), fields)
                .map_err(|error| format!("error[native_ir.tuple_value_layout]: {error}"))?,
        );
        let encoded_layout = Arc::from(
            encode_aggregate_layout(&descriptor)
                .map_err(|error| format!("error[native_ir.tuple_value_abi]: {error}"))?,
        );
        let fields = items
            .iter()
            .zip(element_types)
            .map(|(item, ty)| {
                lower_typed_value(
                    item,
                    ty,
                    params,
                    param_types,
                    functions,
                    function_types,
                    constructors,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(Some(NativeExpr::Construct {
            descriptor,
            encoded_layout,
            fields,
        }));
    }
    match (body, collection_shape(expected)) {
        (CoreExpr::List(items), Some(CollectionShape::List(element))) => {
            let semantic = semantic(expected)?;
            let args = lower_typed_values(
                items,
                element,
                params,
                param_types,
                functions,
                function_types,
                constructors,
            )?;
            Ok(Some(NativeExpr::ManagedOperation {
                encoded: Arc::from(encode_list_from_elements_operation(semantic)),
                args,
            }))
        }
        (CoreExpr::ListCons { head, tail }, Some(CollectionShape::List(element))) => {
            let semantic = semantic(expected)?;
            let head = lower_typed_value(
                head,
                element,
                params,
                param_types,
                functions,
                function_types,
                constructors,
            )?;
            let tail = lower_boundary_collection_value(
                tail,
                Some(expected),
                params,
                param_types,
                functions,
                function_types,
                constructors,
            )?
            .unwrap_or(lower_expr_with_constructors(
                tail,
                params,
                param_types,
                functions,
                function_types,
                constructors,
            )?);
            Ok(Some(NativeExpr::ManagedOperation {
                encoded: Arc::from(encode_list_prepend_operation(semantic)),
                args: vec![head, tail],
            }))
        }
        (CoreExpr::Map(fields), Some(CollectionShape::Map(key, value))) => {
            let semantic = semantic(expected)?;
            let mut args = Vec::with_capacity(fields.len().saturating_mul(2));
            for field in fields {
                let key_expr = map_key_expr(&field.key, key)?;
                args.push(lower_typed_value(
                    &key_expr,
                    key,
                    params,
                    param_types,
                    functions,
                    function_types,
                    constructors,
                )?);
                args.push(lower_typed_value(
                    &field.value,
                    value,
                    params,
                    param_types,
                    functions,
                    function_types,
                    constructors,
                )?);
            }
            Ok(Some(NativeExpr::ManagedOperation {
                encoded: Arc::from(encode_map_from_entries_operation(semantic)),
                args,
            }))
        }
        _ => Ok(None),
    }
}

enum CollectionShape<'a> {
    List(&'a CoreType),
    Map(&'a CoreType, &'a CoreType),
}

fn collection_shape(ty: &CoreType) -> Option<CollectionShape<'_>> {
    match ty {
        CoreType::List(element) => Some(CollectionShape::List(element)),
        CoreType::Apply { constructor, args }
            if constructor.rsplit('.').next() == Some("List") && args.len() == 1 =>
        {
            Some(CollectionShape::List(&args[0]))
        }
        CoreType::Apply { constructor, args }
            if constructor.rsplit('.').next() == Some("Map") && args.len() == 2 =>
        {
            Some(CollectionShape::Map(&args[0], &args[1]))
        }
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn lower_typed_values(
    values: &[CoreExpr],
    expected: &CoreType,
    params: &HashMap<String, usize>,
    param_types: &HashMap<String, NativeType>,
    functions: &HashMap<(String, usize), usize>,
    function_types: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
) -> Result<Vec<NativeExpr>, String> {
    values
        .iter()
        .map(|value| {
            lower_typed_value(
                value,
                expected,
                params,
                param_types,
                functions,
                function_types,
                constructors,
            )
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn lower_typed_value(
    value: &CoreExpr,
    expected: &CoreType,
    params: &HashMap<String, usize>,
    param_types: &HashMap<String, NativeType>,
    functions: &HashMap<(String, usize), usize>,
    function_types: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
) -> Result<NativeExpr, String> {
    let expected_native =
        native_type(Some(expected), &expected.contract_text()).ok_or_else(|| {
            format!(
                "error[native_ir.collection_type]: `{}` is not a native collection field",
                expected.contract_text()
            )
        })?;
    let actual =
        infer_native_type_with_constructors(value, param_types, function_types, constructors)
            .ok_or_else(|| {
                "error[native_ir.collection_value]: cannot infer collection value".to_string()
            })?;
    if actual != expected_native {
        return Err(
            "error[native_ir.collection_value]: collection value type mismatch".to_string(),
        );
    }
    lower_expr_with_constructors(
        value,
        params,
        param_types,
        functions,
        function_types,
        constructors,
    )
}

fn semantic(ty: &CoreType) -> Result<SemanticTypeId, String> {
    SemanticTypeId::from_canonical(&ty.contract_text())
        .map_err(|error| format!("error[native_ir.collection_type]: {error}"))
}

fn map_key_expr(key: &str, ty: &CoreType) -> Result<CoreExpr, String> {
    match ty {
        CoreType::String => serde_json::to_string(key)
            .map(CoreExpr::Binary)
            .map_err(|error| format!("error[native_ir.map_key]: {error}")),
        CoreType::Atom | CoreType::AtomLiteral(_) => Ok(CoreExpr::Atom(key.to_string())),
        CoreType::Int => key
            .parse::<i64>()
            .map(CoreExpr::Int)
            .map_err(|_| format!("error[native_ir.map_key]: `{key}` is not an Int key")),
        _ => Err(format!(
            "error[native_ir.map_key]: `{}` has no native literal-key semantics",
            ty.contract_text()
        )),
    }
}
