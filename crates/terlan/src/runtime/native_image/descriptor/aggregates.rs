//! Encoding and validation of managed aggregate image layouts.

use super::*;

/// Encodes the canonical ordered aggregate-layout table.
pub(super) fn encode_managed_layouts(
    layouts: &[TvmManagedLayoutDescriptor],
) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    push_u16_count(&mut bytes, layouts.len())?;
    for layout in layouts {
        bytes.extend_from_slice(&layout.semantic_id);
        push_u32(
            &mut bytes,
            u32::try_from(layout.encoded_layout.len()).map_err(|_| {
                "error[tvm.image.managed_layout_size]: managed layout exceeds u32".to_string()
            })?,
        );
        bytes.extend_from_slice(&layout.encoded_layout);
    }
    Ok(bytes)
}

/// Decodes the bounded aggregate-layout table before semantic validation.
pub(super) fn decode_managed_layouts(
    bytes: &[u8],
) -> Result<Vec<TvmManagedLayoutDescriptor>, String> {
    let mut reader = Reader::new(bytes);
    let count = reader.u16()? as usize;
    let mut layouts = Vec::with_capacity(count);
    for _ in 0..count {
        let semantic_id = reader.array()?;
        let length = reader.u32()? as usize;
        layouts.push(TvmManagedLayoutDescriptor {
            semantic_id,
            encoded_layout: reader.take(length)?.to_vec(),
        });
    }
    reader.finish()?;
    Ok(layouts)
}

/// Validates ordering, semantic ownership, and canonical aggregate bytes.
pub(super) fn validate_managed_layouts(
    layouts: &[TvmManagedLayoutDescriptor],
) -> Result<(), String> {
    for pair in layouts.windows(2) {
        let left = (&pair[0].semantic_id, pair[0].encoded_layout.as_slice());
        let right = (&pair[1].semantic_id, pair[1].encoded_layout.as_slice());
        if left >= right {
            return Err(
                "error[tvm.image.managed_layout_order]: managed layouts must be unique and ordered"
                    .to_string(),
            );
        }
    }
    let mut decoded_layouts = BTreeMap::new();
    for layout in layouts {
        let decoded = decode_aggregate_layout(&layout.encoded_layout)
            .map_err(|error| format!("error[tvm.image.managed_layout]: {error}"))?;
        if decoded.managed().semantic_id().bytes() != layout.semantic_id {
            return Err(
                "error[tvm.image.managed_layout_identity]: layout semantic identity mismatch"
                    .to_string(),
            );
        }
        let canonical = encode_aggregate_layout(&decoded)
            .map_err(|error| format!("error[tvm.image.managed_layout]: {error}"))?;
        if canonical != layout.encoded_layout {
            return Err(
                "error[tvm.image.managed_layout_canonical]: aggregate layout is not canonical"
                    .to_string(),
            );
        }
        decoded_layouts
            .entry(layout.semantic_id)
            .or_insert_with(Vec::new)
            .push(decoded);
    }
    for variants in decoded_layouts.values() {
        if variants.len() < 2 {
            continue;
        }
        let first = &variants[0];
        if first.kind() != ManagedAggregateKind::Constructor
            && variants.iter().all(|variant| variant == first)
        {
            continue;
        }
        if variants.iter().any(|variant| {
            variant.kind() != ManagedAggregateKind::Constructor
                || variant.canonical_type() != first.canonical_type()
                || variant.variant_count() != first.variant_count()
        }) {
            let layouts = variants
                .iter()
                .map(|variant| {
                    format!(
                        "{}:{:?}:{:?}/{:?}:fields={:?}",
                        variant.canonical_type(),
                        variant.kind(),
                        variant.discriminant(),
                        variant.variant_count(),
                        variant
                            .fields()
                            .iter()
                            .map(|field| (field.name(), field.field_type()))
                            .collect::<Vec<_>>()
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            return Err(format!(
                "error[tvm.image.managed_layout_family]: one semantic identity has incompatible aggregate layouts: {layouts}"
            ));
        }
        let mut names = BTreeSet::new();
        let mut discriminants = BTreeSet::new();
        for variant in variants {
            if !names.insert(variant.variant_name())
                || !discriminants.insert(variant.discriminant())
            {
                let layouts = variants
                    .iter()
                    .map(|variant| {
                        format!(
                            "{:?}:{:?}:fields={:?}",
                            variant.variant_name(),
                            variant.discriminant(),
                            variant
                                .fields()
                                .iter()
                                .map(|field| (field.name(), field.field_type()))
                                .collect::<Vec<_>>()
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                return Err(format!(
                    "error[tvm.image.managed_layout_variant]: constructor variants for `{}` must have unique names and discriminants: {layouts}",
                    first.canonical_type()
                ));
            }
        }
    }
    Ok(())
}
