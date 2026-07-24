use cranelift_codegen::isa::CallConv;
use object::write::Object as WriteObject;
use object::{BinaryFormat, Object, ObjectKind, ObjectSection, ObjectSymbol, SectionKind};
use sha2::{Digest, Sha256};

use crate::runtime::native_boundary::adapter_abi::PUBLIC_ADAPTER_ABI_VERSION;

use super::descriptor::{
    decode_descriptor, encode_descriptor, TvmExecutableDescriptor, TvmImageIntegrity,
    TvmImageTarget,
};

const ELF_DESCRIPTOR_SECTION: &str = ".note.terlan.tvm";
const MACHO_DESCRIPTOR_SECTION: &str = "__tvm_desc";
const PE_DESCRIPTOR_SECTION: &str = ".tvm$D";
const SUPPORTED_RUNTIME_ABI: u16 = 2;
const SUPPORTED_NATIVE_BOUNDARY: u16 = PUBLIC_ADAPTER_ABI_VERSION;

/// Returns the exact host target identity used for native image emission and
/// admission.
pub fn host_tvm_target() -> Result<TvmImageTarget, String> {
    let builder = cranelift_native::builder()
        .map_err(|error| format!("error[tvm.image.host_target]: {error}"))?;
    let triple = builder.triple().clone();
    Ok(TvmImageTarget {
        triple: triple.to_string(),
        architecture: triple.architecture.to_string(),
        operating_system: triple.operating_system.to_string(),
        calling_convention: CallConv::triple_default(&triple).to_string(),
    })
}

/// Encodes a placeholder descriptor into a relocatable object matching the
/// Cranelift application object. The platform linker therefore embeds the
/// descriptor without a post-link object-copy utility.
pub fn descriptor_object_for_native(
    native_object: &[u8],
    descriptor: &TvmExecutableDescriptor,
) -> Result<Vec<u8>, String> {
    descriptor_object_for_native_with_debug(native_object, descriptor, &[])
}

/// Encodes the descriptor and optional compiler source map into one link input.
pub fn descriptor_object_for_native_with_debug(
    native_object: &[u8],
    descriptor: &TvmExecutableDescriptor,
    debug_metadata: &[u8],
) -> Result<Vec<u8>, String> {
    let native = object::File::parse(native_object)
        .map_err(|error| format!("error[tvm.image.native_object]: {error}"))?;
    let (segment, section_name) = descriptor_section_identity(native.format())?;
    let mut object = WriteObject::new(native.format(), native.architecture(), native.endianness());
    let section = object.add_section(
        segment.as_bytes().to_vec(),
        section_name.as_bytes().to_vec(),
        SectionKind::ReadOnlyData,
    );
    object
        .section_mut(section)
        .set_data(encode_descriptor(descriptor)?, 8);
    if !debug_metadata.is_empty() {
        let (debug_segment, debug_section, debug_kind) = match native.format() {
            BinaryFormat::Elf => (Vec::new(), b".debug_terlan".to_vec(), SectionKind::Debug),
            BinaryFormat::MachO => (
                b"__TERLAN".to_vec(),
                b"__terlan".to_vec(),
                SectionKind::ReadOnlyData,
            ),
            BinaryFormat::Coff => (Vec::new(), b".debug$T".to_vec(), SectionKind::Debug),
            format => {
                return Err(format!(
                    "error[tvm.image.debug_format]: unsupported native debug format {format:?}"
                ))
            }
        };
        let debug = object.add_section(debug_segment, debug_section, debug_kind);
        object
            .section_mut(debug)
            .set_data(debug_metadata.to_vec(), 1);
    }
    object
        .write()
        .map_err(|error| format!("error[tvm.image.descriptor_object]: {error}"))
}

/// Replaces the fixed-size placeholder descriptor with the final code and
/// immutable-data digests without relinking or changing section offsets.
pub fn seal_tvm_image(
    bytes: &mut [u8],
    descriptor: &TvmExecutableDescriptor,
) -> Result<TvmExecutableDescriptor, String> {
    let (section_offset, section_size, code_digest, immutable_data_digest) = {
        let file = object::File::parse(&*bytes)
            .map_err(|error| format!("error[tvm.image.native_format]: {error}"))?;
        let (_, section_name) = descriptor_section_identity(file.format())?;
        let mut sections = file.sections().filter_map(|section| {
            section
                .name()
                .ok()
                .filter(|name| *name == section_name)
                .map(|_| section)
        });
        let section = sections.next().ok_or_else(|| {
            format!("error[tvm.image.descriptor_section]: missing `{section_name}` section")
        })?;
        if sections.next().is_some() {
            return Err(format!(
                "error[tvm.image.descriptor_section]: duplicate `{section_name}` section"
            ));
        }
        let (offset, size) = section.file_range().ok_or_else(|| {
            "error[tvm.image.descriptor_section]: descriptor has no file range".to_string()
        })?;
        (
            usize::try_from(offset).map_err(|_| {
                "error[tvm.image.descriptor_section]: descriptor offset is too large".to_string()
            })?,
            usize::try_from(size).map_err(|_| {
                "error[tvm.image.descriptor_section]: descriptor size is too large".to_string()
            })?,
            section_digest(&file, section_name, |kind| kind == SectionKind::Text)?,
            section_digest(&file, section_name, is_immutable_data)?,
        )
    };
    let mut sealed = descriptor.clone();
    sealed.integrity = TvmImageIntegrity {
        code_digest,
        immutable_data_digest,
    };
    let encoded = encode_descriptor(&sealed)?;
    if encoded.len() != section_size {
        return Err(format!(
            "error[tvm.image.descriptor_section]: encoded descriptor is {} bytes, linked section is {section_size} bytes",
            encoded.len()
        ));
    }
    let end = section_offset.checked_add(section_size).ok_or_else(|| {
        "error[tvm.image.descriptor_section]: descriptor range overflow".to_string()
    })?;
    let destination = bytes.get_mut(section_offset..end).ok_or_else(|| {
        "error[tvm.image.descriptor_section]: descriptor range exceeds image".to_string()
    })?;
    destination.copy_from_slice(&encoded);
    Ok(sealed)
}

/// Static admission result for one native TVM executable image.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TvmNativeImageInspection {
    pub format: &'static str,
    pub architecture: String,
    pub descriptor_section: &'static str,
    pub descriptor_digest: [u8; 32],
    pub descriptor: TvmExecutableDescriptor,
}

/// Inspects one ELF, Mach-O, or PE image without executing it.
pub fn inspect_tvm_image(
    bytes: &[u8],
    expected_target: &str,
) -> Result<TvmNativeImageInspection, String> {
    if bytes.starts_with(b"{") || bytes.starts_with(b"[") {
        return Err("error[tvm.image.native_format]: JSON is not a TVM image".to_string());
    }
    let file = object::File::parse(bytes)
        .map_err(|error| format!("error[tvm.image.native_format]: {error}"))?;
    let has_entry_marker = file.symbols().any(|symbol| {
        symbol.name().is_ok_and(|name| {
            name == "terlan_tvm_image_entry_v1" || name == "_terlan_tvm_image_entry_v1"
        })
    });
    if file.kind() != ObjectKind::Executable
        && !(file.kind() == ObjectKind::Dynamic && (file.entry() != 0 || has_entry_marker))
    {
        return Err("error[tvm.image.native_kind]: TVM image must be an executable".to_string());
    }
    let (format, section_name) = match file.format() {
        BinaryFormat::Elf => ("elf", ELF_DESCRIPTOR_SECTION),
        BinaryFormat::MachO => ("mach-o", MACHO_DESCRIPTOR_SECTION),
        BinaryFormat::Coff | BinaryFormat::Pe => ("pe", PE_DESCRIPTOR_SECTION),
        other => {
            return Err(format!(
                "error[tvm.image.native_format]: unsupported native format {other:?}"
            ));
        }
    };
    let sections = file
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
            "error[tvm.image.descriptor_section]: expected exactly one `{section_name}` section"
        ));
    };
    let descriptor_bytes = section
        .data()
        .map_err(|error| format!("error[tvm.image.descriptor_section]: {error}"))?;
    let descriptor = decode_descriptor(descriptor_bytes)?;
    let descriptor_digest = descriptor_bytes[descriptor_bytes.len() - 32..]
        .try_into()
        .expect("validated descriptor always has a digest footer");
    if !(descriptor.runtime_abi_min..=descriptor.runtime_abi_max).contains(&SUPPORTED_RUNTIME_ABI) {
        return Err(format!(
            "error[tvm.image.runtime_abi]: image requires runtime ABI {}..{}; loader supports {SUPPORTED_RUNTIME_ABI}",
            descriptor.runtime_abi_min, descriptor.runtime_abi_max
        ));
    }
    if !(descriptor.native_boundary_min..=descriptor.native_boundary_max)
        .contains(&SUPPORTED_NATIVE_BOUNDARY)
    {
        return Err(format!(
            "error[tvm.image.native_boundary]: image requires NativeBoundary {}..{}; loader supports {SUPPORTED_NATIVE_BOUNDARY}",
            descriptor.native_boundary_min, descriptor.native_boundary_max
        ));
    }
    if descriptor.target.triple != expected_target {
        return Err(format!(
            "error[tvm.image.target]: image target `{}` does not match `{expected_target}`",
            descriptor.target.triple
        ));
    }
    let host_target = host_tvm_target()?;
    if expected_target == host_target.triple {
        validate_host_target_identity(&descriptor.target, &host_target)?;
    }
    let architecture = format!("{:?}", file.architecture()).to_ascii_lowercase();
    if !architecture_matches(&architecture, &descriptor.target.architecture) {
        return Err(format!(
            "error[tvm.image.architecture]: native architecture `{architecture}` does not match descriptor `{}`",
            descriptor.target.architecture
        ));
    }
    let code_digest = section_digest(&file, section_name, |kind| kind == SectionKind::Text)?;
    if code_digest != descriptor.integrity.code_digest {
        return Err("error[tvm.image.code_digest]: executable section digest mismatch".to_string());
    }
    let immutable_data_digest = section_digest(&file, section_name, is_immutable_data)?;
    if immutable_data_digest != descriptor.integrity.immutable_data_digest {
        return Err("error[tvm.image.data_digest]: immutable-data digest mismatch".to_string());
    }
    Ok(TvmNativeImageInspection {
        format,
        architecture,
        descriptor_section: section_name,
        descriptor_digest,
        descriptor,
    })
}

/// Rejects descriptor ABI dimensions that disagree with the executable host.
pub(super) fn validate_host_target_identity(
    actual: &TvmImageTarget,
    expected: &TvmImageTarget,
) -> Result<(), String> {
    for (field, actual, expected) in [
        ("architecture", &actual.architecture, &expected.architecture),
        (
            "operating_system",
            &actual.operating_system,
            &expected.operating_system,
        ),
        (
            "calling_convention",
            &actual.calling_convention,
            &expected.calling_convention,
        ),
    ] {
        if actual != expected {
            return Err(format!(
                "error[tvm.image.{field}]: image target declares `{actual}`; host requires `{expected}`"
            ));
        }
    }
    Ok(())
}

#[cfg(all(test, target_os = "linux"))]
pub(super) fn native_section_digests(bytes: &[u8]) -> Result<([u8; 32], [u8; 32]), String> {
    let file = object::File::parse(bytes)
        .map_err(|error| format!("error[tvm.image.native_format]: {error}"))?;
    let code = section_digest(&file, "", |kind| kind == SectionKind::Text)?;
    let data = section_digest(&file, "", is_immutable_data)?;
    Ok((code, data))
}

fn is_immutable_data(kind: SectionKind) -> bool {
    matches!(
        kind,
        SectionKind::ReadOnlyData | SectionKind::ReadOnlyString | SectionKind::OtherString
    )
}

fn descriptor_section_identity(
    format: BinaryFormat,
) -> Result<(&'static str, &'static str), String> {
    match format {
        BinaryFormat::Elf => Ok(("", ELF_DESCRIPTOR_SECTION)),
        BinaryFormat::MachO => Ok(("__TERLAN", MACHO_DESCRIPTOR_SECTION)),
        BinaryFormat::Coff | BinaryFormat::Pe => Ok(("", PE_DESCRIPTOR_SECTION)),
        other => Err(format!(
            "error[tvm.image.native_format]: unsupported native format {other:?}"
        )),
    }
}

fn architecture_matches(actual: &str, expected: &str) -> bool {
    let expected = expected.to_ascii_lowercase().replace('-', "_");
    actual == expected
        || matches!(
            (actual, expected.as_str()),
            ("x86_64", "amd64") | ("aarch64", "arm64")
        )
}

fn section_digest<'data>(
    file: &object::File<'data>,
    excluded_name: &str,
    include: impl Fn(SectionKind) -> bool,
) -> Result<[u8; 32], String> {
    let mut sections = file
        .sections()
        .filter_map(|section| {
            let name = section.name().ok()?;
            (name != excluded_name && include(section.kind())).then_some(section)
        })
        .collect::<Vec<_>>();
    sections.sort_by(|left, right| {
        let left_name = left.name().unwrap_or_default();
        let right_name = right.name().unwrap_or_default();
        (left.address(), left_name).cmp(&(right.address(), right_name))
    });
    let mut digest = Sha256::new();
    for section in sections {
        let name = section
            .name()
            .map_err(|error| format!("error[tvm.image.section_name]: {error}"))?;
        let data = section
            .data()
            .map_err(|error| format!("error[tvm.image.section_data]: {error}"))?;
        digest.update((name.len() as u64).to_le_bytes());
        digest.update(name.as_bytes());
        digest.update((data.len() as u64).to_le_bytes());
        digest.update(data);
    }
    Ok(digest.finalize().into())
}
