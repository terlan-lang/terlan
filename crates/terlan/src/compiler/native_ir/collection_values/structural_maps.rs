//! Checked anonymous records, with source evaluation order independent of layout.

use std::collections::HashMap;
use std::sync::Arc;

use crate::runtime::native_image::managed::encode_aggregate_layout;
use crate::terlan_typeck::{CoreMapExprField, CoreMapTypeField};

use super::{lower_typed_value, NativeConstructorLayouts, NativeExpr, NativeType};

pub(super) fn lower(
    fields: &[CoreMapExprField],
    field_types: &[CoreMapTypeField],
    params: &HashMap<String, usize>,
    param_types: &HashMap<String, NativeType>,
    functions: &HashMap<(String, usize), usize>,
    function_types: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
) -> super::super::NativeIrResult<NativeExpr> {
    let descriptor = Arc::new(super::super::aggregate_types::map_record_descriptor(
        field_types,
    )?);
    if fields.len() != field_types.len() {
        return Err("error[native_ir.map_record_value]: field count mismatch".into());
    }
    let encoded_layout = Arc::from(
        encode_aggregate_layout(&descriptor)
            .map_err(|error| format!("error[native_ir.map_record_value_abi]: {error}"))?,
    );
    let first_slot = params.values().copied().max().map_or(0, |slot| slot + 1);
    let mut locals = params.clone();
    let mut bindings = Vec::with_capacity(fields.len());
    let mut slots = HashMap::new();
    for (index, field) in fields.iter().enumerate() {
        let ty = field_types
            .iter()
            .find(|ty| ty.key == field.key)
            .ok_or_else(|| {
                format!(
                    "error[native_ir.map_record_value]: unexpected field `{}`",
                    field.key
                )
            })?;
        if slots
            .insert(field.key.as_str(), first_slot + index)
            .is_some()
        {
            return Err(format!(
                "error[native_ir.map_record_value]: duplicate field `{}`",
                field.key
            )
            .into());
        }
        bindings.push(lower_typed_value(
            &field.value,
            &ty.value,
            &locals,
            param_types,
            functions,
            function_types,
            constructors,
        )?);
        // Reserve preceding bindings when nested field expressions allocate locals.
        locals.insert(
            format!("$map_field_{first_slot}_{index}"),
            first_slot + index,
        );
    }
    let fields = field_types
        .iter()
        .map(|ty| NativeExpr::Param(slots[ty.key.as_str()]))
        .collect();
    Ok(NativeExpr::Let {
        bindings,
        body: Box::new(NativeExpr::Construct {
            descriptor,
            encoded_layout,
            fields,
        }),
    })
}
