use super::*;
use terlan_runtime_abi::{BoundaryError, ErrorDomain};

fn collection_error(rendered: impl Into<String>) -> BoundaryError {
    BoundaryError::message(
        ErrorDomain::NativeImageAdmission,
        "encode or decode native-image collections",
        rendered,
    )
}

/// Encodes the canonical ordered managed-collection schema table.
pub(super) fn encode_managed_collections(
    layouts: &[TvmManagedCollectionDescriptor],
) -> Result<Vec<u8>, BoundaryError> {
    let mut bytes = Vec::new();
    push_u16_count(&mut bytes, layouts.len()).map_err(collection_error)?;
    for layout in layouts {
        bytes.extend_from_slice(&layout.semantic_id);
        push_u32(
            &mut bytes,
            u32::try_from(layout.encoded_layout.len()).map_err(|_| {
                collection_error(
                    "error[tvm.image.managed_collection_size]: collection schema exceeds u32",
                )
            })?,
        );
        bytes.extend_from_slice(&layout.encoded_layout);
    }
    Ok(bytes)
}

/// Decodes the bounded managed-collection schema table before validation.
pub(super) fn decode_managed_collections(
    bytes: &[u8],
) -> Result<Vec<TvmManagedCollectionDescriptor>, BoundaryError> {
    let mut reader = Reader::new(bytes);
    let count = reader.u16().map_err(collection_error)? as usize;
    let mut layouts = Vec::with_capacity(count);
    for _ in 0..count {
        let semantic_id = reader.array().map_err(collection_error)?;
        let length = reader.u32().map_err(collection_error)? as usize;
        layouts.push(TvmManagedCollectionDescriptor {
            semantic_id,
            encoded_layout: reader.take(length).map_err(collection_error)?.to_vec(),
        });
    }
    reader.finish().map_err(collection_error)?;
    Ok(layouts)
}

/// Validates collection ordering, semantic ownership, and canonical bytes.
pub(super) fn validate_managed_collections(
    layouts: &[TvmManagedCollectionDescriptor],
) -> Result<(), BoundaryError> {
    for pair in layouts.windows(2) {
        let left = (&pair[0].semantic_id, pair[0].encoded_layout.as_slice());
        let right = (&pair[1].semantic_id, pair[1].encoded_layout.as_slice());
        if left >= right {
            return Err(collection_error(
                "error[tvm.image.managed_collection_order]: collection schemas must be unique and ordered"
            ));
        }
    }
    for layout in layouts {
        let decoded = decode_collection_layout(&layout.encoded_layout).map_err(|error| {
            collection_error(format!("error[tvm.image.managed_collection]: {error}"))
        })?;
        if decoded.semantic_id().bytes() != layout.semantic_id {
            return Err(collection_error(
                "error[tvm.image.managed_collection_identity]: collection semantic identity mismatch"
            ));
        }
        let canonical = encode_collection_layout(&decoded).map_err(|error| {
            collection_error(format!("error[tvm.image.managed_collection]: {error}"))
        })?;
        if canonical != layout.encoded_layout {
            return Err(collection_error(
                "error[tvm.image.managed_collection_canonical]: collection schema is not canonical",
            ));
        }
    }
    Ok(())
}

/// Encodes one canonical bounded list of UTF-8 identities.
pub(super) fn encode_text_list(values: &[String]) -> Result<Vec<u8>, BoundaryError> {
    let mut bytes = Vec::new();
    push_u16_count(&mut bytes, values.len()).map_err(collection_error)?;
    for value in values {
        push_text(&mut bytes, value).map_err(collection_error)?;
    }
    Ok(bytes)
}

/// Decodes one bounded list of UTF-8 identities.
pub(super) fn decode_text_list(bytes: &[u8]) -> Result<Vec<String>, BoundaryError> {
    let mut reader = Reader::new(bytes);
    let count = reader.u16().map_err(collection_error)? as usize;
    let values = (0..count)
        .map(|_| reader.text().map_err(collection_error))
        .collect::<Result<Vec<_>, _>>()?;
    reader.finish().map_err(collection_error)?;
    Ok(values)
}

/// Validates canonical ordering through the shared finite atom-table type.
pub(super) fn validate_atoms(atoms: &[String]) -> Result<(), BoundaryError> {
    let table = AtomTable::new(atoms.iter().cloned())
        .map_err(|error| collection_error(format!("error[tvm.image.atoms]: {error}")))?;
    let canonical = table.identities().collect::<Vec<_>>();
    if canonical.len() != atoms.len()
        || canonical
            .iter()
            .zip(atoms)
            .any(|(canonical, actual)| *canonical != actual)
    {
        return Err(collection_error(
            "error[tvm.image.atom_order]: atom identities must be unique and ordered",
        ));
    }
    Ok(())
}
