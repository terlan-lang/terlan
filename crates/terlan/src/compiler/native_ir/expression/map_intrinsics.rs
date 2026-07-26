//! Direct-AOT lowering for the typed persistent map surface.

use std::collections::HashMap;

use crate::runtime::native_image::managed::{
    encode_map_clear_operation, encode_map_contains_operation, encode_map_empty_operation,
    encode_map_from_entry_list_operation, encode_map_get_option_operation,
    encode_map_is_empty_operation, encode_map_iterator_operation, encode_map_length_operation,
    encode_map_put_operation, encode_map_remove_operation, encode_map_take_operation,
    SemanticTypeId,
};
use crate::terlan_typeck::{
    CoreIntrinsicCall, CoreIntrinsicId, CorePrimitiveIntrinsic, CoreTupleTypeElem, CoreType,
};

use super::{
    infer_native_type_with_constructors, lower_expr_with_constructors, native_type,
    NativeConstructorLayouts, NativeExpr, NativeType,
};

pub(super) fn infer_map_intrinsic_type(call: &CoreIntrinsicCall) -> Option<NativeType> {
    match call.id {
        CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::MapIsEmpty | CorePrimitiveIntrinsic::MapContainsKey,
        ) => Some(NativeType::Bool),
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::MapSize) => Some(NativeType::Int),
        CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::MapNew
            | CorePrimitiveIntrinsic::MapFromEntries
            | CorePrimitiveIntrinsic::MapGet
            | CorePrimitiveIntrinsic::MapTake
            | CorePrimitiveIntrinsic::MapIterator
            | CorePrimitiveIntrinsic::MapPut
            | CorePrimitiveIntrinsic::MapRemove
            | CorePrimitiveIntrinsic::MapClear,
        ) => native_type(Some(&call.return_type), &call.return_type.contract_text()),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn lower_map_intrinsic(
    call: &CoreIntrinsicCall,
    params: &HashMap<String, usize>,
    param_types: &HashMap<String, NativeType>,
    functions: &HashMap<(String, usize), usize>,
    function_types: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
) -> Result<NativeExpr, String> {
    let CoreIntrinsicId::Primitive(intrinsic) = &call.id else {
        return Err("error[native_ir.map_intrinsic]: expected primitive intrinsic".to_string());
    };
    let map_semantic = map_semantic(call, param_types, function_types, constructors)?;
    if *intrinsic == CorePrimitiveIntrinsic::MapFromEntries && call.args.len() == 1 {
        let (key, value) = map_arguments(&call.return_type)?;
        let pair = CoreType::Tuple(vec![
            CoreTupleTypeElem::Type(key.clone()),
            CoreTupleTypeElem::Type(value.clone()),
        ]);
        let list = CoreType::List(Box::new(pair.clone()));
        let entries = super::super::collection_values::lower_boundary_collection_value(
            &call.args[0],
            Some(&list),
            params,
            param_types,
            functions,
            function_types,
            constructors,
        )?
        .ok_or_else(|| {
            "error[native_ir.map_intrinsic]: from_entries requires a typed entry list".to_string()
        })?;
        return Ok(NativeExpr::ManagedOperation {
            encoded: encode_map_from_entry_list_operation(
                map_semantic,
                semantic(&list)?,
                semantic(&pair)?,
            )
            .into(),
            args: vec![entries],
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
        CorePrimitiveIntrinsic::MapNew if lowered.is_empty() => {
            encode_map_empty_operation(map_semantic)
        }
        CorePrimitiveIntrinsic::MapIsEmpty if lowered.len() == 1 => {
            encode_map_is_empty_operation(map_semantic)
        }
        CorePrimitiveIntrinsic::MapSize if lowered.len() == 1 => {
            encode_map_length_operation(map_semantic)
        }
        CorePrimitiveIntrinsic::MapGet if lowered.len() == 2 => {
            encode_map_get_option_operation(map_semantic, semantic(&call.return_type)?)
        }
        CorePrimitiveIntrinsic::MapContainsKey if lowered.len() == 2 => {
            encode_map_contains_operation(map_semantic)
        }
        CorePrimitiveIntrinsic::MapPut if lowered.len() == 3 => {
            encode_map_put_operation(map_semantic)
        }
        CorePrimitiveIntrinsic::MapRemove if lowered.len() == 2 => {
            encode_map_remove_operation(map_semantic)
        }
        CorePrimitiveIntrinsic::MapClear if lowered.len() == 1 => {
            encode_map_clear_operation(map_semantic)
        }
        CorePrimitiveIntrinsic::MapTake if lowered.len() == 2 => {
            let option = take_option_type(&call.return_type)?;
            encode_map_take_operation(
                map_semantic,
                semantic(option)?,
                semantic(&call.return_type)?,
            )
        }
        CorePrimitiveIntrinsic::MapIterator if lowered.len() == 1 => {
            let pair = list_element(&call.return_type)?;
            encode_map_iterator_operation(
                map_semantic,
                semantic(&call.return_type)?,
                semantic(pair)?,
            )
        }
        _ => {
            return Err(format!(
                "error[native_ir.map_intrinsic]: invalid `{intrinsic:?}` arity {}",
                lowered.len()
            ))
        }
    };
    Ok(NativeExpr::ManagedOperation {
        encoded: encoded.into(),
        args: lowered,
    })
}

fn map_semantic(
    call: &CoreIntrinsicCall,
    param_types: &HashMap<String, NativeType>,
    function_types: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
) -> Result<SemanticTypeId, String> {
    if let Some(argument) = call.args.first() {
        if let Some(NativeType::ManagedRef(semantic)) =
            infer_native_type_with_constructors(argument, param_types, function_types, constructors)
        {
            return Ok(semantic);
        }
    }
    managed_semantic(&call.return_type)
}

fn managed_semantic(ty: &CoreType) -> Result<SemanticTypeId, String> {
    let NativeType::ManagedRef(semantic) = native_type(Some(ty), &ty.contract_text())
        .ok_or_else(|| "error[native_ir.map_intrinsic]: unsupported managed type".to_string())?
    else {
        return Err("error[native_ir.map_intrinsic]: type is not managed".to_string());
    };
    Ok(semantic)
}

fn semantic(ty: &CoreType) -> Result<SemanticTypeId, String> {
    SemanticTypeId::from_canonical(&ty.contract_text())
        .map_err(|error| format!("error[native_ir.map_intrinsic]: {error}"))
}

fn map_arguments(ty: &CoreType) -> Result<(&CoreType, &CoreType), String> {
    match ty {
        CoreType::Apply { constructor, args }
            if constructor.rsplit('.').next() == Some("Map") && args.len() == 2 =>
        {
            Ok((&args[0], &args[1]))
        }
        _ => Err("error[native_ir.map_intrinsic]: map type is not concrete".to_string()),
    }
}

fn take_option_type(ty: &CoreType) -> Result<&CoreType, String> {
    match ty {
        CoreType::Tuple(elements) if elements.len() == 2 => match &elements[0] {
            CoreTupleTypeElem::Type(option) | CoreTupleTypeElem::Field { ty: option, .. } => {
                Ok(option)
            }
        },
        _ => Err("error[native_ir.map_intrinsic]: take result is not concrete".to_string()),
    }
}

fn list_element(ty: &CoreType) -> Result<&CoreType, String> {
    match ty {
        CoreType::List(element) => Ok(element),
        CoreType::Apply { constructor, args }
            if constructor.rsplit('.').next() == Some("List") && args.len() == 1 =>
        {
            Ok(&args[0])
        }
        _ => Err("error[native_ir.map_intrinsic]: iterator result is not concrete".to_string()),
    }
}
