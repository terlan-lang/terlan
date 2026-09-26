//! Shares checked nominal record construction across aggregate boundaries.

use super::*;

/// Lowers fields against the registered record's substituted parameter types.
pub(super) fn lower(
    value: &CoreExpr,
    expected: &CoreType,
    params: &HashMap<String, usize>,
    param_types: &HashMap<String, NativeType>,
    functions: &HashMap<(String, usize), usize>,
    function_types: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
) -> crate::compiler::native_ir::NativeIrResult<Option<NativeExpr>> {
    crate::compiler::native_ir::constructors::lower_structural_record_construct(
        value,
        expected,
        constructors,
        |field, expected| {
            let ty = native_type(Some(expected), &expected.contract_text(), constructors)
                .ok_or_else(|| {
                    "error[native_ir.structural_record_field_type]: cannot infer field".to_string()
                })?;
            let lowered = lower_typed_value(
                field,
                expected,
                params,
                param_types,
                functions,
                function_types,
                constructors,
            )?;
            Ok((lowered, ty))
        },
    )
    .map_err(Into::into)
}
