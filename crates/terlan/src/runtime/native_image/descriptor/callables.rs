//! Canonical callable-table codec and admission checks.

use super::codec::*;
use super::{TvmBoundaryType, TvmCallableDescriptor, TvmExecutableDescriptor};

/// Encodes the closed image-local callable dispatch table.
pub(super) fn encode_callables(callables: &[TvmCallableDescriptor]) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    push_u16_count(&mut bytes, callables.len())?;
    for callable in callables {
        push_u64(&mut bytes, callable.id);
        for types in [&callable.parameters, &callable.results, &callable.captures] {
            push_u16_count(&mut bytes, types.len())?;
            for boundary_type in types {
                encode_boundary_type(&mut bytes, boundary_type);
            }
        }
    }
    Ok(bytes)
}

/// Decodes the closed image-local callable dispatch table.
pub(super) fn decode_callables(bytes: &[u8]) -> Result<Vec<TvmCallableDescriptor>, String> {
    let mut reader = Reader::new(bytes);
    let count = reader.u16()? as usize;
    let mut callables = Vec::with_capacity(count);
    for _ in 0..count {
        let id = reader.u64()?;
        let mut decode_types = || -> Result<Vec<TvmBoundaryType>, String> {
            let count = reader.u16()? as usize;
            (0..count)
                .map(|_| decode_boundary_type(&mut reader))
                .collect()
        };
        callables.push(TvmCallableDescriptor {
            id,
            parameters: decode_types()?,
            results: decode_types()?,
            captures: decode_types()?,
        });
    }
    reader.finish()?;
    Ok(callables)
}

/// Validates callable signatures against image membership and resource ownership.
pub(super) fn validate_callables(descriptor: &TvmExecutableDescriptor) -> Result<(), String> {
    for callable in &descriptor.callables {
        if callable.results.len() != 1 {
            return Err(
                "error[tvm.image.callable_result]: format 1 requires exactly one callable result"
                    .to_string(),
            );
        }
        if descriptor
            .continuations
            .iter()
            .any(|continuation| continuation.id == callable.id)
        {
            return Err(
                "error[tvm.image.callable_collision]: callable and continuation IDs must be disjoint"
                    .to_string(),
            );
        }
        if let Some(export) = descriptor
            .exports
            .iter()
            .find(|export| export.id == callable.id)
        {
            if export.parameters != callable.parameters || export.results != callable.results {
                return Err(
                    "error[tvm.image.callable_export_signature]: callable and export signatures differ"
                        .to_string(),
                );
            }
        }
        for boundary_type in callable
            .parameters
            .iter()
            .chain(&callable.results)
            .chain(&callable.captures)
        {
            if matches!(boundary_type, TvmBoundaryType::Json) {
                return Err(
                    "error[tvm.image.callable_json]: callable closure state cannot use untraced JSON"
                        .to_string(),
                );
            }
            if let TvmBoundaryType::NativeResource(type_id) = boundary_type {
                if !descriptor
                    .resources
                    .iter()
                    .any(|resource| resource.type_id == *type_id)
                {
                    return Err(format!(
                        "error[tvm.image.resource_reference]: callable {} references undeclared resource {type_id}",
                        callable.id
                    ));
                }
            }
        }
    }
    Ok(())
}
