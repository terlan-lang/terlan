//! Release-package admission for target-native TVM executable images.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::runtime::vm::pure_native::PureNativeExecutionShard;
use crate::runtime::vm::ReplValue;

use super::debug::inspect_tvm_native_debug;
use super::{host_tvm_target, inspect_tvm_image, reject_tvm_image_sidecars};

const RELEASE_SCHEMA: &str = "terlan.release-artifact.v1";

/// Manifest identity for one packaged target-native TVM self-test image.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PackagedTvmImageMetadata {
    /// Path relative to the installed Terlan share directory.
    pub path: String,
    /// SHA-256 digest of the complete executable image.
    pub sha256: String,
    /// Native target triple admitted by the image descriptor.
    pub target_triple: String,
    /// Native object format parsed from the admitted executable.
    pub object_format: String,
    /// Architecture independently encoded by the image descriptor.
    pub architecture: String,
    /// Operating system independently encoded by the image descriptor.
    pub operating_system: String,
    /// Calling convention independently encoded by the image descriptor.
    pub calling_convention: String,
    /// SHA-256 digest of the embedded canonical image descriptor.
    pub descriptor_digest: String,
    /// Compiler identity embedded in the image descriptor.
    pub compiler: String,
    /// Build identity embedded in the image descriptor.
    pub build: String,
    /// Package identity embedded in the image descriptor.
    pub package: String,
    /// Module identity embedded in the image descriptor.
    pub module: String,
    /// Qualified zero-arity export executed during package admission.
    pub entry: String,
    /// Ordered continuation identities embedded in the image descriptor.
    pub continuation_ids: Vec<u64>,
    /// Number of canonical native source records embedded in the image.
    pub native_debug_record_count: usize,
}

/// Successful admission and execution report for one release package.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PackageValidationReport {
    /// Canonical release metadata path used for admission.
    pub release_metadata: String,
    /// Exact native image path that was admitted and executed.
    pub image: String,
    /// Target triple shared by the package and image.
    pub target_triple: String,
    /// Qualified export that returned `true`.
    pub entry: String,
    /// Embedded descriptor digest admitted by the package metadata.
    pub descriptor_digest: String,
}

#[derive(Debug, Deserialize)]
/// Minimal release-manifest fields required for native self-test admission.
struct ReleaseMetadata {
    /// Release metadata schema identity.
    schema: String,
    /// Native target triple for all executable release payloads.
    target_triple: String,
    /// Exact native image identity and execution contract.
    native_self_test: PackagedTvmImageMetadata,
}

/// Inspects an executable image and returns metadata suitable for a release manifest.
pub fn describe_packaged_tvm_image(
    image_path: &Path,
    package_path: &str,
    entry: &str,
) -> Result<PackagedTvmImageMetadata, String> {
    validate_package_relative_path(package_path)?;
    let bytes = fs::read(image_path).map_err(|error| {
        format!(
            "error[tvm.package.image_read]: cannot read `{}`: {error}",
            image_path.display()
        )
    })?;
    let target = host_tvm_target()?;
    let inspection = inspect_tvm_image(&bytes, &target.triple)?;
    let debug_records = inspect_tvm_native_debug(&bytes)?;
    let canonical_entry = format!("{entry}/0");
    let export = inspection
        .descriptor
        .exports
        .iter()
        .find(|export| export.name == entry || export.name == canonical_entry)
        .ok_or_else(|| format!("error[tvm.package.entry]: image does not export `{entry}`"))?;
    if !export.parameters.is_empty() {
        return Err(format!(
            "error[tvm.package.entry]: package self-test export `{entry}` must have arity 0"
        ));
    }
    Ok(PackagedTvmImageMetadata {
        path: package_path.to_string(),
        sha256: digest_hex(&bytes),
        target_triple: inspection.descriptor.target.triple.clone(),
        object_format: inspection.format.to_string(),
        architecture: inspection.descriptor.target.architecture.clone(),
        operating_system: inspection.descriptor.target.operating_system.clone(),
        calling_convention: inspection.descriptor.target.calling_convention.clone(),
        descriptor_digest: hex_bytes(&inspection.descriptor_digest),
        compiler: inspection.descriptor.identity.compiler.clone(),
        build: inspection.descriptor.identity.build.clone(),
        package: inspection.descriptor.identity.package.clone(),
        module: inspection.descriptor.identity.module.clone(),
        entry: entry.to_string(),
        continuation_ids: inspection
            .descriptor
            .continuations
            .iter()
            .map(|continuation| continuation.id)
            .collect(),
        native_debug_record_count: debug_records.len(),
    })
}

/// Admits and executes the packaged self-test image rooted at an archive or installation.
pub fn validate_and_execute_release_package(
    root: &Path,
) -> Result<PackageValidationReport, String> {
    let metadata_path = locate_release_metadata(root)?;
    let release_bytes = fs::read(&metadata_path).map_err(|error| {
        format!(
            "error[tvm.package.metadata_read]: cannot read `{}`: {error}",
            metadata_path.display()
        )
    })?;
    let release: ReleaseMetadata = serde_json::from_slice(&release_bytes)
        .map_err(|error| format!("error[tvm.package.metadata]: {error}"))?;
    if release.schema != RELEASE_SCHEMA {
        return Err(format!(
            "error[tvm.package.schema]: unsupported release schema `{}`",
            release.schema
        ));
    }
    let expected_target = host_tvm_target()?.triple;
    if release.target_triple != expected_target {
        return Err(format!(
            "error[tvm.package.release_target]: release target `{}` does not match host `{expected_target}`",
            release.target_triple
        ));
    }
    if release.native_self_test.target_triple != release.target_triple {
        return Err(
            "error[tvm.package.image_target]: packaged image target does not match release target"
                .to_string(),
        );
    }
    let image_path = locate_packaged_image(root, &metadata_path, &release.native_self_test.path)?;
    reject_tvm_image_sidecars(&image_path)
        .map_err(|error| error.replace("tvm.image.sidecar", "tvm.package.sidecar"))?;
    let actual = describe_packaged_tvm_image(
        &image_path,
        &release.native_self_test.path,
        &release.native_self_test.entry,
    )?;
    if actual != release.native_self_test {
        return Err(render_metadata_drift(&release.native_self_test, &actual));
    }

    let mut shard = PureNativeExecutionShard::load_image(&image_path)?;
    let loaded_digest = hex_bytes(&shard.whole_image_digest()?);
    if loaded_digest != actual.sha256 {
        return Err(
            "error[tvm.package.loaded_image_drift]: loaded image does not match admitted package bytes"
                .to_string(),
        );
    }
    let result = shard.call(&actual.entry, &[]);
    let shutdown = shard.shutdown();
    let value = result?;
    shutdown?;
    if value != ReplValue::Bool(true) {
        return Err(format!(
            "error[tvm.package.self_test]: `{}` must return true, found {}",
            actual.entry,
            value.render()
        ));
    }
    Ok(PackageValidationReport {
        release_metadata: metadata_path.display().to_string(),
        image: image_path.display().to_string(),
        target_triple: actual.target_triple,
        entry: actual.entry,
        descriptor_digest: actual.descriptor_digest,
    })
}

/// Resolves exactly one release manifest from archive or installed layouts.
fn locate_release_metadata(root: &Path) -> Result<PathBuf, String> {
    let candidates = [
        root.join("terlan-release.json"),
        root.join("share/terlan/terlan-release.json"),
    ];
    let existing = candidates
        .into_iter()
        .filter(|candidate| candidate.is_file())
        .collect::<Vec<_>>();
    match existing.as_slice() {
        [path] => Ok(path.clone()),
        [] => Err(format!(
            "error[tvm.package.metadata_missing]: `{}` contains no terlan-release.json",
            root.display()
        )),
        _ => Err(
            "error[tvm.package.metadata_ambiguous]: multiple release manifests found".to_string(),
        ),
    }
}

/// Resolves exactly one metadata-bound image from archive or installed layouts.
fn locate_packaged_image(
    root: &Path,
    metadata_path: &Path,
    package_path: &str,
) -> Result<PathBuf, String> {
    validate_package_relative_path(package_path)?;
    let relative = Path::new(package_path);
    let metadata_parent = metadata_path.parent().ok_or_else(|| {
        "error[tvm.package.metadata_path]: release manifest has no parent directory".to_string()
    })?;
    let candidates = [
        metadata_parent.join(relative),
        root.join("share/terlan").join(relative),
    ];
    let existing = candidates
        .into_iter()
        .filter(|candidate| candidate.is_file())
        .collect::<BTreeSet<_>>();
    match existing.len() {
        0 => Err(format!(
            "error[tvm.package.image_missing]: packaged image `{package_path}` is missing"
        )),
        1 => Ok(existing.into_iter().next().expect("one candidate exists")),
        _ => Err(format!(
            "error[tvm.package.image_ambiguous]: packaged image `{package_path}` exists in multiple layouts"
        )),
    }
}

/// Rejects non-native, absolute, and traversal-bearing package image paths.
fn validate_package_relative_path(value: &str) -> Result<(), String> {
    let path = Path::new(value);
    if path.extension().and_then(|extension| extension.to_str()) != Some("tvm") {
        return Err(
            "error[tvm.package.image_extension]: packaged image must end in `.tvm`".to_string(),
        );
    }
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(
            "error[tvm.package.image_path]: packaged image path must be a normalized relative path"
                .to_string(),
        );
    }
    Ok(())
}

/// Selects the first deterministic manifest field that differs from the image.
fn render_metadata_drift(
    expected: &PackagedTvmImageMetadata,
    actual: &PackagedTvmImageMetadata,
) -> String {
    let field = if expected.sha256 != actual.sha256 {
        "sha256"
    } else if expected.descriptor_digest != actual.descriptor_digest {
        "descriptor_digest"
    } else if expected.target_triple != actual.target_triple {
        "target_triple"
    } else if expected.object_format != actual.object_format {
        "object_format"
    } else if expected.architecture != actual.architecture {
        "architecture"
    } else if expected.operating_system != actual.operating_system {
        "operating_system"
    } else if expected.calling_convention != actual.calling_convention {
        "calling_convention"
    } else if expected.compiler != actual.compiler {
        "compiler"
    } else if expected.build != actual.build {
        "build"
    } else if expected.package != actual.package {
        "package"
    } else if expected.module != actual.module {
        "module"
    } else if expected.entry != actual.entry {
        "entry"
    } else if expected.continuation_ids != actual.continuation_ids {
        "continuation_ids"
    } else {
        "native_debug_record_count"
    };
    format!("error[tvm.package.metadata_drift]: packaged image `{field}` does not match release metadata")
}

/// Returns a lowercase SHA-256 digest for a complete native image.
fn digest_hex(bytes: &[u8]) -> String {
    hex_bytes(&Sha256::digest(bytes))
}

/// Encodes bytes as canonical lowercase hexadecimal text.
fn hex_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}
