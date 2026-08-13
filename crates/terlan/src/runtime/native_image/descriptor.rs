pub use super::boundary_type::TvmBoundaryType;
use super::managed::{
    decode_aggregate_layout, decode_collection_layout, encode_aggregate_layout,
    encode_collection_layout, AtomTable, ManagedAggregateKind,
};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

mod codec;
use codec::*;
mod callables;
use callables::*;
mod collections;
use collections::*;

const MAGIC: &[u8; 8] = b"TVMDSC01";
const FORMAT_MAJOR: u16 = 1;
const FORMAT_MINOR: u16 = 5;
const HEADER_LEN: usize = 32;
const DIGEST_LEN: usize = 32;
// Bounded for decoding while accommodating large closed AOT applications.
const MAX_DESCRIPTOR_LEN: usize = 16 * 1024 * 1024;
const MAX_TEXT_LEN: usize = u16::MAX as usize;
const OPTIONAL_RECORD: u16 = 1;

/// Runtime-ABI-3 symbol loaded by the supervised native-image worker.
pub const TVM_DISPATCH_SYMBOL_V3: &str = "terlan_native_dispatch_v3";
/// Format-1 native image entry marker used for static admission and linking.
pub const TVM_IMAGE_ENTRY_SYMBOL_V1: &str = "terlan_tvm_image_entry_v1";

const TARGET_RECORD: u16 = 1;
const IDENTITY_RECORD: u16 = 2;
const EXPORTS_RECORD: u16 = 3;
const CAPABILITIES_RECORD: u16 = 4;
const RESOURCES_RECORD: u16 = 5;
const DEPENDENCIES_RECORD: u16 = 6;
const INTEGRITY_RECORD: u16 = 7;
const SIGNATURE_RECORD: u16 = 8;
const CONTINUATIONS_RECORD: u16 = 9;
const MANAGED_LAYOUTS_RECORD: u16 = 10;
const MANAGED_COLLECTIONS_RECORD: u16 = 11;
const ATOMS_RECORD: u16 = 12;
const CALLABLES_RECORD: u16 = 13;

const REQUIRED_RECORDS: &[u16] = &[
    TARGET_RECORD,
    IDENTITY_RECORD,
    EXPORTS_RECORD,
    CAPABILITIES_RECORD,
    RESOURCES_RECORD,
    DEPENDENCIES_RECORD,
    INTEGRITY_RECORD,
];

/// Target and calling-convention identity encoded into a TVM image.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TvmImageTarget {
    pub triple: String,
    pub architecture: String,
    pub operating_system: String,
    pub calling_convention: String,
}

/// Deterministic compiler, build, package, and module identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TvmImageIdentity {
    pub compiler: String,
    pub build: String,
    pub package: String,
    pub module: String,
}

/// One callable native export and its exact boundary signature.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TvmExportDescriptor {
    pub id: u64,
    pub name: String,
    pub parameters: Vec<TvmBoundaryType>,
    pub results: Vec<TvmBoundaryType>,
}

/// One compiler-generated native continuation entry and its owned value shape.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TvmContinuationDescriptor {
    pub id: u64,
    pub parameters: Vec<TvmBoundaryType>,
    pub results: Vec<TvmBoundaryType>,
}

/// One image-local target admitted for managed closure invocation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TvmCallableDescriptor {
    /// Stable callable identity accepted by the native dispatch symbol.
    pub id: u64,
    /// Exact caller-supplied parameter identity, excluding captured values.
    pub parameters: Vec<TvmBoundaryType>,
    /// Exact result identity returned by the callable.
    pub results: Vec<TvmBoundaryType>,
    /// Immutable environment shape prepended to parameters for native dispatch.
    pub captures: Vec<TvmBoundaryType>,
}

/// One VM-owned native resource kind exposed by the image.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TvmNativeResourceDescriptor {
    pub type_id: u64,
    pub owner_capability_id: u64,
    pub cleanup_export_id: u64,
}

/// One content-addressed native dependency ABI.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TvmDependencyDescriptor {
    pub id: u64,
    pub abi_digest: [u8; 32],
}

/// One canonical fixed aggregate layout admitted with an executable image.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TvmManagedLayoutDescriptor {
    /// Stable semantic identity shared by compatible active variants.
    pub semantic_id: [u8; 16],
    /// Canonical bounded aggregate-layout bytes consumed by the actor heap.
    pub encoded_layout: Vec<u8>,
}

/// One canonical managed collection schema admitted with an executable image.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TvmManagedCollectionDescriptor {
    /// Stable semantic identity of the public collection root.
    pub semantic_id: [u8; 16],
    /// Canonical bounded schema used to reconstruct the existing storage profile.
    pub encoded_layout: Vec<u8>,
}

/// Integrity digests for executable and immutable data sections.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TvmImageIntegrity {
    pub code_digest: [u8; 32],
    pub immutable_data_digest: [u8; 32],
}

/// Optional package-policy signature evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TvmSignatureDescriptor {
    pub signer: String,
    pub signature: Vec<u8>,
}

/// Canonical descriptor embedded in one target-native TVM executable image.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TvmExecutableDescriptor {
    pub runtime_abi_min: u16,
    pub runtime_abi_max: u16,
    pub native_boundary_min: u16,
    pub native_boundary_max: u16,
    pub target: TvmImageTarget,
    pub identity: TvmImageIdentity,
    pub exports: Vec<TvmExportDescriptor>,
    pub capabilities: Vec<u64>,
    pub resources: Vec<TvmNativeResourceDescriptor>,
    pub dependencies: Vec<TvmDependencyDescriptor>,
    pub continuations: Vec<TvmContinuationDescriptor>,
    /// Closed image-local dispatch membership for owned closure values.
    pub callables: Vec<TvmCallableDescriptor>,
    /// Canonical fixed aggregate layouts required by public and generated calls.
    pub managed_layouts: Vec<TvmManagedLayoutDescriptor>,
    /// Canonical List, Map, and Set schemas required by public boundaries.
    pub managed_collections: Vec<TvmManagedCollectionDescriptor>,
    /// Canonically ordered finite atom identities owned by this image generation.
    pub atoms: Vec<String>,
    pub integrity: TvmImageIntegrity,
    pub signature: Option<TvmSignatureDescriptor>,
}

/// Encodes one descriptor using the frozen format-1 canonical record order.
pub(super) fn encode_descriptor_untyped(
    descriptor: &TvmExecutableDescriptor,
) -> Result<Vec<u8>, String> {
    validate_descriptor(descriptor)?;
    let mut records = Vec::new();
    append_record(
        &mut records,
        TARGET_RECORD,
        0,
        encode_target(&descriptor.target)?,
    )?;
    append_record(
        &mut records,
        IDENTITY_RECORD,
        0,
        encode_identity(&descriptor.identity)?,
    )?;
    append_record(
        &mut records,
        EXPORTS_RECORD,
        0,
        encode_exports(&descriptor.exports)?,
    )?;
    append_record(
        &mut records,
        CAPABILITIES_RECORD,
        0,
        encode_u64_list(&descriptor.capabilities)?,
    )?;
    append_record(
        &mut records,
        RESOURCES_RECORD,
        0,
        encode_resources(&descriptor.resources)?,
    )?;
    append_record(
        &mut records,
        DEPENDENCIES_RECORD,
        0,
        encode_dependencies(&descriptor.dependencies)?,
    )?;
    append_record(
        &mut records,
        INTEGRITY_RECORD,
        0,
        encode_integrity(&descriptor.integrity),
    )?;
    if let Some(signature) = &descriptor.signature {
        append_record(
            &mut records,
            SIGNATURE_RECORD,
            OPTIONAL_RECORD,
            encode_signature(signature)?,
        )?;
    }
    if !descriptor.continuations.is_empty() {
        append_record(
            &mut records,
            CONTINUATIONS_RECORD,
            OPTIONAL_RECORD,
            encode_continuations(&descriptor.continuations)?,
        )?;
    }
    if !descriptor.managed_layouts.is_empty() {
        append_record(
            &mut records,
            MANAGED_LAYOUTS_RECORD,
            OPTIONAL_RECORD,
            encode_managed_layouts(&descriptor.managed_layouts)?,
        )?;
    }
    if !descriptor.managed_collections.is_empty() {
        append_record(
            &mut records,
            MANAGED_COLLECTIONS_RECORD,
            OPTIONAL_RECORD,
            encode_managed_collections(&descriptor.managed_collections)?,
        )?;
    }
    if !descriptor.atoms.is_empty() {
        append_record(
            &mut records,
            ATOMS_RECORD,
            OPTIONAL_RECORD,
            encode_text_list(&descriptor.atoms)?,
        )?;
    }
    if !descriptor.callables.is_empty() {
        append_record(
            &mut records,
            CALLABLES_RECORD,
            OPTIONAL_RECORD,
            encode_callables(&descriptor.callables)?,
        )?;
    }

    let total_len = HEADER_LEN
        .checked_add(records.len())
        .and_then(|len| len.checked_add(DIGEST_LEN))
        .ok_or_else(|| {
            "error[tvm.image.descriptor_size]: descriptor length overflow".to_string()
        })?;
    if total_len > MAX_DESCRIPTOR_LEN {
        return Err("error[tvm.image.descriptor_size]: descriptor exceeds 16 MiB".to_string());
    }
    let record_count = u16::try_from(
        7 + usize::from(descriptor.signature.is_some())
            + usize::from(!descriptor.continuations.is_empty())
            + usize::from(!descriptor.managed_layouts.is_empty())
            + usize::from(!descriptor.managed_collections.is_empty())
            + usize::from(!descriptor.atoms.is_empty())
            + usize::from(!descriptor.callables.is_empty()),
    )
    .map_err(|_| "error[tvm.image.record_count]: too many descriptor records".to_string())?;
    let mut bytes = Vec::with_capacity(total_len);
    bytes.extend_from_slice(MAGIC);
    push_u16(&mut bytes, FORMAT_MAJOR);
    push_u16(&mut bytes, FORMAT_MINOR);
    push_u16(&mut bytes, HEADER_LEN as u16);
    push_u16(&mut bytes, record_count);
    push_u32(
        &mut bytes,
        u32::try_from(total_len)
            .map_err(|_| "error[tvm.image.descriptor_size]: descriptor is too large".to_string())?,
    );
    push_u16(&mut bytes, descriptor.runtime_abi_min);
    push_u16(&mut bytes, descriptor.runtime_abi_max);
    push_u16(&mut bytes, descriptor.native_boundary_min);
    push_u16(&mut bytes, descriptor.native_boundary_max);
    push_u32(&mut bytes, 0);
    bytes.extend_from_slice(&records);
    let digest = Sha256::digest(&bytes);
    bytes.extend_from_slice(&digest);
    Ok(bytes)
}

/// Decodes and validates one canonical format-1 descriptor.
pub(super) fn decode_descriptor_untyped(bytes: &[u8]) -> Result<TvmExecutableDescriptor, String> {
    if bytes.len() < HEADER_LEN + DIGEST_LEN || bytes.len() > MAX_DESCRIPTOR_LEN {
        return Err("error[tvm.image.descriptor_size]: invalid descriptor length".to_string());
    }
    if bytes.get(..MAGIC.len()) != Some(MAGIC) {
        return Err("error[tvm.image.magic]: invalid TVM descriptor magic".to_string());
    }
    let mut header = Reader::new(&bytes[MAGIC.len()..HEADER_LEN]);
    let major = header.u16()?;
    let minor = header.u16()?;
    if major != FORMAT_MAJOR || minor > FORMAT_MINOR {
        return Err(format!(
            "error[tvm.image.format_version]: unsupported descriptor version {major}.{minor}"
        ));
    }
    if header.u16()? as usize != HEADER_LEN {
        return Err("error[tvm.image.header_size]: non-canonical header length".to_string());
    }
    let record_count = header.u16()? as usize;
    let total_len = header.u32()? as usize;
    if total_len != bytes.len() {
        return Err("error[tvm.image.descriptor_size]: declared length mismatch".to_string());
    }
    let runtime_abi_min = header.u16()?;
    let runtime_abi_max = header.u16()?;
    let native_boundary_min = header.u16()?;
    let native_boundary_max = header.u16()?;
    if header.u32()? != 0 {
        return Err("error[tvm.image.reserved]: reserved header bits must be zero".to_string());
    }
    let digest_offset = bytes.len() - DIGEST_LEN;
    let actual_digest = Sha256::digest(&bytes[..digest_offset]);
    if actual_digest[..] != bytes[digest_offset..] {
        return Err("error[tvm.image.descriptor_digest]: descriptor digest mismatch".to_string());
    }

    let mut reader = Reader::new(&bytes[HEADER_LEN..digest_offset]);
    let mut records = Vec::with_capacity(record_count);
    let mut previous_kind = 0;
    for _ in 0..record_count {
        let kind = reader.u16()?;
        let flags = reader.u16()?;
        let len = reader.u32()? as usize;
        if kind <= previous_kind {
            return Err(
                "error[tvm.image.record_order]: records must be unique and ordered".to_string(),
            );
        }
        previous_kind = kind;
        if flags & !OPTIONAL_RECORD != 0 {
            return Err("error[tvm.image.record_flags]: unknown record flags".to_string());
        }
        let payload = reader.take(len)?;
        if kind > CALLABLES_RECORD && flags & OPTIONAL_RECORD == 0 {
            return Err(format!(
                "error[tvm.image.record_kind]: unknown mandatory record {kind}"
            ));
        }
        records.push((kind, flags, payload));
    }
    if !reader.is_empty() {
        return Err("error[tvm.image.record_count]: trailing uncounted records".to_string());
    }
    for required in REQUIRED_RECORDS {
        if !records.iter().any(|(kind, _, _)| kind == required) {
            return Err(format!(
                "error[tvm.image.missing_record]: missing descriptor record {required}"
            ));
        }
    }

    let descriptor = TvmExecutableDescriptor {
        runtime_abi_min,
        runtime_abi_max,
        native_boundary_min,
        native_boundary_max,
        target: decode_target(record(&records, TARGET_RECORD)?)?,
        identity: decode_identity(record(&records, IDENTITY_RECORD)?)?,
        exports: decode_exports(record(&records, EXPORTS_RECORD)?)?,
        capabilities: decode_u64_list(record(&records, CAPABILITIES_RECORD)?)?,
        resources: decode_resources(record(&records, RESOURCES_RECORD)?)?,
        dependencies: decode_dependencies(record(&records, DEPENDENCIES_RECORD)?)?,
        continuations: records
            .iter()
            .find(|(kind, _, _)| *kind == CONTINUATIONS_RECORD)
            .map(|(_, _, payload)| decode_continuations(payload))
            .transpose()?
            .unwrap_or_default(),
        callables: records
            .iter()
            .find(|(kind, _, _)| *kind == CALLABLES_RECORD)
            .map(|(_, _, payload)| decode_callables(payload))
            .transpose()?
            .unwrap_or_default(),
        managed_layouts: records
            .iter()
            .find(|(kind, _, _)| *kind == MANAGED_LAYOUTS_RECORD)
            .map(|(_, _, payload)| decode_managed_layouts(payload))
            .transpose()?
            .unwrap_or_default(),
        managed_collections: records
            .iter()
            .find(|(kind, _, _)| *kind == MANAGED_COLLECTIONS_RECORD)
            .map(|(_, _, payload)| decode_managed_collections(payload))
            .transpose()?
            .unwrap_or_default(),
        atoms: records
            .iter()
            .find(|(kind, _, _)| *kind == ATOMS_RECORD)
            .map(|(_, _, payload)| decode_text_list(payload))
            .transpose()?
            .unwrap_or_default(),
        integrity: decode_integrity(record(&records, INTEGRITY_RECORD)?)?,
        signature: records
            .iter()
            .find(|(kind, _, _)| *kind == SIGNATURE_RECORD)
            .map(|(_, _, payload)| decode_signature(payload))
            .transpose()?,
    };
    validate_descriptor(&descriptor)?;
    Ok(descriptor)
}

/// Encodes one executable descriptor with a typed image-boundary failure.
pub fn encode_descriptor(
    descriptor: &TvmExecutableDescriptor,
) -> Result<Vec<u8>, terlan_runtime_abi::BoundaryError> {
    encode_descriptor_untyped(descriptor).map_err(|error| {
        terlan_runtime_abi::BoundaryError::message(
            terlan_runtime_abi::ErrorDomain::NativeImageAdmission,
            "encode TVM executable descriptor",
            error,
        )
    })
}

/// Decodes one executable descriptor with a typed image-boundary failure.
pub fn decode_descriptor(
    bytes: &[u8],
) -> Result<TvmExecutableDescriptor, terlan_runtime_abi::BoundaryError> {
    decode_descriptor_untyped(bytes).map_err(|error| {
        terlan_runtime_abi::BoundaryError::message(
            terlan_runtime_abi::ErrorDomain::NativeImageAdmission,
            "decode TVM executable descriptor",
            error,
        )
    })
}

fn validate_descriptor(descriptor: &TvmExecutableDescriptor) -> Result<(), String> {
    if descriptor.runtime_abi_min == 0 || descriptor.runtime_abi_min > descriptor.runtime_abi_max {
        return Err("error[tvm.image.runtime_abi]: invalid runtime ABI range".to_string());
    }
    if descriptor.native_boundary_min == 0
        || descriptor.native_boundary_min > descriptor.native_boundary_max
    {
        return Err("error[tvm.image.native_boundary]: invalid protocol range".to_string());
    }
    for value in [
        &descriptor.target.triple,
        &descriptor.target.architecture,
        &descriptor.target.operating_system,
        &descriptor.target.calling_convention,
        &descriptor.identity.compiler,
        &descriptor.identity.build,
        &descriptor.identity.package,
        &descriptor.identity.module,
    ] {
        validate_text(value)?;
    }
    validate_sorted_unique(descriptor.exports.iter().map(|entry| entry.id), "export")?;
    validate_sorted_unique(descriptor.capabilities.iter().copied(), "capability")?;
    validate_sorted_unique(
        descriptor.resources.iter().map(|entry| entry.type_id),
        "resource",
    )?;
    validate_sorted_unique(
        descriptor.dependencies.iter().map(|entry| entry.id),
        "dependency",
    )?;
    validate_sorted_unique(
        descriptor.continuations.iter().map(|entry| entry.id),
        "continuation",
    )?;
    validate_sorted_unique(
        descriptor.callables.iter().map(|entry| entry.id),
        "callable",
    )?;
    validate_managed_layouts(&descriptor.managed_layouts)?;
    validate_managed_collections(&descriptor.managed_collections)?;
    validate_atoms(&descriptor.atoms)?;
    validate_nonzero(descriptor.exports.iter().map(|entry| entry.id), "export")?;
    validate_nonzero(descriptor.capabilities.iter().copied(), "capability")?;
    validate_nonzero(
        descriptor.resources.iter().map(|entry| entry.type_id),
        "resource",
    )?;
    validate_nonzero(
        descriptor.dependencies.iter().map(|entry| entry.id),
        "dependency",
    )?;
    validate_nonzero(
        descriptor.continuations.iter().map(|entry| entry.id),
        "continuation",
    )?;
    validate_nonzero(
        descriptor.callables.iter().map(|entry| entry.id),
        "callable",
    )?;
    if descriptor.continuations.iter().any(|continuation| {
        descriptor
            .exports
            .iter()
            .any(|export| export.id == continuation.id)
    }) {
        return Err(
            "error[tvm.image.continuation_collision]: continuation and export IDs must be disjoint"
                .to_string(),
        );
    }
    let mut export_names = BTreeSet::new();
    for export in &descriptor.exports {
        validate_text(&export.name)?;
        if !export_names.insert(export.name.as_str()) {
            return Err("error[tvm.image.export_name]: export names must be unique".to_string());
        }
        if export.results.len() > 1 {
            return Err(
                "error[tvm.image.export_result]: format 1 permits at most one result".to_string(),
            );
        }
        for boundary_type in export.parameters.iter().chain(&export.results) {
            if let TvmBoundaryType::NativeResource(type_id) = boundary_type {
                if !descriptor
                    .resources
                    .iter()
                    .any(|resource| resource.type_id == *type_id)
                {
                    return Err(format!(
                        "error[tvm.image.resource_reference]: export {} references undeclared resource {type_id}",
                        export.id
                    ));
                }
            }
        }
    }
    for resource in &descriptor.resources {
        if !descriptor
            .capabilities
            .contains(&resource.owner_capability_id)
        {
            return Err(format!(
                "error[tvm.image.resource_capability]: resource {} references undeclared capability {}",
                resource.type_id, resource.owner_capability_id
            ));
        }
        if !descriptor
            .exports
            .iter()
            .any(|export| export.id == resource.cleanup_export_id)
        {
            return Err(format!(
                "error[tvm.image.resource_cleanup]: resource {} references undeclared cleanup export {}",
                resource.type_id, resource.cleanup_export_id
            ));
        }
    }
    for continuation in &descriptor.continuations {
        if continuation.results.len() > 1 {
            return Err(
                "error[tvm.image.continuation_result]: format 1 permits at most one continuation result"
                    .to_string(),
            );
        }
        for boundary_type in continuation.parameters.iter().chain(&continuation.results) {
            if let TvmBoundaryType::NativeResource(type_id) = boundary_type {
                if !descriptor
                    .resources
                    .iter()
                    .any(|resource| resource.type_id == *type_id)
                {
                    return Err(format!(
                        "error[tvm.image.resource_reference]: continuation {} references undeclared resource {type_id}",
                        continuation.id
                    ));
                }
            }
        }
    }
    validate_callables(descriptor)?;
    if let Some(signature) = &descriptor.signature {
        validate_text(&signature.signer)?;
        if signature.signature.is_empty() || signature.signature.len() > 4096 {
            return Err("error[tvm.image.signature]: invalid signature length".to_string());
        }
    }
    Ok(())
}

fn validate_nonzero(values: impl Iterator<Item = u64>, label: &str) -> Result<(), String> {
    if values.into_iter().any(|value| value == 0) {
        return Err(format!(
            "error[tvm.image.{label}_id]: {label} identifiers must be nonzero"
        ));
    }
    Ok(())
}

fn validate_sorted_unique(values: impl Iterator<Item = u64>, label: &str) -> Result<(), String> {
    let mut previous = None;
    for value in values {
        if previous.is_some_and(|prior| prior >= value) {
            return Err(format!(
                "error[tvm.image.{label}_order]: {label} identifiers must be sorted and unique"
            ));
        }
        previous = Some(value);
    }
    Ok(())
}

fn validate_text(value: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > MAX_TEXT_LEN || value.chars().any(char::is_control) {
        return Err(
            "error[tvm.image.text]: descriptor text is empty, oversized, or contains controls"
                .to_string(),
        );
    }
    Ok(())
}

fn append_record(
    bytes: &mut Vec<u8>,
    kind: u16,
    flags: u16,
    payload: Vec<u8>,
) -> Result<(), String> {
    push_u16(bytes, kind);
    push_u16(bytes, flags);
    push_u32(
        bytes,
        u32::try_from(payload.len())
            .map_err(|_| "error[tvm.image.record_size]: record is too large".to_string())?,
    );
    bytes.extend_from_slice(&payload);
    Ok(())
}

fn encode_target(target: &TvmImageTarget) -> Result<Vec<u8>, String> {
    encode_texts([
        target.triple.as_str(),
        target.architecture.as_str(),
        target.operating_system.as_str(),
        target.calling_convention.as_str(),
    ])
}

fn decode_target(bytes: &[u8]) -> Result<TvmImageTarget, String> {
    let mut reader = Reader::new(bytes);
    let result = TvmImageTarget {
        triple: reader.text()?,
        architecture: reader.text()?,
        operating_system: reader.text()?,
        calling_convention: reader.text()?,
    };
    reader.finish()?;
    Ok(result)
}

fn encode_identity(identity: &TvmImageIdentity) -> Result<Vec<u8>, String> {
    encode_texts([
        identity.compiler.as_str(),
        identity.build.as_str(),
        identity.package.as_str(),
        identity.module.as_str(),
    ])
}

fn decode_identity(bytes: &[u8]) -> Result<TvmImageIdentity, String> {
    let mut reader = Reader::new(bytes);
    let result = TvmImageIdentity {
        compiler: reader.text()?,
        build: reader.text()?,
        package: reader.text()?,
        module: reader.text()?,
    };
    reader.finish()?;
    Ok(result)
}

fn encode_exports(exports: &[TvmExportDescriptor]) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    push_u16_count(&mut bytes, exports.len())?;
    for export in exports {
        push_u64(&mut bytes, export.id);
        push_text(&mut bytes, &export.name)?;
        push_u16_count(&mut bytes, export.parameters.len())?;
        for parameter in &export.parameters {
            encode_boundary_type(&mut bytes, parameter);
        }
        push_u16_count(&mut bytes, export.results.len())?;
        for result in &export.results {
            encode_boundary_type(&mut bytes, result);
        }
    }
    Ok(bytes)
}

fn decode_exports(bytes: &[u8]) -> Result<Vec<TvmExportDescriptor>, String> {
    let mut reader = Reader::new(bytes);
    let count = reader.u16()? as usize;
    let mut exports = Vec::with_capacity(count);
    for _ in 0..count {
        let id = reader.u64()?;
        let name = reader.text()?;
        let parameter_count = reader.u16()? as usize;
        let mut parameters = Vec::with_capacity(parameter_count);
        for _ in 0..parameter_count {
            parameters.push(decode_boundary_type(&mut reader)?);
        }
        let result_count = reader.u16()? as usize;
        let mut results = Vec::with_capacity(result_count);
        for _ in 0..result_count {
            results.push(decode_boundary_type(&mut reader)?);
        }
        exports.push(TvmExportDescriptor {
            id,
            name,
            parameters,
            results,
        });
    }
    reader.finish()?;
    Ok(exports)
}

fn encode_continuations(continuations: &[TvmContinuationDescriptor]) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    push_u16_count(&mut bytes, continuations.len())?;
    for continuation in continuations {
        push_u64(&mut bytes, continuation.id);
        push_u16_count(&mut bytes, continuation.parameters.len())?;
        for parameter in &continuation.parameters {
            encode_boundary_type(&mut bytes, parameter);
        }
        push_u16_count(&mut bytes, continuation.results.len())?;
        for result in &continuation.results {
            encode_boundary_type(&mut bytes, result);
        }
    }
    Ok(bytes)
}

fn decode_continuations(bytes: &[u8]) -> Result<Vec<TvmContinuationDescriptor>, String> {
    let mut reader = Reader::new(bytes);
    let count = reader.u16()? as usize;
    let mut continuations = Vec::with_capacity(count);
    for _ in 0..count {
        let id = reader.u64()?;
        let parameter_count = reader.u16()? as usize;
        let mut parameters = Vec::with_capacity(parameter_count);
        for _ in 0..parameter_count {
            parameters.push(decode_boundary_type(&mut reader)?);
        }
        let result_count = reader.u16()? as usize;
        let mut results = Vec::with_capacity(result_count);
        for _ in 0..result_count {
            results.push(decode_boundary_type(&mut reader)?);
        }
        continuations.push(TvmContinuationDescriptor {
            id,
            parameters,
            results,
        });
    }
    reader.finish()?;
    Ok(continuations)
}

/// Encodes the canonical ordered aggregate-layout table.
fn encode_managed_layouts(layouts: &[TvmManagedLayoutDescriptor]) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    push_u16_count(&mut bytes, layouts.len())?;
    for layout in layouts {
        bytes.extend_from_slice(&layout.semantic_id);
        push_u32(
            &mut bytes,
            u32::try_from(layout.encoded_layout.len()).map_err(|_| {
                "error[tvm.image.managed_layout_size]: managed layout exceeds u32".to_string()
            })?,
        );
        bytes.extend_from_slice(&layout.encoded_layout);
    }
    Ok(bytes)
}

/// Decodes the bounded aggregate-layout table before semantic validation.
fn decode_managed_layouts(bytes: &[u8]) -> Result<Vec<TvmManagedLayoutDescriptor>, String> {
    let mut reader = Reader::new(bytes);
    let count = reader.u16()? as usize;
    let mut layouts = Vec::with_capacity(count);
    for _ in 0..count {
        let semantic_id = reader.array()?;
        let length = reader.u32()? as usize;
        layouts.push(TvmManagedLayoutDescriptor {
            semantic_id,
            encoded_layout: reader.take(length)?.to_vec(),
        });
    }
    reader.finish()?;
    Ok(layouts)
}

/// Validates ordering, semantic ownership, and canonical aggregate bytes.
fn validate_managed_layouts(layouts: &[TvmManagedLayoutDescriptor]) -> Result<(), String> {
    for pair in layouts.windows(2) {
        let left = (&pair[0].semantic_id, pair[0].encoded_layout.as_slice());
        let right = (&pair[1].semantic_id, pair[1].encoded_layout.as_slice());
        if left >= right {
            return Err(
                "error[tvm.image.managed_layout_order]: managed layouts must be unique and ordered"
                    .to_string(),
            );
        }
    }
    let mut decoded_layouts = BTreeMap::new();
    for layout in layouts {
        let decoded = decode_aggregate_layout(&layout.encoded_layout)
            .map_err(|error| format!("error[tvm.image.managed_layout]: {error}"))?;
        if decoded.managed().semantic_id().bytes() != layout.semantic_id {
            return Err(
                "error[tvm.image.managed_layout_identity]: layout semantic identity mismatch"
                    .to_string(),
            );
        }
        let canonical = encode_aggregate_layout(&decoded)
            .map_err(|error| format!("error[tvm.image.managed_layout]: {error}"))?;
        if canonical != layout.encoded_layout {
            return Err(
                "error[tvm.image.managed_layout_canonical]: aggregate layout is not canonical"
                    .to_string(),
            );
        }
        decoded_layouts
            .entry(layout.semantic_id)
            .or_insert_with(Vec::new)
            .push(decoded);
    }
    for variants in decoded_layouts.values() {
        if variants.len() < 2 {
            continue;
        }
        let first = &variants[0];
        if variants.iter().any(|variant| {
            variant.kind() != ManagedAggregateKind::Constructor
                || variant.canonical_type() != first.canonical_type()
                || variant.variant_count() != first.variant_count()
        }) {
            let layouts = variants
                .iter()
                .map(|variant| {
                    format!(
                        "{}:{:?}:{:?}/{:?}",
                        variant.canonical_type(),
                        variant.kind(),
                        variant.discriminant(),
                        variant.variant_count()
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            return Err(format!(
                "error[tvm.image.managed_layout_family]: one semantic identity has incompatible aggregate layouts: {layouts}"
            ));
        }
        let mut names = BTreeSet::new();
        let mut discriminants = BTreeSet::new();
        for variant in variants {
            if !names.insert(variant.variant_name())
                || !discriminants.insert(variant.discriminant())
            {
                return Err(
                    "error[tvm.image.managed_layout_variant]: constructor variants must have unique names and discriminants"
                        .to_string(),
                );
            }
        }
    }
    Ok(())
}
