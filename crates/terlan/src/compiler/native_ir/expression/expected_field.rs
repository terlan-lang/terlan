//! Checked field lowering and field-specific diagnostic translation.

use super::{
    infer_native_type_for_lowering, lower_expr_with_constructors, native_type,
    NativeConstructorLayouts, NativeExpr, NativeType,
};
use crate::terlan_typeck::{CoreExpr, CoreType};
use std::collections::HashMap;

/// Checked field-lowering inputs shared by constructor and record expressions.
pub(super) struct ExpectedFieldContext<'a> {
    /// Local parameter positions in the native function.
    pub(super) params: &'a HashMap<String, usize>,
    /// Checked native types of local parameters.
    pub(super) param_types: &'a HashMap<String, NativeType>,
    /// Available functions keyed by source name and arity.
    pub(super) functions: &'a HashMap<(String, usize), usize>,
    /// Native result types of the available functions.
    pub(super) function_types: &'a HashMap<(String, usize), NativeType>,
    /// Managed constructor layouts admitted to the module.
    pub(super) constructors: &'a NativeConstructorLayouts,
}

/// Lowers one field against its checked type, including scalar control flow
/// embedded inside a constructor or record value.
pub(super) fn lower_expected_field(
    field: &CoreExpr,
    expected: &CoreType,
    type_error_code: &str,
    context: &ExpectedFieldContext<'_>,
) -> Result<(NativeExpr, NativeType), String> {
    let expected_native = native_type(Some(expected), &expected.contract_text())
        .ok_or_else(|| format!("error[{type_error_code}]: expected field type is not native"))?;
    let lowered = super::super::collection_values::try_lower_typed_value(
        field,
        expected,
        context.params,
        context.param_types,
        context.functions,
        context.function_types,
        context.constructors,
    )
    .map_err(|error| remap_field_type_error(error, type_error_code))?;
    if let Some(lowered) = lowered {
        return Ok((lowered, expected_native));
    }
    let actual = infer_native_type_for_lowering(
        field,
        context.param_types,
        context.function_types,
        context.constructors,
    )?
    .ok_or_else(|| {
        format!("error[native_ir.constructor_control_field]: cannot infer `{field:?}`")
    })?;
    if actual != expected_native {
        return Err(format!(
            "error[{type_error_code}]: expected {expected_native:?}, found {actual:?}"
        ));
    }
    let lowered = lower_expr_with_constructors(
        field,
        context.params,
        context.param_types,
        context.functions,
        context.function_types,
        context.constructors,
    )?;
    Ok((lowered, expected_native))
}

fn remap_field_type_error(error: String, type_error_code: &str) -> String {
    if error.starts_with("error[native_ir.collection_value]:")
        || error.starts_with("error[native_ir.collection_control_type]:")
    {
        let detail = error
            .split_once(": ")
            .map_or(error.as_str(), |(_, detail)| detail);
        format!("error[{type_error_code}]: {detail}")
    } else {
        error
    }
}
