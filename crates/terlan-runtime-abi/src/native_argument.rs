//! Borrowed, exact argument conversion shared by package-owned bindings.

use crate::{BoundaryError, ErrorDomain, NativeValue};

/// Reads a copied-value argument without coercion or retaining actor memory.
pub trait FromNativeValue<'a>: Sized {
    fn from_native(value: &'a NativeValue) -> Result<Self, BoundaryError>;
}

impl<'a> FromNativeValue<'a> for &'a str {
    fn from_native(value: &'a NativeValue) -> Result<Self, BoundaryError> {
        match value {
            NativeValue::String(value) => Ok(value),
            _ => Err(expected("String")),
        }
    }
}

impl FromNativeValue<'_> for i64 {
    fn from_native(value: &NativeValue) -> Result<Self, BoundaryError> {
        match value {
            NativeValue::Int(value) => Ok(*value),
            _ => Err(expected("Int")),
        }
    }
}

impl FromNativeValue<'_> for bool {
    fn from_native(value: &NativeValue) -> Result<Self, BoundaryError> {
        match value {
            NativeValue::Bool(value) => Ok(*value),
            _ => Err(expected("Bool")),
        }
    }
}

impl FromNativeValue<'_> for f64 {
    fn from_native(value: &NativeValue) -> Result<Self, BoundaryError> {
        match value {
            NativeValue::Float(value) => Ok(*value),
            _ => Err(expected("Float")),
        }
    }
}

impl FromNativeValue<'_> for String {
    fn from_native(value: &NativeValue) -> Result<Self, BoundaryError> {
        <&str>::from_native(value).map(str::to_owned)
    }
}

impl<'a> FromNativeValue<'a> for &'a [u8] {
    fn from_native(value: &'a NativeValue) -> Result<Self, BoundaryError> {
        match value {
            NativeValue::Bytes(value) => Ok(value),
            _ => Err(expected("Bytes")),
        }
    }
}

impl<'a, T: FromNativeValue<'a>> FromNativeValue<'a> for Vec<T> {
    fn from_native(value: &'a NativeValue) -> Result<Self, BoundaryError> {
        match value {
            NativeValue::List(values) => values.iter().map(T::from_native).collect(),
            _ => Err(expected("List")),
        }
    }
}

impl<'a, T: FromNativeValue<'a>> FromNativeValue<'a> for Option<T> {
    fn from_native(value: &'a NativeValue) -> Result<Self, BoundaryError> {
        match value {
            NativeValue::Record { name, fields } if name == "None" && fields.is_empty() => Ok(None),
            NativeValue::Record { name, fields } if name == "Some" => match fields.as_slice() {
                [(field, value)] if field == "value" => T::from_native(value).map(Some),
                _ => Err(expected("Option with one Some.value field")),
            },
            _ => Err(expected("Option")),
        }
    }
}

fn expected(expected: &str) -> BoundaryError {
    BoundaryError::message(
        ErrorDomain::NativeBoundary,
        "native package argument",
        format!("error[dispatch.type]: expected {expected}"),
    )
}

#[cfg(test)]
#[path = "native_argument_test.rs"]
mod tests;
