//! Managed field projection across constructor variants.

use super::*;

/// Resolves one named field projection across every admitted physical variant.
pub(in super::super) fn managed_field_projection(
    base: NativeType,
    record_name: Option<&str>,
    field: &str,
    layouts: &NativeConstructorLayouts,
) -> Result<(Arc<[u8]>, NativeType), String> {
    let NativeType::ManagedRef(semantic) = base else {
        return Err(format!(
            "error[native_ir.field_base]: field `{field}` requires a managed aggregate"
        ));
    };
    if let Some(record_name) = record_name {
        let identifies_receiver = layouts
            .iter()
            .any(|((identity, _), layout)| identity == record_name && layout.result == base);
        if !identifies_receiver {
            return Err(format!(
                "error[native_ir.record_identity]: `{record_name}` does not identify the receiver type"
            ));
        }
    }
    let mut seen = HashSet::<Arc<[u8]>>::new();
    let mut projection = None;
    let mut found_layout = false;
    for layout in layouts.values().filter(|layout| layout.result == base) {
        if !seen.insert(layout.encoded_layout.clone()) {
            continue;
        }
        found_layout = true;
        let (index, descriptor) = layout
            .descriptor
            .fields()
            .iter()
            .enumerate()
            .find(|(_, descriptor)| descriptor.name() == Some(field))
            .ok_or_else(|| {
                format!(
                    "error[native_ir.field_missing]: field `{field}` is not present in every `{}` layout",
                    layout.descriptor.canonical_type()
                )
            })?;
        let field_type = native_field_type(descriptor.field_type())?;
        match projection {
            Some(expected) if expected != (index, field_type) => {
                return Err(format!(
                    "error[native_ir.field_ambiguous]: field `{field}` has incompatible physical layouts"
                ));
            }
            None => projection = Some((index, field_type)),
            _ => {}
        }
    }
    if !found_layout {
        return Err(format!(
            "error[native_ir.field_layout]: managed field `{field}` has no admitted layout"
        ));
    }
    let (index, field_type) = projection.ok_or_else(|| {
        format!("error[native_ir.field_missing]: managed field `{field}` has no physical slot")
    })?;
    let encoded = if field_type.is_managed_reference() {
        encode_aggregate_field_operation(semantic, index)
    } else {
        encode_aggregate_scalar_field_operation(semantic, index)
    }
    .map_err(|error| format!("error[native_ir.field_operation]: {error}"))?;
    Ok((Arc::from(encoded), field_type))
}
