//! Checked native and semantic identity recovery for lexical control values.

use std::collections::HashMap;

use crate::terlan_typeck::{CoreExpr, CoreType};

use super::{NativeType, YieldLoweringEnvironment};

/// Recovers the native result type of one lexical expression, including
/// structured cases whose pattern bindings are not visible to scalar inference.
pub(super) fn lexical_result_type(
    expr: &CoreExpr,
    native_types: &HashMap<String, NativeType>,
    core_types: &HashMap<String, CoreType>,
    environment: &YieldLoweringEnvironment<'_>,
) -> Option<NativeType> {
    super::super::structured_case::contains_case(expr)
        .then(|| {
            super::super::structured_case::core_expr_type_with_constructors(
                expr,
                core_types,
                environment.function_core_types,
                environment.constructors,
            )
            .map(|core_type| {
                super::super::structured_case::canonical_control_type(
                    core_type,
                    environment.constructors,
                )
            })
            .and_then(|core_type| {
                super::super::native_type(Some(&core_type), &core_type.contract_text())
            })
        })
        .flatten()
        .or_else(|| {
            super::super::infer_native_type_with_constructors(
                expr,
                native_types,
                environment.function_types,
                environment.constructors,
            )
        })
        .or_else(|| {
            super::super::structured_case::contains_case(expr)
                .then(|| {
                    super::super::structured_case::structured_result_type(
                        expr,
                        native_types,
                        core_types,
                        super::super::structured_case::StructuredCaseEnvironment {
                            functions: environment.functions,
                            function_types: environment.function_types,
                            function_core_types: environment.function_core_types,
                            constructors: environment.constructors,
                        },
                    )
                    .ok()
                })
                .flatten()
        })
        .or_else(|| {
            super::super::structured_case::core_expr_type(
                expr,
                core_types,
                environment.function_core_types,
            )
            .and_then(|core_type| {
                super::super::native_type(Some(&core_type), &core_type.contract_text())
            })
        })
}

/// Recovers the semantic identity paired with an already inferred native value.
pub(super) fn lexical_core_result_type(
    expr: &CoreExpr,
    native_type: NativeType,
    core_types: &HashMap<String, CoreType>,
    environment: &YieldLoweringEnvironment<'_>,
) -> Option<CoreType> {
    super::super::structured_case::core_expr_type_with_constructors(
        expr,
        core_types,
        environment.function_core_types,
        environment.constructors,
    )
    .or_else(|| {
        super::super::constructors::result_core_type_for_native(
            native_type,
            environment.constructors,
        )
    })
    .map(|core_type| {
        super::super::structured_case::canonical_control_type(core_type, environment.constructors)
    })
}
