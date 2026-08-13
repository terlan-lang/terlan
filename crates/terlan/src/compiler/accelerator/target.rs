//! Deterministic accelerator target, library, and toolchain admission.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{
    AcceleratorAvailability, AcceleratorDependencyClosure, AcceleratorDeterminism,
    AcceleratorError, AcceleratorPackageDescriptor, AcceleratorResult, AcceleratorTarget,
    AcceleratorToolchain,
};

/// Stable accelerator target-plan report schema.
pub const ACCELERATOR_TARGET_PLAN_SCHEMA: &str = "terlan.accelerator-target-plan.v1";

/// Admission lane selected without probing accelerator hardware.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AcceleratorAdmissionMode {
    /// Validate package resolution and target metadata only.
    CheckOnly,
    /// Execute prebuilt artifacts through a configured driver API.
    DriverOnly,
    /// Compile accelerator kernels with an explicitly bound toolchain.
    ToolkitAot,
}

/// Explicit native library available to one target build.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorLibraryBinding {
    /// Descriptor library identity.
    pub name: String,
    /// Exact library version or ABI identity.
    pub version: String,
    /// Immutable installation or artifact identity.
    pub identity: String,
}

/// Explicit maintained toolchain binding supplied by configuration or package metadata.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorToolchainBinding {
    /// Descriptor toolchain identity.
    pub name: String,
    /// Exact executable version.
    pub version: String,
    /// Explicit executable path; ambient PATH discovery is forbidden.
    pub executable: PathBuf,
    /// Host target triples supported by this executable.
    pub target_triples: Vec<String>,
    /// Accelerator artifact formats emitted by this executable.
    pub artifact_formats: Vec<String>,
    /// Native libraries made available to generated artifacts.
    pub libraries: Vec<String>,
    /// Header or SDK identities made available to compilation.
    pub headers: Vec<String>,
    /// SPDX license expression or reviewed toolchain license identity.
    pub license: String,
    /// Expected SHA-256 digest of the executable bytes.
    pub executable_sha256: String,
}

/// Hardware-independent target request supplied by a build profile.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorTargetRequest {
    /// Accelerator backend selected by the target profile.
    pub backend: String,
    /// Requested host target triple.
    pub target_triple: String,
    /// Requested accelerator architecture when statically selected.
    pub architecture: Option<String>,
    /// Artifact format required by this build.
    pub artifact_format: String,
    /// Requested admission lane.
    pub mode: AcceleratorAdmissionMode,
    /// Driver API identity supplied by the target profile.
    pub driver_api: Option<String>,
    /// Explicit native library bindings.
    pub libraries: Vec<AcceleratorLibraryBinding>,
    /// Explicit external toolchain bindings.
    pub toolchains: Vec<AcceleratorToolchainBinding>,
    /// Device-local memory budget available to admitted packages.
    pub device_memory_bytes: Option<u64>,
    /// Pinned-host memory budget available to admitted packages.
    pub pinned_host_memory_bytes: Option<u64>,
    /// Whether the target profile supplies unified memory.
    pub unified_memory: bool,
    /// Maximum nondeterminism admitted by the target profile.
    pub determinism: AcceleratorDeterminism,
}

/// Verified immutable external toolchain included in a target plan.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorAdmittedToolchain {
    /// Stable toolchain identity.
    pub name: String,
    /// Verified version.
    pub version: String,
    /// Canonical explicit executable path.
    pub executable: String,
    /// Verified executable digest.
    pub executable_sha256: String,
    /// Reviewed license identity.
    pub license: String,
}

/// One package admitted for the requested target.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorAdmittedPackage {
    /// Package identity.
    pub package: String,
    /// Descriptor provenance.
    pub source: String,
    /// Package accelerator backend.
    pub backend: String,
    /// Capabilities owned by the admitted package.
    pub capabilities: Vec<String>,
    /// Selected artifact format.
    pub artifact_format: String,
    /// Selected accelerator architecture.
    pub architecture: Option<String>,
    /// Driver API requirement.
    pub driver_api: Option<String>,
    /// Required native library identities.
    pub libraries: Vec<String>,
    /// Required external toolchain identities.
    pub toolchains: Vec<String>,
}

/// Deterministic accelerator target-admission output.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorTargetPlan {
    /// Stable report schema.
    pub schema: &'static str,
    /// Requested host target triple.
    pub target_triple: String,
    /// Requested admission lane.
    pub mode: AcceleratorAdmissionMode,
    /// True when planning completed without loading a driver or probing a device.
    pub hardware_probe_free: bool,
    /// Admitted packages in deterministic package order.
    pub packages: Vec<AcceleratorAdmittedPackage>,
    /// Explicitly verified external toolchains.
    pub toolchains: Vec<AcceleratorAdmittedToolchain>,
    /// Assumptions intentionally left unresolved by check-only admission.
    pub deferred_requirements: Vec<String>,
}

impl AcceleratorTargetPlan {
    /// Admits one resolved package closure against an explicit target request.
    pub fn admit(
        closure: &AcceleratorDependencyClosure,
        request: &AcceleratorTargetRequest,
    ) -> AcceleratorResult<Self> {
        target_lexicon::Triple::from_str(&request.target_triple)
            .map_err(|error| format!("error[accelerator.target-triple]: {error}"))?;
        require_identity(&request.artifact_format, "artifact format")?;
        let libraries =
            unique_bindings(&request.libraries, |value| value.name.as_str(), "library")?;
        let toolchains = unique_bindings(
            &request.toolchains,
            |value| value.name.as_str(),
            "toolchain",
        )?;
        let mut admitted_packages = Vec::new();
        let mut admitted_toolchains = BTreeMap::new();
        let mut deferred_requirements = Vec::new();
        for package in &closure.packages {
            if package.descriptor.backend != request.backend {
                return Err(mismatch(package, "backend", &request.backend));
            }
            let target = select_target(package, request)?;
            validate_architecture(package, target, request)?;
            validate_driver(package, target, request, &mut deferred_requirements)?;
            validate_memory(package, target, request)?;
            validate_determinism(package, target, request)?;
            let package_libraries =
                validate_libraries(package, request, &libraries, &mut deferred_requirements)?;
            let package_toolchains = validate_toolchains(
                package,
                request,
                &toolchains,
                &mut admitted_toolchains,
                &mut deferred_requirements,
            )?;
            admitted_packages.push(AcceleratorAdmittedPackage {
                package: package.package.clone(),
                source: package.source.clone(),
                backend: package.descriptor.backend.clone(),
                capabilities: package.descriptor.capabilities.clone(),
                artifact_format: request.artifact_format.clone(),
                architecture: request.architecture.clone(),
                driver_api: target.driver_api.clone(),
                libraries: package_libraries,
                toolchains: package_toolchains,
            });
        }
        deferred_requirements.sort();
        deferred_requirements.dedup();
        Ok(Self {
            schema: ACCELERATOR_TARGET_PLAN_SCHEMA,
            target_triple: request.target_triple.clone(),
            mode: request.mode,
            hardware_probe_free: true,
            packages: admitted_packages,
            toolchains: admitted_toolchains.into_values().collect(),
            deferred_requirements,
        })
    }
}

/// Selects one available package target and requested artifact format.
fn select_target<'a>(
    package: &'a AcceleratorPackageDescriptor,
    request: &AcceleratorTargetRequest,
) -> AcceleratorResult<&'a AcceleratorTarget> {
    let target = package
        .descriptor
        .targets
        .iter()
        .find(|target| target.triple == request.target_triple)
        .ok_or_else(|| mismatch(package, "host target", &request.target_triple))?;
    if target.availability == AcceleratorAvailability::Unsupported {
        return Err(mismatch(
            package,
            "unsupported host target",
            &request.target_triple,
        ));
    }
    if !target.artifact_formats.contains(&request.artifact_format) {
        return Err(mismatch(
            package,
            "artifact format",
            &request.artifact_format,
        ));
    }
    Ok(target)
}

/// Rejects architecture mismatches against target and kernel metadata.
fn validate_architecture(
    package: &AcceleratorPackageDescriptor,
    target: &AcceleratorTarget,
    request: &AcceleratorTargetRequest,
) -> AcceleratorResult<()> {
    let Some(architecture) = &request.architecture else {
        return Ok(());
    };
    if !target.architectures.is_empty() && !target.architectures.contains(architecture) {
        return Err(mismatch(package, "accelerator architecture", architecture));
    }
    for kernel in &package.descriptor.kernels {
        if kernel.artifact_format == request.artifact_format
            && !kernel.target_architectures.is_empty()
            && !kernel.target_architectures.contains(architecture)
        {
            return Err(format!(
                "error[accelerator.kernel-architecture]: package `{}` kernel `{}` does not admit `{architecture}`",
                package.package, kernel.id
            ).into());
        }
    }
    Ok(())
}

/// Validates driver requirements while preserving check-only operation.
fn validate_driver(
    package: &AcceleratorPackageDescriptor,
    target: &AcceleratorTarget,
    request: &AcceleratorTargetRequest,
    deferred: &mut Vec<String>,
) -> AcceleratorResult<()> {
    let Some(required) = &target.driver_api else {
        return Ok(());
    };
    if request.mode == AcceleratorAdmissionMode::CheckOnly && request.driver_api.is_none() {
        deferred.push(format!("{}:driver-api:{required}", package.package));
        return Ok(());
    }
    if request.driver_api.as_deref() != Some(required.as_str()) {
        return Err(mismatch(package, "driver API", required));
    }
    Ok(())
}

/// Validates static memory and unified-memory requirements.
fn validate_memory(
    package: &AcceleratorPackageDescriptor,
    target: &AcceleratorTarget,
    request: &AcceleratorTargetRequest,
) -> AcceleratorResult<()> {
    for (name, available, required) in [
        (
            "device memory",
            request.device_memory_bytes,
            target.memory.minimum_device_bytes,
        ),
        (
            "pinned host memory",
            request.pinned_host_memory_bytes,
            target.memory.minimum_pinned_host_bytes,
        ),
    ] {
        if request.mode != AcceleratorAdmissionMode::CheckOnly
            && available.is_none_or(|available| available < required)
        {
            return Err(mismatch(package, name, &required.to_string()));
        }
    }
    if target.memory.unified_memory && !request.unified_memory {
        return Err(mismatch(package, "unified memory", "required"));
    }
    Ok(())
}

/// Rejects a target profile weaker than the package determinism contract.
fn validate_determinism(
    package: &AcceleratorPackageDescriptor,
    target: &AcceleratorTarget,
    request: &AcceleratorTargetRequest,
) -> AcceleratorResult<()> {
    let level = |value| match value {
        AcceleratorDeterminism::Strict => 0,
        AcceleratorDeterminism::BestEffort => 1,
        AcceleratorDeterminism::Nondeterministic => 2,
    };
    if level(request.determinism) > level(target.determinism) {
        return Err(mismatch(package, "determinism", "insufficient"));
    }
    Ok(())
}

/// Resolves package libraries only from explicit immutable bindings.
fn validate_libraries(
    package: &AcceleratorPackageDescriptor,
    request: &AcceleratorTargetRequest,
    bindings: &BTreeMap<&str, &AcceleratorLibraryBinding>,
    deferred: &mut Vec<String>,
) -> AcceleratorResult<Vec<String>> {
    let mut admitted = Vec::new();
    for library in &package.descriptor.host_libraries {
        let Some(binding) = bindings.get(library.name.as_str()) else {
            if request.mode == AcceleratorAdmissionMode::CheckOnly {
                deferred.push(format!("{}:library:{}", package.package, library.name));
                continue;
            }
            if library.required {
                return Err(mismatch(package, "native library", &library.name));
            }
            continue;
        };
        if binding.version != library.version || !valid_sha256_identity(&binding.identity) {
            return Err(mismatch(package, "native library identity", &library.name));
        }
        admitted.push(format!(
            "{}@{}#{}",
            binding.name, binding.version, binding.identity
        ));
    }
    Ok(admitted)
}

/// Returns whether an immutable library identity is a complete SHA-256 digest.
fn valid_sha256_identity(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|digest| {
        digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
    })
}

/// Resolves and verifies required AOT toolchains from explicit bindings.
fn validate_toolchains(
    package: &AcceleratorPackageDescriptor,
    request: &AcceleratorTargetRequest,
    bindings: &BTreeMap<&str, &AcceleratorToolchainBinding>,
    admitted: &mut BTreeMap<String, AcceleratorAdmittedToolchain>,
    deferred: &mut Vec<String>,
) -> AcceleratorResult<Vec<String>> {
    let mut selected = Vec::new();
    for toolchain in package.descriptor.toolchains.iter().filter(|toolchain| {
        toolchain
            .artifact_formats
            .contains(&request.artifact_format)
    }) {
        if request.mode != AcceleratorAdmissionMode::ToolkitAot && !toolchain.required {
            continue;
        }
        let Some(binding) = bindings.get(toolchain.name.as_str()) else {
            if request.mode == AcceleratorAdmissionMode::CheckOnly {
                deferred.push(format!("{}:toolchain:{}", package.package, toolchain.name));
                continue;
            }
            return Err(mismatch(package, "toolchain", &toolchain.name));
        };
        validate_toolchain_assets(package, binding)?;
        let verified = verify_toolchain(toolchain, binding, request)?;
        selected.push(toolchain.name.clone());
        admitted.insert(toolchain.name.clone(), verified);
    }
    Ok(selected)
}

/// Validates explicitly inventoried SDK assets against package requirements.
fn validate_toolchain_assets(
    package: &AcceleratorPackageDescriptor,
    binding: &AcceleratorToolchainBinding,
) -> AcceleratorResult<()> {
    for (kind, values) in [
        ("library", binding.libraries.as_slice()),
        ("header", binding.headers.as_slice()),
    ] {
        let mut unique = std::collections::BTreeSet::new();
        for value in values {
            require_identity(value, kind)?;
            if !unique.insert(value) {
                return Err(
                    format!("error[accelerator.toolchain-{kind}]: duplicate `{value}`").into(),
                );
            }
        }
    }
    for library in package
        .descriptor
        .host_libraries
        .iter()
        .filter(|library| library.required)
    {
        if !binding.libraries.contains(&library.name) {
            return Err(format!(
                "error[accelerator.toolchain-library]: toolchain `{}` omits required library `{}`",
                binding.name, library.name
            )
            .into());
        }
    }
    Ok(())
}

/// Verifies executable bytes and declared target capabilities without invoking it.
fn verify_toolchain(
    expected: &AcceleratorToolchain,
    binding: &AcceleratorToolchainBinding,
    request: &AcceleratorTargetRequest,
) -> AcceleratorResult<AcceleratorAdmittedToolchain> {
    if binding.version != expected.version
        || !binding.target_triples.contains(&request.target_triple)
        || !binding.artifact_formats.contains(&request.artifact_format)
        || binding.license.trim().is_empty()
    {
        return Err(format!(
            "error[accelerator.toolchain-contract]: toolchain `{}` metadata mismatch",
            expected.name
        )
        .into());
    }
    let executable = std::fs::canonicalize(&binding.executable).map_err(|error| {
        format!(
            "error[accelerator.toolchain-executable]: {}: {error}",
            binding.executable.display()
        )
    })?;
    if !executable.is_file() {
        return Err(format!(
            "error[accelerator.toolchain-executable]: {} is not a file",
            executable.display()
        )
        .into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&executable)
            .map_err(|error| {
                format!(
                    "error[accelerator.toolchain-executable]: {}: {error}",
                    executable.display()
                )
            })?
            .permissions()
            .mode();
        if mode & 0o111 == 0 {
            return Err(format!(
                "error[accelerator.toolchain-executable]: {} is not executable",
                executable.display()
            )
            .into());
        }
    }
    let digest = sha256_file(&executable)?;
    if digest != binding.executable_sha256 {
        return Err(format!(
            "error[accelerator.toolchain-identity]: `{}` executable digest mismatch",
            expected.name
        )
        .into());
    }
    Ok(AcceleratorAdmittedToolchain {
        name: expected.name.clone(),
        version: binding.version.clone(),
        executable: executable.to_string_lossy().into_owned(),
        executable_sha256: digest,
        license: binding.license.clone(),
    })
}

/// Returns the lowercase SHA-256 digest of one explicitly selected executable.
pub fn accelerator_toolchain_sha256(path: &Path) -> AcceleratorResult<String> {
    sha256_file(path)
}

/// Hashes one file without interpreting or executing it.
fn sha256_file(path: &Path) -> AcceleratorResult<String> {
    let bytes = std::fs::read(path).map_err(|error| {
        format!(
            "error[accelerator.toolchain-executable]: {}: {error}",
            path.display()
        )
    })?;
    Ok(Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

/// Builds a unique identity map for explicit bindings.
fn unique_bindings<'a, T>(
    values: &'a [T],
    identity: impl Fn(&'a T) -> &'a str,
    kind: &str,
) -> AcceleratorResult<BTreeMap<&'a str, &'a T>> {
    let mut output = BTreeMap::new();
    for value in values {
        let name = identity(value);
        require_identity(name, kind)?;
        if output.insert(name, value).is_some() {
            return Err(format!("error[accelerator.{kind}-duplicate]: `{name}`").into());
        }
    }
    Ok(output)
}

/// Rejects empty or whitespace-bearing target identities.
fn require_identity(value: &str, kind: &str) -> AcceleratorResult<()> {
    if value.is_empty() || value != value.trim() || value.chars().any(char::is_whitespace) {
        return Err(format!("error[accelerator.{kind}-identity]: `{value}`").into());
    }
    Ok(())
}

/// Creates one stable package-specific mismatch diagnostic.
fn mismatch(package: &AcceleratorPackageDescriptor, kind: &str, value: &str) -> AcceleratorError {
    AcceleratorError::message(
        "admit accelerator target",
        format!(
            "error[accelerator.target-mismatch]: package `{}` {} `{value}` is not admitted",
            package.package, kind
        ),
    )
}

use std::str::FromStr;

#[cfg(test)]
#[path = "target_test.rs"]
mod target_test;
