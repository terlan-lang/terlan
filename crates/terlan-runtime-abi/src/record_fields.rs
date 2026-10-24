//! Shape-checked, consuming record projection for package-owned descriptors.

use crate::{DescriptorValue, NativeAdapterError, OwnedDescriptor};

/// Moves an exact record's fields into declaration order without cloning values.
/// Unknown, missing, repeated, and renamed fields fail closed. Record identity
/// and field names are ABI data; this operation grants no callback authority.
pub fn into_record_fields<V: DescriptorValue, const N: usize>(
    value: V,
    name: &str,
    names: [&str; N],
) -> Result<[V; N], NativeAdapterError> {
    let failure = || NativeAdapterError::new("native.record", "record shape mismatch", 0);
    let OwnedDescriptor::Record(actual, fields) = value.into_descriptor() else {
        return Err(failure());
    };
    if actual != name {
        return Err(failure());
    }
    let mut fields: [(String, V); N] = fields.try_into().map_err(|_| failure())?;
    for (index, name) in names.iter().enumerate() {
        if names[..index].contains(name) {
            return Err(failure());
        }
        let offset = fields[index..]
            .iter()
            .position(|(field, _)| field == name)
            .ok_or_else(failure)?;
        fields.swap(index, index + offset);
    }
    Ok(fields.map(|(_, value)| value))
}

/// Extracts a named record using one-to-one Rust field identifiers.
///
/// The expression is evaluated once, fields may arrive in any order, and all
/// payload allocations are moved. Field types remain the package's contract.
///
/// ```
/// use terlan_runtime_abi::{native_record, native_record_fields, NativeValue};
/// let text = String::from("hello");
/// let count = 3_i64;
/// let value = native_record!(Message, {text, count});
/// let [count, text] = native_record_fields!(value, Message {count, text}).unwrap();
/// assert_eq!(count, NativeValue::Int(3));
/// assert_eq!(text, NativeValue::String("hello".into()));
/// ```
///
/// Duplicate declarations are compilation errors, not ambiguous projections.
///
/// ```compile_fail
/// use terlan_runtime_abi::{native_record_fields, NativeValue};
/// let value = NativeValue::Unit;
/// let _ = native_record_fields!(value, Message {text, text});
/// ```
#[macro_export]
macro_rules! native_record_fields {
    ($value:expr, $name:ident { $($field:ident),* $(,)? }) => {{
        let __native_record_source = $value;
        let ($($field,)*) = ($(stringify!($field),)*);
        $crate::into_record_fields(__native_record_source, stringify!($name), [$($field),*])
    }};
}

#[cfg(test)]
#[path = "record_fields_test.rs"]
mod tests;
