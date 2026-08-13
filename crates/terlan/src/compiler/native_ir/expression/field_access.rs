//! Managed aggregate field-access lowering.

use std::collections::HashMap;

use crate::terlan_typeck::CoreExpr;

use super::{
    infer_native_type_for_lowering, lower_expr_with_constructors, managed_field_projection,
    NativeConstructorLayouts, NativeExpr, NativeType,
};

/// Checked receiver identity of one managed field projection.
pub(super) struct ManagedFieldAccess<'a> {
    pub(super) base: &'a CoreExpr,
    pub(super) record_name: Option<&'a str>,
    pub(super) field: &'a str,
}

/// Lowers one checked named field read through the bounded managed-operation ABI.
pub(super) fn lower_managed_field_access(
    access: ManagedFieldAccess<'_>,
    params: &HashMap<String, usize>,
    param_types: &HashMap<String, NativeType>,
    functions: &HashMap<(String, usize), usize>,
    function_types: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
) -> Result<NativeExpr, String> {
    let ManagedFieldAccess {
        base,
        record_name,
        field,
    } = access;
    let base_type =
        infer_native_type_for_lowering(base, param_types, function_types, constructors)?
            .ok_or_else(|| {
                format!(
                    "error[native_ir.field_base]: cannot infer receiver `{}` for field `{field}`",
                    base.contract_text()
                )
            })?;
    let (encoded, _) = managed_field_projection(base_type, record_name, field, constructors)?;
    let base = lower_expr_with_constructors(
        base,
        params,
        param_types,
        functions,
        function_types,
        constructors,
    )?;
    Ok(NativeExpr::ManagedOperation {
        encoded,
        args: vec![base],
    })
}
