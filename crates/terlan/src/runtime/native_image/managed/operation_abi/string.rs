//! Actor-local managed string operations for the generated-code ABI.

use super::super::ManagedString;
use super::{
    reference_word, ActorHeap, ManagedFieldValue, ManagedLayoutRegistry, ManagedList,
    ManagedMemoryError, SemanticTypeId, TvmRef,
};

/// Compares two actor-owned managed strings after validating both references.
pub(super) fn strings_equal(
    heap: &ActorHeap,
    left: i64,
    right: i64,
) -> Result<bool, ManagedMemoryError> {
    let left = reference_word(left)?.cast::<ManagedString>();
    let right = reference_word(right)?.cast::<ManagedString>();
    Ok(heap.read_string(left)? == heap.read_string(right)?)
}

/// Allocates the concatenation of two actor-owned managed strings.
pub(super) fn append_strings(
    heap: &mut ActorHeap,
    left: i64,
    right: i64,
) -> Result<TvmRef<ManagedString>, ManagedMemoryError> {
    heap.concatenate_strings(
        reference_word(left)?.cast::<ManagedString>(),
        reference_word(right)?.cast::<ManagedString>(),
    )
}

/// Allocates one result for an arbitrary managed string concatenation.
pub(super) fn concatenate_strings(
    heap: &mut ActorHeap,
    words: &[i64],
) -> Result<TvmRef<ManagedString>, ManagedMemoryError> {
    let values = words
        .iter()
        .map(|word| reference_word(*word).map(TvmRef::cast::<ManagedString>))
        .collect::<Result<Vec<_>, _>>()?;
    heap.concatenate_strings_many(&values)
}

/// Prepends an image-owned string literal to one actor-owned managed string.
pub(super) fn prepend_string_literal(
    heap: &mut ActorHeap,
    literal: &str,
    right: i64,
) -> Result<TvmRef<ManagedString>, ManagedMemoryError> {
    heap.prepend_string_literal(literal, reference_word(right)?.cast::<ManagedString>())
}

/// Allocates the ordered concatenation of one actor-owned managed string list.
pub(super) fn join_string_list(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    list: i64,
) -> Result<TvmRef<ManagedString>, ManagedMemoryError> {
    let list = reference_word(list)?.cast::<ManagedList>();
    let semantic = heap.descriptor(list)?.semantic_id();
    let descriptor = layouts
        .collection(semantic)
        .and_then(|collection| collection.list_descriptor())
        .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
    let expected = super::super::ManagedFieldType::Reference(SemanticTypeId::from_canonical(
        "std.core.String",
    )?);
    if descriptor.element_type() != expected {
        return Err(ManagedMemoryError::ManagedTypeMismatch);
    }
    let elements = heap.list_elements(descriptor, list)?;
    let mut fragments = Vec::with_capacity(elements.len());
    let mut capacity = 0usize;
    for element in elements {
        let ManagedFieldValue::Reference(reference) = element else {
            return Err(ManagedMemoryError::InvalidAggregateField);
        };
        let fragment = heap.read_string(reference.cast::<ManagedString>())?;
        capacity = capacity
            .checked_add(fragment.len())
            .ok_or(ManagedMemoryError::AllocationLimitExceeded)?;
        fragments.push(fragment.to_string());
    }
    let mut joined = String::with_capacity(capacity);
    for fragment in fragments {
        joined.push_str(&fragment);
    }
    heap.allocate_string(&joined)
}

/// Applies one maintained string transform after validating the managed input.
pub(super) fn transform_string(
    heap: &mut ActorHeap,
    value: i64,
    transform: fn(&str) -> String,
) -> Result<TvmRef<ManagedString>, ManagedMemoryError> {
    let value = heap
        .read_string(reference_word(value)?.cast::<ManagedString>())?
        .to_string();
    heap.allocate_string(&transform(&value))
}
