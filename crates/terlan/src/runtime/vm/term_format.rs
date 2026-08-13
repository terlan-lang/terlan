use super::ReplValue;

mod decoder;

pub(crate) use decoder::decode_tetf_distribution_envelope;

#[cfg(test)]
pub(crate) use decoder::decode_tetf;

const MAGIC: &[u8; 4] = b"TETF";
const VERSION: u8 = 1;
const PROFILE_RUNTIME_TERM: u8 = 1;
const PROFILE_DISTRIBUTION_ENVELOPE: u8 = 2;

const TAG_UNIT: u8 = 0x01;
const TAG_FALSE: u8 = 0x02;
const TAG_TRUE: u8 = 0x03;
const TAG_INT: u8 = 0x04;
const TAG_FLOAT_TEXT: u8 = 0x05;
const TAG_STRING: u8 = 0x06;
const TAG_ATOM: u8 = 0x07;
const TAG_TYPE: u8 = 0x08;
const TAG_TUPLE: u8 = 0x09;
const TAG_LIST: u8 = 0x0a;
const TAG_MAP: u8 = 0x0b;
const TAG_SET: u8 = 0x0c;
const TAG_RECORD: u8 = 0x0d;
const TAG_BYTES: u8 = 0x0e;
const TAG_BITSTRING: u8 = 0x0f;
const TAG_VM_REF: u8 = 0x20;
const TAG_DISTRIBUTION_ENVELOPE: u8 = 0x21;

/// VM reference kind admitted into TETF control envelopes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TetfVmRefKind {
    Process,
    Monitor,
    Timer,
    Resource,
}

impl TetfVmRefKind {
    /// Returns the stable TETF tag payload for this VM reference kind.
    const fn tag(self) -> u8 {
        match self {
            Self::Process => 1,
            Self::Monitor => 2,
            Self::Timer => 3,
            Self::Resource => 4,
        }
    }
}

/// VM reference value encoded by Terlan-owned TETF.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TetfVmRef {
    pub(crate) kind: TetfVmRefKind,
    pub(crate) node_id: String,
    pub(crate) local_id: u64,
    pub(crate) epoch: u64,
}

impl TetfVmRef {
    /// Creates a VM reference with node and epoch identity.
    pub(crate) fn new(
        kind: TetfVmRefKind,
        node_id: impl Into<String>,
        local_id: u64,
        epoch: u64,
    ) -> Self {
        Self {
            kind,
            node_id: node_id.into(),
            local_id,
            epoch,
        }
    }
}

/// Distribution-ready message envelope encoded by TETF.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TetfDistributionEnvelope {
    pub(crate) trace_id: String,
    pub(crate) from_node_id: String,
    pub(crate) to_node_id: String,
    pub(crate) epoch: u64,
    pub(crate) refs: Vec<TetfVmRef>,
    pub(crate) payload: ReplValue,
}

impl TetfDistributionEnvelope {
    /// Creates a distribution envelope with trace, node, epoch, refs, and payload.
    pub(crate) fn new(
        trace_id: impl Into<String>,
        from_node_id: impl Into<String>,
        to_node_id: impl Into<String>,
        epoch: u64,
        refs: Vec<TetfVmRef>,
        payload: ReplValue,
    ) -> Self {
        Self {
            trace_id: trace_id.into(),
            from_node_id: from_node_id.into(),
            to_node_id: to_node_id.into(),
            epoch,
            refs,
            payload,
        }
    }
}

/// Encodes one Terlan VM value into the initial Terlan External Term Format.
///
/// Inputs:
/// - `value`: VM evaluator value to encode.
/// - `declared_atoms`: finite atom manifest admitted by the compiler/runtime.
///
/// Output:
/// - Versioned deterministic binary payload, or a stable encoding error.
///
/// Transformation:
/// - Writes a `TETF` envelope followed by a compact recursive term. The format
///   is Terlan-owned and intentionally not Erlang ETF-compatible.
#[cfg(test)]
pub(crate) fn encode_tetf(value: &ReplValue, declared_atoms: &[String]) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(MAGIC);
    bytes.push(VERSION);
    bytes.push(PROFILE_RUNTIME_TERM);
    encode_value(value, declared_atoms, &mut bytes)?;
    Ok(bytes)
}

/// Encodes a VM reference as a standalone TETF control value.
///
/// Inputs:
/// - `reference`: VM ref with kind, node id, local id, and epoch.
///
/// Output:
/// - Versioned deterministic TETF bytes.
///
/// Transformation:
/// - Writes a Terlan-owned reference term. The format is not ETF-compatible.
#[cfg(test)]
pub(crate) fn encode_tetf_vm_ref(reference: &TetfVmRef) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(MAGIC);
    bytes.push(VERSION);
    bytes.push(PROFILE_DISTRIBUTION_ENVELOPE);
    encode_vm_ref(reference, &mut bytes)?;
    Ok(bytes)
}

/// Encodes a distribution envelope as TETF control data.
///
/// Inputs:
/// - `envelope`: traceable message envelope with refs and VM payload.
/// - `declared_atoms`: finite atom manifest admitted by the compiler/runtime.
///
/// Output:
/// - Versioned deterministic TETF bytes, or a stable encoding error.
///
/// Transformation:
/// - Writes metadata before the payload so transports can route, trace, and
///   reject stale epochs without decoding a Terlan value first.
pub(crate) fn encode_tetf_distribution_envelope(
    envelope: &TetfDistributionEnvelope,
    declared_atoms: &[String],
) -> Result<Vec<u8>, String> {
    validate_text_field("trace_id", &envelope.trace_id)?;
    validate_text_field("from_node_id", &envelope.from_node_id)?;
    validate_text_field("to_node_id", &envelope.to_node_id)?;
    validate_epoch(envelope.epoch)?;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(MAGIC);
    bytes.push(VERSION);
    bytes.push(PROFILE_DISTRIBUTION_ENVELOPE);
    bytes.push(TAG_DISTRIBUTION_ENVELOPE);
    write_text(&mut bytes, &envelope.trace_id)?;
    write_text(&mut bytes, &envelope.from_node_id)?;
    write_text(&mut bytes, &envelope.to_node_id)?;
    write_u64(&mut bytes, envelope.epoch);
    write_len(&mut bytes, envelope.refs.len())?;
    for reference in &envelope.refs {
        encode_vm_ref(reference, &mut bytes)?;
    }
    encode_value(&envelope.payload, declared_atoms, &mut bytes)?;
    Ok(bytes)
}

/// Appends one recursive TETF value payload without writing the outer envelope.
fn encode_value(
    value: &ReplValue,
    declared_atoms: &[String],
    bytes: &mut Vec<u8>,
) -> Result<(), String> {
    match value {
        ReplValue::Unit => bytes.push(TAG_UNIT),
        ReplValue::Bool(false) => bytes.push(TAG_FALSE),
        ReplValue::Bool(true) => bytes.push(TAG_TRUE),
        ReplValue::Int(value) => {
            bytes.push(TAG_INT);
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        ReplValue::Float(value) => {
            bytes.push(TAG_FLOAT_TEXT);
            write_text(bytes, value)?;
        }
        ReplValue::String(value) => {
            bytes.push(TAG_STRING);
            write_text(bytes, value)?;
        }
        ReplValue::StringBytes(value) => {
            let value = std::str::from_utf8(value)
                .map_err(|error| format!("error[tetf_string_utf8]: {error}"))?;
            bytes.push(TAG_STRING);
            write_text(bytes, value)?;
        }
        ReplValue::Bytes(value) => {
            bytes.push(TAG_BYTES);
            write_len(bytes, value.len())?;
            bytes.extend_from_slice(value);
        }
        ReplValue::BitString(value) => {
            bytes.push(TAG_BITSTRING);
            write_len(bytes, value.bit_len())?;
            write_len(bytes, value.byte_len())?;
            bytes.extend_from_slice(value.packed_bytes());
        }
        ReplValue::Atom(value) => {
            if !declared_atoms.iter().any(|atom| atom == value) {
                return Err(format!(
                    "error[tetf_atom]: atom `{value}` is not in the declared atom manifest"
                ));
            }
            bytes.push(TAG_ATOM);
            write_text(bytes, value)?;
        }
        ReplValue::Type(value) => {
            bytes.push(TAG_TYPE);
            write_text(bytes, value)?;
        }
        ReplValue::Tuple(items) => {
            bytes.push(TAG_TUPLE);
            write_len(bytes, items.len())?;
            for item in items {
                encode_value(item, declared_atoms, bytes)?;
            }
        }
        ReplValue::Record { name, fields } => {
            validate_text_field("record_name", name)?;
            bytes.push(TAG_RECORD);
            write_text(bytes, name)?;
            let mut encoded_fields = Vec::new();
            for (field, value) in fields {
                validate_text_field("record_field", field)?;
                let mut encoded_value = Vec::new();
                encode_value(value, declared_atoms, &mut encoded_value)?;
                encoded_fields.push((field, encoded_value));
            }
            encoded_fields.sort_by(|left, right| left.0.cmp(right.0));
            if let Some(duplicate) = encoded_fields
                .windows(2)
                .find(|pair| pair[0].0 == pair[1].0)
            {
                return Err(format!(
                    "error[tetf_canonical]: duplicate record field `{}`",
                    duplicate[0].0
                ));
            }
            write_len(bytes, encoded_fields.len())?;
            for (field, value) in encoded_fields {
                write_text(bytes, field)?;
                bytes.extend_from_slice(&value);
            }
        }
        ReplValue::List(items) => {
            bytes.push(TAG_LIST);
            write_len(bytes, items.len())?;
            for item in items {
                encode_value(item, declared_atoms, bytes)?;
            }
        }
        ReplValue::Map(entries) => {
            bytes.push(TAG_MAP);
            let mut encoded_entries = Vec::new();
            for (key, value) in entries {
                let mut encoded_key = Vec::new();
                let mut encoded_value = Vec::new();
                encode_value(key, declared_atoms, &mut encoded_key)?;
                encode_value(value, declared_atoms, &mut encoded_value)?;
                encoded_entries.push((encoded_key, encoded_value));
            }
            encoded_entries.sort_by(|left, right| left.0.cmp(&right.0));
            reject_duplicate_map_keys(&encoded_entries)?;
            write_len(bytes, encoded_entries.len())?;
            for (key, value) in encoded_entries {
                bytes.extend_from_slice(&key);
                bytes.extend_from_slice(&value);
            }
        }
        ReplValue::MapIndexed(map) => {
            bytes.push(TAG_MAP);
            let mut encoded_entries = Vec::new();
            for (key, value) in map.to_entries() {
                let mut encoded_key = Vec::new();
                let mut encoded_value = Vec::new();
                encode_value(&key, declared_atoms, &mut encoded_key)?;
                encode_value(&value, declared_atoms, &mut encoded_value)?;
                encoded_entries.push((encoded_key, encoded_value));
            }
            encoded_entries.sort_by(|left, right| left.0.cmp(&right.0));
            reject_duplicate_map_keys(&encoded_entries)?;
            write_len(bytes, encoded_entries.len())?;
            for (key, value) in encoded_entries {
                bytes.extend_from_slice(&key);
                bytes.extend_from_slice(&value);
            }
        }
        ReplValue::Set(items) => {
            bytes.push(TAG_SET);
            let mut encoded_items = Vec::new();
            for item in items {
                let mut encoded = Vec::new();
                encode_value(item, declared_atoms, &mut encoded)?;
                encoded_items.push(encoded);
            }
            encoded_items.sort();
            encoded_items.dedup();
            write_len(bytes, encoded_items.len())?;
            for item in encoded_items {
                bytes.extend_from_slice(&item);
            }
        }
        ReplValue::RandomGenerator(_) => {
            return Err(
                "error[tetf_unsupported]: random generator state has no TETF encoding".to_string(),
            );
        }
        ReplValue::Iterator { .. } => {
            return Err("error[tetf_unsupported]: iterator state has no TETF encoding".to_string());
        }
    }
    Ok(())
}

/// Writes a UTF-8 text field as a checked length followed by raw bytes.
fn write_text(bytes: &mut Vec<u8>, value: &str) -> Result<(), String> {
    write_len(bytes, value.len())?;
    bytes.extend_from_slice(value.as_bytes());
    Ok(())
}

/// Appends one VM reference payload.
fn encode_vm_ref(reference: &TetfVmRef, bytes: &mut Vec<u8>) -> Result<(), String> {
    validate_vm_ref(reference)?;
    bytes.push(TAG_VM_REF);
    bytes.push(reference.kind.tag());
    write_text(bytes, &reference.node_id)?;
    write_u64(bytes, reference.local_id);
    write_u64(bytes, reference.epoch);
    Ok(())
}

/// Validates a TETF text metadata field before serialization.
fn validate_text_field(field: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!(
            "error[tetf_invalid_metadata]: `{field}` must not be empty"
        ));
    }
    Ok(())
}

/// Validates VM reference metadata before it crosses a runtime boundary.
fn validate_vm_ref(reference: &TetfVmRef) -> Result<(), String> {
    validate_text_field("node_id", &reference.node_id)?;
    if reference.local_id == 0 {
        return Err("error[tetf_invalid_ref]: VM reference local id must be non-zero".to_string());
    }
    validate_epoch(reference.epoch)?;
    Ok(())
}

/// Validates epoch metadata before serializing cross-runtime control data.
fn validate_epoch(epoch: u64) -> Result<(), String> {
    if epoch == 0 {
        return Err("error[tetf_stale_ref]: VM reference epoch must be non-zero".to_string());
    }
    Ok(())
}

/// Writes a u64 in stable big-endian order.
fn write_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

/// Writes a TETF v1 length field after enforcing the u32 size limit.
fn write_len(bytes: &mut Vec<u8>, len: usize) -> Result<(), String> {
    let len = u32::try_from(len)
        .map_err(|_| "error[tetf_size]: term length exceeds TETF v1 u32 limit".to_string())?;
    bytes.extend_from_slice(&len.to_be_bytes());
    Ok(())
}

/// Rejects duplicate canonical map keys before emitting a malleable payload.
fn reject_duplicate_map_keys(entries: &[(Vec<u8>, Vec<u8>)]) -> Result<(), String> {
    if entries.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return Err("error[tetf_canonical]: duplicate map key".to_string());
    }
    Ok(())
}

#[cfg(test)]
#[path = "term_format_runtime_test.rs"]
#[cfg(test)]
mod term_format_runtime_test;
