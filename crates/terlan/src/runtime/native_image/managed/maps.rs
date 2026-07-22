//! Actor-local adaptive persistent maps with compiler-specialized key semantics.

use std::sync::Arc;

use crate::runtime::map_layout::should_use_indexed_map;

use super::aggregates::{decode_typed_slot, encode_typed_slot, validate_typed_value};
use super::slots::align_up;
use super::{
    ActorHeap, AllocationClass, ManagedFieldType, ManagedFieldValue, ManagedList,
    ManagedListDescriptor, ManagedMemoryError, ManagedTypeDescriptor, SemanticTypeId, TvmRef,
};

#[path = "maps/achamp.rs"]
mod achamp;

const ROOT_MAGIC: u32 = 0x3150_414d;
const ENTRY_MAGIC: u32 = 0x3152_544e;
const ROOT_HEADER_BYTES: usize = 16;
const INDEXED_ROOT_BYTES: usize = 32;
const INDEXED_TRIE_OFFSET: usize = 16;
const INDEXED_ORDER_OFFSET: usize = 24;
const ENTRY_HEADER_BYTES: usize = 8;
const FORM_EMPTY: u8 = 0;
const FORM_FLAT: u8 = 1;
const FORM_INDEXED: u8 = 2;

/// Supplies the canonical equality and stable hash operations for one checked map key type.
pub trait ManagedKeySemantics {
    /// Compares two checked keys by Terlan value semantics rather than managed-reference identity.
    fn equivalent(
        &mut self,
        heap: &ActorHeap,
        left: ManagedFieldValue,
        right: ManagedFieldValue,
    ) -> Result<bool, ManagedMemoryError>;

    /// Produces a deterministic hash compatible with `equivalent` across relocation.
    fn hash(
        &mut self,
        heap: &ActorHeap,
        value: ManagedFieldValue,
    ) -> Result<u64, ManagedMemoryError>;
}

/// Canonical key semantics for closed scalar key types.
#[derive(Clone, Copy, Debug, Default)]
pub struct ManagedScalarKeySemantics;

impl ManagedKeySemantics for ManagedScalarKeySemantics {
    fn equivalent(
        &mut self,
        _heap: &ActorHeap,
        left: ManagedFieldValue,
        right: ManagedFieldValue,
    ) -> Result<bool, ManagedMemoryError> {
        match (left, right) {
            (ManagedFieldValue::Reference(_), _) | (_, ManagedFieldValue::Reference(_)) => {
                Err(ManagedMemoryError::InvalidAggregateField)
            }
            _ => Ok(left == right),
        }
    }

    fn hash(
        &mut self,
        _heap: &ActorHeap,
        value: ManagedFieldValue,
    ) -> Result<u64, ManagedMemoryError> {
        let (tag, bytes): (u8, Vec<u8>) = match value {
            ManagedFieldValue::Unit => (0, Vec::new()),
            ManagedFieldValue::Bool(value) => (1, vec![u8::from(value)]),
            ManagedFieldValue::Int(value) => (2, value.to_le_bytes().to_vec()),
            ManagedFieldValue::Float(value) => {
                let canonical = if value == 0.0 { 0.0 } else { value };
                (3, canonical.to_bits().to_le_bytes().to_vec())
            }
            ManagedFieldValue::Atom(value) => (4, value.get().to_le_bytes().to_vec()),
            ManagedFieldValue::Reference(_) => {
                return Err(ManagedMemoryError::InvalidAggregateField)
            }
        };
        Ok(stable_hash(tag, &bytes))
    }
}

/// Canonical structural key semantics for actor-local managed strings.
#[derive(Clone, Copy, Debug, Default)]
pub struct ManagedStringKeySemantics;

impl ManagedKeySemantics for ManagedStringKeySemantics {
    fn equivalent(
        &mut self,
        heap: &ActorHeap,
        left: ManagedFieldValue,
        right: ManagedFieldValue,
    ) -> Result<bool, ManagedMemoryError> {
        Ok(read_string_key(heap, left)? == read_string_key(heap, right)?)
    }

    fn hash(
        &mut self,
        heap: &ActorHeap,
        value: ManagedFieldValue,
    ) -> Result<u64, ManagedMemoryError> {
        let value = read_string_key(heap, value)?;
        Ok(stable_public_string_hash(value))
    }
}

/// Opens one managed string key after validating its physical value category.
fn read_string_key<'a>(
    heap: &'a ActorHeap,
    value: ManagedFieldValue,
) -> Result<&'a str, ManagedMemoryError> {
    let ManagedFieldValue::Reference(reference) = value else {
        return Err(ManagedMemoryError::InvalidAggregateField);
    };
    heap.read_string(reference.cast())
}

/// Matches the stable public `String` hash used when managed maps are built.
fn stable_public_string_hash(value: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    write_hash_byte(&mut hash, 4);
    for byte in (value.len() as u64).to_le_bytes() {
        write_hash_byte(&mut hash, byte);
    }
    for byte in value.as_bytes() {
        write_hash_byte(&mut hash, *byte);
    }
    hash
}

/// Advances one FNV-1a state by one byte.
fn write_hash_byte(hash: &mut u64, byte: u8) {
    *hash ^= u64::from(byte);
    *hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
}

/// Compile-time marker for one actor-local immutable map root.
#[derive(Debug)]
pub struct ManagedMap;

/// Observable storage family selected for one managed map.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManagedMapProfile {
    /// Canonical zero-entry representation.
    Empty,
    /// Insertion-ordered packed key/value entries.
    Flat,
    /// Persistent A-CHAMP index plus an insertion-order RRB list.
    Indexed,
}

/// Canonical typed descriptor shared by all physical forms of `Map[K, V]`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagedMapDescriptor {
    semantic_id: SemanticTypeId,
    entry_semantic_id: SemanticTypeId,
    node_semantic_id: SemanticTypeId,
    key_type: ManagedFieldType,
    value_type: ManagedFieldType,
    order: ManagedListDescriptor,
}

impl ManagedMapDescriptor {
    /// Creates a map descriptor from its canonical checked key and value types.
    pub fn new(
        canonical_type: &str,
        key_type: ManagedFieldType,
        value_type: ManagedFieldType,
    ) -> Result<Self, ManagedMemoryError> {
        if canonical_type.is_empty() {
            return Err(ManagedMemoryError::InvalidAggregateShape);
        }
        let semantic_id = SemanticTypeId::from_canonical(canonical_type)?;
        let entry_semantic_id =
            SemanticTypeId::from_canonical(&format!("{canonical_type}#map-entry"))?;
        let node_semantic_id =
            SemanticTypeId::from_canonical(&format!("{canonical_type}#achamp-node"))?;
        let order = ManagedListDescriptor::new(
            &format!("{canonical_type}#insertion-order"),
            ManagedFieldType::Reference(entry_semantic_id),
        )?;
        Ok(Self {
            semantic_id,
            entry_semantic_id,
            node_semantic_id,
            key_type,
            value_type,
            order,
        })
    }

    /// Returns the canonical map semantic identity.
    pub fn semantic_id(&self) -> SemanticTypeId {
        self.semantic_id
    }

    /// Returns the statically selected key slot category.
    pub fn key_type(&self) -> ManagedFieldType {
        self.key_type
    }

    /// Returns the statically selected value slot category.
    pub fn value_type(&self) -> ManagedFieldType {
        self.value_type
    }

    /// Returns the private entry semantic identity used by the index and order list.
    pub(super) fn entry_semantic_id(&self) -> SemanticTypeId {
        self.entry_semantic_id
    }

    /// Returns the private A-CHAMP node semantic identity.
    pub(super) fn node_semantic_id(&self) -> SemanticTypeId {
        self.node_semantic_id
    }
}

/// Opaque marker for one key/value entry shared by the index and order list.
#[derive(Debug)]
pub(super) struct MapEntry;

/// Physical offsets and stride for one packed flat-root key/value entry.
#[derive(Clone, Copy, Debug)]
struct FlatEntryLayout {
    key_offset: usize,
    value_offset: usize,
    stride: usize,
    alignment: usize,
}

/// Decoded and validated map root representation.
#[derive(Clone, Copy, Debug)]
enum RootStorage {
    Empty,
    Flat,
    Indexed {
        trie: TvmRef<achamp::AChampNode>,
        order: TvmRef<ManagedList>,
    },
}

/// Decoded map root cardinality and physical storage.
#[derive(Clone, Copy, Debug)]
struct RootView {
    length: usize,
    storage: RootStorage,
}

impl ActorHeap {
    /// Materializes an immutable insertion-ordered map and replaces duplicate values in place.
    pub fn map_from_entries<S: ManagedKeySemantics>(
        &mut self,
        descriptor: &ManagedMapDescriptor,
        entries: &[(ManagedFieldValue, ManagedFieldValue)],
        semantics: &mut S,
    ) -> Result<TvmRef<ManagedMap>, ManagedMemoryError> {
        let unique = unique_entries(self, descriptor, entries, semantics)?;
        allocate_map(self, descriptor, &unique, semantics)
    }

    /// Allocates the canonical empty representation for one map type.
    pub fn map_empty(
        &mut self,
        descriptor: &ManagedMapDescriptor,
    ) -> Result<TvmRef<ManagedMap>, ManagedMemoryError> {
        allocate_flat_map(self, descriptor, &[])
    }

    /// Returns the number of unique keys in a managed map.
    pub fn map_length(
        &self,
        descriptor: &ManagedMapDescriptor,
        map: TvmRef<ManagedMap>,
    ) -> Result<usize, ManagedMemoryError> {
        Ok(read_root(self, descriptor, map)?.length)
    }

    /// Reports whether a managed map contains no entries.
    pub fn map_is_empty(
        &self,
        descriptor: &ManagedMapDescriptor,
        map: TvmRef<ManagedMap>,
    ) -> Result<bool, ManagedMemoryError> {
        Ok(self.map_length(descriptor, map)? == 0)
    }

    /// Returns the current physical map profile.
    pub fn map_profile(
        &self,
        descriptor: &ManagedMapDescriptor,
        map: TvmRef<ManagedMap>,
    ) -> Result<ManagedMapProfile, ManagedMemoryError> {
        Ok(match read_root(self, descriptor, map)?.storage {
            RootStorage::Empty => ManagedMapProfile::Empty,
            RootStorage::Flat => ManagedMapProfile::Flat,
            RootStorage::Indexed { .. } => ManagedMapProfile::Indexed,
        })
    }

    /// Decodes all entries in stable insertion order without key rehashing.
    pub fn map_entries(
        &self,
        descriptor: &ManagedMapDescriptor,
        map: TvmRef<ManagedMap>,
    ) -> Result<Vec<(ManagedFieldValue, ManagedFieldValue)>, ManagedMemoryError> {
        let root = read_root(self, descriptor, map)?;
        match root.storage {
            RootStorage::Empty => Ok(Vec::new()),
            RootStorage::Flat => {
                decode_flat_entries(self, descriptor, map, self.read(map)?, root.length)
            }
            RootStorage::Indexed { order, .. } => self
                .list_elements(&descriptor.order, order)?
                .into_iter()
                .map(|value| {
                    let ManagedFieldValue::Reference(entry) = value else {
                        return Err(ManagedMemoryError::CorruptedCollection);
                    };
                    decode_entry(self, descriptor, entry.cast())
                })
                .collect(),
        }
    }

    /// Looks up one key through compiler-specialized structural equality and hashing.
    pub fn map_get<S: ManagedKeySemantics>(
        &self,
        descriptor: &ManagedMapDescriptor,
        map: TvmRef<ManagedMap>,
        key: ManagedFieldValue,
        semantics: &mut S,
    ) -> Result<Option<ManagedFieldValue>, ManagedMemoryError> {
        validate_typed_value(self, descriptor.key_type, key)?;
        let root = read_root(self, descriptor, map)?;
        match root.storage {
            RootStorage::Empty => Ok(None),
            RootStorage::Flat => {
                let entries =
                    decode_flat_entries(self, descriptor, map, self.read(map)?, root.length)?;
                Ok(find_key(self, &entries, key, semantics)?.map(|index| entries[index].1))
            }
            RootStorage::Indexed { trie, .. } => {
                let hash = semantics.hash(self, key)?;
                let Some(entry) = achamp::lookup(self, descriptor, trie, hash, key, semantics)?
                else {
                    return Ok(None);
                };
                decode_entry(self, descriptor, entry).map(|(_, value)| Some(value))
            }
        }
    }

    /// Reports whether one structurally equal key is present.
    pub fn map_contains_key<S: ManagedKeySemantics>(
        &self,
        descriptor: &ManagedMapDescriptor,
        map: TvmRef<ManagedMap>,
        key: ManagedFieldValue,
        semantics: &mut S,
    ) -> Result<bool, ManagedMemoryError> {
        Ok(self.map_get(descriptor, map, key, semantics)?.is_some())
    }

    /// Returns a persistent map with one key inserted or replaced in its original position.
    pub fn map_put<S: ManagedKeySemantics>(
        &mut self,
        descriptor: &ManagedMapDescriptor,
        map: TvmRef<ManagedMap>,
        key: ManagedFieldValue,
        value: ManagedFieldValue,
        semantics: &mut S,
    ) -> Result<TvmRef<ManagedMap>, ManagedMemoryError> {
        validate_typed_value(self, descriptor.key_type, key)?;
        validate_typed_value(self, descriptor.value_type, value)?;
        let root = read_root(self, descriptor, map)?;
        match root.storage {
            RootStorage::Empty | RootStorage::Flat => {
                let mut entries = self.map_entries(descriptor, map)?;
                if let Some(index) = find_key(self, &entries, key, semantics)? {
                    entries[index].1 = value;
                } else {
                    entries.push((key, value));
                }
                allocate_map(self, descriptor, &entries, semantics)
            }
            RootStorage::Indexed { trie, order } => {
                self.map_put_indexed(descriptor, root.length, trie, order, key, value, semantics)
            }
        }
    }

    /// Removes one key and returns its optional value plus the persistent remainder.
    pub fn map_take<S: ManagedKeySemantics>(
        &mut self,
        descriptor: &ManagedMapDescriptor,
        map: TvmRef<ManagedMap>,
        key: ManagedFieldValue,
        semantics: &mut S,
    ) -> Result<(Option<ManagedFieldValue>, TvmRef<ManagedMap>), ManagedMemoryError> {
        validate_typed_value(self, descriptor.key_type, key)?;
        let root = read_root(self, descriptor, map)?;
        match root.storage {
            RootStorage::Empty => Ok((None, map)),
            RootStorage::Flat => {
                let mut entries = self.map_entries(descriptor, map)?;
                let Some(index) = find_key(self, &entries, key, semantics)? else {
                    return Ok((None, map));
                };
                let (_, value) = entries.remove(index);
                Ok((Some(value), allocate_flat_map(self, descriptor, &entries)?))
            }
            RootStorage::Indexed { trie, order } => {
                let hash = semantics.hash(self, key)?;
                let Some(entry) = achamp::lookup(self, descriptor, trie, hash, key, semantics)?
                else {
                    return Ok((None, map));
                };
                let (_, value) = decode_entry(self, descriptor, entry)?;
                let new_trie = achamp::remove(self, descriptor, trie, hash, entry, 0)?
                    .ok_or(ManagedMemoryError::CorruptedCollection)?;
                let removal = self.list_from_elements(
                    &descriptor.order,
                    &[ManagedFieldValue::Reference(entry.erase())],
                )?;
                let new_order =
                    self.list_subtract(&descriptor.order, order, removal, |_heap, left, right| {
                        Ok(left == right)
                    })?;
                let new_length = root.length - 1;
                if should_use_indexed_map(new_length) {
                    Ok((
                        Some(value),
                        allocate_indexed_root(self, descriptor, new_length, new_trie, new_order)?,
                    ))
                } else {
                    let entries = entries_from_order(self, descriptor, new_order)?;
                    Ok((Some(value), allocate_flat_map(self, descriptor, &entries)?))
                }
            }
        }
    }

    /// Removes one key while retaining the previous root when the key is absent.
    pub fn map_remove<S: ManagedKeySemantics>(
        &mut self,
        descriptor: &ManagedMapDescriptor,
        map: TvmRef<ManagedMap>,
        key: ManagedFieldValue,
        semantics: &mut S,
    ) -> Result<TvmRef<ManagedMap>, ManagedMemoryError> {
        self.map_take(descriptor, map, key, semantics)
            .map(|(_, remainder)| remainder)
    }

    /// Returns an empty map, preserving an already empty root.
    pub fn map_clear(
        &mut self,
        descriptor: &ManagedMapDescriptor,
        map: TvmRef<ManagedMap>,
    ) -> Result<TvmRef<ManagedMap>, ManagedMemoryError> {
        if self.map_is_empty(descriptor, map)? {
            Ok(map)
        } else {
            self.map_empty(descriptor)
        }
    }

    /// Applies one path-copy update to an indexed root and its insertion-order list.
    fn map_put_indexed<S: ManagedKeySemantics>(
        &mut self,
        descriptor: &ManagedMapDescriptor,
        length: usize,
        trie: TvmRef<achamp::AChampNode>,
        order: TvmRef<ManagedList>,
        key: ManagedFieldValue,
        value: ManagedFieldValue,
        semantics: &mut S,
    ) -> Result<TvmRef<ManagedMap>, ManagedMemoryError> {
        let hash = semantics.hash(self, key)?;
        if let Some(previous) = achamp::lookup(self, descriptor, trie, hash, key, semantics)? {
            let (canonical_key, _) = decode_entry(self, descriptor, previous)?;
            let replacement = allocate_entry(self, descriptor, canonical_key, value)?;
            let new_trie = achamp::replace(self, descriptor, trie, hash, previous, replacement, 0)?;
            let position = order_position(self, descriptor, order, previous)?;
            let new_order = self.list_update(
                &descriptor.order,
                order,
                position,
                ManagedFieldValue::Reference(replacement.erase()),
            )?;
            allocate_indexed_root(self, descriptor, length, new_trie, new_order)
        } else {
            let entry = allocate_entry(self, descriptor, key, value)?;
            let new_trie = achamp::insert(self, descriptor, trie, hash, entry, 0)?;
            let new_order = self.list_append(
                &descriptor.order,
                order,
                ManagedFieldValue::Reference(entry.erase()),
            )?;
            allocate_indexed_root(self, descriptor, length + 1, new_trie, new_order)
        }
    }
}

/// Validates and deduplicates construction entries before managed allocation.
fn unique_entries<S: ManagedKeySemantics>(
    heap: &ActorHeap,
    descriptor: &ManagedMapDescriptor,
    entries: &[(ManagedFieldValue, ManagedFieldValue)],
    semantics: &mut S,
) -> Result<Vec<(ManagedFieldValue, ManagedFieldValue)>, ManagedMemoryError> {
    let mut unique = Vec::with_capacity(entries.len());
    for &(key, value) in entries {
        validate_typed_value(heap, descriptor.key_type, key)?;
        validate_typed_value(heap, descriptor.value_type, value)?;
        if let Some(index) = find_key(heap, &unique, key, semantics)? {
            unique[index].1 = value;
        } else {
            unique.push((key, value));
        }
    }
    Ok(unique)
}

/// Locates a key without imposing pointer identity on managed values.
fn find_key<S: ManagedKeySemantics>(
    heap: &ActorHeap,
    entries: &[(ManagedFieldValue, ManagedFieldValue)],
    key: ManagedFieldValue,
    semantics: &mut S,
) -> Result<Option<usize>, ManagedMemoryError> {
    for (index, (candidate, _)) in entries.iter().enumerate() {
        if semantics.equivalent(heap, *candidate, key)? {
            return Ok(Some(index));
        }
    }
    Ok(None)
}

/// Selects and allocates the canonical root profile for one entry sequence.
fn allocate_map<S: ManagedKeySemantics>(
    heap: &mut ActorHeap,
    descriptor: &ManagedMapDescriptor,
    entries: &[(ManagedFieldValue, ManagedFieldValue)],
    semantics: &mut S,
) -> Result<TvmRef<ManagedMap>, ManagedMemoryError> {
    if !should_use_indexed_map(entries.len()) {
        return allocate_flat_map(heap, descriptor, entries);
    }
    let hashes = entries
        .iter()
        .map(|(key, _)| semantics.hash(heap, *key))
        .collect::<Result<Vec<_>, _>>()?;
    let mut indexed = Vec::with_capacity(entries.len());
    let mut order_values = Vec::with_capacity(entries.len());
    for (&(key, value), hash) in entries.iter().zip(hashes) {
        let entry = allocate_entry(heap, descriptor, key, value)?;
        indexed.push(achamp::IndexedEntry { hash, entry });
        order_values.push(ManagedFieldValue::Reference(entry.erase()));
    }
    let trie = achamp::build(heap, descriptor, &indexed, 0)?;
    let order = heap.list_from_elements(&descriptor.order, &order_values)?;
    allocate_indexed_root(heap, descriptor, entries.len(), trie, order)
}

/// Allocates one packed empty or flat map root.
fn allocate_flat_map(
    heap: &mut ActorHeap,
    descriptor: &ManagedMapDescriptor,
    entries: &[(ManagedFieldValue, ManagedFieldValue)],
) -> Result<TvmRef<ManagedMap>, ManagedMemoryError> {
    let layout = flat_entry_layout(descriptor)?;
    let size = ROOT_HEADER_BYTES
        .checked_add(
            layout
                .stride
                .checked_mul(entries.len())
                .ok_or(ManagedMemoryError::CollectionTooLarge)?,
        )
        .ok_or(ManagedMemoryError::CollectionTooLarge)?;
    let form = if entries.is_empty() {
        FORM_EMPTY
    } else {
        FORM_FLAT
    };
    let mut payload = vec![0; size];
    write_root_header(&mut payload, form, entries.len())?;
    let mut references = Vec::new();
    for (index, &(key, value)) in entries.iter().enumerate() {
        let base = flat_entry_base(layout, index)?;
        encode_typed_slot(
            heap,
            &mut payload,
            base + layout.key_offset,
            descriptor.key_type,
            key,
            &mut references,
        )?;
        encode_typed_slot(
            heap,
            &mut payload,
            base + layout.value_offset,
            descriptor.value_type,
            value,
            &mut references,
        )?;
    }
    let managed = root_descriptor(
        descriptor,
        form,
        entries.len(),
        size,
        references.iter().map(|item| item.0).collect(),
    )?;
    heap.allocate(managed, &payload, &references)
}

/// Allocates one indexed root referencing its trie and insertion-order list.
fn allocate_indexed_root(
    heap: &mut ActorHeap,
    descriptor: &ManagedMapDescriptor,
    length: usize,
    trie: TvmRef<achamp::AChampNode>,
    order: TvmRef<ManagedList>,
) -> Result<TvmRef<ManagedMap>, ManagedMemoryError> {
    let mut payload = vec![0; INDEXED_ROOT_BYTES];
    write_root_header(&mut payload, FORM_INDEXED, length)?;
    heap.allocate(
        root_descriptor(
            descriptor,
            FORM_INDEXED,
            length,
            INDEXED_ROOT_BYTES,
            vec![INDEXED_TRIE_OFFSET, INDEXED_ORDER_OFFSET],
        )?,
        &payload,
        &[
            (INDEXED_TRIE_OFFSET, trie.erase()),
            (INDEXED_ORDER_OFFSET, order.erase()),
        ],
    )
}

/// Allocates one immutable entry shared by an indexed root's trie and order list.
pub(super) fn allocate_entry(
    heap: &mut ActorHeap,
    descriptor: &ManagedMapDescriptor,
    key: ManagedFieldValue,
    value: ManagedFieldValue,
) -> Result<TvmRef<MapEntry>, ManagedMemoryError> {
    let layout = flat_entry_layout(descriptor)?;
    let base = align_up(ENTRY_HEADER_BYTES, layout.alignment)?;
    let size = base
        .checked_add(layout.stride)
        .ok_or(ManagedMemoryError::CollectionTooLarge)?;
    let mut payload = vec![0; size];
    payload[..4].copy_from_slice(&ENTRY_MAGIC.to_le_bytes());
    let mut references = Vec::new();
    encode_typed_slot(
        heap,
        &mut payload,
        base + layout.key_offset,
        descriptor.key_type,
        key,
        &mut references,
    )?;
    encode_typed_slot(
        heap,
        &mut payload,
        base + layout.value_offset,
        descriptor.value_type,
        value,
        &mut references,
    )?;
    let mut representation = vec![b'E'];
    descriptor.key_type.encode(&mut representation);
    descriptor.value_type.encode(&mut representation);
    heap.allocate(
        Arc::new(ManagedTypeDescriptor::new_specialized(
            descriptor.entry_semantic_id,
            size,
            8,
            references.iter().map(|item| item.0).collect(),
            AllocationClass::Young,
            &representation,
        )?),
        &payload,
        &references,
    )
}

/// Decodes one private indexed-map entry after semantic and magic validation.
pub(super) fn decode_entry(
    heap: &ActorHeap,
    descriptor: &ManagedMapDescriptor,
    entry: TvmRef<MapEntry>,
) -> Result<(ManagedFieldValue, ManagedFieldValue), ManagedMemoryError> {
    if heap.descriptor(entry)?.semantic_id() != descriptor.entry_semantic_id {
        return Err(ManagedMemoryError::CorruptedCollection);
    }
    let payload = heap.read(entry)?;
    if read_u32(payload, 0)? != ENTRY_MAGIC {
        return Err(ManagedMemoryError::CorruptedCollection);
    }
    let layout = flat_entry_layout(descriptor)?;
    let base = align_up(ENTRY_HEADER_BYTES, layout.alignment)?;
    Ok((
        decode_typed_slot(
            heap,
            entry.erase(),
            payload,
            base + layout.key_offset,
            descriptor.key_type,
        )?,
        decode_typed_slot(
            heap,
            entry.erase(),
            payload,
            base + layout.value_offset,
            descriptor.value_type,
        )?,
    ))
}

/// Decodes one flat root's entries.
fn decode_flat_entries(
    heap: &ActorHeap,
    descriptor: &ManagedMapDescriptor,
    map: TvmRef<ManagedMap>,
    payload: &[u8],
    length: usize,
) -> Result<Vec<(ManagedFieldValue, ManagedFieldValue)>, ManagedMemoryError> {
    let layout = flat_entry_layout(descriptor)?;
    (0..length)
        .map(|index| {
            let base = flat_entry_base(layout, index)?;
            Ok((
                decode_typed_slot(
                    heap,
                    map.erase(),
                    payload,
                    base + layout.key_offset,
                    descriptor.key_type,
                )?,
                decode_typed_slot(
                    heap,
                    map.erase(),
                    payload,
                    base + layout.value_offset,
                    descriptor.value_type,
                )?,
            ))
        })
        .collect()
}

/// Materializes entry values from an indexed insertion-order list.
fn entries_from_order(
    heap: &ActorHeap,
    descriptor: &ManagedMapDescriptor,
    order: TvmRef<ManagedList>,
) -> Result<Vec<(ManagedFieldValue, ManagedFieldValue)>, ManagedMemoryError> {
    heap.list_elements(&descriptor.order, order)?
        .into_iter()
        .map(|value| {
            let ManagedFieldValue::Reference(entry) = value else {
                return Err(ManagedMemoryError::CorruptedCollection);
            };
            decode_entry(heap, descriptor, entry.cast())
        })
        .collect()
}

/// Finds one exact private entry reference in the insertion-order list.
fn order_position(
    heap: &ActorHeap,
    descriptor: &ManagedMapDescriptor,
    order: TvmRef<ManagedList>,
    entry: TvmRef<MapEntry>,
) -> Result<usize, ManagedMemoryError> {
    heap.list_elements(&descriptor.order, order)?
        .iter()
        .position(|value| *value == ManagedFieldValue::Reference(entry.erase()))
        .ok_or(ManagedMemoryError::CorruptedCollection)
}

/// Validates and opens one map root.
fn read_root(
    heap: &ActorHeap,
    descriptor: &ManagedMapDescriptor,
    map: TvmRef<ManagedMap>,
) -> Result<RootView, ManagedMemoryError> {
    if heap.descriptor(map)?.semantic_id() != descriptor.semantic_id {
        return Err(ManagedMemoryError::ManagedTypeMismatch);
    }
    let payload = heap.read(map)?;
    if read_u32(payload, 0)? != ROOT_MAGIC {
        return Err(ManagedMemoryError::CorruptedCollection);
    }
    let form = *payload
        .get(4)
        .ok_or(ManagedMemoryError::CorruptedCollection)?;
    let length = read_u64(payload, 8)?;
    let storage = match form {
        FORM_EMPTY if length == 0 && payload.len() == ROOT_HEADER_BYTES => RootStorage::Empty,
        FORM_FLAT if length > 0 => {
            let layout = flat_entry_layout(descriptor)?;
            let expected = ROOT_HEADER_BYTES
                .checked_add(
                    layout
                        .stride
                        .checked_mul(length)
                        .ok_or(ManagedMemoryError::CorruptedCollection)?,
                )
                .ok_or(ManagedMemoryError::CorruptedCollection)?;
            if payload.len() != expected {
                return Err(ManagedMemoryError::CorruptedCollection);
            }
            RootStorage::Flat
        }
        FORM_INDEXED if should_use_indexed_map(length) && payload.len() == INDEXED_ROOT_BYTES => {
            RootStorage::Indexed {
                trie: heap.reference_field(map, INDEXED_TRIE_OFFSET)?.cast(),
                order: heap.reference_field(map, INDEXED_ORDER_OFFSET)?.cast(),
            }
        }
        _ => return Err(ManagedMemoryError::CorruptedCollection),
    };
    Ok(RootView { length, storage })
}

/// Builds one shape-specific map root descriptor.
fn root_descriptor(
    descriptor: &ManagedMapDescriptor,
    form: u8,
    count: usize,
    size: usize,
    reference_offsets: Vec<usize>,
) -> Result<Arc<ManagedTypeDescriptor>, ManagedMemoryError> {
    let mut representation = vec![b'M', form];
    descriptor.key_type.encode(&mut representation);
    descriptor.value_type.encode(&mut representation);
    representation.extend_from_slice(&(count as u64).to_le_bytes());
    ManagedTypeDescriptor::new_specialized(
        descriptor.semantic_id,
        size,
        8,
        reference_offsets,
        AllocationClass::Young,
        &representation,
    )
    .map(Arc::new)
}

/// Computes the repeated packed layout for one flat key/value entry.
fn flat_entry_layout(
    descriptor: &ManagedMapDescriptor,
) -> Result<FlatEntryLayout, ManagedMemoryError> {
    let (key_size, key_alignment) = descriptor.key_type.layout();
    let (value_size, value_alignment) = descriptor.value_type.layout();
    let alignment = key_alignment.max(value_alignment).max(1);
    let key_offset = align_up(0, key_alignment)?;
    let value_offset = align_up(
        key_offset
            .checked_add(key_size)
            .ok_or(ManagedMemoryError::CollectionTooLarge)?,
        value_alignment,
    )?;
    let stride = align_up(
        value_offset
            .checked_add(value_size)
            .ok_or(ManagedMemoryError::CollectionTooLarge)?,
        alignment,
    )?
    .max(1);
    Ok(FlatEntryLayout {
        key_offset,
        value_offset,
        stride,
        alignment,
    })
}

/// Computes one checked flat-root entry base offset.
fn flat_entry_base(layout: FlatEntryLayout, index: usize) -> Result<usize, ManagedMemoryError> {
    align_up(ROOT_HEADER_BYTES, layout.alignment)?
        .checked_add(
            layout
                .stride
                .checked_mul(index)
                .ok_or(ManagedMemoryError::CollectionTooLarge)?,
        )
        .ok_or(ManagedMemoryError::CollectionTooLarge)
}

/// Writes one checked root header.
fn write_root_header(
    payload: &mut [u8],
    form: u8,
    length: usize,
) -> Result<(), ManagedMemoryError> {
    payload[..4].copy_from_slice(&ROOT_MAGIC.to_le_bytes());
    payload[4] = form;
    let length = u64::try_from(length).map_err(|_| ManagedMemoryError::CollectionTooLarge)?;
    payload[8..16].copy_from_slice(&length.to_le_bytes());
    Ok(())
}

/// Computes a process-independent FNV-1a hash for one scalar key encoding.
fn stable_hash(tag: u8, bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in std::iter::once(&tag).chain(bytes) {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    hash
}

/// Reads one checked little-endian `u32` field.
fn read_u32(payload: &[u8], offset: usize) -> Result<u32, ManagedMemoryError> {
    let bytes = payload
        .get(offset..offset + 4)
        .ok_or(ManagedMemoryError::CorruptedCollection)?
        .try_into()
        .map_err(|_| ManagedMemoryError::CorruptedCollection)?;
    Ok(u32::from_le_bytes(bytes))
}

/// Reads one checked collection cardinality.
fn read_u64(payload: &[u8], offset: usize) -> Result<usize, ManagedMemoryError> {
    let bytes = payload
        .get(offset..offset + 8)
        .ok_or(ManagedMemoryError::CorruptedCollection)?
        .try_into()
        .map_err(|_| ManagedMemoryError::CorruptedCollection)?;
    usize::try_from(u64::from_le_bytes(bytes)).map_err(|_| ManagedMemoryError::CorruptedCollection)
}

#[cfg(test)]
#[path = "managed_map_test.rs"]
mod managed_map_test;
