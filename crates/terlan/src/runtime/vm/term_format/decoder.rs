use std::sync::Arc;

use super::{
    validate_epoch, validate_text_field, validate_vm_ref, ReplValue, TetfDistributionEnvelope,
    TetfVmRef, TetfVmRefKind, MAGIC, PROFILE_DISTRIBUTION_ENVELOPE, PROFILE_RUNTIME_TERM, TAG_ATOM,
    TAG_BITSTRING, TAG_BYTES, TAG_DISTRIBUTION_ENVELOPE, TAG_FALSE, TAG_FLOAT_TEXT, TAG_INT,
    TAG_LIST, TAG_MAP, TAG_RECORD, TAG_SET, TAG_STRING, TAG_TRUE, TAG_TUPLE, TAG_TYPE, TAG_UNIT,
    TAG_VM_REF, VERSION,
};
use crate::runtime::vm::bitstring::VmBitString;

const MAX_NESTING_DEPTH: usize = 128;

/// Decodes one complete Terlan-owned runtime value.
///
/// Inputs:
/// - `bytes`: versioned TETF runtime-term bytes.
/// - `declared_atoms`: finite atom manifest admitted by the receiver.
///
/// Output:
/// - The decoded VM value, or a stable validation error.
///
/// Transformation:
/// - Validates the runtime profile, recursively decodes one canonical value,
///   and rejects undeclared atoms, malformed lengths, or trailing data.
#[cfg(test)]
pub(crate) fn decode_tetf(bytes: &[u8], declared_atoms: &[String]) -> Result<ReplValue, String> {
    let mut decoder = Decoder::new(bytes, declared_atoms);
    decoder.read_header(PROFILE_RUNTIME_TERM)?;
    let value = decoder.decode_value(0)?;
    decoder.finish()?;
    Ok(value)
}

/// Decodes one complete Terlan-owned distribution envelope.
///
/// The decoder accepts only canonical TETF v1 payloads. It bounds recursive
/// nesting, validates all declared lengths before allocation, rejects atoms
/// outside the receiver's manifest, and refuses trailing data.
pub(crate) fn decode_tetf_distribution_envelope(
    bytes: &[u8],
    declared_atoms: &[String],
) -> Result<TetfDistributionEnvelope, String> {
    let mut decoder = Decoder::new(bytes, declared_atoms);
    decoder.read_header(PROFILE_DISTRIBUTION_ENVELOPE)?;
    decoder.expect_tag(TAG_DISTRIBUTION_ENVELOPE)?;

    let trace_id = decoder.read_text()?;
    let from_node_id = decoder.read_text()?;
    let to_node_id = decoder.read_text()?;
    let epoch = decoder.read_u64()?;
    validate_text_field("trace_id", &trace_id)?;
    validate_text_field("from_node_id", &from_node_id)?;
    validate_text_field("to_node_id", &to_node_id)?;
    validate_epoch(epoch)?;

    let reference_count = decoder.read_len()?;
    decoder.require_remaining_items(reference_count, 2)?;
    let mut refs = Vec::with_capacity(reference_count);
    for _ in 0..reference_count {
        refs.push(decoder.decode_vm_ref()?);
    }
    let payload = decoder.decode_value(0)?;
    decoder.finish()?;

    Ok(TetfDistributionEnvelope::new(
        trace_id,
        from_node_id,
        to_node_id,
        epoch,
        refs,
        payload,
    ))
}

struct Decoder<'a> {
    bytes: &'a [u8],
    offset: usize,
    declared_atoms: &'a [String],
}

impl<'a> Decoder<'a> {
    fn new(bytes: &'a [u8], declared_atoms: &'a [String]) -> Self {
        Self {
            bytes,
            offset: 0,
            declared_atoms,
        }
    }

    fn read_header(&mut self, profile: u8) -> Result<(), String> {
        if self.read_exact(MAGIC.len())? != MAGIC {
            return Err("error[tetf_header]: invalid TETF magic".to_string());
        }
        let version = self.read_u8()?;
        if version != VERSION {
            return Err(format!(
                "error[tetf_version]: unsupported TETF version {version}"
            ));
        }
        let actual_profile = self.read_u8()?;
        if actual_profile != profile {
            return Err(format!(
                "error[tetf_profile]: expected profile {profile}, found {actual_profile}"
            ));
        }
        Ok(())
    }

    fn decode_vm_ref(&mut self) -> Result<TetfVmRef, String> {
        self.expect_tag(TAG_VM_REF)?;
        let kind = match self.read_u8()? {
            1 => TetfVmRefKind::Process,
            2 => TetfVmRefKind::Monitor,
            3 => TetfVmRefKind::Timer,
            4 => TetfVmRefKind::Resource,
            tag => return Err(format!("error[tetf_ref]: unknown VM reference kind {tag}")),
        };
        let reference = TetfVmRef::new(kind, self.read_text()?, self.read_u64()?, self.read_u64()?);
        validate_vm_ref(&reference)?;
        Ok(reference)
    }

    fn decode_value(&mut self, depth: usize) -> Result<ReplValue, String> {
        if depth >= MAX_NESTING_DEPTH {
            return Err(format!(
                "error[tetf_depth]: nesting exceeds {MAX_NESTING_DEPTH} levels"
            ));
        }
        let tag = self.read_u8()?;
        match tag {
            TAG_UNIT => Ok(ReplValue::Unit),
            TAG_FALSE => Ok(ReplValue::Bool(false)),
            TAG_TRUE => Ok(ReplValue::Bool(true)),
            TAG_INT => Ok(ReplValue::Int(i64::from_be_bytes(self.read_array::<8>()?))),
            TAG_FLOAT_TEXT => Ok(ReplValue::Float(self.read_text()?)),
            TAG_STRING => Ok(ReplValue::String(self.read_text()?)),
            TAG_ATOM => self.decode_atom(),
            TAG_TYPE => Ok(ReplValue::Type(self.read_text()?)),
            TAG_TUPLE => Ok(ReplValue::Tuple(self.decode_sequence(depth)?)),
            TAG_LIST => Ok(ReplValue::List(self.decode_sequence(depth)?)),
            TAG_MAP => self.decode_map(depth),
            TAG_SET => Ok(ReplValue::Set(self.decode_canonical_set(depth)?)),
            TAG_RECORD => self.decode_record(depth),
            TAG_BYTES => {
                let len = self.read_len()?;
                let bytes: Arc<[u8]> = self.read_exact(len)?.to_vec().into();
                Ok(ReplValue::Bytes(bytes))
            }
            TAG_BITSTRING => self.decode_bitstring(),
            _ => Err(format!(
                "error[tetf_tag]: unknown runtime value tag {tag:#04x}"
            )),
        }
    }

    fn decode_atom(&mut self) -> Result<ReplValue, String> {
        let atom = self.read_text()?;
        if !self.declared_atoms.iter().any(|declared| declared == &atom) {
            return Err(format!(
                "error[tetf_atom]: atom `{atom}` is not in the declared atom manifest"
            ));
        }
        Ok(ReplValue::Atom(atom))
    }

    fn decode_sequence(&mut self, depth: usize) -> Result<Vec<ReplValue>, String> {
        let len = self.read_len()?;
        self.require_remaining_items(len, 1)?;
        let mut values = Vec::with_capacity(len);
        for _ in 0..len {
            values.push(self.decode_value(depth + 1)?);
        }
        Ok(values)
    }

    fn decode_map(&mut self, depth: usize) -> Result<ReplValue, String> {
        let len = self.read_len()?;
        self.require_remaining_items(len, 2)?;
        let mut entries = Vec::with_capacity(len);
        let mut previous_key = None::<Vec<u8>>;
        for _ in 0..len {
            let key_start = self.offset;
            let key = self.decode_value(depth + 1)?;
            let encoded_key = self.bytes[key_start..self.offset].to_vec();
            require_strictly_increasing(previous_key.as_deref(), &encoded_key, "map key")?;
            let value = self.decode_value(depth + 1)?;
            previous_key = Some(encoded_key);
            entries.push((key, value));
        }
        Ok(ReplValue::Map(entries))
    }

    fn decode_canonical_set(&mut self, depth: usize) -> Result<Vec<ReplValue>, String> {
        let len = self.read_len()?;
        self.require_remaining_items(len, 1)?;
        let mut values = Vec::with_capacity(len);
        let mut previous_item = None::<Vec<u8>>;
        for _ in 0..len {
            let item_start = self.offset;
            let value = self.decode_value(depth + 1)?;
            let encoded_item = self.bytes[item_start..self.offset].to_vec();
            require_strictly_increasing(previous_item.as_deref(), &encoded_item, "set item")?;
            previous_item = Some(encoded_item);
            values.push(value);
        }
        Ok(values)
    }

    fn decode_record(&mut self, depth: usize) -> Result<ReplValue, String> {
        let name = self.read_text()?;
        validate_text_field("record_name", &name)?;
        let len = self.read_len()?;
        self.require_remaining_items(len, 2)?;
        let mut fields = Vec::with_capacity(len);
        let mut previous_field = None::<String>;
        for _ in 0..len {
            let field = self.read_text()?;
            validate_text_field("record_field", &field)?;
            if previous_field
                .as_ref()
                .is_some_and(|previous| previous >= &field)
            {
                return Err(
                    "error[tetf_canonical]: record fields must be strictly ordered".to_string(),
                );
            }
            let value = self.decode_value(depth + 1)?;
            previous_field = Some(field.clone());
            fields.push((field, value));
        }
        Ok(ReplValue::Record { name, fields })
    }

    fn decode_bitstring(&mut self) -> Result<ReplValue, String> {
        let bit_len = self.read_len()?;
        let byte_len = self.read_len()?;
        let expected_byte_len = bit_len.div_ceil(8);
        if byte_len != expected_byte_len {
            return Err(format!(
                "error[tetf_bitstring]: bit length {bit_len} requires {expected_byte_len} bytes, found {byte_len}"
            ));
        }
        let bytes = self.read_exact(byte_len)?.to_vec();
        let value = VmBitString::from_bytes(&bytes, bit_len)
            .map_err(|error| format!("error[tetf_bitstring]: {error}"))?;
        if value.packed_bytes() != bytes {
            return Err(
                "error[tetf_canonical]: bitstring has non-zero unused trailing bits".to_string(),
            );
        }
        Ok(ReplValue::BitString(value))
    }

    fn expect_tag(&mut self, expected: u8) -> Result<(), String> {
        let actual = self.read_u8()?;
        if actual == expected {
            return Ok(());
        }
        Err(format!(
            "error[tetf_tag]: expected tag {expected:#04x}, found {actual:#04x}"
        ))
    }

    fn read_text(&mut self) -> Result<String, String> {
        let len = self.read_len()?;
        let bytes = self.read_exact(len)?;
        std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|_| "error[tetf_utf8]: text field is not valid UTF-8".to_string())
    }

    fn read_len(&mut self) -> Result<usize, String> {
        Ok(u32::from_be_bytes(self.read_array::<4>()?) as usize)
    }

    fn read_u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_be_bytes(self.read_array::<8>()?))
    }

    fn read_u8(&mut self) -> Result<u8, String> {
        Ok(self.read_exact(1)?[0])
    }

    fn read_array<const N: usize>(&mut self) -> Result<[u8; N], String> {
        let mut bytes = [0; N];
        bytes.copy_from_slice(self.read_exact(N)?);
        Ok(bytes)
    }

    fn read_exact(&mut self, len: usize) -> Result<&'a [u8], String> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or_else(|| "error[tetf_size]: field length overflows host limits".to_string())?;
        let bytes = self.bytes.get(self.offset..end).ok_or_else(|| {
            "error[tetf_truncated]: payload ended before field completed".to_string()
        })?;
        self.offset = end;
        Ok(bytes)
    }

    fn require_remaining_items(&self, count: usize, minimum_bytes: usize) -> Result<(), String> {
        let required = count.checked_mul(minimum_bytes).ok_or_else(|| {
            "error[tetf_size]: declared collection length overflows host limits".to_string()
        })?;
        if required <= self.bytes.len().saturating_sub(self.offset) {
            return Ok(());
        }
        Err("error[tetf_size]: declared collection length exceeds remaining payload".to_string())
    }

    fn finish(self) -> Result<(), String> {
        if self.offset == self.bytes.len() {
            return Ok(());
        }
        Err(format!(
            "error[tetf_trailing]: {} unconsumed payload bytes",
            self.bytes.len() - self.offset
        ))
    }
}

fn require_strictly_increasing(
    previous: Option<&[u8]>,
    current: &[u8],
    item: &str,
) -> Result<(), String> {
    if previous.is_none_or(|previous| previous < current) {
        return Ok(());
    }
    Err(format!(
        "error[tetf_canonical]: {item}s must be strictly ordered"
    ))
}
