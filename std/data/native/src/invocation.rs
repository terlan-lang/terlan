//! Package-owned operation execution over typed host-local resources.

use crate::{Json, JsonError};
use terlan_runtime_abi::{
    FromNativeValue, NativeAdapterError, NativeResourceOperation, NativeResourceValue,
};

/// Executes a read-only or allocating JSON operation after exact arity validation.
/// Resource inputs are borrowed, and newly allocated resources are returned to
/// the host for owner-checked storage. This function grants no handle authority.
pub fn invoke(
    operation: &str,
    values: &[NativeResourceValue<&Json>],
) -> Result<NativeResourceValue<Json>, NativeAdapterError> {
    let contract = validate(operation, values.len())?;
    if contract.mutates_receiver {
        return Err(NativeAdapterError::new(
            "dispatch.mutable_receiver_requires_direct_lowering",
            format!(
                "operation `{operation}` mutates a receiver and must use direct native lowering"
            ),
            0,
        ));
    }
    let args = Arguments { operation, values };
    use NativeResourceValue::{Resource, Value};
    Ok(match operation {
        "std.data.json.null" => Resource(crate::null()),
        "std.data.json.bool" => Resource(crate::r#bool(args.read(0, "Bool")?)),
        "std.data.json.int" => Resource(crate::int(args.read(0, "Int")?)),
        "std.data.json.float" => Resource(crate::float(args.read(0, "Float")?)?),
        "std.data.json.string" => Resource(crate::string(args.read(0, "String")?)),
        "std.data.json.array" => Resource(crate::array()),
        "std.data.json.object" => Resource(crate::object()),
        "std.data.json.parse" => Resource(crate::parse(args.read(0, "String")?)?),
        "std.data.json.to_string" => Value(crate::to_string(args.json(0)?).into()),
        "std.data.json.stringify" => Value(crate::stringify(args.json(0)?)?.into()),
        "std.data.json.stringify_pretty" => Value(crate::stringify_pretty(args.json(0)?)?.into()),
        "std.data.json.get" => Resource(crate::get(args.json(0)?, args.read(1, "String")?)?),
        "std.data.json.keys" => Value(crate::keys(args.json(0)?)?.into()),
        "std.data.json.object_length" => Value(crate::object_length(args.json(0)?)?.into()),
        "std.data.json.length" => Value(crate::length(args.json(0)?)?.into()),
        "std.data.json.at" => Resource(crate::at(args.json(0)?, args.read(1, "Int")?)?),
        "std.data.json.as_string" => Value(crate::as_string(args.json(0)?)?.into()),
        "std.data.json.as_int" => Value(crate::as_int(args.json(0)?)?.into()),
        "std.data.json.as_float" => Value(crate::as_float(args.json(0)?)?.into()),
        "std.data.json.as_bool" => Value(crate::as_bool(args.json(0)?)?.into()),
        "std.data.json.is_null" => Value(crate::is_null(args.json(0)?).into()),
        "std.data.json.string_field_rows" => Value(
            crate::string_field_rows(args.json(0)?, &args.read::<Vec<&str>>(1, "List[String]")?)?
                .into(),
        ),
        "std.data.json.nested_string_field_rows" => Value(
            crate::nested_string_field_rows(
                args.json(0)?,
                &args.read::<Vec<&str>>(1, "List[String]")?,
                args.read(2, "String")?,
                &args.read::<Vec<&str>>(3, "List[String]")?,
            )?
            .into(),
        ),
        "std.data.json.nested_string_field_rows_page" => Value(
            crate::nested_string_field_rows_page(
                args.json(0)?,
                args.read(1, "Int")?,
                args.read(2, "Int")?,
                &args.read::<Vec<&str>>(3, "List[String]")?,
                args.read(4, "String")?,
                &args.read::<Vec<&str>>(5, "List[String]")?,
            )?
            .into(),
        ),
        "std.data.json.string_object_rows" => Resource(crate::string_object_rows(
            &args.read::<Vec<&str>>(0, "List[String]")?,
            &args.read::<Vec<Vec<String>>>(1, "List[List[String]]")?,
        )?),
        "std.data.json.required_fields" => Value(
            crate::required_fields(
                args.json(0)?,
                &args.read::<Vec<&str>>(1, "List[String]")?,
                &args.read::<Vec<&str>>(2, "List[String]")?,
                &args.read::<Vec<&str>>(3, "List[String]")?,
            )?
            .into(),
        ),
        "std.data.json.required_field_rows" => Value(
            crate::required_field_rows(
                args.json(0)?,
                &args.read::<Vec<&str>>(1, "List[String]")?,
                &args.read::<Vec<&str>>(2, "List[String]")?,
                &args.read::<Vec<&str>>(3, "List[String]")?,
            )?
            .into(),
        ),
        "std.data.json.required_field_rows_page" => Value(
            crate::required_field_rows_page(
                args.json(0)?,
                args.read(1, "Int")?,
                args.read(2, "Int")?,
                &args.read::<Vec<&str>>(3, "List[String]")?,
                &args.read::<Vec<&str>>(4, "List[String]")?,
                &args.read::<Vec<&str>>(5, "List[String]")?,
            )?
            .into(),
        ),
        _ => return Err(unknown(operation)),
    })
}

/// Mutates an exclusively borrowed receiver; arguments exclude the receiver.
///
/// Hosts validate every argument's owner before calling and return the original
/// receiver handle on success. Hosts supply owned argument snapshots so insertion
/// neither aliases another resource nor performs a second deep copy.
pub fn mutate(
    operation: &str,
    receiver: &mut Json,
    values: Vec<NativeResourceValue<Json>>,
) -> Result<(), NativeAdapterError> {
    let contract = validate(operation, values.len().saturating_add(1))?;
    if !contract.mutates_receiver {
        return Err(NativeAdapterError::new(
            "dispatch.immutable_operation",
            format!("Operation `{operation}` does not mutate a receiver."),
            0,
        ));
    }
    let mut values = values.into_iter();
    match operation {
        "std.data.json.array_push" => {
            crate::push(receiver, owned_json(operation, 1, values.next())?)?
        }
        "std.data.json.array_extend" => {
            crate::extend(receiver, owned_json(operation, 1, values.next())?)?
        }
        "std.data.json.array_set" => crate::set(
            receiver,
            owned_value(operation, 1, "Int", values.next())?,
            owned_json(operation, 2, values.next())?,
        )?,
        "std.data.json.object_put" => crate::put(
            receiver,
            &owned_value::<String>(operation, 1, "String", values.next())?,
            owned_json(operation, 2, values.next())?,
        )?,
        "std.data.json.object_remove" => crate::remove(
            receiver,
            &owned_value::<String>(operation, 1, "String", values.next())?,
        )?,
        _ => return Err(unknown(operation)),
    }
    Ok(())
}

fn owned_json(
    operation: &str,
    index: usize,
    value: Option<NativeResourceValue<Json>>,
) -> Result<Json, NativeAdapterError> {
    match value {
        Some(NativeResourceValue::Resource(value)) => Ok(value),
        _ => Err(type_error(operation, index, "Json")),
    }
}

fn owned_value<T: for<'a> FromNativeValue<'a>>(
    operation: &str,
    index: usize,
    expected: &str,
    value: Option<NativeResourceValue<Json>>,
) -> Result<T, NativeAdapterError> {
    match value {
        Some(NativeResourceValue::Value(value)) => {
            T::from_native(&value).map_err(|_| type_error(operation, index, expected))
        }
        _ => Err(type_error(operation, index, expected)),
    }
}

fn type_error(operation: &str, index: usize, expected: &str) -> NativeAdapterError {
    NativeAdapterError::new(
        "dispatch.type",
        format!("Operation `{operation}` argument {index} must be `{expected}`."),
        0,
    )
}

impl From<JsonError> for NativeAdapterError {
    fn from(error: JsonError) -> Self {
        Self::new(error.code(), error.message(), error.offset())
    }
}

fn validate(
    operation: &str,
    actual: usize,
) -> Result<&'static NativeResourceOperation, NativeAdapterError> {
    let contract = crate::operation(operation).ok_or_else(|| unknown(operation))?;
    if actual != contract.arity {
        return Err(NativeAdapterError::new(
            "dispatch.arity",
            format!(
                "Operation `{operation}` expects {} argument(s), got {actual}.",
                contract.arity
            ),
            0,
        ));
    }
    Ok(contract)
}

fn unknown(operation: &str) -> NativeAdapterError {
    NativeAdapterError::new(
        "dispatch.unknown_operation",
        format!("No NativeBoundary adapter is registered for `{operation}`."),
        0,
    )
}

struct Arguments<'a> {
    operation: &'a str,
    values: &'a [NativeResourceValue<&'a Json>],
}

impl<'a> Arguments<'a> {
    fn read<T: FromNativeValue<'a>>(
        &self,
        index: usize,
        expected: &str,
    ) -> Result<T, NativeAdapterError> {
        match self.values.get(index) {
            Some(NativeResourceValue::Value(value)) => {
                T::from_native(value).map_err(|_| self.type_error(index, expected))
            }
            _ => Err(self.type_error(index, expected)),
        }
    }

    fn json(&self, index: usize) -> Result<&'a Json, NativeAdapterError> {
        match self.values.get(index) {
            Some(NativeResourceValue::Resource(value)) => Ok(value),
            _ => Err(self.type_error(index, "Json")),
        }
    }

    fn type_error(&self, index: usize, expected: &str) -> NativeAdapterError {
        type_error(self.operation, index, expected)
    }
}

#[cfg(test)]
#[path = "invocation_test.rs"]
mod tests;
