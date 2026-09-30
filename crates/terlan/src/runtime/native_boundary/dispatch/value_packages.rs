//! Value-only package bindings on the legacy resource dispatch boundary.

use super::{DispatchError, NativeBoundaryValue as Value};
use terlan_runtime_abi::{NativeBinding, NativeValue};

pub(super) fn dispatch(binding: &NativeBinding, args: &[Value]) -> Result<Value, DispatchError> {
    let args = args.iter().map(to_native).collect::<Result<Vec<_>, _>>()?;
    (binding.invoke)(&args)
        .map(from_native)
        .map_err(|error| DispatchError::new(error.code().to_owned(), error.context(), 0))
}

pub(super) fn to_native(value: &Value) -> Result<NativeValue, DispatchError> {
    Ok(match value {
        Value::Map(entries) => NativeValue::Map(
            entries
                .iter()
                .map(|(key, value)| Ok((to_native(key)?, to_native(value)?)))
                .collect::<Result<_, DispatchError>>()?,
        ),
        Value::Unit => NativeValue::Unit,
        Value::Text(value) => NativeValue::String(value.clone()),
        Value::Bytes(value) => NativeValue::Bytes(value.clone()),
        Value::Int(value) => NativeValue::Int(*value),
        Value::Float(value) => NativeValue::Float(*value),
        Value::Bool(value) => NativeValue::Bool(*value),
        Value::Atom(value) => NativeValue::Atom(value.clone()),
        Value::List(values) => {
            NativeValue::List(values.iter().map(to_native).collect::<Result<_, _>>()?)
        }
        Value::Tuple(values) => {
            NativeValue::Tuple(values.iter().map(to_native).collect::<Result<_, _>>()?)
        }
        Value::Record { name, fields } => NativeValue::Record {
            name: name.clone(),
            fields: fields
                .iter()
                .map(|(name, value)| Ok((name.clone(), to_native(value)?)))
                .collect::<Result<_, DispatchError>>()?,
        },
        Value::OptionalText(value) => value.clone().into(),
        _ => {
            return Err(DispatchError::new(
                "native_package.value",
                "value-only bindings cannot receive resources",
                0,
            ))
        }
    })
}

pub(super) fn from_native(value: NativeValue) -> Value {
    match value {
        NativeValue::Map(entries) => Value::Map(
            entries
                .into_iter()
                .map(|(key, value)| (from_native(key), from_native(value)))
                .collect(),
        ),
        NativeValue::Unit => Value::Unit,
        NativeValue::String(value) => Value::Text(value),
        NativeValue::Bytes(value) => Value::Bytes(value),
        NativeValue::Int(value) => Value::Int(value),
        NativeValue::Float(value) => Value::Float(value),
        NativeValue::Bool(value) => Value::Bool(value),
        NativeValue::Atom(value) => Value::Atom(value),
        NativeValue::List(values) => Value::List(values.into_iter().map(from_native).collect()),
        NativeValue::Tuple(values) => Value::Tuple(values.into_iter().map(from_native).collect()),
        NativeValue::Record { name, fields } => Value::Record {
            name,
            fields: fields
                .into_iter()
                .map(|(name, value)| (name, from_native(value)))
                .collect(),
        },
    }
}

#[cfg(test)]
#[path = "value_packages_test.rs"]
mod tests;
