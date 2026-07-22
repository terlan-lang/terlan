//! Primitive codecs for the frozen native-image descriptor format.

use super::{
    validate_text, TvmBoundaryType, TvmDependencyDescriptor, TvmImageIntegrity,
    TvmNativeResourceDescriptor, TvmSignatureDescriptor,
};

pub(super) fn encode_boundary_type(bytes: &mut Vec<u8>, ty: &TvmBoundaryType) {
    match ty {
        TvmBoundaryType::Unit => bytes.push(0),
        TvmBoundaryType::Bool => bytes.push(1),
        TvmBoundaryType::Int => bytes.push(2),
        TvmBoundaryType::Float => bytes.push(3),
        TvmBoundaryType::Binary => bytes.push(4),
        TvmBoundaryType::String => bytes.push(5),
        TvmBoundaryType::Json => bytes.push(6),
        TvmBoundaryType::NativeResource(id) => {
            bytes.push(7);
            push_u64(bytes, *id);
        }
        TvmBoundaryType::Atom => bytes.push(8),
        TvmBoundaryType::Bytes => bytes.push(9),
        TvmBoundaryType::Managed(semantic_id) => {
            bytes.push(10);
            bytes.extend_from_slice(semantic_id);
        }
    }
}

pub(super) fn decode_boundary_type(reader: &mut Reader<'_>) -> Result<TvmBoundaryType, String> {
    match reader.u8()? {
        0 => Ok(TvmBoundaryType::Unit),
        1 => Ok(TvmBoundaryType::Bool),
        2 => Ok(TvmBoundaryType::Int),
        3 => Ok(TvmBoundaryType::Float),
        4 => Ok(TvmBoundaryType::Binary),
        5 => Ok(TvmBoundaryType::String),
        6 => Ok(TvmBoundaryType::Json),
        7 => Ok(TvmBoundaryType::NativeResource(reader.u64()?)),
        8 => Ok(TvmBoundaryType::Atom),
        9 => Ok(TvmBoundaryType::Bytes),
        10 => Ok(TvmBoundaryType::Managed(reader.array()?)),
        tag => Err(format!(
            "error[tvm.image.boundary_type]: unknown type tag {tag}"
        )),
    }
}

pub(super) fn encode_u64_list(values: &[u64]) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    push_u16_count(&mut bytes, values.len())?;
    for value in values {
        push_u64(&mut bytes, *value);
    }
    Ok(bytes)
}

pub(super) fn decode_u64_list(bytes: &[u8]) -> Result<Vec<u64>, String> {
    let mut reader = Reader::new(bytes);
    let count = reader.u16()? as usize;
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        values.push(reader.u64()?);
    }
    reader.finish()?;
    Ok(values)
}

pub(super) fn encode_resources(
    resources: &[TvmNativeResourceDescriptor],
) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    push_u16_count(&mut bytes, resources.len())?;
    for resource in resources {
        push_u64(&mut bytes, resource.type_id);
        push_u64(&mut bytes, resource.owner_capability_id);
        push_u64(&mut bytes, resource.cleanup_export_id);
    }
    Ok(bytes)
}

pub(super) fn decode_resources(bytes: &[u8]) -> Result<Vec<TvmNativeResourceDescriptor>, String> {
    let mut reader = Reader::new(bytes);
    let count = reader.u16()? as usize;
    let mut resources = Vec::with_capacity(count);
    for _ in 0..count {
        resources.push(TvmNativeResourceDescriptor {
            type_id: reader.u64()?,
            owner_capability_id: reader.u64()?,
            cleanup_export_id: reader.u64()?,
        });
    }
    reader.finish()?;
    Ok(resources)
}

pub(super) fn encode_dependencies(
    dependencies: &[TvmDependencyDescriptor],
) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    push_u16_count(&mut bytes, dependencies.len())?;
    for dependency in dependencies {
        push_u64(&mut bytes, dependency.id);
        bytes.extend_from_slice(&dependency.abi_digest);
    }
    Ok(bytes)
}

pub(super) fn decode_dependencies(bytes: &[u8]) -> Result<Vec<TvmDependencyDescriptor>, String> {
    let mut reader = Reader::new(bytes);
    let count = reader.u16()? as usize;
    let mut dependencies = Vec::with_capacity(count);
    for _ in 0..count {
        dependencies.push(TvmDependencyDescriptor {
            id: reader.u64()?,
            abi_digest: reader.array()?,
        });
    }
    reader.finish()?;
    Ok(dependencies)
}

pub(super) fn encode_integrity(integrity: &TvmImageIntegrity) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(64);
    bytes.extend_from_slice(&integrity.code_digest);
    bytes.extend_from_slice(&integrity.immutable_data_digest);
    bytes
}

pub(super) fn decode_integrity(bytes: &[u8]) -> Result<TvmImageIntegrity, String> {
    let mut reader = Reader::new(bytes);
    let result = TvmImageIntegrity {
        code_digest: reader.array()?,
        immutable_data_digest: reader.array()?,
    };
    reader.finish()?;
    Ok(result)
}

pub(super) fn encode_signature(signature: &TvmSignatureDescriptor) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    push_text(&mut bytes, &signature.signer)?;
    push_u16_count(&mut bytes, signature.signature.len())?;
    bytes.extend_from_slice(&signature.signature);
    Ok(bytes)
}

pub(super) fn decode_signature(bytes: &[u8]) -> Result<TvmSignatureDescriptor, String> {
    let mut reader = Reader::new(bytes);
    let signer = reader.text()?;
    let len = reader.u16()? as usize;
    let signature = reader.take(len)?.to_vec();
    reader.finish()?;
    Ok(TvmSignatureDescriptor { signer, signature })
}

pub(super) fn encode_texts<'a>(
    values: impl IntoIterator<Item = &'a str>,
) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    for value in values {
        push_text(&mut bytes, value)?;
    }
    Ok(bytes)
}

pub(super) fn push_text(bytes: &mut Vec<u8>, value: &str) -> Result<(), String> {
    validate_text(value)?;
    push_u16_count(bytes, value.len())?;
    bytes.extend_from_slice(value.as_bytes());
    Ok(())
}

pub(super) fn push_u16_count(bytes: &mut Vec<u8>, value: usize) -> Result<(), String> {
    push_u16(
        bytes,
        u16::try_from(value)
            .map_err(|_| "error[tvm.image.count]: descriptor count exceeds u16".to_string())?,
    );
    Ok(())
}

pub(super) fn push_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

pub(super) fn push_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

pub(super) fn push_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

pub(super) fn record<'a>(records: &[(u16, u16, &'a [u8])], kind: u16) -> Result<&'a [u8], String> {
    records
        .iter()
        .find(|(record_kind, _, _)| *record_kind == kind)
        .map(|(_, _, payload)| *payload)
        .ok_or_else(|| format!("error[tvm.image.missing_record]: missing descriptor record {kind}"))
}

pub(super) struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    pub(super) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    pub(super) fn take(&mut self, len: usize) -> Result<&'a [u8], String> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or_else(|| "error[tvm.image.truncated]: descriptor offset overflow".to_string())?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| "error[tvm.image.truncated]: truncated descriptor".to_string())?;
        self.offset = end;
        Ok(value)
    }

    pub(super) fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    pub(super) fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    pub(super) fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    pub(super) fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(self.array()?))
    }

    pub(super) fn array<const N: usize>(&mut self) -> Result<[u8; N], String> {
        self.take(N)?
            .try_into()
            .map_err(|_| "error[tvm.image.truncated]: invalid fixed-width field".to_string())
    }

    pub(super) fn text(&mut self) -> Result<String, String> {
        let len = self.u16()? as usize;
        let bytes = self.take(len)?;
        let value = std::str::from_utf8(bytes)
            .map_err(|_| "error[tvm.image.text]: descriptor text is not UTF-8".to_string())?;
        validate_text(value)?;
        Ok(value.to_string())
    }

    pub(super) fn finish(self) -> Result<(), String> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err("error[tvm.image.record_size]: trailing record bytes".to_string())
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.offset == self.bytes.len()
    }
}
