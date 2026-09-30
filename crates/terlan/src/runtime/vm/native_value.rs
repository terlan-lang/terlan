//! Generic conversion of package-owned copied values into VM values.

use super::{ReplValue, VmRuntimeResult};
use terlan_runtime_abi::NativeValue;

pub(crate) fn to_native(value: &ReplValue) -> VmRuntimeResult<NativeValue> {
    Ok(match value {
        ReplValue::Map(entries) => NativeValue::Map(entries.iter().map(|(key, value)| Ok((to_native(key)?, to_native(value)?))).collect::<VmRuntimeResult<_>>()?),
        ReplValue::Unit => NativeValue::Unit,
        ReplValue::Bool(value) => NativeValue::Bool(*value),
        ReplValue::Int(value) => NativeValue::Int(*value),
        ReplValue::Float(value) => NativeValue::Float(value.parse().map_err(|_| "error[native_package.value]: invalid Float")?),
        ReplValue::String(value) => NativeValue::String(value.clone()),
        ReplValue::StringBytes(value) => NativeValue::String(std::str::from_utf8(value).map_err(|_| "error[native_package.value]: invalid UTF-8 String")?.to_owned()),
        ReplValue::Bytes(value) => NativeValue::Bytes(value.to_vec()),
        ReplValue::Atom(value) => NativeValue::Atom(value.clone()),
        ReplValue::Tuple(values) => NativeValue::Tuple(values.iter().map(to_native).collect::<VmRuntimeResult<_>>()?),
        ReplValue::List(values) => NativeValue::List(values.iter().map(to_native).collect::<VmRuntimeResult<_>>()?),
        ReplValue::Record { name, fields } => NativeValue::Record {
            name: name.clone(),
            fields: fields.iter().map(|(name, value)| Ok((name.clone(), to_native(value)?))).collect::<VmRuntimeResult<_>>()?,
        },
        _ => return Err("error[native_package.value]: operation requires an owned value, not a runtime identity".into()),
    })
}

pub(crate) fn from_native(value: NativeValue) -> ReplValue {
    match value {
        NativeValue::Map(entries) => ReplValue::Map(
            entries
                .into_iter()
                .map(|(key, value)| (from_native(key), from_native(value)))
                .collect(),
        ),
        NativeValue::Unit => ReplValue::Unit,
        NativeValue::Bool(value) => ReplValue::Bool(value),
        NativeValue::Int(value) => ReplValue::Int(value),
        NativeValue::Float(value) => ReplValue::Float(value.to_string()),
        NativeValue::String(value) => ReplValue::String(value),
        NativeValue::Bytes(value) => ReplValue::Bytes(value.into()),
        NativeValue::Atom(value) => ReplValue::Atom(value),
        NativeValue::Tuple(values) => {
            ReplValue::Tuple(values.into_iter().map(from_native).collect())
        }
        NativeValue::List(values) => ReplValue::List(values.into_iter().map(from_native).collect()),
        NativeValue::Record { name, fields } => ReplValue::Record {
            name,
            fields: fields
                .into_iter()
                .map(|(name, value)| (name, from_native(value)))
                .collect(),
        },
    }
}

/// Replaces a copied value without retaining stale fields or collection entries.
/// Matching records preserve nested aggregate storage for repeated ingress.
pub(crate) fn replace_native(target: &mut ReplValue, source: NativeValue) {
    match (target, source) {
        (
            ReplValue::Record { name, fields },
            NativeValue::Record {
                name: next_name,
                fields: next_fields,
            },
        ) if *name == next_name
            && fields
                .iter()
                .map(|(name, _)| name)
                .eq(next_fields.iter().map(|(name, _)| name)) =>
        {
            for ((_, target), (_, source)) in fields.iter_mut().zip(next_fields) {
                replace_native(target, source);
            }
        }
        (ReplValue::Map(entries), NativeValue::Map(next)) => {
            entries.clear();
            entries.extend(
                next.into_iter()
                    .map(|(key, value)| (from_native(key), from_native(value))),
            );
        }
        (ReplValue::List(values), NativeValue::List(next))
        | (ReplValue::Tuple(values), NativeValue::Tuple(next)) => {
            values.clear();
            values.extend(next.into_iter().map(from_native));
        }
        (target, source) => *target = from_native(source),
    }
}

#[cfg(test)]
#[path = "native_value_test.rs"]
mod tests;
