//! Immutable VM-owned bitstring construction, storage, and checked slicing.

use std::fmt;
use std::sync::Arc;

/// Canonical immutable bitstring value.
///
/// Bits use network order inside each byte. Unused low bits in the final byte
/// are always zero, so equality and hashing depend only on the logical value.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct VmBitString {
    bytes: Arc<[u8]>,
    bit_len: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) enum VmBitStringEndian {
    Big,
    Little,
}

/// Stable validation failures for VM bitstring operations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmBitStringError {
    BitLengthOverflow,
    #[cfg(test)]
    ByteLengthMismatch {
        expected: usize,
        actual: usize,
    },
    #[cfg(test)]
    BitLengthMismatch {
        expected: usize,
        actual: usize,
    },
    BitLengthExceedsStorage {
        bit_len: usize,
        available_bits: usize,
    },
    #[cfg(test)]
    RangeOverflow,
    #[cfg(test)]
    RangeOutOfBounds {
        start: usize,
        end: usize,
        bit_len: usize,
    },
    #[cfg(test)]
    NotByteAligned {
        bit_len: usize,
    },
    #[cfg(test)]
    InvalidUtf8Scalar {
        value: i64,
    },
    #[cfg(test)]
    InvalidUtf8ScalarEncoding,
    #[cfg(test)]
    InvalidUtf16Scalar {
        value: i64,
    },
    #[cfg(test)]
    InvalidUtf16ScalarEncoding {
        endian: VmBitStringEndian,
    },
    #[cfg(test)]
    InvalidUtf32Scalar {
        value: i64,
    },
    #[cfg(test)]
    InvalidUtf32ScalarEncoding {
        endian: VmBitStringEndian,
    },
    #[cfg(test)]
    InvalidIntegerWidth {
        bit_width: usize,
    },
    #[cfg(test)]
    IntegerOutOfRange {
        value: i64,
        bit_width: usize,
        signed: bool,
    },
}

impl fmt::Display for VmBitStringError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BitLengthOverflow => write!(formatter, "bit length exceeds host limits"),
            #[cfg(test)]
            Self::ByteLengthMismatch { expected, actual } => write!(
                formatter,
                "expected exactly {expected} bytes, found {actual}"
            ),
            #[cfg(test)]
            Self::BitLengthMismatch { expected, actual } => write!(
                formatter,
                "expected exactly {expected} bits, found {actual}"
            ),
            Self::BitLengthExceedsStorage {
                bit_len,
                available_bits,
            } => write!(
                formatter,
                "bit length {bit_len} exceeds available storage of {available_bits} bits"
            ),
            #[cfg(test)]
            Self::RangeOverflow => write!(formatter, "bitstring slice range overflow"),
            #[cfg(test)]
            Self::RangeOutOfBounds {
                start,
                end,
                bit_len,
            } => write!(
                formatter,
                "bitstring slice range {start}..{end} exceeds bit length {bit_len}"
            ),
            #[cfg(test)]
            Self::NotByteAligned { bit_len } => {
                write!(formatter, "bitstring length {bit_len} is not byte-aligned")
            }
            #[cfg(test)]
            Self::InvalidUtf8Scalar { value } => {
                write!(formatter, "integer {value} is not a valid UTF-8 scalar")
            }
            #[cfg(test)]
            Self::InvalidUtf8ScalarEncoding => {
                write!(
                    formatter,
                    "bitstring does not encode exactly one UTF-8 scalar"
                )
            }
            #[cfg(test)]
            Self::InvalidUtf16Scalar { value } => {
                write!(formatter, "integer {value} is not a valid UTF-16 scalar")
            }
            #[cfg(test)]
            Self::InvalidUtf16ScalarEncoding { endian } => write!(
                formatter,
                "bitstring does not encode exactly one {} UTF-16 scalar",
                endian.label()
            ),
            #[cfg(test)]
            Self::InvalidUtf32Scalar { value } => {
                write!(formatter, "integer {value} is not a valid UTF-32 scalar")
            }
            #[cfg(test)]
            Self::InvalidUtf32ScalarEncoding { endian } => write!(
                formatter,
                "bitstring does not encode exactly one {} UTF-32 scalar",
                endian.label()
            ),
            #[cfg(test)]
            Self::InvalidIntegerWidth { bit_width } => write!(
                formatter,
                "integer bit width {bit_width} must be between 1 and 63"
            ),
            #[cfg(test)]
            Self::IntegerOutOfRange {
                value,
                bit_width,
                signed,
            } => write!(
                formatter,
                "integer {value} does not fit {} {bit_width}-bit field",
                if *signed { "signed" } else { "unsigned" }
            ),
        }
    }
}

#[cfg(test)]
impl VmBitStringEndian {
    const fn label(self) -> &'static str {
        match self {
            Self::Big => "big-endian",
            Self::Little => "little-endian",
        }
    }
}

impl VmBitString {
    /// Builds a byte-aligned bitstring only when storage has the declared size.
    #[cfg(test)]
    pub(crate) fn from_exact_bytes(
        bytes: impl AsRef<[u8]>,
        byte_len: usize,
    ) -> Result<Self, VmBitStringError> {
        let bytes = bytes.as_ref();
        if bytes.len() != byte_len {
            return Err(VmBitStringError::ByteLengthMismatch {
                expected: byte_len,
                actual: bytes.len(),
            });
        }
        let bit_len = byte_len
            .checked_mul(8)
            .ok_or(VmBitStringError::BitLengthOverflow)?;
        Self::from_bytes(bytes, bit_len)
    }

    /// Builds canonical bit storage from a byte prefix and logical bit length.
    pub(crate) fn from_bytes(
        bytes: impl AsRef<[u8]>,
        bit_len: usize,
    ) -> Result<Self, VmBitStringError> {
        let bytes = bytes.as_ref();
        let available_bits = bytes
            .len()
            .checked_mul(8)
            .ok_or(VmBitStringError::BitLengthOverflow)?;
        if bit_len > available_bits {
            return Err(VmBitStringError::BitLengthExceedsStorage {
                bit_len,
                available_bits,
            });
        }

        let storage_len = bit_len
            .checked_add(7)
            .ok_or(VmBitStringError::BitLengthOverflow)?
            / 8;
        let mut canonical = bytes[..storage_len].to_vec();
        mask_unused_trailing_bits(&mut canonical, bit_len);
        Ok(Self {
            bytes: canonical.into(),
            bit_len,
        })
    }

    /// Preserves an existing bitstring only when its logical size is exact.
    #[cfg(test)]
    pub(crate) fn require_exact_bit_len(&self, bit_len: usize) -> Result<Self, VmBitStringError> {
        if self.bit_len != bit_len {
            return Err(VmBitStringError::BitLengthMismatch {
                expected: bit_len,
                actual: self.bit_len,
            });
        }
        Ok(self.clone())
    }

    /// Encodes one Unicode scalar into a byte-aligned bitstring.
    #[cfg(test)]
    pub(crate) fn from_utf8_scalar(value: i64) -> Result<Self, VmBitStringError> {
        let scalar = u32::try_from(value)
            .ok()
            .and_then(char::from_u32)
            .ok_or(VmBitStringError::InvalidUtf8Scalar { value })?;
        let mut buffer = [0u8; 4];
        let encoded = scalar.encode_utf8(&mut buffer);
        Self::from_bytes(encoded.as_bytes(), encoded.len() * 8)
    }

    /// Decodes one exact byte-aligned UTF-8 scalar.
    #[cfg(test)]
    pub(crate) fn to_utf8_scalar(&self) -> Result<i64, VmBitStringError> {
        if !self.is_byte_aligned() {
            return Err(VmBitStringError::NotByteAligned {
                bit_len: self.bit_len,
            });
        }
        let text = std::str::from_utf8(&self.bytes)
            .map_err(|_| VmBitStringError::InvalidUtf8ScalarEncoding)?;
        let mut scalars = text.chars();
        let scalar = scalars
            .next()
            .ok_or(VmBitStringError::InvalidUtf8ScalarEncoding)?;
        if scalars.next().is_some() {
            return Err(VmBitStringError::InvalidUtf8ScalarEncoding);
        }
        Ok(i64::from(u32::from(scalar)))
    }

    /// Encodes one Unicode scalar as one or two UTF-16 code units.
    #[cfg(test)]
    pub(crate) fn from_utf16_scalar(
        value: i64,
        endian: VmBitStringEndian,
    ) -> Result<Self, VmBitStringError> {
        let scalar = u32::try_from(value)
            .ok()
            .and_then(char::from_u32)
            .ok_or(VmBitStringError::InvalidUtf16Scalar { value })?;
        let mut units = [0_u16; 2];
        let encoded = scalar.encode_utf16(&mut units);
        let mut bytes = Vec::with_capacity(encoded.len() * 2);
        for unit in encoded {
            let encoded_unit = match endian {
                VmBitStringEndian::Big => unit.to_be_bytes(),
                VmBitStringEndian::Little => unit.to_le_bytes(),
            };
            bytes.extend_from_slice(&encoded_unit);
        }
        Self::from_bytes(&bytes, bytes.len() * 8)
    }

    /// Decodes exactly one UTF-16 scalar in the requested wire order.
    #[cfg(test)]
    pub(crate) fn to_utf16_scalar(
        &self,
        endian: VmBitStringEndian,
    ) -> Result<i64, VmBitStringError> {
        if !self.is_byte_aligned() {
            return Err(VmBitStringError::NotByteAligned {
                bit_len: self.bit_len,
            });
        }
        if !matches!(self.bytes.len(), 2 | 4) {
            return Err(VmBitStringError::InvalidUtf16ScalarEncoding { endian });
        }
        let units = self.bytes.chunks_exact(2).map(|bytes| match endian {
            VmBitStringEndian::Big => u16::from_be_bytes([bytes[0], bytes[1]]),
            VmBitStringEndian::Little => u16::from_le_bytes([bytes[0], bytes[1]]),
        });
        let mut scalars = char::decode_utf16(units);
        let scalar = scalars
            .next()
            .transpose()
            .map_err(|_| VmBitStringError::InvalidUtf16ScalarEncoding { endian })?
            .ok_or(VmBitStringError::InvalidUtf16ScalarEncoding { endian })?;
        if scalars.next().is_some() {
            return Err(VmBitStringError::InvalidUtf16ScalarEncoding { endian });
        }
        Ok(i64::from(u32::from(scalar)))
    }

    /// Encodes one Unicode scalar as one UTF-32 code unit.
    #[cfg(test)]
    pub(crate) fn from_utf32_scalar(
        value: i64,
        endian: VmBitStringEndian,
    ) -> Result<Self, VmBitStringError> {
        let scalar = u32::try_from(value)
            .ok()
            .and_then(char::from_u32)
            .ok_or(VmBitStringError::InvalidUtf32Scalar { value })?;
        let value = u32::from(scalar);
        let bytes = match endian {
            VmBitStringEndian::Big => value.to_be_bytes(),
            VmBitStringEndian::Little => value.to_le_bytes(),
        };
        Self::from_bytes(bytes, 32)
    }

    /// Decodes exactly one UTF-32 scalar in the requested wire order.
    #[cfg(test)]
    pub(crate) fn to_utf32_scalar(
        &self,
        endian: VmBitStringEndian,
    ) -> Result<i64, VmBitStringError> {
        if !self.is_byte_aligned() {
            return Err(VmBitStringError::NotByteAligned {
                bit_len: self.bit_len,
            });
        }
        let bytes: [u8; 4] = self
            .bytes
            .as_ref()
            .try_into()
            .map_err(|_| VmBitStringError::InvalidUtf32ScalarEncoding { endian })?;
        let value = match endian {
            VmBitStringEndian::Big => u32::from_be_bytes(bytes),
            VmBitStringEndian::Little => u32::from_le_bytes(bytes),
        };
        char::from_u32(value)
            .map(|scalar| i64::from(u32::from(scalar)))
            .ok_or(VmBitStringError::InvalidUtf32ScalarEncoding { endian })
    }

    /// Encodes one checked integer field using the requested wire ordering.
    #[cfg(test)]
    pub(crate) fn from_integer(
        value: i64,
        bit_width: usize,
        signed: bool,
        endian: VmBitStringEndian,
    ) -> Result<Self, VmBitStringError> {
        if !(1..=63).contains(&bit_width) {
            return Err(VmBitStringError::InvalidIntegerWidth { bit_width });
        }
        if !integer_fits(value, bit_width, signed) {
            return Err(VmBitStringError::IntegerOutOfRange {
                value,
                bit_width,
                signed,
            });
        }

        let mask = (1_u64 << bit_width) - 1;
        let raw = (value as u64) & mask;
        let mut output = vec![0u8; bit_width.div_ceil(8)];
        match endian {
            VmBitStringEndian::Big => {
                for output_bit in 0..bit_width {
                    let source_shift = bit_width - output_bit - 1;
                    if raw & (1 << source_shift) != 0 {
                        set_bit(&mut output, output_bit);
                    }
                }
            }
            VmBitStringEndian::Little => {
                let mut output_bit = 0;
                let mut group_start = 0;
                while group_start < bit_width {
                    let group_width = (bit_width - group_start).min(8);
                    for group_bit in (0..group_width).rev() {
                        if raw & (1 << (group_start + group_bit)) != 0 {
                            set_bit(&mut output, output_bit);
                        }
                        output_bit += 1;
                    }
                    group_start += group_width;
                }
            }
        }
        Self::from_bytes(output, bit_width)
    }

    /// Decodes the complete logical value using one checked integer policy.
    #[cfg(test)]
    pub(crate) fn to_integer(
        &self,
        signed: bool,
        endian: VmBitStringEndian,
    ) -> Result<i64, VmBitStringError> {
        decode_integer(&self.bytes, 0, self.bit_len, signed, endian)
    }

    /// Returns the logical bit length.
    pub(crate) const fn bit_len(&self) -> usize {
        self.bit_len
    }

    /// Returns the canonical packed storage length.
    pub(crate) fn byte_len(&self) -> usize {
        self.bytes.len()
    }

    /// Returns whether the logical value ends on a byte boundary.
    #[cfg(test)]
    pub(crate) const fn is_byte_aligned(&self) -> bool {
        self.bit_len % 8 == 0
    }

    /// Returns canonical packed bytes, including the masked final partial byte.
    pub(crate) fn packed_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Shares canonical packed storage with a byte-oriented VM subsystem.
    pub(crate) fn packed_storage(&self) -> Arc<[u8]> {
        Arc::clone(&self.bytes)
    }

    /// Reads one checked network-order bit.
    #[cfg(test)]
    pub(crate) fn bit_at(&self, bit_offset: usize) -> Option<bool> {
        (bit_offset < self.bit_len).then(|| bit_is_set(&self.bytes, bit_offset))
    }

    /// Converts an aligned bitstring to immutable byte storage.
    #[cfg(test)]
    pub(crate) fn to_bytes(&self) -> Result<Arc<[u8]>, VmBitStringError> {
        if !self.is_byte_aligned() {
            return Err(VmBitStringError::NotByteAligned {
                bit_len: self.bit_len,
            });
        }
        Ok(Arc::clone(&self.bytes))
    }

    /// Copies one checked bit range into canonical zero-based storage.
    #[cfg(test)]
    pub(crate) fn slice(&self, start: usize, bit_len: usize) -> Result<Self, VmBitStringError> {
        let end = start
            .checked_add(bit_len)
            .ok_or(VmBitStringError::RangeOverflow)?;
        if end > self.bit_len {
            return Err(VmBitStringError::RangeOutOfBounds {
                start,
                end,
                bit_len: self.bit_len,
            });
        }

        let output_len = bit_len
            .checked_add(7)
            .ok_or(VmBitStringError::BitLengthOverflow)?
            / 8;
        let mut output = vec![0u8; output_len];
        for output_bit in 0..bit_len {
            if bit_is_set(&self.bytes, start + output_bit) {
                set_bit(&mut output, output_bit);
            }
        }
        Self::from_bytes(output, bit_len)
    }

    /// Concatenates two logical bit sequences without inserting byte padding.
    #[cfg(test)]
    pub(crate) fn concat(&self, suffix: &Self) -> Result<Self, VmBitStringError> {
        let bit_len = self
            .bit_len
            .checked_add(suffix.bit_len)
            .ok_or(VmBitStringError::BitLengthOverflow)?;
        let output_len = bit_len
            .checked_add(7)
            .ok_or(VmBitStringError::BitLengthOverflow)?
            / 8;
        let mut output = vec![0u8; output_len];
        copy_set_bits(&self.bytes, self.bit_len, &mut output, 0);
        copy_set_bits(&suffix.bytes, suffix.bit_len, &mut output, self.bit_len);
        Self::from_bytes(output, bit_len)
    }
}

/// Decodes one checked bit range for BitString and Bytes operations.
#[cfg(test)]
pub(crate) fn decode_integer(
    bytes: &[u8],
    bit_offset: usize,
    bit_width: usize,
    signed: bool,
    endian: VmBitStringEndian,
) -> Result<i64, VmBitStringError> {
    if !(1..=63).contains(&bit_width) {
        return Err(VmBitStringError::InvalidIntegerWidth { bit_width });
    }
    let available_bits = bytes
        .len()
        .checked_mul(8)
        .ok_or(VmBitStringError::BitLengthOverflow)?;
    let bit_end = bit_offset
        .checked_add(bit_width)
        .ok_or(VmBitStringError::RangeOverflow)?;
    if bit_end > available_bits {
        return Err(VmBitStringError::RangeOutOfBounds {
            start: bit_offset,
            end: bit_end,
            bit_len: available_bits,
        });
    }

    let raw = match endian {
        VmBitStringEndian::Big => decode_big_endian(bytes, bit_offset, bit_end),
        VmBitStringEndian::Little => decode_little_endian(bytes, bit_offset, bit_end),
    };
    Ok(if signed {
        sign_extend(raw, bit_width)
    } else {
        raw as i64
    })
}

#[cfg(test)]
fn decode_big_endian(bytes: &[u8], bit_offset: usize, bit_end: usize) -> u64 {
    let mut value = 0_u64;
    for bit_index in bit_offset..bit_end {
        value = (value << 1) | u64::from(bit_is_set(bytes, bit_index));
    }
    value
}

#[cfg(test)]
fn decode_little_endian(bytes: &[u8], bit_offset: usize, bit_end: usize) -> u64 {
    let mut value = 0_u64;
    let mut group_start = bit_offset;
    let mut value_shift = 0_usize;
    while group_start < bit_end {
        let group_end = group_start.saturating_add(8).min(bit_end);
        let group = decode_big_endian(bytes, group_start, group_end);
        value |= group << value_shift;
        value_shift += group_end - group_start;
        group_start = group_end;
    }
    value
}

#[cfg(test)]
fn sign_extend(value: u64, bit_width: usize) -> i64 {
    let sign_bit = 1_u64 << (bit_width - 1);
    if value & sign_bit == 0 {
        value as i64
    } else {
        (value | (!0_u64 << bit_width)) as i64
    }
}

fn mask_unused_trailing_bits(bytes: &mut [u8], bit_len: usize) {
    let used_bits = bit_len % 8;
    if used_bits == 0 {
        return;
    }
    if let Some(last) = bytes.last_mut() {
        *last &= u8::MAX << (8 - used_bits);
    }
}

#[cfg(test)]
fn bit_is_set(bytes: &[u8], bit_offset: usize) -> bool {
    let byte = bytes[bit_offset / 8];
    let shift = 7 - (bit_offset % 8);
    byte & (1 << shift) != 0
}

#[cfg(test)]
fn set_bit(bytes: &mut [u8], bit_offset: usize) {
    let shift = 7 - (bit_offset % 8);
    bytes[bit_offset / 8] |= 1 << shift;
}

#[cfg(test)]
fn copy_set_bits(source: &[u8], bit_len: usize, target: &mut [u8], target_start: usize) {
    for source_bit in 0..bit_len {
        if bit_is_set(source, source_bit) {
            set_bit(target, target_start + source_bit);
        }
    }
}

#[cfg(test)]
fn integer_fits(value: i64, bit_width: usize, signed: bool) -> bool {
    if signed {
        let limit = 1_i64 << (bit_width - 1);
        (-limit..limit).contains(&value)
    } else {
        value >= 0 && (bit_width == 63 || value < (1_i64 << bit_width))
    }
}

#[cfg(test)]
#[path = "bitstring_test.rs"]
mod bitstring_test;
