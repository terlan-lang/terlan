//! Canonical accelerator scalar types.

use serde::{Deserialize, Serialize};

use super::AcceleratorValueError;

/// Compiler-owned scalar representation used across native and accelerator values.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AcceleratorScalarType {
    /// Canonical one-byte Boolean representation.
    Bool,
    /// Unsigned 8-bit integer.
    U8,
    /// Signed 8-bit integer.
    I8,
    /// Unsigned 16-bit integer.
    U16,
    /// Signed 16-bit integer.
    I16,
    /// Unsigned 32-bit integer.
    U32,
    /// Signed 32-bit integer.
    I32,
    /// Unsigned 64-bit integer.
    U64,
    /// Signed 64-bit integer.
    I64,
    /// IEEE binary16 floating point.
    F16,
    /// Brain floating point with eight exponent bits.
    Bf16,
    /// IEEE binary32 floating point.
    F32,
    /// IEEE binary64 floating point.
    F64,
}

impl AcceleratorScalarType {
    /// Returns the stable descriptor identifier.
    pub const fn identifier(self) -> &'static str {
        match self {
            Self::Bool => "bool",
            Self::U8 => "u8",
            Self::I8 => "i8",
            Self::U16 => "u16",
            Self::I16 => "i16",
            Self::U32 => "u32",
            Self::I32 => "i32",
            Self::U64 => "u64",
            Self::I64 => "i64",
            Self::F16 => "f16",
            Self::Bf16 => "bf16",
            Self::F32 => "f32",
            Self::F64 => "f64",
        }
    }

    /// Returns the canonical in-memory width in bytes.
    pub const fn byte_width(self) -> u64 {
        match self {
            Self::Bool | Self::U8 | Self::I8 => 1,
            Self::U16 | Self::I16 | Self::F16 | Self::Bf16 => 2,
            Self::U32 | Self::I32 | Self::F32 => 4,
            Self::U64 | Self::I64 | Self::F64 => 8,
        }
    }

    /// Returns the minimum natural alignment in bytes.
    pub const fn alignment(self) -> u64 {
        self.byte_width()
    }

    /// Returns whether this scalar uses an integer representation.
    pub const fn is_integer(self) -> bool {
        matches!(
            self,
            Self::U8
                | Self::I8
                | Self::U16
                | Self::I16
                | Self::U32
                | Self::I32
                | Self::U64
                | Self::I64
        )
    }

    /// Returns whether this scalar uses a floating-point representation.
    pub const fn is_float(self) -> bool {
        matches!(self, Self::F16 | Self::Bf16 | Self::F32 | Self::F64)
    }
}

impl TryFrom<&str> for AcceleratorScalarType {
    type Error = AcceleratorValueError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "bool" => Ok(Self::Bool),
            "u8" => Ok(Self::U8),
            "i8" => Ok(Self::I8),
            "u16" => Ok(Self::U16),
            "i16" => Ok(Self::I16),
            "u32" => Ok(Self::U32),
            "i32" => Ok(Self::I32),
            "u64" => Ok(Self::U64),
            "i64" => Ok(Self::I64),
            "f16" => Ok(Self::F16),
            "bf16" => Ok(Self::Bf16),
            "f32" => Ok(Self::F32),
            "f64" => Ok(Self::F64),
            _ => Err(AcceleratorValueError::UnsupportedScalarType(
                value.to_string(),
            )),
        }
    }
}
