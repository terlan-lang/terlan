//! Owned values exchanged with statically linked, safe native packages.

use crate::{BoundaryError, ErrorDomain};

/// Pointer-free package values. Resource ownership is intentionally not part
/// of this value-only interface.
#[derive(Clone, Debug, PartialEq)]
pub enum NativeValue {
    Unit,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Bytes(Vec<u8>),
    Atom(String),
    Tuple(Vec<Self>),
    List(Vec<Self>),
    Map(Vec<(Self, Self)>),
    Record {
        name: String,
        fields: Vec<(String, Self)>,
    },
}

impl From<String> for NativeValue {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<&str> for NativeValue {
    fn from(value: &str) -> Self {
        Self::String(value.to_owned())
    }
}

impl From<bool> for NativeValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<i64> for NativeValue {
    fn from(value: i64) -> Self {
        Self::Int(value)
    }
}

impl From<f64> for NativeValue {
    fn from(value: f64) -> Self {
        Self::Float(value)
    }
}

impl<T: Into<NativeValue>> From<Vec<T>> for NativeValue {
    fn from(values: Vec<T>) -> Self {
        Self::List(values.into_iter().map(Into::into).collect())
    }
}

macro_rules! tuple_conversion {
    ($($ty:ident : $field:tt),+ $(,)?) => {
        impl<$($ty: Into<NativeValue>),+> From<($($ty,)+)> for NativeValue {
            fn from(value: ($($ty,)+)) -> Self {
                Self::Tuple(vec![$(value.$field.into()),+])
            }
        }
    };
}

tuple_conversion!(A: 0);
tuple_conversion!(A: 0, B: 1);
tuple_conversion!(A: 0, B: 1, C: 2);
tuple_conversion!(A: 0, B: 1, C: 2, D: 3);
tuple_conversion!(A: 0, B: 1, C: 2, D: 3, E: 4);
tuple_conversion!(A: 0, B: 1, C: 2, D: 3, E: 4, F: 5);

impl<T: Into<NativeValue>> From<Option<T>> for NativeValue {
    fn from(value: Option<T>) -> Self {
        match value {
            Some(value) => Self::Record {
                name: "Some".into(),
                fields: vec![("value".into(), value.into())],
            },
            None => Self::Record {
                name: "None".into(),
                fields: Vec::new(),
            },
        }
    }
}

impl<T: Into<NativeValue>, E: Into<NativeValue>> From<Result<T, E>> for NativeValue {
    fn from(value: Result<T, E>) -> Self {
        let (name, field, value) = match value {
            Ok(value) => ("Ok", "value", value.into()),
            Err(reason) => ("Err", "reason", reason.into()),
        };
        Self::Record {
            name: name.into(),
            fields: vec![(field.into(), value)],
        }
    }
}

/// One package-owned operation. Callbacks validate their argument contract and
/// must not perform blocking I/O or retain actor memory.
pub struct NativeBinding {
    pub operation: &'static str,
    pub arity: usize,
    pub invoke: fn(&[NativeValue]) -> Result<NativeValue, BoundaryError>,
}

impl NativeBinding {
    /// Rejects wrong arity before argument conversion or package execution.
    pub fn validate_arity(&self, received: usize) -> Result<(), BoundaryError> {
        validate_native_arity(self.operation, self.arity, received)
    }
}

pub(crate) fn validate_native_arity(
    operation: &str,
    expected: usize,
    received: usize,
) -> Result<(), BoundaryError> {
    if received == expected {
        return Ok(());
    }
    Err(BoundaryError::message(
        ErrorDomain::NativeBoundary,
        "native package arguments",
        format!(
            "error[native_package.arguments]: operation `{operation}` expects {expected} arguments, received {received}"
        ),
    ))
}

#[cfg(test)]
#[path = "native_value_test.rs"]
mod tests;
