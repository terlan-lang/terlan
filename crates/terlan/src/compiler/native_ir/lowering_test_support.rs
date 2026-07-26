//! Compatibility helpers for focused NativeIR lowering tests.

use std::collections::{HashMap, HashSet};

use crate::terlan_typeck::CoreFunction;

use super::{
    lower_native_function_with_callables, ComposedCallProfile, NativeConstructorLayouts,
    NativeContinuation, NativeFunction, NativeType,
};

/// Lowers one function without application-level escaping-callable metadata.
#[allow(clippy::too_many_arguments)]
pub(super) fn lower_native_function(
    module: &str,
    function: &CoreFunction,
    identities: &HashMap<(String, usize), usize>,
    function_types: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
    suspending_functions: &HashSet<(String, usize)>,
    call_profiles: &HashMap<usize, ComposedCallProfile>,
    stable_ids: &mut HashSet<u64>,
) -> Result<(NativeFunction, Vec<NativeContinuation>), String> {
    lower_native_function_with_callables(
        module,
        function,
        identities,
        function_types,
        &HashMap::new(),
        &HashMap::new(),
        &mut Vec::new(),
        constructors,
        suspending_functions,
        call_profiles,
        stable_ids,
    )
}
