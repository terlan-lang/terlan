//! Bounded scatter/gather normalization for VM-owned byte values.

use std::fmt;
#[cfg(test)]
use std::io::IoSlice;
use std::sync::Arc;

use super::ReplValue;

const DEFAULT_MAX_SEGMENTS: usize = 1_024;
const DEFAULT_MAX_NODES: usize = 65_536;

/// Resource limits applied before a normalized vector can reach a driver.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmIoVectorLimits {
    pub(crate) max_bytes: usize,
    pub(crate) max_segments: usize,
    pub(crate) max_nodes: usize,
}

impl VmIoVectorLimits {
    pub(crate) const fn for_byte_limit(max_bytes: usize) -> Self {
        Self {
            max_bytes,
            max_segments: DEFAULT_MAX_SEGMENTS,
            max_nodes: DEFAULT_MAX_NODES,
        }
    }

    #[cfg(test)]
    pub(crate) const fn new(max_bytes: usize, max_segments: usize, max_nodes: usize) -> Self {
        Self {
            max_bytes,
            max_segments,
            max_nodes,
        }
    }
}

/// Stable rejection reasons for malformed or over-budget VM iodata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmIoVectorError {
    InvalidByte(i64),
    NonByteAlignedBitString { bit_len: usize },
    UnsupportedValue,
    ByteCountOverflow,
    ByteLimitExceeded { bytes: usize, limit: usize },
    SegmentLimitExceeded { segments: usize, limit: usize },
    NodeLimitExceeded { nodes: usize, limit: usize },
}

impl fmt::Display for VmIoVectorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidByte(value) => {
                write!(
                    formatter,
                    "integer {value} is outside the byte range 0..=255"
                )
            }
            Self::NonByteAlignedBitString { bit_len } => {
                write!(formatter, "bitstring length {bit_len} is not byte-aligned")
            }
            Self::UnsupportedValue => {
                write!(formatter, "VM iodata contains a non-byte value")
            }
            Self::ByteCountOverflow => write!(formatter, "VM iodata byte count overflow"),
            Self::ByteLimitExceeded { bytes, limit } => {
                write!(formatter, "VM iodata is {bytes} bytes; limit is {limit}")
            }
            Self::SegmentLimitExceeded { segments, limit } => {
                write!(
                    formatter,
                    "VM iodata has {segments} segments; limit is {limit}"
                )
            }
            Self::NodeLimitExceeded { nodes, limit } => {
                write!(formatter, "VM iodata has {nodes} nodes; limit is {limit}")
            }
        }
    }
}

/// Immutable, validated scatter/gather bytes ready for a VM-owned transport.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmIoVector {
    segments: Vec<Arc<[u8]>>,
    total_len: usize,
}

impl VmIoVector {
    /// Normalizes VM iodata iteratively, publishing no partial result on error.
    pub(crate) fn from_value(
        value: &ReplValue,
        limits: VmIoVectorLimits,
    ) -> Result<Self, VmIoVectorError> {
        let mut pending = vec![value];
        let mut segments = Vec::new();
        let mut scalar_run = Vec::new();
        let mut total_len = 0usize;
        let mut visited = 0usize;

        while let Some(value) = pending.pop() {
            visited = visited
                .checked_add(1)
                .ok_or(VmIoVectorError::NodeLimitExceeded {
                    nodes: usize::MAX,
                    limit: limits.max_nodes,
                })?;
            if visited > limits.max_nodes {
                return Err(VmIoVectorError::NodeLimitExceeded {
                    nodes: visited,
                    limit: limits.max_nodes,
                });
            }

            match value {
                ReplValue::Int(value) => {
                    let byte =
                        u8::try_from(*value).map_err(|_| VmIoVectorError::InvalidByte(*value))?;
                    add_bytes(&mut total_len, 1, limits.max_bytes)?;
                    scalar_run.push(byte);
                }
                ReplValue::Bytes(bytes) => {
                    if !bytes.is_empty() {
                        flush_scalar_run(&mut segments, &mut scalar_run, limits.max_segments)?;
                        add_bytes(&mut total_len, bytes.len(), limits.max_bytes)?;
                        push_segment(&mut segments, Arc::clone(bytes), limits.max_segments)?;
                    }
                }
                ReplValue::BitString(bits) => {
                    if bits.bit_len() % 8 != 0 {
                        return Err(VmIoVectorError::NonByteAlignedBitString {
                            bit_len: bits.bit_len(),
                        });
                    }
                    if !bits.packed_bytes().is_empty() {
                        flush_scalar_run(&mut segments, &mut scalar_run, limits.max_segments)?;
                        add_bytes(&mut total_len, bits.packed_bytes().len(), limits.max_bytes)?;
                        push_segment(&mut segments, bits.packed_storage(), limits.max_segments)?;
                    }
                }
                ReplValue::List(items) => {
                    pending.extend(items.iter().rev());
                }
                _ => return Err(VmIoVectorError::UnsupportedValue),
            }
        }
        flush_scalar_run(&mut segments, &mut scalar_run, limits.max_segments)?;
        Ok(Self {
            segments,
            total_len,
        })
    }

    #[cfg(test)]
    pub(crate) fn total_len(&self) -> usize {
        self.total_len
    }

    #[cfg(test)]
    pub(crate) fn segments(&self) -> &[Arc<[u8]>] {
        &self.segments
    }

    /// Returns the number of immutable scatter/gather segments.
    pub(crate) fn segment_count(&self) -> usize {
        self.segments.len()
    }

    #[cfg(test)]
    pub(crate) fn as_io_slices(&self) -> Vec<IoSlice<'_>> {
        self.segments
            .iter()
            .map(|segment| IoSlice::new(segment))
            .collect()
    }

    pub(crate) fn flatten(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.total_len);
        for segment in &self.segments {
            bytes.extend_from_slice(segment);
        }
        bytes
    }
}

fn add_bytes(total: &mut usize, added: usize, limit: usize) -> Result<(), VmIoVectorError> {
    let projected = total
        .checked_add(added)
        .ok_or(VmIoVectorError::ByteCountOverflow)?;
    if projected > limit {
        return Err(VmIoVectorError::ByteLimitExceeded {
            bytes: projected,
            limit,
        });
    }
    *total = projected;
    Ok(())
}

fn flush_scalar_run(
    segments: &mut Vec<Arc<[u8]>>,
    scalar_run: &mut Vec<u8>,
    limit: usize,
) -> Result<(), VmIoVectorError> {
    if scalar_run.is_empty() {
        return Ok(());
    }
    let run = Arc::from(std::mem::take(scalar_run));
    push_segment(segments, run, limit)
}

fn push_segment(
    segments: &mut Vec<Arc<[u8]>>,
    segment: Arc<[u8]>,
    limit: usize,
) -> Result<(), VmIoVectorError> {
    let projected = segments
        .len()
        .checked_add(1)
        .ok_or(VmIoVectorError::SegmentLimitExceeded {
            segments: usize::MAX,
            limit,
        })?;
    if projected > limit {
        return Err(VmIoVectorError::SegmentLimitExceeded {
            segments: projected,
            limit,
        });
    }
    segments.push(segment);
    Ok(())
}

#[cfg(test)]
#[path = "iovec_beam_suite_parity_test.rs"]
mod iovec_beam_suite_parity_test;
