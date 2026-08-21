//! Direct-AOT lowering for scalar Boolean intrinsics.

use std::collections::HashMap;
use std::sync::Arc;

use crate::runtime::native_image::managed::encode_string_literal;
use crate::terlan_typeck::{CoreIntrinsicCall, CoreIntrinsicId, CorePrimitiveIntrinsic};

use super::{lower_expr_with_constructors, NativeConstructorLayouts, NativeExpr, NativeType};

/// Lowers Boolean text conversion without crossing a managed runtime boundary.
pub(super) fn lower_boolean_intrinsic(
    call: &CoreIntrinsicCall,
    params: &HashMap<String, usize>,
    param_types: &HashMap<String, NativeType>,
    functions: &HashMap<(String, usize), usize>,
    function_types: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
) -> super::super::NativeIrResult<NativeExpr> {
    if !matches!(
        call.id,
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::BoolToString)
    ) || call.args.len() != 1
    {
        return Err(
            "error[native_ir.bool_intrinsic]: unsupported Boolean intrinsic"
                .to_string()
                .into(),
        );
    }
    let condition = lower_expr_with_constructors(
        &call.args[0],
        params,
        param_types,
        functions,
        function_types,
        constructors,
    )?;
    let text = |value: &str| {
        encode_string_literal(value)
            .map(|encoded| NativeExpr::ManagedLiteral {
                encoded: Arc::from(encoded),
            })
            .map_err(|error| format!("error[native_ir.bool_intrinsic]: {error}"))
    };
    Ok(NativeExpr::If {
        clauses: vec![
            (condition, text("true")?),
            (NativeExpr::Bool(true), text("false")?),
        ],
    })
}

/// Returns the managed string representation produced by Boolean rendering.
pub(super) fn infer_boolean_intrinsic_type(call: &CoreIntrinsicCall) -> Option<NativeType> {
    matches!(
        call.id,
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::BoolToString)
    )
    .then_some(NativeType::StringRef)
}
