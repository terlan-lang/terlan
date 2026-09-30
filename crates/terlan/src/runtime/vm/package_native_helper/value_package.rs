//! Generic adapter for safe package-owned operations over copied values.

use crate::runtime::vm::native_value::{from_native, to_native};
use crate::runtime::vm::pure_native::PureNativeCapabilityRequest;
use crate::runtime::vm::{ReplValue, VmRuntimeResult};
use terlan_runtime_abi::NativeBinding;

use crate::std_native_packages as packages;

pub(super) fn binding(operation: &str) -> Option<&'static NativeBinding> {
    packages::value_binding(operation)
}

pub(super) fn call(
    binding: &NativeBinding,
    request: &PureNativeCapabilityRequest,
) -> VmRuntimeResult<ReplValue> {
    let arguments = request
        .package_arguments
        .as_ref()
        .ok_or("error[native_package.arguments]: package call has no decoded arguments")?;
    binding.validate_arity(arguments.len())?;
    let arguments = arguments
        .iter()
        .map(to_native)
        .collect::<VmRuntimeResult<Vec<_>>>()?;
    let value = (binding.invoke)(&arguments).map_err(|error| error.to_string())?;
    Ok(from_native(value))
}

#[cfg(test)]
#[path = "value_package_test.rs"]
mod tests;
