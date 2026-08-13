//! Direct-AOT lowering for the typed persistent set lookup surface.

use std::collections::HashMap;

use crate::runtime::native_image::managed::{
    encode_set_add_operation, encode_set_clear_operation, encode_set_contains_operation,
    encode_set_empty_operation, encode_set_from_list_operation, encode_set_is_empty_operation,
    encode_set_iterator_operation, encode_set_length_operation, encode_set_remove_operation,
    SemanticTypeId,
};
use crate::terlan_typeck::{CoreIntrinsicCall, CoreIntrinsicId, CorePrimitiveIntrinsic, CoreType};

use super::{
    infer_native_type_with_constructors, lower_expr_with_constructors, native_type,
    NativeConstructorLayouts, NativeExpr, NativeType,
};

pub(super) fn infer_set_intrinsic_type(call: &CoreIntrinsicCall) -> Option<NativeType> {
    match call.id {
        CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::SetContains | CorePrimitiveIntrinsic::SetIsEmpty,
        ) => Some(NativeType::Bool),
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::SetSize) => Some(NativeType::Int),
        CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::SetNew
            | CorePrimitiveIntrinsic::SetFromList
            | CorePrimitiveIntrinsic::SetIterator
            | CorePrimitiveIntrinsic::SetAdd
            | CorePrimitiveIntrinsic::SetRemove
            | CorePrimitiveIntrinsic::SetClear,
        ) => native_type(Some(&call.return_type), &call.return_type.contract_text()),
        _ => None,
    }
}

pub(super) fn lower_set_intrinsic(
    call: &CoreIntrinsicCall,
    params: &HashMap<String, usize>,
    param_types: &HashMap<String, NativeType>,
    functions: &HashMap<(String, usize), usize>,
    function_types: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
) -> super::super::NativeIrResult<NativeExpr> {
    let CoreIntrinsicId::Primitive(intrinsic) = &call.id else {
        return Err(
            "error[native_ir.set_intrinsic]: expected primitive intrinsic"
                .to_string()
                .into(),
        );
    };
    let set_semantic = set_semantic(call, param_types, function_types, constructors)?;
    if *intrinsic == CorePrimitiveIntrinsic::SetNew && call.args.is_empty() {
        return Ok(NativeExpr::ManagedOperation {
            encoded: encode_set_empty_operation(set_semantic).into(),
            args: Vec::new(),
        });
    }
    let lowered = call
        .args
        .iter()
        .map(|argument| {
            lower_expr_with_constructors(
                argument,
                params,
                param_types,
                functions,
                function_types,
                constructors,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let encoded = match intrinsic {
        CorePrimitiveIntrinsic::SetFromList if lowered.len() == 1 => {
            let list_semantic = call
                .args
                .first()
                .and_then(|argument| {
                    infer_native_type_with_constructors(
                        argument,
                        param_types,
                        function_types,
                        constructors,
                    )
                })
                .and_then(|ty| match ty {
                    NativeType::ManagedRef(semantic) => Some(semantic),
                    _ => None,
                })
                .ok_or_else(|| {
                    "error[native_ir.set_intrinsic]: from_list operand is not managed".to_string()
                })?;
            encode_set_from_list_operation(set_semantic, list_semantic)
        }
        CorePrimitiveIntrinsic::SetContains if lowered.len() == 2 => {
            encode_set_contains_operation(set_semantic)
        }
        CorePrimitiveIntrinsic::SetIsEmpty if lowered.len() == 1 => {
            encode_set_is_empty_operation(set_semantic)
        }
        CorePrimitiveIntrinsic::SetSize if lowered.len() == 1 => {
            encode_set_length_operation(set_semantic)
        }
        CorePrimitiveIntrinsic::SetAdd if lowered.len() == 2 => {
            encode_set_add_operation(set_semantic)
        }
        CorePrimitiveIntrinsic::SetRemove if lowered.len() == 2 => {
            encode_set_remove_operation(set_semantic)
        }
        CorePrimitiveIntrinsic::SetClear if lowered.len() == 1 => {
            encode_set_clear_operation(set_semantic)
        }
        CorePrimitiveIntrinsic::SetIterator if lowered.len() == 1 => {
            let element = iterator_element(&call.return_type)?;
            let list = CoreType::List(Box::new(element.clone()));
            encode_set_iterator_operation(set_semantic, semantic(&list)?)
        }
        _ => {
            return Err(format!(
                "error[native_ir.set_intrinsic]: invalid `{intrinsic:?}` arity {}",
                lowered.len()
            )
            .into())
        }
    };
    Ok(NativeExpr::ManagedOperation {
        encoded: encoded.into(),
        args: lowered,
    })
}

fn set_semantic(
    call: &CoreIntrinsicCall,
    param_types: &HashMap<String, NativeType>,
    function_types: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
) -> super::super::NativeIrResult<SemanticTypeId> {
    if let Some(argument) = call.args.first() {
        if let Some(NativeType::ManagedRef(semantic)) =
            infer_native_type_with_constructors(argument, param_types, function_types, constructors)
        {
            if !matches!(
                call.id,
                CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::SetFromList)
            ) {
                return Ok(semantic);
            }
        }
    }
    let NativeType::ManagedRef(semantic) =
        native_type(Some(&call.return_type), &call.return_type.contract_text()).ok_or_else(
            || "error[native_ir.set_intrinsic]: unsupported managed set type".to_string(),
        )?
    else {
        return Err("error[native_ir.set_intrinsic]: set type is not managed".into());
    };
    Ok(semantic)
}

fn iterator_element(ty: &CoreType) -> super::super::NativeIrResult<&CoreType> {
    match ty {
        CoreType::Apply { constructor, args }
            if constructor.rsplit('.').next() == Some("Iterator") && args.len() == 1 =>
        {
            Ok(&args[0])
        }
        _ => Err("error[native_ir.set_intrinsic]: iterator result is not concrete".into()),
    }
}

fn semantic(ty: &CoreType) -> super::super::NativeIrResult<SemanticTypeId> {
    Ok(SemanticTypeId::from_canonical(&ty.contract_text())
        .map_err(|error| format!("error[native_ir.set_intrinsic]: {error}"))?)
}
