//! Actor-local managed string operations for the generated-code ABI.

use std::collections::HashMap;

use super::super::{managed_string_semantic_id, ManagedFieldType, ManagedString};
use super::{
    option_layout, reference_word, unique_layout, ActorHeap, ManagedFieldValue,
    ManagedLayoutRegistry, ManagedList, ManagedMemoryError, SemanticTypeId, TvmRef,
};

const MAGIC: &[u8; 4] = b"TVMU";
const VERSION: u16 = 1;
const HEADER_BYTES: usize = 8;
const SEMANTIC_BYTES: usize = 16;
const CONTAINS: u8 = 1;
const STARTS_WITH: u8 = 2;
const ENDS_WITH: u8 = 3;
const SPLIT: u8 = 4;
const SPLIT_ONCE: u8 = 5;
const LOWERCASE: u8 = 6;
const REPLACE: u8 = 7;
const SHA256: u8 = 8;
const LENGTH: u8 = 9;
const BYTE_SIZE: u8 = 10;
const TRIM: u8 = 11;
const TRIM_START: u8 = 12;
const TRIM_END: u8 = 13;
const COMPARE: u8 = 14;
const CHARACTERS: u8 = 15;
const CODEPOINTS: u8 = 16;
const UTF8_BYTE_AT: u8 = 17;
const UTF8_SLICE: u8 = 18;
const UTF8_FIND_ANY_BYTE: u8 = 19;
const SPLIT_BYTES: usize = HEADER_BYTES + SEMANTIC_BYTES;
const SPLIT_ONCE_BYTES: usize = HEADER_BYTES + SEMANTIC_BYTES * 2;

/// Encodes exact UTF-8 substring membership.
pub fn encode_string_contains_operation() -> Vec<u8> {
    header(CONTAINS)
}

/// Encodes exact UTF-8 prefix membership.
pub fn encode_string_starts_with_operation() -> Vec<u8> {
    header(STARTS_WITH)
}

/// Encodes exact UTF-8 suffix membership.
pub fn encode_string_ends_with_operation() -> Vec<u8> {
    header(ENDS_WITH)
}

/// Encodes allocation of all split segments into one concrete string list.
pub fn encode_string_split_operation(list_semantic: SemanticTypeId) -> Vec<u8> {
    operation_with_semantics(SPLIT, &[list_semantic])
}

/// Encodes the first split as `Option[{String, String}]`.
pub fn encode_string_split_once_operation(
    option_semantic: SemanticTypeId,
    pair_semantic: SemanticTypeId,
) -> Vec<u8> {
    operation_with_semantics(SPLIT_ONCE, &[option_semantic, pair_semantic])
}

/// Encodes Unicode lowercase conversion into a new managed string.
pub fn encode_string_lowercase_operation() -> Vec<u8> {
    header(LOWERCASE)
}

/// Encodes replacement of every exact substring into a new managed string.
pub fn encode_string_replace_operation() -> Vec<u8> {
    header(REPLACE)
}

/// Encodes a lowercase SHA-256 digest into a new managed string.
pub fn encode_string_sha256_operation() -> Vec<u8> {
    header(SHA256)
}

/// Encodes Unicode scalar-count measurement for one managed string.
pub fn encode_string_length_operation() -> Vec<u8> {
    header(LENGTH)
}

/// Encodes UTF-8 byte-count measurement for one managed string.
pub fn encode_string_byte_size_operation() -> Vec<u8> {
    header(BYTE_SIZE)
}

/// Encodes Unicode whitespace trimming on both ends of a managed string.
pub fn encode_string_trim_operation() -> Vec<u8> {
    header(TRIM)
}

/// Encodes Unicode whitespace trimming at the start of a managed string.
pub fn encode_string_trim_start_operation() -> Vec<u8> {
    header(TRIM_START)
}

/// Encodes Unicode whitespace trimming at the end of a managed string.
pub fn encode_string_trim_end_operation() -> Vec<u8> {
    header(TRIM_END)
}

/// Encodes lexicographic Unicode scalar comparison as the image-local
/// `lt`, `eq`, or `gt` atom.
pub fn encode_string_compare_operation() -> Vec<u8> {
    header(COMPARE)
}

/// Encodes Unicode-scalar iteration into one concrete string list.
pub fn encode_string_characters_operation(list_semantic: SemanticTypeId) -> Vec<u8> {
    operation_with_semantics(CHARACTERS, &[list_semantic])
}

/// Encodes Unicode scalar iteration into one compact integer list.
pub fn encode_string_codepoints_operation(list_semantic: SemanticTypeId) -> Vec<u8> {
    operation_with_semantics(CODEPOINTS, &[list_semantic])
}

/// Encodes indexed access to one UTF-8 byte after a caller-side bounds check.
pub fn encode_string_utf8_byte_at_operation() -> Vec<u8> {
    header(UTF8_BYTE_AT)
}

/// Encodes one validated UTF-8 byte range as a new managed string.
pub fn encode_string_utf8_slice_operation() -> Vec<u8> {
    header(UTF8_SLICE)
}

/// Encodes a forward search for any byte from a validated ASCII candidate set.
pub fn encode_string_utf8_find_any_byte_operation() -> Vec<u8> {
    header(UTF8_FIND_ANY_BYTE)
}

pub(super) fn is_string_operation(encoded: &[u8]) -> bool {
    encoded.starts_with(MAGIC)
}

#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(super) fn string_operation_result_is_reference(encoded: &[u8]) -> bool {
    matches!(
        encoded.get(6).copied(),
        Some(
            SPLIT
                | SPLIT_ONCE
                | LOWERCASE
                | REPLACE
                | SHA256
                | TRIM
                | TRIM_START
                | TRIM_END
                | CHARACTERS
                | CODEPOINTS
                | UTF8_SLICE
        )
    )
}

pub(super) fn execute_string_operation(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    encoded: &[u8],
    words: &[i64],
) -> Result<u64, ManagedMemoryError> {
    validate_header(encoded)?;
    match (encoded[6], encoded.len(), words) {
        (CONTAINS, HEADER_BYTES, [value, pattern]) if encoded[7] == 0 => {
            string_predicate(heap, *value, *pattern, |value, pattern| {
                value.contains(pattern)
            })
        }
        (STARTS_WITH, HEADER_BYTES, [value, prefix]) if encoded[7] == 0 => {
            string_predicate(heap, *value, *prefix, |value, prefix| {
                value.starts_with(prefix)
            })
        }
        (ENDS_WITH, HEADER_BYTES, [value, suffix]) if encoded[7] == 0 => {
            string_predicate(heap, *value, *suffix, |value, suffix| {
                value.ends_with(suffix)
            })
        }
        (SPLIT, SPLIT_BYTES, [value, separator]) if encoded[7] == 0 => split_string(
            heap,
            layouts,
            semantic_at(encoded, HEADER_BYTES)?,
            *value,
            *separator,
        ),
        (SPLIT_ONCE, SPLIT_ONCE_BYTES, [value, separator]) if encoded[7] == 0 => split_string_once(
            heap,
            layouts,
            semantic_at(encoded, HEADER_BYTES)?,
            semantic_at(encoded, HEADER_BYTES + SEMANTIC_BYTES)?,
            *value,
            *separator,
        ),
        (CHARACTERS, SPLIT_BYTES, [value]) if encoded[7] == 0 => {
            string_characters(heap, layouts, semantic_at(encoded, HEADER_BYTES)?, *value)
        }
        (CODEPOINTS, SPLIT_BYTES, [value]) if encoded[7] == 0 => {
            string_codepoints(heap, layouts, semantic_at(encoded, HEADER_BYTES)?, *value)
        }
        (UTF8_BYTE_AT, HEADER_BYTES, [value, index]) if encoded[7] == 0 => {
            let index =
                usize::try_from(*index).map_err(|_| ManagedMemoryError::InvalidAggregateField)?;
            heap.read_string(reference_word(*value)?.cast::<ManagedString>())?
                .as_bytes()
                .get(index)
                .copied()
                .map(u64::from)
                .ok_or(ManagedMemoryError::InvalidAggregateField)
        }
        (UTF8_SLICE, HEADER_BYTES, [value, start, length]) if encoded[7] == 0 => {
            string_utf8_slice(heap, *value, *start, *length)
        }
        (UTF8_FIND_ANY_BYTE, HEADER_BYTES, [value, start, candidates]) if encoded[7] == 0 => {
            string_utf8_find_any_byte(heap, *value, *start, *candidates)
        }
        (LOWERCASE, HEADER_BYTES, [value]) if encoded[7] == 0 => {
            let value = heap
                .read_string(reference_word(*value)?.cast::<ManagedString>())?
                .to_lowercase();
            heap.allocate_string(&value)
                .map(|value| value.erase().encoded_abi_word())
        }
        (REPLACE, HEADER_BYTES, [value, pattern, replacement]) if encoded[7] == 0 => {
            let value = heap
                .read_string(reference_word(*value)?.cast::<ManagedString>())?
                .to_owned();
            let pattern = heap
                .read_string(reference_word(*pattern)?.cast::<ManagedString>())?
                .to_owned();
            let replacement = heap
                .read_string(reference_word(*replacement)?.cast::<ManagedString>())?
                .to_owned();
            heap.allocate_string(&value.replace(&pattern, &replacement))
                .map(|value| value.erase().encoded_abi_word())
        }
        (SHA256, HEADER_BYTES, [value]) if encoded[7] == 0 => {
            use sha2::Digest as _;
            use std::fmt::Write as _;
            let value = heap.read_string(reference_word(*value)?.cast::<ManagedString>())?;
            let digest = sha2::Sha256::digest(value.as_bytes());
            let mut hexadecimal = String::with_capacity(64);
            for byte in digest {
                let _ = write!(&mut hexadecimal, "{byte:02x}");
            }
            heap.allocate_string(&hexadecimal)
                .map(|value| value.erase().encoded_abi_word())
        }
        (LENGTH, HEADER_BYTES, [value]) if encoded[7] == 0 => {
            let length = heap
                .read_string(reference_word(*value)?.cast::<ManagedString>())?
                .chars()
                .count();
            u64::try_from(length).map_err(|_| ManagedMemoryError::AllocationLimitExceeded)
        }
        (BYTE_SIZE, HEADER_BYTES, [value]) if encoded[7] == 0 => {
            let length = heap
                .read_string(reference_word(*value)?.cast::<ManagedString>())?
                .len();
            u64::try_from(length).map_err(|_| ManagedMemoryError::AllocationLimitExceeded)
        }
        (TRIM, HEADER_BYTES, [value]) if encoded[7] == 0 => trim_string(heap, *value, str::trim),
        (TRIM_START, HEADER_BYTES, [value]) if encoded[7] == 0 => {
            trim_string(heap, *value, str::trim_start)
        }
        (TRIM_END, HEADER_BYTES, [value]) if encoded[7] == 0 => {
            trim_string(heap, *value, str::trim_end)
        }
        (COMPARE, HEADER_BYTES, [left, right]) if encoded[7] == 0 => {
            let ordering = {
                let left = heap.read_string(reference_word(*left)?.cast::<ManagedString>())?;
                let right = heap.read_string(reference_word(*right)?.cast::<ManagedString>())?;
                left.cmp(right)
            };
            let identity = match ordering {
                std::cmp::Ordering::Less => "lt",
                std::cmp::Ordering::Equal => "eq",
                std::cmp::Ordering::Greater => "gt",
            };
            Ok(u64::from(layouts.atom_index(identity)?.get()))
        }
        _ => Err(ManagedMemoryError::InvalidManagedOperation),
    }
}

fn trim_string(
    heap: &mut ActorHeap,
    value: i64,
    trim: for<'a> fn(&'a str) -> &'a str,
) -> Result<u64, ManagedMemoryError> {
    let trimmed = {
        let value = heap.read_string(reference_word(value)?.cast::<ManagedString>())?;
        trim(value).to_owned()
    };
    heap.allocate_string(&trimmed)
        .map(|value| value.erase().encoded_abi_word())
}

fn string_predicate(
    heap: &ActorHeap,
    value: i64,
    pattern: i64,
    predicate: fn(&str, &str) -> bool,
) -> Result<u64, ManagedMemoryError> {
    let value = heap.read_string(reference_word(value)?.cast::<ManagedString>())?;
    let pattern = heap.read_string(reference_word(pattern)?.cast::<ManagedString>())?;
    Ok(u64::from(predicate(value, pattern)))
}

fn split_string(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    list_semantic: SemanticTypeId,
    value: i64,
    separator: i64,
) -> Result<u64, ManagedMemoryError> {
    let value = heap
        .read_string(reference_word(value)?.cast::<ManagedString>())?
        .to_owned();
    let separator = heap
        .read_string(reference_word(separator)?.cast::<ManagedString>())?
        .to_owned();
    let segments = value
        .split(&separator)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let mut fields = Vec::with_capacity(segments.len());
    for segment in segments {
        fields.push(ManagedFieldValue::Reference(
            heap.allocate_string(&segment)?.erase(),
        ));
    }
    let descriptor = layouts
        .collection(list_semantic)
        .and_then(|collection| collection.list_descriptor())
        .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
    if descriptor.element_type() != ManagedFieldType::Reference(managed_string_semantic_id()) {
        return Err(ManagedMemoryError::ManagedTypeMismatch);
    }
    heap.list_from_elements(descriptor, &fields)
        .map(|list| list.erase().encoded_abi_word())
}

fn string_characters(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    list_semantic: SemanticTypeId,
    value: i64,
) -> Result<u64, ManagedMemoryError> {
    let source = heap
        .read_string(reference_word(value)?.cast::<ManagedString>())?
        .to_owned();
    let mut fields = Vec::with_capacity(source.chars().count());
    let mut interned = HashMap::<char, TvmRef<()>>::new();
    for character in source.chars() {
        let reference = match interned.get(&character).copied() {
            Some(reference) => reference,
            None => {
                let reference = heap.allocate_string(&character.to_string())?.erase();
                interned.insert(character, reference);
                reference
            }
        };
        fields.push(ManagedFieldValue::Reference(reference));
    }
    let descriptor = layouts
        .collection(list_semantic)
        .and_then(|collection| collection.list_descriptor())
        .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
    if descriptor.element_type() != ManagedFieldType::Reference(managed_string_semantic_id()) {
        return Err(ManagedMemoryError::ManagedTypeMismatch);
    }
    heap.list_from_elements(descriptor, &fields)
        .map(|list| list.erase().encoded_abi_word())
}

fn string_codepoints(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    list_semantic: SemanticTypeId,
    value: i64,
) -> Result<u64, ManagedMemoryError> {
    let source = heap
        .read_string(reference_word(value)?.cast::<ManagedString>())?
        .to_owned();
    let fields = source
        .chars()
        .map(|character| ManagedFieldValue::Int(i64::from(u32::from(character))))
        .collect::<Vec<_>>();
    let descriptor = layouts
        .collection(list_semantic)
        .and_then(|collection| collection.list_descriptor())
        .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
    if descriptor.element_type() != ManagedFieldType::Int {
        return Err(ManagedMemoryError::ManagedTypeMismatch);
    }
    heap.list_from_elements(descriptor, &fields)
        .map(|list| list.erase().encoded_abi_word())
}

fn string_utf8_slice(
    heap: &mut ActorHeap,
    value: i64,
    start: i64,
    length: i64,
) -> Result<u64, ManagedMemoryError> {
    let start = usize::try_from(start).map_err(|_| ManagedMemoryError::InvalidAggregateField)?;
    let length = usize::try_from(length).map_err(|_| ManagedMemoryError::InvalidAggregateField)?;
    let end = start
        .checked_add(length)
        .ok_or(ManagedMemoryError::InvalidAggregateField)?;
    let slice = heap
        .read_string(reference_word(value)?.cast::<ManagedString>())?
        .as_bytes()
        .get(start..end)
        .ok_or(ManagedMemoryError::InvalidAggregateField)?;
    let slice =
        std::str::from_utf8(slice).map_err(|_| ManagedMemoryError::InvalidAggregateField)?;
    let owned = slice.to_owned();
    heap.allocate_string(&owned)
        .map(|value| value.erase().encoded_abi_word())
}

fn string_utf8_find_any_byte(
    heap: &ActorHeap,
    value: i64,
    start: i64,
    candidates: i64,
) -> Result<u64, ManagedMemoryError> {
    let start = usize::try_from(start).map_err(|_| ManagedMemoryError::InvalidAggregateField)?;
    let value = heap.read_string(reference_word(value)?.cast::<ManagedString>())?;
    if start > value.len() {
        return Err(ManagedMemoryError::InvalidAggregateField);
    }
    let candidates = heap.read_string(reference_word(candidates)?.cast::<ManagedString>())?;
    if !candidates.is_ascii() {
        return Err(ManagedMemoryError::InvalidAggregateField);
    }
    let found = value.as_bytes()[start..]
        .iter()
        .position(|byte| candidates.as_bytes().contains(byte))
        .map(|offset| start + offset)
        .map_or(Ok(-1_i64), |index| {
            i64::try_from(index).map_err(|_| ManagedMemoryError::InvalidSequenceLength)
        })?;
    Ok(u64::from_ne_bytes(found.to_ne_bytes()))
}

fn split_string_once(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    option_semantic: SemanticTypeId,
    pair_semantic: SemanticTypeId,
    value: i64,
    separator: i64,
) -> Result<u64, ManagedMemoryError> {
    let value = heap
        .read_string(reference_word(value)?.cast::<ManagedString>())?
        .to_owned();
    let separator = heap
        .read_string(reference_word(separator)?.cast::<ManagedString>())?
        .to_owned();
    let Some((left, right)) = value.split_once(&separator) else {
        let layout = option_layout(layouts, option_semantic, "None", 0)?;
        return heap
            .allocate_aggregate_ref(layout, &[])
            .map(|option| option.erase().encoded_abi_word());
    };
    let left = heap.allocate_string(left)?.erase();
    let right = heap.allocate_string(right)?.erase();
    let pair_layout = unique_layout(layouts, pair_semantic, 2)?;
    let pair = heap.allocate_aggregate_ref(
        pair_layout,
        &[
            ManagedFieldValue::Reference(left),
            ManagedFieldValue::Reference(right),
        ],
    )?;
    let option_layout = option_layout(layouts, option_semantic, "Some", 1)?;
    heap.allocate_aggregate_ref(option_layout, &[ManagedFieldValue::Reference(pair.erase())])
        .map(|option| option.erase().encoded_abi_word())
}

fn operation_with_semantics(operation: u8, semantics: &[SemanticTypeId]) -> Vec<u8> {
    let mut encoded = header(operation);
    for semantic in semantics {
        encoded.extend_from_slice(&semantic.bytes());
    }
    encoded
}

fn header(operation: u8) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(SPLIT_ONCE_BYTES);
    encoded.extend_from_slice(MAGIC);
    encoded.extend_from_slice(&VERSION.to_le_bytes());
    encoded.push(operation);
    encoded.push(0);
    encoded
}

fn validate_header(encoded: &[u8]) -> Result<(), ManagedMemoryError> {
    if encoded.len() < HEADER_BYTES
        || encoded.get(..4) != Some(MAGIC)
        || encoded.get(4..6) != Some(&VERSION.to_le_bytes())
    {
        return Err(ManagedMemoryError::InvalidManagedOperation);
    }
    Ok(())
}

fn semantic_at(encoded: &[u8], offset: usize) -> Result<SemanticTypeId, ManagedMemoryError> {
    encoded
        .get(offset..offset + SEMANTIC_BYTES)
        .and_then(|bytes| <[u8; SEMANTIC_BYTES]>::try_from(bytes).ok())
        .map(SemanticTypeId::from_bytes)
        .ok_or(ManagedMemoryError::InvalidManagedOperation)
}

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
