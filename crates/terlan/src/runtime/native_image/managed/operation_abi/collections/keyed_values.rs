//! Managed map and set operations with checked key types.

use super::*;

pub(super) fn map_get(
    heap: &ActorHeap,
    descriptor: &super::super::super::ManagedMapDescriptor,
    map: TvmRef<ManagedMap>,
    key: ManagedFieldValue,
) -> Result<Option<ManagedFieldValue>, ManagedMemoryError> {
    if string_keys(descriptor)? {
        heap.map_get(descriptor, map, key, &mut ManagedStringKeySemantics)
    } else {
        heap.map_get(descriptor, map, key, &mut ManagedScalarKeySemantics)
    }
}

pub(super) fn map_put(
    heap: &mut ActorHeap,
    descriptor: &super::super::super::ManagedMapDescriptor,
    map: TvmRef<ManagedMap>,
    key: ManagedFieldValue,
    value: ManagedFieldValue,
) -> Result<TvmRef<ManagedMap>, ManagedMemoryError> {
    if string_keys(descriptor)? {
        heap.map_put(descriptor, map, key, value, &mut ManagedStringKeySemantics)
    } else {
        heap.map_put(descriptor, map, key, value, &mut ManagedScalarKeySemantics)
    }
}

pub(super) fn map_take(
    heap: &mut ActorHeap,
    descriptor: &super::super::super::ManagedMapDescriptor,
    map: TvmRef<ManagedMap>,
    key: ManagedFieldValue,
) -> Result<(Option<ManagedFieldValue>, TvmRef<ManagedMap>), ManagedMemoryError> {
    if string_keys(descriptor)? {
        heap.map_take(descriptor, map, key, &mut ManagedStringKeySemantics)
    } else {
        heap.map_take(descriptor, map, key, &mut ManagedScalarKeySemantics)
    }
}

pub(super) fn map_remove(
    heap: &mut ActorHeap,
    descriptor: &super::super::super::ManagedMapDescriptor,
    map: TvmRef<ManagedMap>,
    key: ManagedFieldValue,
) -> Result<TvmRef<ManagedMap>, ManagedMemoryError> {
    map_take(heap, descriptor, map, key).map(|(_, remainder)| remainder)
}

pub(super) fn map_from_entries(
    heap: &mut ActorHeap,
    descriptor: &super::super::super::ManagedMapDescriptor,
    entries: &[(ManagedFieldValue, ManagedFieldValue)],
) -> Result<TvmRef<ManagedMap>, ManagedMemoryError> {
    if string_keys(descriptor)? {
        heap.map_from_entries(descriptor, entries, &mut ManagedStringKeySemantics)
    } else {
        heap.map_from_entries(descriptor, entries, &mut ManagedScalarKeySemantics)
    }
}

pub(super) fn string_keys(
    descriptor: &super::super::super::ManagedMapDescriptor,
) -> Result<bool, ManagedMemoryError> {
    Ok(descriptor.key_type()
        == ManagedFieldType::Reference(SemanticTypeId::from_canonical("std.core.String")?))
}

pub(super) fn set_from_elements(
    heap: &mut ActorHeap,
    descriptor: &super::super::super::ManagedSetDescriptor,
    elements: &[ManagedFieldValue],
) -> Result<TvmRef<ManagedSet>, ManagedMemoryError> {
    if string_set(descriptor)? {
        heap.set_from_elements(descriptor, elements, &mut ManagedStringKeySemantics)
    } else {
        heap.set_from_elements(descriptor, elements, &mut ManagedScalarKeySemantics)
    }
}

pub(super) fn set_contains(
    heap: &ActorHeap,
    descriptor: &super::super::super::ManagedSetDescriptor,
    set: TvmRef<ManagedSet>,
    element: ManagedFieldValue,
) -> Result<bool, ManagedMemoryError> {
    if string_set(descriptor)? {
        heap.set_contains(descriptor, set, element, &mut ManagedStringKeySemantics)
    } else {
        heap.set_contains(descriptor, set, element, &mut ManagedScalarKeySemantics)
    }
}

pub(super) fn set_add(
    heap: &mut ActorHeap,
    descriptor: &super::super::super::ManagedSetDescriptor,
    set: TvmRef<ManagedSet>,
    element: ManagedFieldValue,
) -> Result<TvmRef<ManagedSet>, ManagedMemoryError> {
    if string_set(descriptor)? {
        heap.set_add(descriptor, set, element, &mut ManagedStringKeySemantics)
    } else {
        heap.set_add(descriptor, set, element, &mut ManagedScalarKeySemantics)
    }
}

pub(super) fn set_remove(
    heap: &mut ActorHeap,
    descriptor: &super::super::super::ManagedSetDescriptor,
    set: TvmRef<ManagedSet>,
    element: ManagedFieldValue,
) -> Result<TvmRef<ManagedSet>, ManagedMemoryError> {
    if string_set(descriptor)? {
        heap.set_remove(descriptor, set, element, &mut ManagedStringKeySemantics)
    } else {
        heap.set_remove(descriptor, set, element, &mut ManagedScalarKeySemantics)
    }
}

pub(super) fn string_set(
    descriptor: &super::super::super::ManagedSetDescriptor,
) -> Result<bool, ManagedMemoryError> {
    Ok(descriptor.element_type()
        == ManagedFieldType::Reference(SemanticTypeId::from_canonical("std.core.String")?))
}
