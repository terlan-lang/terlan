//! Canonical source identities embedded in admitted TVM native images.

use object::{BinaryFormat, Object, ObjectSection};

const MAGIC: &[u8; 8] = b"TVMDBG01";

/// One compiler source identity carried by a native function.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TvmNativeDebugRecord {
    /// Source file used to compile the function.
    pub(crate) source_file: String,
    /// Fully qualified Terlan module name.
    pub(crate) module: String,
    /// Terlan function name.
    pub(crate) function: String,
    /// Number of function parameters.
    pub(crate) arity: usize,
    /// Inclusive UTF-8 byte offset where the declaration starts.
    pub(crate) span_start: usize,
    /// Exclusive UTF-8 byte offset where the declaration ends.
    pub(crate) span_end: usize,
    /// Checked CoreIR schema that produced the function.
    pub(crate) core_schema: String,
    /// Compiler proof-readiness classification for the function module.
    pub(crate) proof_readiness: String,
}

/// Encodes ordered native source records into the canonical debug section.
pub(crate) fn encode_tvm_native_debug(records: &[TvmNativeDebugRecord]) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(MAGIC);
    push_u32(&mut bytes, records.len())?;
    for record in records {
        push_string(&mut bytes, &record.source_file)?;
        push_string(&mut bytes, &record.module)?;
        push_string(&mut bytes, &record.function)?;
        push_u32(&mut bytes, record.arity)?;
        push_u64(&mut bytes, record.span_start)?;
        push_u64(&mut bytes, record.span_end)?;
        push_string(&mut bytes, &record.core_schema)?;
        push_string(&mut bytes, &record.proof_readiness)?;
    }
    decode_tvm_native_debug(&bytes)?;
    Ok(bytes)
}

/// Decodes and validates one canonical native debug section.
pub(crate) fn decode_tvm_native_debug(bytes: &[u8]) -> Result<Vec<TvmNativeDebugRecord>, String> {
    let mut input = bytes;
    if take(&mut input, MAGIC.len())? != MAGIC {
        return Err("error[tvm.debug.magic]: invalid native debug section".to_string());
    }
    let count = read_u32(&mut input)? as usize;
    let mut records = Vec::with_capacity(count);
    for _ in 0..count {
        records.push(TvmNativeDebugRecord {
            source_file: read_string(&mut input)?,
            module: read_string(&mut input)?,
            function: read_string(&mut input)?,
            arity: read_u32(&mut input)? as usize,
            span_start: read_u64(&mut input)? as usize,
            span_end: read_u64(&mut input)? as usize,
            core_schema: read_string(&mut input)?,
            proof_readiness: read_string(&mut input)?,
        });
    }
    if !input.is_empty() {
        return Err("error[tvm.debug.trailing]: trailing native debug bytes".to_string());
    }
    Ok(records)
}

/// Extracts and decodes the unique native debug section from an executable image.
pub(crate) fn inspect_tvm_native_debug(
    image_bytes: &[u8],
) -> Result<Vec<TvmNativeDebugRecord>, String> {
    let image = object::File::parse(image_bytes)
        .map_err(|error| format!("error[tvm.debug.native_format]: {error}"))?;
    let section_name = match image.format() {
        BinaryFormat::Elf => ".debug_terlan",
        BinaryFormat::MachO => "__terlan",
        BinaryFormat::Coff => ".debug$T",
        format => {
            return Err(format!(
                "error[tvm.debug.native_format]: unsupported native format {format:?}"
            ));
        }
    };
    let sections = image
        .sections()
        .filter_map(|section| {
            section
                .name()
                .ok()
                .filter(|name| *name == section_name)
                .map(|_| section)
        })
        .collect::<Vec<_>>();
    let [section] = sections.as_slice() else {
        return Err(format!(
            "error[tvm.debug.section]: expected exactly one `{section_name}` section"
        ));
    };
    let bytes = section
        .data()
        .map_err(|error| format!("error[tvm.debug.section]: {error}"))?;
    decode_tvm_native_debug(bytes)
}

/// Appends one length-prefixed UTF-8 field.
fn push_string(output: &mut Vec<u8>, value: &str) -> Result<(), String> {
    push_u32(output, value.len())?;
    output.extend_from_slice(value.as_bytes());
    Ok(())
}

/// Appends one checked little-endian `u32` value.
fn push_u32(output: &mut Vec<u8>, value: usize) -> Result<(), String> {
    let value = u32::try_from(value)
        .map_err(|_| "error[tvm.debug.size]: native debug value exceeds u32".to_string())?;
    output.extend_from_slice(&value.to_le_bytes());
    Ok(())
}

/// Appends one checked little-endian `u64` value.
fn push_u64(output: &mut Vec<u8>, value: usize) -> Result<(), String> {
    let value = u64::try_from(value)
        .map_err(|_| "error[tvm.debug.size]: native debug value exceeds u64".to_string())?;
    output.extend_from_slice(&value.to_le_bytes());
    Ok(())
}

/// Reads one length-prefixed UTF-8 field.
fn read_string(input: &mut &[u8]) -> Result<String, String> {
    let length = read_u32(input)? as usize;
    String::from_utf8(take(input, length)?.to_vec())
        .map_err(|_| "error[tvm.debug.utf8]: invalid native debug string".to_string())
}

/// Reads one little-endian `u32` value.
fn read_u32(input: &mut &[u8]) -> Result<u32, String> {
    Ok(u32::from_le_bytes(
        take(input, 4)?.try_into().expect("exact u32 bytes"),
    ))
}

/// Reads one little-endian `u64` value.
fn read_u64(input: &mut &[u8]) -> Result<u64, String> {
    Ok(u64::from_le_bytes(
        take(input, 8)?.try_into().expect("exact u64 bytes"),
    ))
}

/// Removes an exact prefix from the remaining section bytes.
fn take<'a>(input: &mut &'a [u8], count: usize) -> Result<&'a [u8], String> {
    if input.len() < count {
        return Err("error[tvm.debug.truncated]: truncated native debug section".to_string());
    }
    let (value, remaining) = input.split_at(count);
    *input = remaining;
    Ok(value)
}
