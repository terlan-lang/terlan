//! Descriptor-directed managed-value conversion at the direct native boundary.

use std::collections::BTreeSet;
use std::num::NonZeroUsize;
use std::sync::Arc;

use crate::runtime::native_image::managed::{
    ActorHeap, ManagedAggregate, ManagedAggregateDescriptor, ManagedAggregateKind,
    ManagedCollectionDescriptor, ManagedCollectionKind, ManagedFieldType, ManagedFieldValue,
    ManagedKeySemantics, ManagedLayoutRegistry, ManagedList, ManagedMap, ManagedMemoryError,
    ManagedScalarKeySemantics, ManagedSet, SemanticTypeId, TvmRef,
};
use crate::runtime::vm::bitstring::VmBitString;
use crate::runtime::vm::ReplValue;

const MAX_PUBLIC_MANAGED_DEPTH: usize = 256;
const MAX_PUBLIC_MANAGED_VALUES: usize = 65_536;

/// Allocates one complete public managed graph through an admitted root identity.
pub(super) fn allocate_public_managed(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    semantic: SemanticTypeId,
    value: &ReplValue,
) -> Result<i64, String> {
    let mut budget = MAX_PUBLIC_MANAGED_VALUES;
    let reference = allocate_managed(heap, layouts, semantic, value, 0, &mut budget)?;
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
        &mut BTreeSet::new(),
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
) -> Result<TvmRef<()>, String> {
    if let Some(descriptor) = layouts.collection(semantic) {
        return allocate_collection(heap, layouts, descriptor, value, depth, budget);
    }
    allocate_aggregate(heap, layouts, semantic, value, depth, budget)
}

/// Allocates one collection through its existing actor-heap storage profile.
fn allocate_collection(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    descriptor: &ManagedCollectionDescriptor,
    value: &ReplValue,
    depth: usize,
    budget: &mut usize,
) -> Result<TvmRef<()>, String> {
    consume_budget(depth, budget)?;
    match descriptor.kind() {
        ManagedCollectionKind::List => {
            let ReplValue::List(elements) = value else {
                return collection_mismatch("List", value);
            };
            let list = descriptor.list_descriptor().expect("checked list schema");
            let elements = elements
                .iter()
                .map(|value| {
                    allocate_field(heap, layouts, list.element_type(), value, depth + 1, budget)
                })
                .collect::<Result<Vec<_>, _>>()?;
            heap.list_from_elements(list, &elements)
                .map(TvmRef::erase)
                .map_err(managed_allocation_error)
        }
        ManagedCollectionKind::Map => {
            let entries = value
                .map_entries_owned()
                .ok_or_else(|| collection_type_error("Map", value))?;
            let map = descriptor.map_descriptor().expect("checked map schema");
            let entries = entries
                .iter()
                .map(|(key, value)| {
                    Ok((
                        allocate_field(heap, layouts, map.key_type(), key, depth + 1, budget)?,
                        allocate_field(heap, layouts, map.value_type(), value, depth + 1, budget)?,
                    ))
                })
                .collect::<Result<Vec<_>, String>>()?;
            let mut semantics = PublicKeySemantics::new(layouts, map.key_type(), *budget);
            let result = heap.map_from_entries(map, &entries, &mut semantics);
            *budget = semantics.remaining_budget();
            result.map(TvmRef::erase).map_err(managed_allocation_error)
        }
        ManagedCollectionKind::Set => {
            let ReplValue::Set(elements) = value else {
                return collection_mismatch("Set", value);
            };
            let set = descriptor.set_descriptor().expect("checked set schema");
            let elements = elements
                .iter()
                .map(|value| {
                    allocate_field(heap, layouts, set.element_type(), value, depth + 1, budget)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let mut semantics = PublicKeySemantics::new(layouts, set.element_type(), *budget);
            let result = heap.set_from_elements(set, &elements, &mut semantics);
            *budget = semantics.remaining_budget();
            result.map(TvmRef::erase).map_err(managed_allocation_error)
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
) -> Result<TvmRef<()>, String> {
    consume_budget(depth, budget)?;
    let (descriptor, fields) = select_layout(layouts, semantic, value)?;
    let values = descriptor
        .fields()
        .iter()
        .zip(fields)
        .map(|(field, value)| {
            allocate_field(heap, layouts, field.field_type(), value, depth + 1, budget)
        })
        .collect::<Result<Vec<_>, _>>()?;
    heap.allocate_aggregate(descriptor, &values)
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
            let reference = allocate_reference(heap, layouts, semantic, value, depth, budget)?;
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
) -> Result<TvmRef<()>, String> {
    if semantic == sequence_semantic("std.core.String")? {
        let ReplValue::String(value) = value else {
            return reference_mismatch("String", value);
        };
        return heap
            .allocate_string(value)
            .map(TvmRef::erase)
            .map_err(managed_allocation_error);
    }
    if semantic == sequence_semantic("std.binary.Bytes")? {
        let ReplValue::Bytes(value) = value else {
            return reference_mismatch("Bytes", value);
        };
        return heap
            .allocate_bytes(value)
            .map(TvmRef::erase)
            .map_err(managed_allocation_error);
    }
    if semantic == sequence_semantic("std.binary.Binary")? {
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
    allocate_managed(heap, layouts, semantic, value, depth, budget)
}

/// Selects exactly one admitted active layout and borrows its public fields.
fn select_layout<'a>(
    layouts: &ManagedLayoutRegistry,
    semantic: SemanticTypeId,
    value: &'a ReplValue,
) -> Result<(Arc<ManagedAggregateDescriptor>, Vec<&'a ReplValue>), String> {
    let matches = layouts
        .layouts(semantic)
        .iter()
        .filter_map(|layout| public_fields(layout, value).map(|fields| (layout.clone(), fields)))
        .collect::<Vec<_>>();
    match matches.len() {
        1 => Ok(matches.into_iter().next().expect("one checked layout")),
        0 => Err(format!(
            "error[execution_shard.managed_layout]: no admitted fixed layout matches `{value:?}`"
        )),
        count => Err(format!(
            "error[execution_shard.managed_layout]: public value ambiguously matches {count} admitted layouts"
        )),
    }
}

/// Matches one public aggregate shape against an admitted active descriptor.
fn public_fields<'a>(
    descriptor: &ManagedAggregateDescriptor,
    value: &'a ReplValue,
) -> Option<Vec<&'a ReplValue>> {
    match (descriptor.kind(), value) {
        (ManagedAggregateKind::Tuple, ReplValue::Tuple(values))
        | (ManagedAggregateKind::FixedArray, ReplValue::List(values))
            if values.len() == descriptor.fields().len() =>
        {
            Some(values.iter().collect())
        }
        (ManagedAggregateKind::Record, ReplValue::Record { name, fields })
            if type_name_matches(descriptor.canonical_type(), name)
                && named_fields_match(descriptor, fields) =>
        {
            Some(fields.iter().map(|(_, value)| value).collect())
        }
        (ManagedAggregateKind::Constructor, ReplValue::Record { name, fields })
            if descriptor.variant_name() == Some(name.as_str())
                && named_fields_match(descriptor, fields) =>
        {
            Some(fields.iter().map(|(_, value)| value).collect())
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
fn type_name_matches(canonical: &str, public: &str) -> bool {
    canonical == public || canonical.rsplit('.').next() == Some(public)
}

/// Recursively materializes one validated managed reference.
fn materialize_managed(
    heap: &ActorHeap,
    layouts: &ManagedLayoutRegistry,
    semantic: SemanticTypeId,
    reference: TvmRef<()>,
    depth: usize,
    budget: &mut usize,
    active: &mut BTreeSet<u64>,
) -> Result<ReplValue, String> {
    consume_budget(depth, budget)?;
    let identity = reference.encoded_abi_word();
    if !active.insert(identity) {
        return Err(
            "error[execution_shard.managed_cycle]: cyclic public managed value".to_string(),
        );
    }
    let result = (|| {
        if let Some(descriptor) = layouts.collection(semantic) {
            materialize_collection(heap, layouts, descriptor, reference, depth, budget, active)
        } else {
            materialize_aggregate(heap, layouts, semantic, reference, depth, budget, active)
        }
    })();
    active.remove(&identity);
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
    active: &mut BTreeSet<u64>,
) -> Result<ReplValue, String> {
    let descriptor = layouts.layout_for_reference(heap, semantic, reference)?;
    let view = heap
        .read_aggregate(reference.cast::<ManagedAggregate>(), &descriptor)
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
    Ok(public_aggregate(&descriptor, values))
}

/// Materializes one List, Map, or Set through its canonical heap reader.
fn materialize_collection(
    heap: &ActorHeap,
    layouts: &ManagedLayoutRegistry,
    descriptor: &ManagedCollectionDescriptor,
    reference: TvmRef<()>,
    depth: usize,
    budget: &mut usize,
    active: &mut BTreeSet<u64>,
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
    active: &mut BTreeSet<u64>,
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
    active: &mut BTreeSet<u64>,
) -> Result<ReplValue, String> {
    if semantic == sequence_semantic("std.core.String")? {
        return heap
            .read_string(reference.cast())
            .map(|value| ReplValue::String(value.to_owned()))
            .map_err(managed_read_error);
    }
    if semantic == sequence_semantic("std.binary.Bytes")? {
        return heap
            .read_bytes(reference.cast())
            .map(|value| ReplValue::Bytes(Arc::from(value)))
            .map_err(managed_read_error);
    }
    if semantic == sequence_semantic("std.binary.Binary")? {
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
            &mut BTreeSet::new(),
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

/// Derives one built-in sequence identity through the checked semantic API.
fn sequence_semantic(canonical: &str) -> Result<SemanticTypeId, String> {
    SemanticTypeId::from_canonical(canonical)
        .map_err(|error| format!("error[execution_shard.managed_type]: {error}"))
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
