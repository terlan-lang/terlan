/// Stable NativeBoundary value tags shared by generated code and the VM.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TvmBoundaryType {
    Unit,
    Bool,
    Int,
    Float,
    Binary,
    String,
    Json,
    NativeResource(u64),
    Atom,
    Bytes,
    Managed([u8; 16]),
}

impl TvmBoundaryType {
    /// Reports whether this value is an actor-local managed-heap reference.
    pub fn is_managed_reference(&self) -> bool {
        matches!(
            self,
            Self::Binary | Self::String | Self::Bytes | Self::Managed(_)
        )
    }

    /// Encodes this boundary identity into the fixed typed-transition header.
    pub fn transition_words(&self) -> [i64; 3] {
        match self {
            Self::Unit => [0, 0, 0],
            Self::Bool => [1, 0, 0],
            Self::Int => [2, 0, 0],
            Self::Float => [3, 0, 0],
            Self::Binary => [4, 0, 0],
            Self::String => [5, 0, 0],
            Self::Json => [6, 0, 0],
            Self::NativeResource(id) => [7, *id as i64, 0],
            Self::Atom => [8, 0, 0],
            Self::Bytes => [9, 0, 0],
            Self::Managed(identity) => {
                let low = u64::from_le_bytes(identity[..8].try_into().expect("fixed low word"));
                let high = u64::from_le_bytes(identity[8..].try_into().expect("fixed high word"));
                [10, low as i64, high as i64]
            }
        }
    }

    /// Decodes and validates one fixed typed-transition header.
    pub fn from_transition_words(words: &[i64]) -> Result<Self, BoundaryError> {
        let [tag, low, high] = words else {
            return Err(decode_error(format!(
                "expected 3 type words, received {}",
                words.len()
            )));
        };
        let scalar = |value| {
            if *low == 0 && *high == 0 {
                Ok(value)
            } else {
                Err(decode_error("scalar type has nonzero identity words"))
            }
        };
        match *tag {
            0 => scalar(Self::Unit),
            1 => scalar(Self::Bool),
            2 => scalar(Self::Int),
            3 => scalar(Self::Float),
            4 => scalar(Self::Binary),
            5 => scalar(Self::String),
            6 => scalar(Self::Json),
            7 if *high == 0 => Ok(Self::NativeResource(*low as u64)),
            7 => Err(decode_error("native resource has a nonzero high word")),
            8 => scalar(Self::Atom),
            9 => scalar(Self::Bytes),
            10 => {
                let mut identity = [0_u8; 16];
                identity[..8].copy_from_slice(&(*low as u64).to_le_bytes());
                identity[8..].copy_from_slice(&(*high as u64).to_le_bytes());
                Ok(Self::Managed(identity))
            }
            tag => Err(decode_error(format!("unknown type tag {tag}"))),
        }
    }
}

fn decode_error(context: impl std::fmt::Display) -> BoundaryError {
    BoundaryError::message(
        ErrorDomain::NativeBoundary,
        "decode typed transition header",
        format!("error[tvm.transition.boundary_type]: {context}"),
    )
}

#[cfg(test)]
#[path = "boundary_type_test.rs"]
mod boundary_type_test;
use crate::{BoundaryError, ErrorDomain};
