//! Descriptor-directed managed-value conversion at the direct native boundary.

use std::num::NonZeroUsize;
use std::sync::Arc;

use smallvec::SmallVec;

use crate::runtime::native_image::managed::{
    managed_binary_semantic_id, managed_bytes_semantic_id, managed_string_semantic_id, ActorHeap,
    ManagedAggregate, ManagedAggregateDescriptor, ManagedAggregateKind,
    ManagedCollectionDescriptor, ManagedCollectionKind, ManagedFieldType, ManagedFieldValue,
    ManagedKeySemantics, ManagedLayoutRegistry, ManagedList, ManagedMap, ManagedMemoryError,
    ManagedScalarKeySemantics, ManagedSet, SemanticTypeId, TvmRef,
};
use crate::runtime::vm::bitstring::VmBitString;
use crate::runtime::vm::ReplValue;

const MAX_PUBLIC_MANAGED_DEPTH: usize = 256;
const MAX_PUBLIC_MANAGED_VALUES: usize = 65_536;
const INLINE_AGGREGATE_FIELDS: usize = 16;
const INLINE_COLLECTION_ELEMENTS: usize = 8;
const INLINE_ACTIVE_REFERENCES: usize = 16;

#[derive(Default)]
struct AllocationMemo {
    // One public request normally contains only a handful of empty typed
    // collections. Keeping their canonical references inline avoids a tree
    // node allocation for each request while preserving semantic identity.
    empty_collections: SmallVec<[(SemanticTypeId, TvmRef<()>); INLINE_COLLECTION_ELEMENTS]>,
    // Projected fixed envelopes often contain several unobservable empty
    // sequence placeholders of the same semantic type. One immutable value is
    // enough for the complete request graph.
    empty_sequences: SmallVec<[(SemanticTypeId, TvmRef<()>); 3]>,
}

impl AllocationMemo {
    fn empty_collection(&self, semantic: SemanticTypeId) -> Option<TvmRef<()>> {
        self.empty_collections
            .iter()
            .find_map(|(candidate, reference)| (*candidate == semantic).then_some(*reference))
    }

    fn remember_empty_collection(&mut self, semantic: SemanticTypeId, reference: TvmRef<()>) {
        debug_assert!(self.empty_collection(semantic).is_none());
        self.empty_collections.push((semantic, reference));
    }

    fn empty_sequence(&self, semantic: SemanticTypeId) -> Option<TvmRef<()>> {
        self.empty_sequences
            .iter()
            .find_map(|(candidate, reference)| (*candidate == semantic).then_some(*reference))
    }

    fn remember_empty_sequence(&mut self, semantic: SemanticTypeId, reference: TvmRef<()>) {
        debug_assert!(self.empty_sequence(semantic).is_none());
        self.empty_sequences.push((semantic, reference));
    }
}

type ActiveReferences = SmallVec<[u64; INLINE_ACTIVE_REFERENCES]>;

/// Allocates one complete public managed graph through an admitted root identity.
pub(super) fn allocate_public_managed(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    semantic: SemanticTypeId,
    value: &ReplValue,
) -> Result<i64, String> {
    let mut budget = MAX_PUBLIC_MANAGED_VALUES;
    let mut memo = AllocationMemo::default();
    let reference = allocate_managed(heap, layouts, semantic, value, 0, &mut budget, &mut memo)?;
    Ok(i64::from_ne_bytes(
        reference.encoded_abi_word().to_ne_bytes(),
    ))
}

/// Materializes one complete managed graph into public runtime values.
pub(super) fn materialize_public_managed(
    heap: &ActorHeap,
    layouts: &ManagedLayoutRegistry,
    semantic: SemanticTypeId,
    word: i64,
) -> Result<ReplValue, String> {
    let encoded = usize::try_from(u64::from_ne_bytes(word.to_ne_bytes()))
        .ok()
        .and_then(NonZeroUsize::new)
        .ok_or_else(|| {
            "error[execution_shard.managed_reference]: invalid reference word".to_string()
        })?;
    let reference = TvmRef::from_encoded(encoded);
    heap.validate_abi_reference(reference.encoded_abi_word(), semantic)
        .map_err(|error| format!("error[execution_shard.managed_reference]: {error}"))?;
    let mut budget = MAX_PUBLIC_MANAGED_VALUES;
    materialize_managed(
        heap,
        layouts,
        semantic,
        reference,
        0,
        &mut budget,
        &mut ActiveReferences::new(),
    )
}

/// Recursively allocates a collection or fixed aggregate by semantic identity.
fn allocate_managed(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    semantic: SemanticTypeId,
    value: &ReplValue,
    depth: usize,
    budget: &mut usize,
    memo: &mut AllocationMemo,
) -> Result<TvmRef<()>, String> {
    if let Some(descriptor) = layouts.collection(semantic) {
        return allocate_collection(heap, layouts, descriptor, value, depth, budget, memo);
    }
    allocate_aggregate(heap, layouts, semantic, value, depth, budget, memo)
}

/// Allocates one collection through its existing actor-heap storage profile.
fn allocate_collection(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    descriptor: &ManagedCollectionDescriptor,
    value: &ReplValue,
    depth: usize,
    budget: &mut usize,
    memo: &mut AllocationMemo,
) -> Result<TvmRef<()>, String> {
    consume_budget(depth, budget)?;
    match descriptor.kind() {
        ManagedCollectionKind::List => {
            let ReplValue::List(elements) = value else {
                return collection_mismatch("List", value);
            };
            if elements.is_empty() {
                if let Some(reference) = memo.empty_collection(descriptor.semantic_id()) {
                    return Ok(reference);
                }
            }
            let list = descriptor.list_descriptor().expect("checked list schema");
            let mut allocated =
                SmallVec::<[ManagedFieldValue; INLINE_COLLECTION_ELEMENTS]>::with_capacity(
                    elements.len(),
                );
            for value in elements {
                allocated.push(allocate_field(
                    heap,
                    layouts,
                    list.element_type(),
                    value,
                    depth + 1,
                    budget,
                    memo,
                )?);
            }
            let reference = heap
                .list_from_elements(list, &allocated)
                .map(TvmRef::erase)
                .map_err(managed_allocation_error)?;
            if allocated.is_empty() {
                memo.remember_empty_collection(descriptor.semantic_id(), reference);
            }
            Ok(reference)
        }
        ManagedCollectionKind::Map => {
            let owned;
            let entries = if let Some(entries) = value.map_entries_ref() {
                entries
            } else {
                owned = value
                    .map_entries_owned()
                    .ok_or_else(|| collection_type_error("Map", value))?;
                owned.as_slice()
            };
            if entries.is_empty() {
                if let Some(reference) = memo.empty_collection(descriptor.semantic_id()) {
                    return Ok(reference);
                }
            }
            let map = descriptor.map_descriptor().expect("checked map schema");
            let mut allocated = SmallVec::<
                [(ManagedFieldValue, ManagedFieldValue); INLINE_COLLECTION_ELEMENTS],
            >::with_capacity(entries.len());
            for (key, value) in entries {
                allocated.push((
                    allocate_field(heap, layouts, map.key_type(), key, depth + 1, budget, memo)?,
                    allocate_field(
                        heap,
                        layouts,
                        map.value_type(),
                        value,
                        depth + 1,
                        budget,
                        memo,
                    )?,
                ));
            }
            let mut semantics = PublicKeySemantics::new(layouts, map.key_type(), *budget);
            let result = heap.map_from_entries(map, &allocated, &mut semantics);
            *budget = semantics.remaining_budget();
            let reference = result
                .map(TvmRef::erase)
                .map_err(managed_allocation_error)?;
            if allocated.is_empty() {
                memo.remember_empty_collection(descriptor.semantic_id(), reference);
            }
            Ok(reference)
        }
        ManagedCollectionKind::Set => {
            let ReplValue::Set(elements) = value else {
                return collection_mismatch("Set", value);
            };
            if elements.is_empty() {
                if let Some(reference) = memo.empty_collection(descriptor.semantic_id()) {
                    return Ok(reference);
                }
            }
            let set = descriptor.set_descriptor().expect("checked set schema");
            let mut allocated =
                SmallVec::<[ManagedFieldValue; INLINE_COLLECTION_ELEMENTS]>::with_capacity(
                    elements.len(),
                );
            for value in elements {
                allocated.push(allocate_field(
                    heap,
                    layouts,
                    set.element_type(),
                    value,
                    depth + 1,
                    budget,
                    memo,
                )?);
            }
            let mut semantics = PublicKeySemantics::new(layouts, set.element_type(), *budget);
            let result = heap.set_from_elements(set, &allocated, &mut semantics);
            *budget = semantics.remaining_budget();
            let reference = result
                .map(TvmRef::erase)
                .map_err(managed_allocation_error)?;
            if allocated.is_empty() {
                memo.remember_empty_collection(descriptor.semantic_id(), reference);
            }
            Ok(reference)
        }
    }
}

/// Recursively allocates one fixed aggregate after selecting its exact active layout.
fn allocate_aggregate(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    semantic: SemanticTypeId,
    value: &ReplValue,
    depth: usize,
    budget: &mut usize,
    memo: &mut AllocationMemo,
) -> Result<TvmRef<()>, String> {
    consume_budget(depth, budget)?;
    let (descriptor, fields) = select_layout(layouts, semantic, value)?;
    let mut values =
        SmallVec::<[ManagedFieldValue; INLINE_AGGREGATE_FIELDS]>::with_capacity(fields.len());
    for (index, field) in descriptor.fields().iter().enumerate() {
        values.push(allocate_field(
            heap,
            layouts,
            field.field_type(),
            fields.value(index),
            depth + 1,
            budget,
            memo,
        )?);
    }
    heap.allocate_aggregate_ref(descriptor, &values)
        .map(TvmRef::erase)
        .map_err(|error| format!("error[execution_shard.managed_allocate]: {error}"))
}

/// Converts one public field according to its exact physical field category.
fn allocate_field(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    field_type: ManagedFieldType,
    value: &ReplValue,
    depth: usize,
    budget: &mut usize,
    memo: &mut AllocationMemo,
) -> Result<ManagedFieldValue, String> {
    consume_budget(depth, budget)?;
    match (field_type, value) {
        (ManagedFieldType::Unit, ReplValue::Unit) => Ok(ManagedFieldValue::Unit),
        (ManagedFieldType::Bool, ReplValue::Bool(value)) => Ok(ManagedFieldValue::Bool(*value)),
        (ManagedFieldType::Int, ReplValue::Int(value)) => Ok(ManagedFieldValue::Int(*value)),
        (ManagedFieldType::Float, ReplValue::Float(value)) => {
            let value = finite_float(value)?;
            Ok(ManagedFieldValue::Float(value))
        }
        (ManagedFieldType::Atom, ReplValue::Atom(value)) => layouts
            .atom_index(value)
            .map(ManagedFieldValue::Atom)
            .map_err(|error| format!("error[execution_shard.managed_atom]: {error}")),
        (ManagedFieldType::Reference(semantic), value) => {
            let reference =
                allocate_reference(heap, layouts, semantic, value, depth, budget, memo)?;
            Ok(ManagedFieldValue::Reference(reference))
        }
        (expected, actual) => Err(format!(
            "error[execution_shard.managed_field]: value `{actual:?}` does not match `{expected:?}`"
        )),
    }
}

/// Allocates a sequence or nested fixed aggregate for one reference field.
fn allocate_reference(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    semantic: SemanticTypeId,
    value: &ReplValue,
    depth: usize,
    budget: &mut usize,
    memo: &mut AllocationMemo,
) -> Result<TvmRef<()>, String> {
    if semantic == managed_string_semantic_id() {
        let ReplValue::String(value) = value else {
            return reference_mismatch("String", value);
        };
        if value.is_empty() {
            if let Some(reference) = memo.empty_sequence(semantic) {
                return Ok(reference);
            }
        }
        let reference = heap
            .allocate_string(value)
            .map(TvmRef::erase)
            .map_err(managed_allocation_error)?;
        if value.is_empty() {
            memo.remember_empty_sequence(semantic, reference);
        }
        return Ok(reference);
    }
    if semantic == managed_bytes_semantic_id() {
        let ReplValue::Bytes(value) = value else {
            return reference_mismatch("Bytes", value);
        };
        if value.is_empty() {
            if let Some(reference) = memo.empty_sequence(semantic) {
                return Ok(reference);
            }
        }
        let reference = heap
            .allocate_bytes(value)
            .map(TvmRef::erase)
            .map_err(managed_allocation_error)?;
        if value.is_empty() {
            memo.remember_empty_sequence(semantic, reference);
        }
        return Ok(reference);
    }
    if semantic == managed_binary_semantic_id() {
        let ReplValue::BitString(value) = value else {
            return reference_mismatch("Binary", value);
        };
        let storage = heap
            .allocate_bytes(value.packed_bytes())
            .map_err(managed_allocation_error)?;
        return heap
            .allocate_binary(storage, 0, value.bit_len())
            .map(TvmRef::erase)
            .map_err(managed_allocation_error);
    }
    allocate_managed(heap, layouts, semantic, value, depth, budget, memo)
}

/// Selects exactly one admitted active layout and borrows its public fields.
fn select_layout<'layout, 'value>(
    layouts: &'layout ManagedLayoutRegistry,
    semantic: SemanticTypeId,
    value: &'value ReplValue,
) -> Result<(&'layout ManagedAggregateDescriptor, PublicFields<'value>), String> {
    let mut matched = None;
    let mut match_count = 0;
    for layout in layouts.layouts(semantic) {
        if let Some(fields) = public_fields(layout, value) {
            match_count += 1;
            if matched.is_none() {
                matched = Some((layout.as_ref(), fields));
            }
        }
    }
    match match_count {
        1 => Ok(matched.expect("one checked layout")),
        0 => {
            let candidates = layouts
                .layouts(semantic)
                .iter()
                .map(|layout| layout.canonical_type())
                .collect::<Vec<_>>();
            Err(format!(
                "error[execution_shard.managed_layout]: no admitted fixed layout matches `{value:?}` for semantic {:?}; candidates {candidates:?}",
                semantic.bytes()
            ))
        }
        count => Err(format!(
            "error[execution_shard.managed_layout]: public value ambiguously matches {count} admitted layouts"
        )),
    }
}

/// Borrowed aggregate fields without allocating a temporary reference vector.
#[derive(Clone, Copy)]
enum PublicFields<'a> {
    Positional(&'a [ReplValue]),
    Named(&'a [(String, ReplValue)]),
}

impl<'a> PublicFields<'a> {
    fn len(self) -> usize {
        match self {
            Self::Positional(values) => values.len(),
            Self::Named(fields) => fields.len(),
        }
    }

    fn value(self, index: usize) -> &'a ReplValue {
        match self {
            Self::Positional(values) => &values[index],
            Self::Named(fields) => &fields[index].1,
        }
    }
}

/// Matches one public aggregate shape against an admitted active descriptor.
fn public_fields<'a>(
    descriptor: &ManagedAggregateDescriptor,
    value: &'a ReplValue,
) -> Option<PublicFields<'a>> {
    match (descriptor.kind(), value) {
        (ManagedAggregateKind::Tuple, ReplValue::Tuple(values))
        | (ManagedAggregateKind::FixedArray, ReplValue::List(values))
            if values.len() == descriptor.fields().len() =>
        {
            Some(PublicFields::Positional(values))
        }
        (ManagedAggregateKind::Record, ReplValue::Record { name, fields })
            if type_name_matches(descriptor.canonical_type(), name)
                && named_fields_match(descriptor, fields) =>
        {
            Some(PublicFields::Named(fields))
        }
        (ManagedAggregateKind::Constructor, ReplValue::Record { name, fields })
            if descriptor.variant_name() == Some(name.as_str())
                && named_fields_match(descriptor, fields) =>
        {
            Some(PublicFields::Named(fields))
        }
        _ => None,
    }
}

/// Reports whether named public fields exactly preserve descriptor order and identity.
fn named_fields_match(
    descriptor: &ManagedAggregateDescriptor,
    fields: &[(String, ReplValue)],
) -> bool {
    fields.len() == descriptor.fields().len()
        && descriptor
            .fields()
            .iter()
            .zip(fields)
            .all(|(expected, (actual, _))| expected.name() == Some(actual.as_str()))
}

/// Accepts a canonical record identity or its unqualified final segment.
pub(super) fn type_name_matches(canonical: &str, public: &str) -> bool {
    let nominal = canonical
        .strip_prefix("Named(")
        .and_then(|name| name.strip_suffix(')'))
        .unwrap_or(canonical);
    nominal == public || nominal.rsplit('.').next() == Some(public)
}

/// Recursively materializes one validated managed reference.
fn materialize_managed(
    heap: &ActorHeap,
    layouts: &ManagedLayoutRegistry,
    semantic: SemanticTypeId,
    reference: TvmRef<()>,
    depth: usize,
    budget: &mut usize,
    active: &mut ActiveReferences,
) -> Result<ReplValue, String> {
    consume_budget(depth, budget)?;
    let identity = reference.encoded_abi_word();
    if active.contains(&identity) {
        return Err(
            "error[execution_shard.managed_cycle]: cyclic public managed value".to_string(),
        );
    }
    active.push(identity);
    let result = if let Some(descriptor) = layouts.collection(semantic) {
        materialize_collection(heap, layouts, descriptor, reference, depth, budget, active)
    } else {
        materialize_aggregate(heap, layouts, semantic, reference, depth, budget, active)
    };
    debug_assert_eq!(active.pop(), Some(identity));
    result
}

/// Materializes one fixed aggregate whose active-reference guard is already held.
fn materialize_aggregate(
    heap: &ActorHeap,
    layouts: &ManagedLayoutRegistry,
    semantic: SemanticTypeId,
    reference: TvmRef<()>,
    depth: usize,
    budget: &mut usize,
    active: &mut ActiveReferences,
) -> Result<ReplValue, String> {
    let descriptor = layouts.layout_for_reference(heap, semantic, reference)?;
    let view = heap
        .read_aggregate(reference.cast::<ManagedAggregate>(), descriptor)
        .map_err(|error| format!("error[execution_shard.managed_read]: {error}"))?;
    let values = descriptor
        .fields()
        .iter()
        .enumerate()
        .map(|(index, field)| {
            let value = view
                .field(index)
                .map_err(|error| format!("error[execution_shard.managed_read]: {error}"))?;
            materialize_field(
                heap,
                layouts,
                field.field_type(),
                value,
                depth + 1,
                budget,
                active,
            )
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(public_aggregate(descriptor, values))
}

/// Materializes one List, Map, or Set through its canonical heap reader.
fn materialize_collection(
    heap: &ActorHeap,
    layouts: &ManagedLayoutRegistry,
    descriptor: &ManagedCollectionDescriptor,
    reference: TvmRef<()>,
    depth: usize,
    budget: &mut usize,
    active: &mut ActiveReferences,
) -> Result<ReplValue, String> {
    match descriptor.kind() {
        ManagedCollectionKind::List => {
            let list = descriptor.list_descriptor().expect("checked list schema");
            heap.list_elements(list, reference.cast::<ManagedList>())
                .map_err(managed_read_error)?
                .into_iter()
                .map(|value| {
                    materialize_field(
                        heap,
                        layouts,
                        list.element_type(),
                        value,
                        depth + 1,
                        budget,
                        active,
                    )
                })
                .collect::<Result<Vec<_>, _>>()
                .map(ReplValue::List)
        }
        ManagedCollectionKind::Map => {
            let map = descriptor.map_descriptor().expect("checked map schema");
            heap.map_entries(map, reference.cast::<ManagedMap>())
                .map_err(managed_read_error)?
                .into_iter()
                .map(|(key, value)| {
                    Ok((
                        materialize_field(
                            heap,
                            layouts,
                            map.key_type(),
                            key,
                            depth + 1,
                            budget,
                            active,
                        )?,
                        materialize_field(
                            heap,
                            layouts,
                            map.value_type(),
                            value,
                            depth + 1,
                            budget,
                            active,
                        )?,
                    ))
                })
                .collect::<Result<Vec<_>, String>>()
                .map(ReplValue::Map)
        }
        ManagedCollectionKind::Set => {
            let set = descriptor.set_descriptor().expect("checked set schema");
            heap.set_elements(set, reference.cast::<ManagedSet>())
                .map_err(managed_read_error)?
                .into_iter()
                .map(|value| {
                    materialize_field(
                        heap,
                        layouts,
                        set.element_type(),
                        value,
                        depth + 1,
                        budget,
                        active,
                    )
                })
                .collect::<Result<Vec<_>, _>>()
                .map(ReplValue::Set)
        }
    }
}

/// Converts one checked managed field into its public runtime representation.
fn materialize_field(
    heap: &ActorHeap,
    layouts: &ManagedLayoutRegistry,
    field_type: ManagedFieldType,
    value: ManagedFieldValue,
    depth: usize,
    budget: &mut usize,
    active: &mut ActiveReferences,
) -> Result<ReplValue, String> {
    consume_budget(depth, budget)?;
    match (field_type, value) {
        (ManagedFieldType::Unit, ManagedFieldValue::Unit) => Ok(ReplValue::Unit),
        (ManagedFieldType::Bool, ManagedFieldValue::Bool(value)) => Ok(ReplValue::Bool(value)),
        (ManagedFieldType::Int, ManagedFieldValue::Int(value)) => Ok(ReplValue::Int(value)),
        (ManagedFieldType::Float, ManagedFieldValue::Float(value)) if value.is_finite() => {
            Ok(ReplValue::Float(value.to_string()))
        }
        (ManagedFieldType::Atom, ManagedFieldValue::Atom(index)) => layouts
            .atom_identity(index)
            .map(|identity| ReplValue::Atom(identity.to_owned()))
            .map_err(|error| format!("error[execution_shard.managed_atom]: {error}")),
        (ManagedFieldType::Reference(semantic), ManagedFieldValue::Reference(reference)) => {
            materialize_reference(heap, layouts, semantic, reference, depth, budget, active)
        }
        _ => Err(
            "error[execution_shard.managed_field]: decoded field violates its layout".to_string(),
        ),
    }
}

/// Materializes a sequence or nested fixed aggregate reference.
fn materialize_reference(
    heap: &ActorHeap,
    layouts: &ManagedLayoutRegistry,
    semantic: SemanticTypeId,
    reference: TvmRef<()>,
    depth: usize,
    budget: &mut usize,
    active: &mut ActiveReferences,
) -> Result<ReplValue, String> {
    if semantic == managed_string_semantic_id() {
        return heap
            .read_string(reference.cast())
            .map(|value| ReplValue::String(value.to_owned()))
            .map_err(managed_read_error);
    }
    if semantic == managed_bytes_semantic_id() {
        return heap
            .read_bytes(reference.cast())
            .map(|value| ReplValue::Bytes(Arc::from(value)))
            .map_err(managed_read_error);
    }
    if semantic == managed_binary_semantic_id() {
        let view = heap
            .read_binary(reference.cast())
            .map_err(managed_read_error)?;
        let byte_length = view.bit_length().checked_add(7).ok_or_else(|| {
            "error[execution_shard.managed_binary]: bit length exceeds host limits".to_string()
        })? / 8;
        let mut packed = vec![0_u8; byte_length];
        for bit in 0..view.bit_length() {
            if view.bit(bit) == Some(true) {
                packed[bit / 8] |= 1 << (7 - bit % 8);
            }
        }
        return VmBitString::from_bytes(packed, view.bit_length())
            .map(ReplValue::BitString)
            .map_err(|error| format!("error[execution_shard.managed_binary]: {error}"));
    }
    materialize_managed(heap, layouts, semantic, reference, depth, budget, active)
}

/// Rebuilds one public aggregate while retaining source field identities.
fn public_aggregate(descriptor: &ManagedAggregateDescriptor, values: Vec<ReplValue>) -> ReplValue {
    match descriptor.kind() {
        ManagedAggregateKind::Tuple => ReplValue::Tuple(values),
        ManagedAggregateKind::FixedArray => ReplValue::List(values),
        ManagedAggregateKind::Record | ManagedAggregateKind::Constructor => {
            let name = descriptor
                .variant_name()
                .unwrap_or_else(|| {
                    descriptor
                        .canonical_type()
                        .rsplit('.')
                        .next()
                        .expect("canonical aggregate identity is nonempty")
                })
                .to_string();
            let fields = descriptor
                .fields()
                .iter()
                .zip(values)
                .map(|(field, value)| (field.name().unwrap_or("_").to_string(), value))
                .collect();
            ReplValue::Record { name, fields }
        }
    }
}

/// Structural key operations for scalar and reference-valued public collections.
struct PublicKeySemantics<'a> {
    /// Immutable schemas used to recursively materialize reference keys.
    layouts: &'a ManagedLayoutRegistry,
    /// Exact checked key slot category.
    field_type: ManagedFieldType,
    /// Remaining work shared across all key hash and equality operations.
    budget: usize,
}

impl<'a> PublicKeySemantics<'a> {
    /// Binds key semantics to one checked map or set schema.
    fn new(
        layouts: &'a ManagedLayoutRegistry,
        field_type: ManagedFieldType,
        budget: usize,
    ) -> Self {
        Self {
            layouts,
            field_type,
            budget,
        }
    }

    /// Returns work not consumed by key hash and equality operations.
    fn remaining_budget(&self) -> usize {
        self.budget
    }

    /// Materializes one reference key for structural equality or hashing.
    fn materialize_key(
        &mut self,
        heap: &ActorHeap,
        value: ManagedFieldValue,
    ) -> Result<ReplValue, ManagedMemoryError> {
        let (ManagedFieldType::Reference(semantic), ManagedFieldValue::Reference(reference)) =
            (self.field_type, value)
        else {
            return Err(ManagedMemoryError::InvalidAggregateField);
        };
        if self.budget == 0 {
            return Err(ManagedMemoryError::CollectionBudgetExceeded);
        }
        self.budget -= 1;
        materialize_reference(
            heap,
            self.layouts,
            semantic,
            reference,
            0,
            &mut self.budget,
            &mut ActiveReferences::new(),
        )
        .map_err(|error| {
            if error.contains("managed_budget") {
                ManagedMemoryError::CollectionBudgetExceeded
            } else {
                ManagedMemoryError::InvalidAggregateField
            }
        })
    }

    /// Resolves one atom key through the image table for stable public semantics.
    fn atom_key(&mut self, value: ManagedFieldValue) -> Result<ReplValue, ManagedMemoryError> {
        let ManagedFieldValue::Atom(index) = value else {
            return Err(ManagedMemoryError::InvalidAggregateField);
        };
        if self.budget == 0 {
            return Err(ManagedMemoryError::CollectionBudgetExceeded);
        }
        self.budget -= 1;
        self.layouts
            .atom_identity(index)
            .map(|identity| ReplValue::Atom(identity.to_owned()))
    }
}

impl ManagedKeySemantics for PublicKeySemantics<'_> {
    /// Compares scalar keys canonically and reference keys by complete public value.
    fn equivalent(
        &mut self,
        heap: &ActorHeap,
        left: ManagedFieldValue,
        right: ManagedFieldValue,
    ) -> Result<bool, ManagedMemoryError> {
        if matches!(self.field_type, ManagedFieldType::Reference(_)) {
            return Ok(self.materialize_key(heap, left)? == self.materialize_key(heap, right)?);
        }
        if self.field_type == ManagedFieldType::Atom {
            return Ok(self.atom_key(left)? == self.atom_key(right)?);
        }
        ManagedScalarKeySemantics.equivalent(heap, left, right)
    }

    /// Hashes scalar keys canonically and reference keys through stable Terlan hashing.
    fn hash(
        &mut self,
        heap: &ActorHeap,
        value: ManagedFieldValue,
    ) -> Result<u64, ManagedMemoryError> {
        if matches!(self.field_type, ManagedFieldType::Reference(_)) {
            return self
                .materialize_key(heap, value)?
                .stable_hash()
                .map_err(|_| ManagedMemoryError::InvalidAggregateField);
        }
        if self.field_type == ManagedFieldType::Atom {
            return self
                .atom_key(value)?
                .stable_hash()
                .map_err(|_| ManagedMemoryError::InvalidAggregateField);
        }
        ManagedScalarKeySemantics.hash(heap, value)
    }
}

/// Charges one node against bounded recursive public conversion.
fn consume_budget(depth: usize, budget: &mut usize) -> Result<(), String> {
    if depth > MAX_PUBLIC_MANAGED_DEPTH || *budget == 0 {
        return Err(
            "error[execution_shard.managed_budget]: public managed value exceeds conversion limits"
                .to_string(),
        );
    }
    *budget -= 1;
    Ok(())
}

/// Reports one exact public collection shape mismatch.
fn collection_mismatch<T>(expected: &str, actual: &ReplValue) -> Result<T, String> {
    Err(collection_type_error(expected, actual))
}

/// Formats one exact public collection shape mismatch.
fn collection_type_error(expected: &str, actual: &ReplValue) -> String {
    format!("error[execution_shard.managed_collection]: value `{actual:?}` is not `{expected}`")
}

/// Parses one finite public floating-point value.
fn finite_float(value: &str) -> Result<f64, String> {
    let value = value
        .parse::<f64>()
        .map_err(|error| format!("error[execution_shard.managed_float]: invalid Float: {error}"))?;
    value
        .is_finite()
        .then_some(value)
        .ok_or_else(|| "error[execution_shard.managed_float]: Float must be finite".to_string())
}

/// Reports one exact managed-reference value mismatch.
fn reference_mismatch<T>(expected: &str, actual: &ReplValue) -> Result<T, String> {
    Err(format!(
        "error[execution_shard.managed_field]: value `{actual:?}` is not `{expected}`"
    ))
}

/// Adds the direct-boundary allocation context to one managed heap failure.
fn managed_allocation_error(error: impl std::fmt::Display) -> String {
    format!("error[execution_shard.managed_allocate]: {error}")
}

/// Adds the direct-boundary read context to one managed heap failure.
fn managed_read_error(error: impl std::fmt::Display) -> String {
    format!("error[execution_shard.managed_read]: {error}")
}
