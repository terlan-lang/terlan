//! Managed aggregate metadata emitted from self-host ABI struct declarations.

#[path = "self_host_managed_layouts/pattern_shapes.rs"]
mod pattern_shapes;
use pattern_shapes::*;
#[path = "self_host_managed_layouts/pattern_predicates.rs"]
mod pattern_predicates;
use pattern_predicates::*;
#[path = "self_host_managed_layouts/pattern_bindings.rs"]
mod pattern_bindings;
use pattern_bindings::*;
#[path = "self_host_managed_layouts/pattern_environments.rs"]
mod pattern_environments;
#[cfg(test)]
use pattern_environments::{pattern_environment_expr, prepare_pattern_expr};
pub(super) use pattern_environments::{
    pattern_environment_expr_with_catalogs, prepare_pattern_expr_with_catalogs,
};
#[path = "self_host_managed_layouts/types.rs"]
mod types;
use types::*;
pub(super) use types::{encoded_collection_layouts, native_type};
#[path = "self_host_managed_layouts/aggregates.rs"]
mod aggregates;
use aggregates::*;
#[cfg(test)]
use aggregates::{aggregate_expr, collection_expr, managed_instruction_expr};
pub(super) use aggregates::{
    aggregate_field_expr, aggregate_record_expr, aggregate_replace_field_expr,
    managed_instruction_expr_with_variants, map_field_expr,
};

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::runtime::native_image::managed::{
    encode_aggregate_field_operation, encode_aggregate_layout,
    encode_aggregate_replace_field_operation, encode_aggregate_scalar_field_operation,
    encode_binary_literal, encode_collection_layout, encode_list_first_operation,
    encode_list_from_elements_operation, encode_list_is_empty_operation,
    encode_list_prepend_operation, encode_list_rest_operation, encode_managed_variant_is_operation,
    encode_map_contains_operation, encode_map_get_operation, encode_string_equal_operation,
    encode_string_literal, encode_string_pattern_extract_operation,
    encode_string_pattern_matches_operation, ManagedAggregateDescriptor,
    ManagedCollectionDescriptor, ManagedFieldType, ManagedStringCaptureKind as StringCaptureKind,
    ManagedStringPatternSegment as StringPatternSegment, SemanticTypeId,
};
use crate::self_host_ir::{BackendEnvelope, IrInstruction, IrStructField};

use super::{NativeBinaryOperator, NativeExpr, NativeIrError, NativeType};

/// Lowers application-wide ABI struct declarations into NativeIR image metadata.
pub(super) fn encoded_struct_layouts(
    envelopes: &[BackendEnvelope],
) -> Result<Vec<Arc<[u8]>>, NativeIrError> {
    let mut layouts = BTreeMap::<String, Arc<[u8]>>::new();
    for envelope in envelopes {
        for layout in &envelope.module.structs {
            let canonical = nominal_contract(&layout.name, &envelope.module.name)?;
            let fields = layout
                .fields
                .iter()
                .map(|field| lower_field(field, &envelope.module.name, &layout.name))
                .collect::<Result<Vec<_>, _>>()?;
            let descriptor =
                ManagedAggregateDescriptor::record(&canonical, fields).map_err(|error| {
                    format!(
                        "error[self_host.native.struct_layout]: layout {:?} is invalid: {error}",
                        layout.name
                    )
                })?;
            let encoded = encode_aggregate_layout(&descriptor).map_err(|error| {
                format!(
                    "error[self_host.native.struct_layout_abi]: layout {:?} cannot be encoded: {error}",
                    layout.name
                )
            })?;
            let encoded = Arc::<[u8]>::from(encoded);
            if let Some(previous) = layouts.insert(canonical.clone(), encoded.clone()) {
                if previous.as_ref() != encoded.as_ref() {
                    return Err((format!(
                        "error[self_host.native.conflicting_struct_layout]: semantic type {canonical:?} has multiple physical layouts"
                    )).into());
                }
            }
            for field in &layout.fields {
                if let Some(type_name) = field.type_name() {
                    inventory_tuple_layouts(
                        &parse_type(type_name)?,
                        &envelope.module.name,
                        &mut layouts,
                    )?;
                }
            }
        }
        for function in &envelope.module.functions {
            for parameter in &function.parameters {
                inventory_tuple_layouts(
                    &parse_type(&parameter.type_name)?,
                    &envelope.module.name,
                    &mut layouts,
                )?;
            }
            inventory_tuple_layouts(
                &parse_type(&function.return_type)?,
                &envelope.module.name,
                &mut layouts,
            )?;
            for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
                inventory_tuple_layouts(
                    &parse_type(&instruction.type_name)?,
                    &envelope.module.name,
                    &mut layouts,
                )?;
                if instruction.opcode == "prepare-pattern" {
                    let (_, encoded) = pattern_layout(instruction)?;
                    let canonical = pattern_canonical(instruction)?;
                    if let Some(previous) = layouts.insert(canonical.clone(), encoded.clone()) {
                        if previous.as_ref() != encoded.as_ref() {
                            return Err((format!(
                                "error[self_host.native.conflicting_pattern_layout]: semantic type {canonical:?} has multiple physical layouts"
                            )).into());
                        }
                    }
                }
            }
        }
    }
    let mut layouts = layouts.into_values().collect::<Vec<_>>();
    layouts.sort_by(|left, right| left.as_ref().cmp(right.as_ref()));
    Ok(layouts)
}

#[cfg(test)]
#[path = "self_host_managed_layouts_empty_array_test.rs"]
mod empty_array_tests;

pub(super) type StructPatternCatalog = std::collections::HashMap<
    String,
    std::collections::HashMap<String, super::self_host_adapter::LocalStructField>,
>;

#[derive(Clone, Copy)]
struct PatternCatalogs<'a> {
    variants: Option<&'a crate::self_host_variant_ir::VariantCatalog>,
    structs: Option<&'a StructPatternCatalog>,
}

enum DecodedStringPatternSegment {
    Literal(String),
    Capture(StringCaptureKind),
}

#[cfg(test)]
#[path = "self_host_managed_layouts_keyed_pattern_test.rs"]
mod keyed_pattern_tests;

#[cfg(test)]
#[path = "self_host_managed_layouts_string_pattern_test.rs"]
mod string_pattern_tests;

#[cfg(test)]
#[path = "self_host_managed_layouts_struct_pattern_test.rs"]
mod struct_pattern_tests;

#[cfg(test)]
#[path = "self_host_managed_layouts_string_capture_test.rs"]
mod string_capture_tests;

#[derive(Clone, Debug, Eq, PartialEq)]
struct AbiType {
    constructor: String,
    arguments: Vec<AbiType>,
}

#[cfg(test)]
#[path = "self_host_managed_layouts_inline_test.rs"]
mod tests;
