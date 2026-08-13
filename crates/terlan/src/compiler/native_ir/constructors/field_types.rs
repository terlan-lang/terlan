use crate::runtime::native_image::managed::{ManagedFieldType, SemanticTypeId};

use super::NativeType;

/// Resolves one managed physical field category into its NativeIR word kind.
pub(super) fn native_field_type(field: ManagedFieldType) -> Result<NativeType, String> {
    Ok(match field {
        ManagedFieldType::Unit => NativeType::Unit,
        ManagedFieldType::Bool => NativeType::Bool,
        ManagedFieldType::Int => NativeType::Int,
        ManagedFieldType::Float => NativeType::Float,
        ManagedFieldType::Atom => NativeType::Atom,
        ManagedFieldType::Reference(identity) if identity == semantic("std.core.String")? => {
            NativeType::StringRef
        }
        ManagedFieldType::Reference(identity) if identity == semantic("std.binary.Bytes")? => {
            NativeType::BytesRef
        }
        ManagedFieldType::Reference(identity) if identity == semantic("std.binary.Binary")? => {
            NativeType::BinaryRef
        }
        ManagedFieldType::Reference(identity) => NativeType::ManagedRef(identity),
    })
}

/// Converts one closed NativeIR value kind into the shared managed field kind.
pub(crate) fn managed_field_type(native: NativeType) -> Result<ManagedFieldType, String> {
    Ok(match native {
        NativeType::Unit => ManagedFieldType::Unit,
        NativeType::Int => ManagedFieldType::Int,
        NativeType::Float => ManagedFieldType::Float,
        NativeType::Bool => ManagedFieldType::Bool,
        NativeType::Atom => ManagedFieldType::Atom,
        NativeType::StringRef => ManagedFieldType::Reference(semantic("std.core.String")?),
        NativeType::BytesRef => ManagedFieldType::Reference(semantic("std.binary.Bytes")?),
        NativeType::BinaryRef => ManagedFieldType::Reference(semantic("std.binary.Binary")?),
        NativeType::ManagedRef(identity) => ManagedFieldType::Reference(identity),
    })
}

fn semantic(canonical: &str) -> Result<SemanticTypeId, String> {
    SemanticTypeId::from_canonical(canonical)
        .map_err(|error| format!("error[native_ir.constructor_type]: {error}"))
}
