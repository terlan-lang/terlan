//! Lexical literals and indirect calls shared by ordinary and suspending cases.

use super::{core_expr_type, lowering::StructuredCaseEnvironment};
use crate::compiler::native_ir::{lower_expr_with_constructors, NativeExpr, NativeType};
use crate::terlan_typeck::{CoreExpr, CoreType};
use std::collections::HashMap;

pub(super) fn lower_plain(
    expr: &CoreExpr,
    params: &HashMap<String, usize>,
    param_types: &HashMap<String, NativeType>,
    core_types: &HashMap<String, CoreType>,
    environment: StructuredCaseEnvironment<'_>,
) -> Result<NativeExpr, String> {
    let StructuredCaseEnvironment {
        functions,
        function_types,
        function_core_types,
        constructors,
    } = environment;
    if matches!(
        expr,
        CoreExpr::Tuple(_) | CoreExpr::List(_) | CoreExpr::Map(_)
    ) {
        if let Some(core_type) = core_expr_type(expr, core_types, function_core_types) {
            if let Some(lowered) =
                crate::compiler::native_ir::collection_values::lower_boundary_collection_value(
                    expr,
                    Some(&core_type),
                    params,
                    param_types,
                    functions,
                    function_types,
                    constructors,
                )?
            {
                return Ok(lowered);
            }
        }
    }
    if let CoreExpr::FunctionCall { callee, args } = expr {
        let (parameter_types, result_type) = closure_invocation_signature(expr, core_types)
            .ok_or_else(|| {
                "error[native_ir.structured_case_closure]: indirect call has no checked arrow type"
                    .to_string()
            })?;
        if parameter_types.len() != args.len() {
            return Err(
                "error[native_ir.structured_case_closure]: indirect call arity mismatch".into(),
            );
        }
        return Ok(NativeExpr::InvokeClosure {
            callee: Box::new(lower_expr_with_constructors(
                callee,
                params,
                param_types,
                functions,
                function_types,
                constructors,
            )?),
            args: args
                .iter()
                .map(|arg| {
                    lower_expr_with_constructors(
                        arg,
                        params,
                        param_types,
                        functions,
                        function_types,
                        constructors,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?,
            parameter_types,
            result_type,
        });
    }
    lower_expr_with_constructors(
        expr,
        params,
        param_types,
        functions,
        function_types,
        constructors,
    )
}

pub(super) fn closure_invocation_signature(
    expr: &CoreExpr,
    core_types: &HashMap<String, CoreType>,
) -> Option<(Vec<NativeType>, NativeType)> {
    let CoreExpr::FunctionCall { callee, .. } = expr else {
        return None;
    };
    let CoreExpr::Var(name) = callee.as_ref() else {
        return None;
    };
    let CoreType::Arrow {
        params,
        return_type,
    } = core_types.get(name)?
    else {
        return None;
    };
    let params = params
        .iter()
        .map(|ty| crate::compiler::native_ir::native_type(Some(ty), &ty.contract_text()))
        .collect::<Option<Vec<_>>>()?;
    let result =
        crate::compiler::native_ir::native_type(Some(return_type), &return_type.contract_text())?;
    Some((params, result))
}
