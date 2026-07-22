//! Lifted-callable metadata admission before native symbol declaration.

use super::super::NativeModule;

/// Rejects ambiguous capture/parameter layouts before code generation.
pub(super) fn validate_callable_shapes(natives: &[NativeModule]) -> Result<(), String> {
    for function in natives.iter().flat_map(|native| &native.functions) {
        let capture_count = function.callable_captures.len();
        if function.arity != function.params.len()
            || capture_count > function.params.len()
            || function.params.get(..capture_count) != Some(function.callable_captures.as_slice())
            || (function.public && capture_count != 0)
        {
            return Err(format!(
                "error[cranelift.callable_shape]: function `{}` has inconsistent lifted capture metadata",
                function.name
            ));
        }
    }
    Ok(())
}
