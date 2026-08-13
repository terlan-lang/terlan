//! Direct-AOT lowering for immutable collection iterator steps.

use std::collections::HashMap;

use crate::runtime::native_image::managed::{encode_iterator_next_operation, SemanticTypeId};
use crate::terlan_typeck::{CoreIntrinsicCall, CoreIntrinsicId, CorePrimitiveIntrinsic, CoreType};

use super::{
    infer_native_type_with_constructors, lower_expr_with_constructors, native_type,
    NativeConstructorLayouts, NativeExpr, NativeType,
};

pub(super) fn infer_iterator_intrinsic_type(call: &CoreIntrinsicCall) -> Option<NativeType> {
    matches!(
        call.id,
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::IteratorNext)
    )
    .then(|| native_type(Some(&call.return_type), &call.return_type.contract_text()))
    .flatten()
}
pub(super) fn lower_iterator_intrinsic(
    call: &CoreIntrinsicCall,
    params: &HashMap<String, usize>,
    param_types: &HashMap<String, NativeType>,
    functions: &HashMap<(String, usize), usize>,
    function_types: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
) -> Result<NativeExpr, String> {
    if call.args.len() != 1 {
        return Err("error[native_ir.iterator_intrinsic]: invalid next arity".to_string());
    }
    let list_semantic = match infer_native_type_with_constructors(
        &call.args[0],
        param_types,
        function_types,
        constructors,
    ) {
        Some(NativeType::ManagedRef(semantic)) => semantic,
        _ => {
            return Err(
                "error[native_ir.iterator_intrinsic]: iterator state is not managed".to_string(),
            )
        }
    };
    let step = option_element(&call.return_type)?;
    let iterator = lower_expr_with_constructors(
        &call.args[0],
        params,
        param_types,
        functions,
        function_types,
        constructors,
    )?;
    Ok(NativeExpr::ManagedOperation {
        encoded: encode_iterator_next_operation(
            list_semantic,
            semantic(&call.return_type)?,
            semantic(step)?,
        )
        .into(),
        args: vec![iterator],
    })
}

fn option_element(ty: &CoreType) -> Result<&CoreType, String> {
    match ty {
        CoreType::Apply { constructor, args }
            if constructor.rsplit('.').next() == Some("Option") && args.len() == 1 =>
        {
            Ok(&args[0])
        }
        _ => Err("error[native_ir.iterator_intrinsic]: next result is not concrete".to_string()),
    }
}

fn semantic(ty: &CoreType) -> Result<SemanticTypeId, String> {
    SemanticTypeId::from_canonical(&ty.contract_text())
        .map_err(|error| format!("error[native_ir.iterator_intrinsic]: {error}"))
}
