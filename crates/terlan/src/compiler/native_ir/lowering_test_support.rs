//! Compatibility helpers for focused NativeIR lowering tests.

use std::collections::{HashMap, HashSet};

use crate::terlan_typeck::CoreFunction;

use super::{
    lower_native_function_with_callables, NativeConstructorLayouts, NativeContinuation,
    NativeFunction, NativeFunctionLoweringEnvironment, NativeFunctionLoweringOutputs,
};

/// Lowers one function without application-level escaping-callable metadata.
pub(super) fn lower_native_function(
    module: &str,
    function: &CoreFunction,
    constructors: &NativeConstructorLayouts,
    stable_ids: &mut HashSet<u64>,
) -> Result<(NativeFunction, Vec<NativeContinuation>), String> {
    lower_native_function_with_callables(
        module,
        function,
        NativeFunctionLoweringEnvironment {
            identities: &HashMap::new(),
            function_types: &HashMap::new(),
            function_core_types: &HashMap::new(),
            callable_shapes: &HashMap::new(),
            constructors,
            suspending_functions: &HashSet::new(),
            call_profiles: &HashMap::new(),
            dynamic_call_profiles: &HashMap::new(),
        },
        NativeFunctionLoweringOutputs {
            lifted_functions: &mut Vec::new(),
            stable_ids,
        },
    )
}
