//! Typed finite-sum metadata bridge for self-hosted native lowering.

use crate::self_host_variant_ir::{ResolvedVariantLayout, VariantCatalog, VariantResolutionError};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManagedVariantDescriptor {
    pub canonical_type: String,
    pub owner: String,
    pub constructor: String,
    pub discriminant: u32,
    pub variant_count: u32,
    pub payload_types: Vec<Option<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VariantPayloadProjection {
    pub index: u32,
    pub type_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VariantPatternPlan {
    pub descriptor: ManagedVariantDescriptor,
    pub projections: Vec<VariantPayloadProjection>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VariantConstructionPlan {
    pub descriptor: ManagedVariantDescriptor,
    pub operand_types: Vec<String>,
}

fn type_base(value: &str) -> &str {
    let value = value.trim();
    let base = value.split_once('[').map_or(value, |(base, _)| base.trim());
    base.rsplit('.').next().unwrap_or(base)
}

fn descriptor(subject_type: &str, resolved: ResolvedVariantLayout) -> ManagedVariantDescriptor {
    ManagedVariantDescriptor {
        canonical_type: subject_type.trim().to_owned(),
        owner: resolved.owner,
        constructor: resolved.name,
        discriminant: resolved.discriminant,
        variant_count: resolved.variant_count,
        payload_types: resolved.payload_types,
    }
}

/// Resolves constructor-pattern tests and payload projections from subject evidence.
pub fn resolve_pattern(
    catalog: &VariantCatalog,
    constructor: &str,
    subject_type: &str,
) -> Result<ManagedVariantDescriptor, VariantResolutionError> {
    let owner = type_base(subject_type);
    catalog
        .resolve(owner, constructor, subject_type)
        .map(|resolved| descriptor(subject_type, resolved))
}

/// Resolves aggregate construction from the expression's declared result type.
pub fn resolve_construction(
    catalog: &VariantCatalog,
    constructor: &str,
    result_type: &str,
) -> Result<ManagedVariantDescriptor, VariantResolutionError> {
    let owner = type_base(result_type);
    catalog
        .resolve(owner, constructor, result_type)
        .map(|resolved| descriptor(result_type, resolved))
}

fn concrete_payload_types(
    descriptor: &ManagedVariantDescriptor,
) -> Result<Vec<String>, VariantResolutionError> {
    descriptor
        .payload_types
        .iter()
        .enumerate()
        .map(|(index, type_name)| {
            type_name.clone().ok_or_else(|| {
                VariantResolutionError::from(format!(
                    "variant `{}.{}` payload {index} has no authoritative type",
                    descriptor.owner, descriptor.constructor
                ))
            })
        })
        .collect()
}

/// Plans a discriminant test followed by typed payload projections.
pub fn plan_pattern(
    catalog: &VariantCatalog,
    constructor: &str,
    subject_type: &str,
    payload_arity: usize,
) -> Result<VariantPatternPlan, VariantResolutionError> {
    let descriptor = resolve_pattern(catalog, constructor, subject_type)?;
    let payload_types = concrete_payload_types(&descriptor)?;
    if payload_types.len() != payload_arity {
        return Err((format!(
            "variant pattern `{}.{}` binds {payload_arity} payloads but its layout requires {}",
            descriptor.owner,
            descriptor.constructor,
            payload_types.len()
        ))
        .into());
    }
    let projections = payload_types
        .into_iter()
        .enumerate()
        .map(|(index, type_name)| VariantPayloadProjection {
            index: index as u32,
            type_name,
        })
        .collect();
    Ok(VariantPatternPlan {
        descriptor,
        projections,
    })
}

/// Plans constructor assembly only when every typed operand matches its layout slot.
pub fn plan_construction(
    catalog: &VariantCatalog,
    constructor: &str,
    result_type: &str,
    operand_types: &[Option<String>],
) -> Result<VariantConstructionPlan, VariantResolutionError> {
    let descriptor = resolve_construction(catalog, constructor, result_type)?;
    let expected = concrete_payload_types(&descriptor)?;
    if expected.len() != operand_types.len() {
        return Err((format!(
            "variant construction `{}.{}` supplies {} payloads but its layout requires {}",
            descriptor.owner,
            descriptor.constructor,
            operand_types.len(),
            expected.len()
        ))
        .into());
    }
    for (index, (expected_type, actual_type)) in
        expected.iter().zip(operand_types.iter()).enumerate()
    {
        let Some(actual_type) = actual_type else {
            return Err((format!(
                "variant construction `{}.{}` payload {index} has no typed operand evidence",
                descriptor.owner, descriptor.constructor
            ))
            .into());
        };
        if actual_type.trim() != expected_type {
            return Err((format!(
                "variant construction `{}.{}` payload {index} has type `{}`; expected `{expected_type}`",
                descriptor.owner,
                descriptor.constructor,
                actual_type.trim()
            )).into());
        }
    }
    Ok(VariantConstructionPlan {
        descriptor,
        operand_types: expected,
    })
}
