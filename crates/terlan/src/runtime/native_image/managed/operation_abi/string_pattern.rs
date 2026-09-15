//! Bounded, full-string capture matching for generated native code.

use super::super::{ActorHeap, ManagedMemoryError, ManagedString, TvmRef};

mod matching;

#[cfg(test)]
mod tests;

const MAGIC: &[u8; 4] = b"TVPS";
const VERSION: u16 = 1;
const MATCHES: u8 = 1;
const EXTRACT: u8 = 2;
const HEADER_BYTES: usize = 12;
const MAX_SEGMENTS: usize = 256;
const MAX_ENCODED_BYTES: usize = 65_536;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ManagedStringCaptureKind {
    String,
    Int,
    Float,
    Bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ManagedStringPatternSegment<'a> {
    Literal(&'a str),
    Capture(ManagedStringCaptureKind),
}

use ManagedStringCaptureKind as Kind;
use ManagedStringPatternSegment as Segment;

#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) fn encode_string_pattern_matches_operation(
    segments: &[Segment<'_>],
) -> Result<Vec<u8>, ManagedMemoryError> {
    encode(MATCHES, segments, 0)
}

#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) fn encode_string_pattern_extract_operation(
    segments: &[Segment<'_>],
    selected: usize,
) -> Result<Vec<u8>, ManagedMemoryError> {
    encode(EXTRACT, segments, selected)
}

pub(super) fn is_string_pattern_operation(encoded: &[u8]) -> bool {
    encoded.starts_with(MAGIC)
}

#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(super) fn string_pattern_result_is_reference(encoded: &[u8]) -> bool {
    decode(encoded).is_ok_and(|operation| {
        operation.tag == EXTRACT
            && capture_kind(&operation.segments, operation.selected) == Some(Kind::String)
    })
}

pub(super) fn execute_string_pattern_operation(
    heap: &mut ActorHeap,
    encoded: &[u8],
    words: &[i64],
) -> Result<u64, ManagedMemoryError> {
    let [word] = words else {
        return Err(ManagedMemoryError::InvalidAggregateArity);
    };
    let operation = decode(encoded)?;
    let text = heap.read_string(super::reference_word(*word)?.cast::<ManagedString>())?;
    let Some(captures) = matching::capture(text, &operation.segments) else {
        return if operation.tag == MATCHES {
            Ok(0)
        } else {
            Err(ManagedMemoryError::InvalidManagedOperation)
        };
    };
    if operation.tag == MATCHES {
        return Ok(1);
    }
    let value = captures
        .get(operation.selected)
        .ok_or(ManagedMemoryError::InvalidManagedOperation)?;
    match value {
        matching::Captured::String(value) => {
            let value = value.to_string();
            heap.allocate_string(&value).map(TvmRef::encoded_abi_word)
        }
        matching::Captured::Int(value) => Ok(*value as u64),
        matching::Captured::Float(value) => Ok(value.to_bits()),
        matching::Captured::Bool(value) => Ok(u64::from(*value)),
    }
}

struct Operation<'a> {
    tag: u8,
    selected: usize,
    segments: Vec<Segment<'a>>,
}

fn capture_kind(segments: &[Segment<'_>], selected: usize) -> Option<Kind> {
    segments
        .iter()
        .filter_map(|segment| match segment {
            Segment::Capture(kind) => Some(*kind),
            Segment::Literal(_) => None,
        })
        .nth(selected)
}

fn validate(segments: &[Segment<'_>]) -> Result<(), ManagedMemoryError> {
    if segments.is_empty() || segments.len() > MAX_SEGMENTS {
        return Err(ManagedMemoryError::InvalidManagedOperation);
    }
    let mut previous_capture = false;
    let mut bytes = HEADER_BYTES;
    for segment in segments {
        match segment {
            Segment::Literal(text) => {
                if text.is_empty() {
                    return Err(ManagedMemoryError::InvalidManagedOperation);
                }
                bytes = bytes
                    .checked_add(5)
                    .and_then(|bytes| bytes.checked_add(text.len()))
                    .ok_or(ManagedMemoryError::InvalidManagedOperation)?;
                previous_capture = false;
            }
            Segment::Capture(_) => {
                if previous_capture {
                    return Err(ManagedMemoryError::InvalidManagedOperation);
                }
                bytes += 1;
                previous_capture = true;
            }
        }
        if bytes > MAX_ENCODED_BYTES {
            return Err(ManagedMemoryError::InvalidManagedOperation);
        }
    }
    Ok(())
}

#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
fn encode(
    tag: u8,
    segments: &[Segment<'_>],
    selected: usize,
) -> Result<Vec<u8>, ManagedMemoryError> {
    validate(segments)?;
    let reference = if tag == EXTRACT {
        capture_kind(segments, selected).ok_or(ManagedMemoryError::InvalidManagedOperation)?
            == Kind::String
    } else {
        false
    };
    let selected =
        u16::try_from(selected).map_err(|_| ManagedMemoryError::InvalidManagedOperation)?;
    let mut encoded = Vec::from(MAGIC.as_slice());
    encoded.extend_from_slice(&VERSION.to_le_bytes());
    encoded.extend([tag, u8::from(reference)]);
    encoded.extend_from_slice(&selected.to_le_bytes());
    encoded.extend_from_slice(&(segments.len() as u16).to_le_bytes());
    for segment in segments {
        match segment {
            Segment::Literal(text) => {
                encoded.push(0);
                encoded.extend_from_slice(&(text.len() as u32).to_le_bytes());
                encoded.extend_from_slice(text.as_bytes());
            }
            Segment::Capture(kind) => encoded.push(match kind {
                Kind::String => 1,
                Kind::Int => 2,
                Kind::Float => 3,
                Kind::Bool => 4,
            }),
        }
    }
    Ok(encoded)
}

fn decode(encoded: &[u8]) -> Result<Operation<'_>, ManagedMemoryError> {
    const INVALID: ManagedMemoryError = ManagedMemoryError::InvalidManagedOperation;
    if encoded.len() < HEADER_BYTES
        || encoded.len() > MAX_ENCODED_BYTES
        || !encoded.starts_with(MAGIC)
        || encoded[4..6] != VERSION.to_le_bytes()
        || !matches!(encoded[6], MATCHES | EXTRACT)
        || encoded[7] > 1
    {
        return Err(INVALID);
    }
    let selected = usize::from(u16::from_le_bytes([encoded[8], encoded[9]]));
    let count = usize::from(u16::from_le_bytes([encoded[10], encoded[11]]));
    if count == 0 || count > MAX_SEGMENTS {
        return Err(INVALID);
    }
    let mut rest = &encoded[HEADER_BYTES..];
    let mut segments = Vec::with_capacity(count);
    for _ in 0..count {
        let (&tag, tail) = rest.split_first().ok_or(INVALID)?;
        rest = tail;
        segments.push(match tag {
            0 => {
                let length = u32::from_le_bytes(
                    rest.get(..4)
                        .ok_or(INVALID)?
                        .try_into()
                        .map_err(|_| INVALID)?,
                ) as usize;
                rest = &rest[4..];
                let text =
                    std::str::from_utf8(rest.get(..length).ok_or(INVALID)?).map_err(|_| INVALID)?;
                rest = &rest[length..];
                Segment::Literal(text)
            }
            1 => Segment::Capture(Kind::String),
            2 => Segment::Capture(Kind::Int),
            3 => Segment::Capture(Kind::Float),
            4 => Segment::Capture(Kind::Bool),
            _ => return Err(INVALID),
        });
    }
    if !rest.is_empty() {
        return Err(INVALID);
    }
    validate(&segments)?;
    let reference = if encoded[6] == EXTRACT {
        capture_kind(&segments, selected).ok_or(INVALID)? == Kind::String
    } else {
        if selected != 0 {
            return Err(INVALID);
        }
        false
    };
    if encoded[7] != u8::from(reference) {
        return Err(INVALID);
    }
    Ok(Operation {
        tag: encoded[6],
        selected,
        segments,
    })
}
