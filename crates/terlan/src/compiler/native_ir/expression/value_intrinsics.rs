//! Direct-AOT specialization for the polymorphic value-to-string intrinsic.

use std::collections::HashMap;

use crate::runtime::native_image::managed::encode_atom_to_string_operation;
use crate::terlan_typeck::{CoreIntrinsicCall, CoreIntrinsicId, CorePrimitiveIntrinsic};

use super::{
    boolean_intrinsics, float_intrinsics, infer_native_type_for_lowering, integer_intrinsics,
    lower_expr_with_constructors, NativeConstructorLayouts, NativeExpr, NativeType,
};

/// Lowers `String(value)` and `Atom.to_string` through representation-specific
/// operations. Atom conversion only reads this image's finite atom inventory.
pub(super) fn lower_value_to_string(
    call: &CoreIntrinsicCall,
    params: &HashMap<String, usize>,
    param_types: &HashMap<String, NativeType>,
    functions: &HashMap<(String, usize), usize>,
    function_types: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
) -> Result<NativeExpr, String> {
    if !matches!(
        call.id,
        CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::ValueToString | CorePrimitiveIntrinsic::AtomToString
        )
    ) || call.args.len() != 1
    {
        return Err(
            "error[native_ir.value_to_string]: string conversion requires exactly one value"
                .to_string(),
        );
    }

    let argument_type =
        infer_native_type_for_lowering(&call.args[0], param_types, function_types, constructors)?
            .ok_or_else(|| {
            "error[native_ir.value_to_string]: cannot determine the native value representation"
                .to_string()
        })?;

    match argument_type {
        unsupported if call.id == CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::AtomToString)
            && unsupported != NativeType::Atom => Err(format!(
                "error[native_ir.value_to_string]: Atom.to_string requires an atom, found {unsupported:?}"
            )),
        NativeType::Atom => Ok(NativeExpr::ManagedOperation {
            encoded: encode_atom_to_string_operation().into(),
            args: vec![lower_expr_with_constructors(
                &call.args[0], params, param_types, functions, function_types, constructors,
            )?],
        }),
        NativeType::Int => integer_intrinsics::lower_integer_intrinsic(
            &specialize(call, CorePrimitiveIntrinsic::IntToString),
            params,
            param_types,
            functions,
            function_types,
            constructors,
        ),
        NativeType::Float => float_intrinsics::lower_float_intrinsic(
            &specialize(call, CorePrimitiveIntrinsic::FloatToString),
            params,
            param_types,
            functions,
            function_types,
            constructors,
        ),
        NativeType::Bool => boolean_intrinsics::lower_boolean_intrinsic(
            &specialize(call, CorePrimitiveIntrinsic::BoolToString),
            params,
            param_types,
            functions,
            function_types,
            constructors,
        )
        .map_err(String::from),
        NativeType::StringRef => lower_expr_with_constructors(
            &call.args[0],
            params,
            param_types,
            functions,
            function_types,
            constructors,
        ),
        unsupported => Err(format!(
            "error[native_ir.value_to_string]: unsupported native value representation {unsupported:?}"
        )),
    }
}

fn specialize(call: &CoreIntrinsicCall, intrinsic: CorePrimitiveIntrinsic) -> CoreIntrinsicCall {
    let mut specialized = call.clone();
    specialized.id = CoreIntrinsicId::Primitive(intrinsic);
    specialized
}
