//! Legacy resource/value adaptation; operation behavior belongs to std.data.

use super::{DispatchError, NativeBoundaryValue as Value};
use crate::terlan_native::json;
use terlan_runtime_abi::NativeResourceValue;

pub(super) fn dispatch(operation: &str, args: &[Value]) -> Result<Value, DispatchError> {
    let args = args
        .iter()
        .enumerate()
        .map(|(index, value)| match value {
            Value::Json(json) => Ok(NativeResourceValue::Resource(json)),
            value => super::value_packages::to_native(value)
                .map(NativeResourceValue::Value)
                .map_err(|_| super::args::type_error(operation, index, "owned value or Json")),
        })
        .collect::<Result<Vec<_>, _>>()?;
    json::invoke(operation, &args).map(|output| match output {
        NativeResourceValue::Resource(json) => Value::Json(json),
        NativeResourceValue::Value(value) => super::value_packages::from_native(value),
    })
}
