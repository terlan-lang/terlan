use super::{
    ManagedMemoryError, SemanticTypeId, HEADER_BYTES, ITERATOR_NEXT, LIST_FIRST_OPTION,
    LIST_REST_OPTION, MAGIC, MAP_FROM_ENTRY_LIST, MAP_GET_OPTION, MAP_ITERATOR, MAP_TAKE,
    OPERATION_BYTES, OPTION_OPERATION_BYTES, SET_FROM_LIST, SET_ITERATOR, TRIPLE_OPERATION_BYTES,
    VERSION,
};

pub(super) fn operation(tag: u8, semantic: SemanticTypeId) -> Vec<u8> {
    operation_with_result(tag, semantic, true)
}

pub(super) fn operation_with_result(tag: u8, semantic: SemanticTypeId, reference: bool) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(OPERATION_BYTES);
    encoded.extend_from_slice(MAGIC);
    encoded.extend_from_slice(&VERSION.to_le_bytes());
    encoded.push(tag);
    encoded.push(u8::from(reference));
    encoded.extend_from_slice(&semantic.bytes());
    encoded
}

pub(super) fn option_operation(
    tag: u8,
    semantic: SemanticTypeId,
    option_semantic: SemanticTypeId,
) -> Vec<u8> {
    multi_semantic_operation(tag, &[semantic, option_semantic])
}

pub(super) fn multi_semantic_operation(tag: u8, semantics: &[SemanticTypeId]) -> Vec<u8> {
    let mut encoded = operation_with_result(tag, semantics[0], true);
    for semantic in &semantics[1..] {
        encoded.extend_from_slice(&semantic.bytes());
    }
    encoded
}

pub(super) fn decode(
    encoded: &[u8],
) -> Result<(u8, Vec<SemanticTypeId>, bool), ManagedMemoryError> {
    let semantic_count = match encoded.get(6).copied() {
        Some(
            LIST_FIRST_OPTION | LIST_REST_OPTION | MAP_GET_OPTION | SET_FROM_LIST | SET_ITERATOR,
        ) => 2,
        Some(MAP_TAKE | MAP_ITERATOR | MAP_FROM_ENTRY_LIST | ITERATOR_NEXT) => 3,
        Some(_) => 1,
        None => return Err(ManagedMemoryError::InvalidAggregateAbi),
    };
    let expected_bytes = match semantic_count {
        1 => OPERATION_BYTES,
        2 => OPTION_OPERATION_BYTES,
        3 => TRIPLE_OPERATION_BYTES,
        _ => unreachable!("closed collection semantic count"),
    };
    if encoded.len() != expected_bytes
        || encoded.get(..4) != Some(MAGIC)
        || encoded.get(4..6) != Some(&VERSION.to_le_bytes())
        || encoded[7] > 1
    {
        return Err(ManagedMemoryError::InvalidAggregateAbi);
    }
    let semantics = encoded[HEADER_BYTES..]
        .chunks_exact(16)
        .map(|bytes| {
            bytes
                .try_into()
                .map(SemanticTypeId::from_bytes)
                .map_err(|_| ManagedMemoryError::InvalidAggregateAbi)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok((encoded[6], semantics, encoded[7] == 1))
}
