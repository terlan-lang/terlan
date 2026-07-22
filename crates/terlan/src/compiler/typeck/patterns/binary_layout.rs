use std::collections::HashMap;

use crate::terlan_syntax::SyntaxPatternOutput;

use crate::terlan_typeck::binary_layout::{
    binary_layout_descriptor, vm_named_type, BinaryLayoutDescriptor, MAX_BYTE_SEGMENT_WIDTH,
    MAX_INTEGER_SEGMENT_WIDTH,
};
use crate::terlan_typeck::{unify, Type, TypeVarId};

/// Checks one binary layout pattern and introduces its typed captures.
pub(super) fn check_binary_layout_pattern(
    pattern: &SyntaxPatternOutput,
    expected: &Type,
    locals: &mut HashMap<String, Type>,
    subst: &mut HashMap<TypeVarId, Type>,
) -> Result<(), String> {
    unify(
        expected,
        &vm_named_type("std.vm.BitString", "BitString"),
        subst,
    )?;
    for field in &pattern.fields {
        let descriptor_text = field.value.text.as_deref().unwrap_or_default();
        let descriptor = binary_layout_descriptor(descriptor_text).ok_or_else(|| {
            format!(
                "invalid_binary_pattern_descriptor: capture `{}` uses `{descriptor_text}`",
                field.key
            )
        })?;
        validate_descriptor_width(&field.key, descriptor)?;
        locals.insert(field.key.clone(), capture_type(descriptor));
    }
    Ok(())
}

fn validate_descriptor_width(
    capture: &str,
    descriptor: BinaryLayoutDescriptor,
) -> Result<(), String> {
    match descriptor {
        BinaryLayoutDescriptor::UInt(width) | BinaryLayoutDescriptor::IntBits(width)
            if !(1..=MAX_INTEGER_SEGMENT_WIDTH).contains(&width) =>
        {
            Err(format!(
                "invalid_binary_pattern_width: capture `{capture}` integer width {width} must be between 1 and {MAX_INTEGER_SEGMENT_WIDTH}"
            ))
        }
        BinaryLayoutDescriptor::Bytes(width)
            if !(1..=MAX_BYTE_SEGMENT_WIDTH).contains(&width) =>
        {
            Err(format!(
                "invalid_binary_pattern_width: capture `{capture}` byte width {width} must be between 1 and {MAX_BYTE_SEGMENT_WIDTH}"
            ))
        }
        BinaryLayoutDescriptor::Bits(width) if width < 1 => Err(format!(
            "invalid_binary_pattern_width: capture `{capture}` bit width {width} must be positive"
        )),
        _ => Ok(()),
    }
}

fn capture_type(descriptor: BinaryLayoutDescriptor) -> Type {
    match descriptor {
        BinaryLayoutDescriptor::UInt(_)
        | BinaryLayoutDescriptor::IntBits(_)
        | BinaryLayoutDescriptor::Utf8
        | BinaryLayoutDescriptor::Utf16
        | BinaryLayoutDescriptor::Utf32 => Type::Int,
        BinaryLayoutDescriptor::Bytes(_) | BinaryLayoutDescriptor::Rest => {
            vm_named_type("std.vm.Bytes", "Bytes")
        }
        BinaryLayoutDescriptor::Bits(_) => vm_named_type("std.vm.BitString", "BitString"),
    }
}
