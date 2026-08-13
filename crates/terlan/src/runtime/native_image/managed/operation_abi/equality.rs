//! Schema-directed structural equality for actor-owned managed values.

use std::collections::HashSet;

use super::super::{
    ActorHeap, ManagedAggregate, ManagedBinary, ManagedBytes, ManagedCollectionKind,
    ManagedFieldType, ManagedFieldValue, ManagedLayoutRegistry, ManagedList, ManagedMap,
    ManagedMemoryError, ManagedSet, ManagedString, SemanticTypeId, TvmRef,
};
use super::reference_word;

const MAGIC: &[u8; 4] = b"TVME";
const VERSION: u16 = 1;
const EQUAL: u8 = 1;
const HEADER_BYTES: usize = 8;
const OPERATION_BYTES: usize = HEADER_BYTES + 16;

/// Encodes structural equality for one admitted managed semantic type.
pub fn encode_managed_value_equal_operation(semantic: SemanticTypeId) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(OPERATION_BYTES);
    encoded.extend_from_slice(MAGIC);
    encoded.extend_from_slice(&VERSION.to_le_bytes());
    encoded.push(EQUAL);
    encoded.push(0);
    encoded.extend_from_slice(&semantic.bytes());
    encoded
}

/// Reports whether bytes identify the structural managed-equality ABI.
pub(super) fn is_equality_operation(encoded: &[u8]) -> bool {
    encoded.starts_with(MAGIC)
}

/// Executes one checked structural equality operation.
pub(super) fn execute_equality_operation(
    heap: &ActorHeap,
    layouts: &ManagedLayoutRegistry,
    encoded: &[u8],
    words: &[i64],
) -> Result<u64, ManagedMemoryError> {
    let semantic = decode(encoded)?;
    let [left, right] = words else {
        return Err(ManagedMemoryError::InvalidAggregateArity);
    };
    let left_immediate = is_immediate_union_word(*left);
    let right_immediate = is_immediate_union_word(*right);
    if left_immediate || right_immediate {
        return Ok(u64::from(
            left_immediate && right_immediate && left == right,
        ));
    }
    let left = reference_word(*left)?;
    let right = reference_word(*right)?;
    let mut visited = HashSet::new();
    references_equal(heap, layouts, semantic, left, right, &mut visited).map(u64::from)
}

/// Distinguishes zero-field union atoms from token-tagged managed references.
///
/// Equality reaches this ABI only after the compiler has proved a managed
/// semantic type. Within that type, a word whose token half is zero is the
/// compact atom representation of a zero-field variant. Managed references
/// always carry a nonzero heap token in the upper 32 bits.
fn is_immediate_union_word(word: i64) -> bool {
    u64::from_ne_bytes(word.to_ne_bytes()) >> 32 == 0
}

/// Decodes one exact structural equality operation.
fn decode(encoded: &[u8]) -> Result<SemanticTypeId, ManagedMemoryError> {
    if encoded.len() != OPERATION_BYTES
        || encoded.get(..4) != Some(MAGIC)
        || encoded.get(4..6) != Some(&VERSION.to_le_bytes())
        || encoded[6] != EQUAL
        || encoded[7] != 0
    {
        return Err(ManagedMemoryError::InvalidAggregateAbi);
    }
    encoded[HEADER_BYTES..]
        .try_into()
        .map(SemanticTypeId::from_bytes)
        .map_err(|_| ManagedMemoryError::InvalidAggregateAbi)
}

/// Compares two references using their admitted semantic descriptor.
fn references_equal(
    heap: &ActorHeap,
    layouts: &ManagedLayoutRegistry,
    semantic: SemanticTypeId,
    left: TvmRef<()>,
    right: TvmRef<()>,
    visited: &mut HashSet<(u64, u64, SemanticTypeId)>,
) -> Result<bool, ManagedMemoryError> {
    require_semantic(heap, semantic, left)?;
    require_semantic(heap, semantic, right)?;
    if left == right {
        return Ok(true);
    }
    let pair = ordered_pair(left.encoded_abi_word(), right.encoded_abi_word());
    if !visited.insert((pair.0, pair.1, semantic)) {
        return Ok(true);
    }

    if semantic == SemanticTypeId::from_canonical("std.core.String")? {
        return Ok(heap.read_string(left.cast::<ManagedString>())?
            == heap.read_string(right.cast::<ManagedString>())?);
    }
    if semantic == SemanticTypeId::from_canonical("std.binary.Bytes")? {
        return Ok(heap.read_bytes(left.cast::<ManagedBytes>())?
            == heap.read_bytes(right.cast::<ManagedBytes>())?);
    }
    if semantic == SemanticTypeId::from_canonical("std.binary.Binary")? {
        return Ok(heap.read_binary(left.cast::<ManagedBinary>())?
            == heap.read_binary(right.cast::<ManagedBinary>())?);
    }
    if let Some(collection) = layouts.collection(semantic) {
        return match collection.kind() {
            ManagedCollectionKind::List => {
                let descriptor = collection
                    .list_descriptor()
                    .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
                let left = heap.list_elements(descriptor, left.cast::<ManagedList>())?;
                let right = heap.list_elements(descriptor, right.cast::<ManagedList>())?;
                ordered_values_equal(
                    heap,
                    layouts,
                    descriptor.element_type(),
                    &left,
                    &right,
                    visited,
                )
            }
            ManagedCollectionKind::Map => {
                let descriptor = collection
                    .map_descriptor()
                    .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
                let left = heap.map_entries(descriptor, left.cast::<ManagedMap>())?;
                let right = heap.map_entries(descriptor, right.cast::<ManagedMap>())?;
                map_entries_equal(heap, layouts, descriptor, &left, &right, visited)
            }
            ManagedCollectionKind::Set => {
                let descriptor = collection
                    .set_descriptor()
                    .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
                let left = heap.set_elements(descriptor, left.cast::<ManagedSet>())?;
                let right = heap.set_elements(descriptor, right.cast::<ManagedSet>())?;
                unordered_values_equal(
                    heap,
                    layouts,
                    descriptor.element_type(),
                    &left,
                    &right,
                    visited,
                )
            }
        };
    }

    let left_layout = aggregate_layout(heap, layouts, semantic, left)?;
    let right_layout = aggregate_layout(heap, layouts, semantic, right)?;
    if left_layout.managed().fingerprint() != right_layout.managed().fingerprint() {
        return Ok(false);
    }
    let left_view = heap.read_aggregate(left.cast::<ManagedAggregate>(), &left_layout)?;
    let right_view = heap.read_aggregate(right.cast::<ManagedAggregate>(), &right_layout)?;
    if left_view.discriminant() != right_view.discriminant() {
        return Ok(false);
    }
    for (index, field) in left_layout.fields().iter().enumerate() {
        if !field_values_equal(
            heap,
            layouts,
            field.field_type(),
            left_view.field(index)?,
            right_view.field(index)?,
            visited,
        )? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Validates that one reference belongs to the operation's semantic type.
fn require_semantic(
    heap: &ActorHeap,
    semantic: SemanticTypeId,
    reference: TvmRef<()>,
) -> Result<(), ManagedMemoryError> {
    (heap.descriptor(reference)?.semantic_id() == semantic)
        .then_some(())
        .ok_or(ManagedMemoryError::ManagedTypeMismatch)
}

/// Resolves one admitted aggregate layout by live object fingerprint.
fn aggregate_layout(
    heap: &ActorHeap,
    layouts: &ManagedLayoutRegistry,
    semantic: SemanticTypeId,
    reference: TvmRef<()>,
) -> Result<std::sync::Arc<super::super::ManagedAggregateDescriptor>, ManagedMemoryError> {
    let fingerprint = heap.descriptor(reference)?.fingerprint();
    layouts
        .layouts(semantic)
        .iter()
        .find(|layout| layout.managed().fingerprint() == fingerprint)
        .cloned()
        .ok_or(ManagedMemoryError::ManagedTypeMismatch)
}

/// Compares two typed managed fields recursively when they hold references.
fn field_values_equal(
    heap: &ActorHeap,
    layouts: &ManagedLayoutRegistry,
    field_type: ManagedFieldType,
    left: ManagedFieldValue,
    right: ManagedFieldValue,
    visited: &mut HashSet<(u64, u64, SemanticTypeId)>,
) -> Result<bool, ManagedMemoryError> {
    match (field_type, left, right) {
        (ManagedFieldType::Unit, ManagedFieldValue::Unit, ManagedFieldValue::Unit) => Ok(true),
        (ManagedFieldType::Bool, ManagedFieldValue::Bool(left), ManagedFieldValue::Bool(right)) => {
            Ok(left == right)
        }
        (ManagedFieldType::Int, ManagedFieldValue::Int(left), ManagedFieldValue::Int(right)) => {
            Ok(left == right)
        }
        (
            ManagedFieldType::Float,
            ManagedFieldValue::Float(left),
            ManagedFieldValue::Float(right),
        ) => Ok(left == right),
        (ManagedFieldType::Atom, ManagedFieldValue::Atom(left), ManagedFieldValue::Atom(right)) => {
            Ok(left == right)
        }
        (
            ManagedFieldType::Reference(semantic),
            ManagedFieldValue::Reference(left),
            ManagedFieldValue::Reference(right),
        ) => references_equal(heap, layouts, semantic, left, right, visited),
        _ => Err(ManagedMemoryError::InvalidAggregateField),
    }
}

/// Compares two values using one admitted managed field schema.
pub(super) fn managed_field_values_equal(
    heap: &ActorHeap,
    layouts: &ManagedLayoutRegistry,
    field_type: ManagedFieldType,
    left: ManagedFieldValue,
    right: ManagedFieldValue,
) -> Result<bool, ManagedMemoryError> {
    field_values_equal(heap, layouts, field_type, left, right, &mut HashSet::new())
}

/// Compares two ordered typed field sequences.
fn ordered_values_equal(
    heap: &ActorHeap,
    layouts: &ManagedLayoutRegistry,
    field_type: ManagedFieldType,
    left: &[ManagedFieldValue],
    right: &[ManagedFieldValue],
    visited: &mut HashSet<(u64, u64, SemanticTypeId)>,
) -> Result<bool, ManagedMemoryError> {
    if left.len() != right.len() {
        return Ok(false);
    }
    for (left, right) in left.iter().copied().zip(right.iter().copied()) {
        if !field_values_equal(heap, layouts, field_type, left, right, visited)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Compares two unordered typed field sequences without relying on host hashes.
fn unordered_values_equal(
    heap: &ActorHeap,
    layouts: &ManagedLayoutRegistry,
    field_type: ManagedFieldType,
    left: &[ManagedFieldValue],
    right: &[ManagedFieldValue],
    visited: &mut HashSet<(u64, u64, SemanticTypeId)>,
) -> Result<bool, ManagedMemoryError> {
    if left.len() != right.len() {
        return Ok(false);
    }
    let mut matched = vec![false; right.len()];
    for left in left.iter().copied() {
        let mut found = false;
        for (index, right) in right.iter().copied().enumerate() {
            let mut candidate_visited = visited.clone();
            if !matched[index]
                && field_values_equal(
                    heap,
                    layouts,
                    field_type,
                    left,
                    right,
                    &mut candidate_visited,
                )?
            {
                *visited = candidate_visited;
                matched[index] = true;
                found = true;
                break;
            }
        }
        if !found {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Compares map entries independent of insertion order.
fn map_entries_equal(
    heap: &ActorHeap,
    layouts: &ManagedLayoutRegistry,
    descriptor: &super::super::ManagedMapDescriptor,
    left: &[(ManagedFieldValue, ManagedFieldValue)],
    right: &[(ManagedFieldValue, ManagedFieldValue)],
    visited: &mut HashSet<(u64, u64, SemanticTypeId)>,
) -> Result<bool, ManagedMemoryError> {
    if left.len() != right.len() {
        return Ok(false);
    }
    let mut matched = vec![false; right.len()];
    for (left_key, left_value) in left.iter().copied() {
        let mut found = false;
        for (index, (right_key, right_value)) in right.iter().copied().enumerate() {
            if matched[index] {
                continue;
            }
            let mut candidate_visited = visited.clone();
            if !field_values_equal(
                heap,
                layouts,
                descriptor.key_type(),
                left_key,
                right_key,
                &mut candidate_visited,
            )? {
                continue;
            }
            if field_values_equal(
                heap,
                layouts,
                descriptor.value_type(),
                left_value,
                right_value,
                &mut candidate_visited,
            )? {
                *visited = candidate_visited;
                matched[index] = true;
                found = true;
                break;
            }
        }
        if !found {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Orders two opaque identities for symmetric recursion detection.
fn ordered_pair(left: u64, right: u64) -> (u64, u64) {
    if left <= right {
        (left, right)
    } else {
        (right, left)
    }
}
