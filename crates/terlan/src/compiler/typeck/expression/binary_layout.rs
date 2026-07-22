use super::*;
use crate::terlan_typeck::binary_layout::{
    binary_layout_descriptor, vm_named_type, BinaryLayoutDescriptor, MAX_BYTE_SEGMENT_WIDTH,
    MAX_INTEGER_SEGMENT_WIDTH,
};

/// Typechecks the executable fixed-width binary constructor subset.
pub(super) fn infer_syntax_binary_layout(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    for field in &expr.fields {
        let descriptor_text = field.value.text.as_deref().unwrap_or_default();
        let Some(descriptor) = binary_layout_descriptor(descriptor_text) else {
            errors.push(format!(
                "invalid_binary_constructor_descriptor: field `{}` uses `{descriptor_text}`",
                field.key
            ));
            continue;
        };
        match descriptor {
            BinaryLayoutDescriptor::UInt(width) | BinaryLayoutDescriptor::IntBits(width) => {
                if !(1..=MAX_INTEGER_SEGMENT_WIDTH).contains(&width) {
                    errors.push(format!(
                        "invalid_binary_constructor_width: field `{}` integer width {width} must be between 1 and {MAX_INTEGER_SEGMENT_WIDTH}",
                        field.key
                    ));
                    continue;
                }
                let Some(field_type) = locals.get(&field.key) else {
                    errors.push(format!(
                        "unknown_binary_constructor_field: `{}` must name an in-scope value",
                        field.key
                    ));
                    continue;
                };
                if let Err(message) = unify(&Type::Int, field_type, subst) {
                    errors.push(format!(
                        "binary_constructor_field_type_mismatch: field `{}` expects Int: {message}",
                        field.key
                    ));
                }
            }
            BinaryLayoutDescriptor::Bytes(width) => {
                if !(1..=MAX_BYTE_SEGMENT_WIDTH).contains(&width) {
                    errors.push(format!(
                        "invalid_binary_constructor_width: field `{}` byte width {width} must be between 1 and {MAX_BYTE_SEGMENT_WIDTH}",
                        field.key
                    ));
                    continue;
                }
                let Some(field_type) = locals.get(&field.key) else {
                    errors.push(format!(
                        "unknown_binary_constructor_field: `{}` must name an in-scope value",
                        field.key
                    ));
                    continue;
                };
                let bytes_type = vm_named_type("std.vm.Bytes", "Bytes");
                if let Err(message) = unify(&bytes_type, field_type, subst) {
                    errors.push(format!(
                        "binary_constructor_field_type_mismatch: field `{}` expects std.vm.Bytes.Bytes: {message}",
                        field.key
                    ));
                }
            }
            BinaryLayoutDescriptor::Bits(width) => {
                if width < 1 {
                    errors.push(format!(
                        "invalid_binary_constructor_width: field `{}` bit width {width} must be positive",
                        field.key
                    ));
                    continue;
                }
                let Some(field_type) = locals.get(&field.key) else {
                    errors.push(format!(
                        "unknown_binary_constructor_field: `{}` must name an in-scope value",
                        field.key
                    ));
                    continue;
                };
                let bitstring_type = vm_named_type("std.vm.BitString", "BitString");
                if let Err(message) = unify(&bitstring_type, field_type, subst) {
                    errors.push(format!(
                        "binary_constructor_field_type_mismatch: field `{}` expects std.vm.BitString.BitString: {message}",
                        field.key
                    ));
                }
            }
            BinaryLayoutDescriptor::Utf8
            | BinaryLayoutDescriptor::Utf16
            | BinaryLayoutDescriptor::Utf32 => {
                let Some(field_type) = locals.get(&field.key) else {
                    errors.push(format!(
                        "unknown_binary_constructor_field: `{}` must name an in-scope value",
                        field.key
                    ));
                    continue;
                };
                if let Err(message) = unify(&Type::Int, field_type, subst) {
                    errors.push(format!(
                        "binary_constructor_field_type_mismatch: field `{}` expects Int Unicode scalar: {message}", field.key
                    ));
                }
            }
            BinaryLayoutDescriptor::Rest => {
                let Some(field_type) = locals.get(&field.key) else {
                    errors.push(format!(
                        "unknown_binary_constructor_field: `{}` must name an in-scope value",
                        field.key
                    ));
                    continue;
                };
                let bytes_type = vm_named_type("std.vm.Bytes", "Bytes");
                if let Err(message) = unify(&bytes_type, field_type, subst) {
                    errors.push(format!(
                        "binary_constructor_field_type_mismatch: field `{}` expects std.vm.Bytes.Bytes for terminal Rest: {message}",
                        field.key
                    ));
                }
            }
        }
    }

    vm_named_type("std.vm.BitString", "BitString")
}
