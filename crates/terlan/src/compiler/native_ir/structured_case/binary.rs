//! Direct structured-case lowering for checked binary layout patterns.

use std::sync::Arc;

use crate::runtime::native_image::managed::{
    encode_binary_pattern_extract_operation, encode_binary_pattern_matches_operation,
    ManagedBinaryPatternEndian, ManagedBinaryPatternField,
};
use crate::terlan_typeck::{
    CoreBinaryPatternDescriptor, CoreBinaryPatternEndian, CoreBinaryPatternField, CoreType,
};

use super::{NativeExpr, NativeType, PatternBinding, PatternPlan};

pub(super) fn binary_plan(
    endian: CoreBinaryPatternEndian,
    fields: &[CoreBinaryPatternField],
    value: NativeExpr,
    value_type: NativeType,
) -> Result<PatternPlan, String> {
    if value_type != NativeType::BinaryRef {
        return Err("error[native_ir.binary_pattern_type]: pattern requires Binary".to_string());
    }
    let endian = match endian {
        CoreBinaryPatternEndian::Big => ManagedBinaryPatternEndian::Big,
        CoreBinaryPatternEndian::Little => ManagedBinaryPatternEndian::Little,
    };
    let descriptors = fields
        .iter()
        .map(|field| managed_binary_field(field.descriptor))
        .collect::<Vec<_>>();
    let predicate = encode_binary_pattern_matches_operation(endian, &descriptors)
        .map_err(|error| format!("error[native_ir.binary_pattern_layout]: {error}"))?;
    let bindings = fields
        .iter()
        .enumerate()
        .filter(|(_, field)| field.name != "_")
        .map(|(index, field)| {
            let encoded = encode_binary_pattern_extract_operation(endian, &descriptors, index)
                .map_err(|error| format!("error[native_ir.binary_pattern_layout]: {error}"))?;
            Ok(PatternBinding {
                name: field.name.clone(),
                value: NativeExpr::ManagedOperation {
                    encoded: Arc::from(encoded),
                    args: vec![value.clone()],
                },
                ty: binary_field_type(field.descriptor),
                core_ty: Some(binary_field_core_type(field.descriptor)),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(PatternPlan {
        predicate: NativeExpr::ManagedOperation {
            encoded: Arc::from(predicate),
            args: vec![value],
        },
        bindings,
    })
}

fn managed_binary_field(field: CoreBinaryPatternDescriptor) -> ManagedBinaryPatternField {
    match field {
        CoreBinaryPatternDescriptor::UInt(width) => ManagedBinaryPatternField::UInt(width),
        CoreBinaryPatternDescriptor::IntBits(width) => ManagedBinaryPatternField::Int(width),
        CoreBinaryPatternDescriptor::Bytes(width) => ManagedBinaryPatternField::Bytes(width),
        CoreBinaryPatternDescriptor::Bits(width) => ManagedBinaryPatternField::Bits(width),
        CoreBinaryPatternDescriptor::Utf8 => ManagedBinaryPatternField::Utf8,
        CoreBinaryPatternDescriptor::Utf16 => ManagedBinaryPatternField::Utf16,
        CoreBinaryPatternDescriptor::Utf32 => ManagedBinaryPatternField::Utf32,
        CoreBinaryPatternDescriptor::Rest => ManagedBinaryPatternField::Rest,
    }
}

fn binary_field_type(field: CoreBinaryPatternDescriptor) -> NativeType {
    match field {
        CoreBinaryPatternDescriptor::Bytes(_) | CoreBinaryPatternDescriptor::Rest => {
            NativeType::BytesRef
        }
        CoreBinaryPatternDescriptor::Bits(_) => NativeType::BinaryRef,
        CoreBinaryPatternDescriptor::UInt(_)
        | CoreBinaryPatternDescriptor::IntBits(_)
        | CoreBinaryPatternDescriptor::Utf8
        | CoreBinaryPatternDescriptor::Utf16
        | CoreBinaryPatternDescriptor::Utf32 => NativeType::Int,
    }
}

fn binary_field_core_type(field: CoreBinaryPatternDescriptor) -> CoreType {
    match field {
        CoreBinaryPatternDescriptor::Bytes(_) | CoreBinaryPatternDescriptor::Rest => {
            CoreType::Named("Bytes".to_string())
        }
        CoreBinaryPatternDescriptor::Bits(_) => CoreType::Named("BitString".to_string()),
        CoreBinaryPatternDescriptor::UInt(_)
        | CoreBinaryPatternDescriptor::IntBits(_)
        | CoreBinaryPatternDescriptor::Utf8
        | CoreBinaryPatternDescriptor::Utf16
        | CoreBinaryPatternDescriptor::Utf32 => CoreType::Int,
    }
}
