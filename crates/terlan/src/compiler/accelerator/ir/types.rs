//! Scalar type contracts shared by accelerator lowering and verification.

use super::*;
use crate::compiler::typeck::CoreType;

/// Maps scalar CoreIR types to the canonical accelerator dtype model.
pub(super) fn lower_core_type(ty: &CoreType) -> Result<AcceleratorIrType, AcceleratorIrError> {
    match ty {
        CoreType::Int => Ok(scalar(AcceleratorScalarType::I64)),
        CoreType::Float | CoreType::Number => Ok(scalar(AcceleratorScalarType::F64)),
        CoreType::Bool => Ok(AcceleratorIrType::Bool),
        CoreType::Named(name) if name == "Unit" => Ok(AcceleratorIrType::Unit),
        _ => Err(AcceleratorIrError::UnsupportedType(format!("{ty:?}"))),
    }
}

/// Returns one canonical scalar IR type.
pub(super) fn scalar(dtype: AcceleratorScalarType) -> AcceleratorIrType {
    AcceleratorIrType::Scalar { dtype }
}

/// Returns a buffer element type after validating mutation access.
pub(super) fn buffer_element_type(
    ty: Option<&AcceleratorIrType>,
    write: bool,
    name: &str,
) -> Result<AcceleratorIrType, AcceleratorIrError> {
    let Some(AcceleratorIrType::Buffer { dtype, access, .. }) = ty else {
        return Err(AcceleratorIrError::UnsupportedType(name.to_string()));
    };
    if write && *access == AcceleratorIrAccess::Read {
        return Err(AcceleratorIrError::UnsupportedEffect(format!(
            "write to read-only buffer `{name}`"
        )));
    }
    if !write && *access == AcceleratorIrAccess::Write {
        return Err(AcceleratorIrError::UnsupportedEffect(format!(
            "read from write-only buffer `{name}`"
        )));
    }
    Ok(scalar(*dtype))
}

/// Requires an integer index type.
pub(super) fn require_integer(ty: &AcceleratorIrType) -> Result<(), AcceleratorIrError> {
    if matches!(
        ty,
        AcceleratorIrType::Scalar {
            dtype: AcceleratorScalarType::I8
                | AcceleratorScalarType::I16
                | AcceleratorScalarType::I32
                | AcceleratorScalarType::I64
                | AcceleratorScalarType::U8
                | AcceleratorScalarType::U16
                | AcceleratorScalarType::U32
                | AcceleratorScalarType::U64
        }
    ) {
        Ok(())
    } else {
        Err(AcceleratorIrError::TypeMismatch("buffer index".to_string()))
    }
}

/// Requires exact first-subset type equality.
pub(super) fn require_same_type(
    left: &AcceleratorIrType,
    right: &AcceleratorIrType,
    context: &str,
) -> Result<(), AcceleratorIrError> {
    if left == right {
        Ok(())
    } else {
        Err(AcceleratorIrError::TypeMismatch(context.to_string()))
    }
}
