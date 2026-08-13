//! Whole-program assembly of specialized accelerator application artifacts.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{
    AcceleratorArtifactDescriptor, AcceleratorArtifactKernel, AcceleratorError,
    AcceleratorPlacement, AcceleratorPlacementPlan, AcceleratorPlacementRegion, AcceleratorResult,
    AcceleratorTargetPlan,
};

/// Stable specialized application-manifest schema.
pub const ACCELERATOR_APPLICATION_MANIFEST_SCHEMA: &str =
    "terlan.accelerator-application-artifact.v1";

/// Host capabilities available to an assembled accelerator runtime.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorRuntimeCapabilitySet {
    /// Explicit driver API identities available in the target artifact.
    pub driver_apis: BTreeSet<String>,
    /// Device-local bytes available to all selected accelerator operations.
    pub device_memory_bytes: u64,
    /// Whether target runtime threads are available.
    pub threading: bool,
    /// Whether bounded blocking package workers are available.
    pub blocking_workers: bool,
    /// Whether asynchronous operations can be cancelled.
    pub cancellation: bool,
    /// Explicit native library identities available to the linker or loader.
    pub native_libraries: BTreeSet<String>,
}

/// Generated artifact bytes paired with their compiler descriptor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcceleratorAssemblyArtifact {
    /// Validated compiler-produced artifact descriptor.
    pub descriptor: AcceleratorArtifactDescriptor,
    /// Exact immutable artifact bytes.
    pub bytes: Vec<u8>,
}

/// Assembly inputs after whole-program placement and target admission.
#[derive(Debug)]
pub struct AcceleratorAssemblyRequest<'a> {
    /// Checked whole-program placement plan.
    pub placement: &'a AcceleratorPlacementPlan,
    /// Admitted target plan, absent for a host-only application.
    pub target: Option<&'a AcceleratorTargetPlan>,
    /// Generated artifacts available for reachable generated regions.
    pub artifacts: &'a [AcceleratorAssemblyArtifact],
    /// Runtime capabilities supplied by the selected target profile.
    pub runtime: &'a AcceleratorRuntimeCapabilitySet,
    /// Host symbols retained by ordinary native reachability analysis.
    pub host_symbols: &'a [String],
    /// Explicit per-actor outstanding accelerator operation limit.
    pub actor_operation_limit: u64,
    /// Explicit per-actor reserved device-memory limit.
    pub actor_device_memory_bytes: u64,
}

/// Static registry entry for one reachable package operation.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorStaticOperation {
    /// Package owning the operation.
    pub package: String,
    /// Package operation identity.
    pub operation: String,
    /// Placement regions that can dispatch this operation.
    pub regions: Vec<String>,
}

/// Static registry entry for one reachable generated kernel.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorStaticKernel {
    /// Placement region lowered to this kernel.
    pub region: String,
    /// Compiler-owned typed launch contract.
    pub descriptor: AcceleratorArtifactKernel,
    /// Content identity of the selected accelerator object.
    pub artifact_sha256: String,
    /// Relative artifact path in the assembled application.
    pub artifact: String,
}

/// Explicit memory budgets recorded in the ordinary application manifest.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorAssemblyMemoryBudget {
    /// Target-wide available device memory.
    pub runtime_device_bytes: u64,
    /// Per-actor outstanding device-memory reservation limit.
    pub actor_device_bytes: u64,
    /// Per-actor outstanding operation limit.
    pub actor_operations: u64,
}

/// Ordinary application artifact manifest extended by selected accelerator closure.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorApplicationArtifactManifest {
    /// Stable manifest schema.
    pub schema: String,
    /// Source application identity.
    pub application: String,
    /// Whether any reachable operation needs accelerator runtime support.
    pub accelerator_selected: bool,
    /// Host code retained by ordinary whole-program reachability.
    pub host_code: Vec<String>,
    /// Selected static package operation registry.
    pub operations: Vec<AcceleratorStaticOperation>,
    /// Selected static generated kernel registry.
    pub kernels: Vec<AcceleratorStaticKernel>,
    /// Scalar types required by selected regions.
    pub dtypes: Vec<String>,
    /// Accelerator architectures required by selected regions.
    pub architectures: Vec<String>,
    /// Explicit host/device transfers retained by placement.
    pub transfers: Vec<String>,
    /// Selected external packages.
    pub packages: Vec<String>,
    /// Selected native libraries.
    pub native_libraries: Vec<String>,
    /// Runtime capabilities required by the selected closure.
    pub runtime_capabilities: Vec<String>,
    /// VM runtime adapters retained by specialized assembly.
    pub runtime_adapters: Vec<String>,
    /// Explicit accelerator memory and outstanding-operation budgets.
    pub memory: Option<AcceleratorAssemblyMemoryBudget>,
    /// Stable actor-owned cleanup policy.
    pub cleanup_policy: Option<String>,
    /// Components proven absent from this artifact.
    pub excluded: Vec<String>,
    /// Descriptor and source provenance retained for diagnostics.
    pub provenance: Vec<String>,
    /// Deterministic hash of all preceding manifest fields.
    pub manifest_sha256: String,
}

/// Complete specialized closure produced for one ordinary application artifact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcceleratorSpecializedArtifact {
    /// Application manifest carrying selected and excluded closure evidence.
    pub manifest: AcceleratorApplicationArtifactManifest,
    /// Static Rust registry source, absent for a CPU-only artifact.
    pub static_registry: Option<String>,
    /// Exact accelerator files keyed by normalized relative path.
    pub files: BTreeMap<String, Vec<u8>>,
    /// Content identity of the complete selected accelerator closure.
    pub artifact_sha256: String,
}

impl AcceleratorSpecializedArtifact {
    /// Assembles a deterministic, reachability-specialized accelerator closure.
    pub fn assemble(request: &AcceleratorAssemblyRequest<'_>) -> AcceleratorResult<Self> {
        validate_common_request(request)?;
        let device_regions = request
            .placement
            .regions
            .iter()
            .filter(|region| region.placement == AcceleratorPlacement::Device)
            .collect::<Vec<_>>();
        if device_regions.is_empty() {
            return assemble_cpu_only(request);
        }
        assemble_selected(request, &device_regions)
    }
}

fn validate_common_request(request: &AcceleratorAssemblyRequest<'_>) -> AcceleratorResult<()> {
    if request.placement.application.trim().is_empty() {
        return Err("error[accelerator.assembly]: application identity must not be empty".into());
    }
    if request.actor_operation_limit == 0 {
        return Err("error[accelerator.assembly]: actor operation limit must be positive".into());
    }
    if request.actor_device_memory_bytes == 0 {
        return Err(
            "error[accelerator.assembly]: actor device-memory limit must be positive".into(),
        );
    }
    require_unique(request.host_symbols, "host symbol")?;
    Ok(())
}

fn assemble_cpu_only(
    request: &AcceleratorAssemblyRequest<'_>,
) -> AcceleratorResult<AcceleratorSpecializedArtifact> {
    let mut manifest = AcceleratorApplicationArtifactManifest {
        schema: ACCELERATOR_APPLICATION_MANIFEST_SCHEMA.to_string(),
        application: request.placement.application.clone(),
        accelerator_selected: false,
        host_code: sorted_unique(request.host_symbols.iter().cloned()),
        operations: Vec::new(),
        kernels: Vec::new(),
        dtypes: Vec::new(),
        architectures: Vec::new(),
        transfers: Vec::new(),
        packages: Vec::new(),
        native_libraries: Vec::new(),
        runtime_capabilities: Vec::new(),
        runtime_adapters: Vec::new(),
        memory: None,
        cleanup_policy: None,
        excluded: accelerator_components(),
        provenance: Vec::new(),
        manifest_sha256: String::new(),
    };
    manifest.manifest_sha256 = hash_manifest(&manifest)?;
    Ok(AcceleratorSpecializedArtifact {
        artifact_sha256: hash_parts([manifest.manifest_sha256.as_bytes()]),
        manifest,
        static_registry: None,
        files: BTreeMap::new(),
    })
}

fn assemble_selected(
    request: &AcceleratorAssemblyRequest<'_>,
    regions: &[&AcceleratorPlacementRegion],
) -> AcceleratorResult<AcceleratorSpecializedArtifact> {
    let target = request.target.ok_or_else(|| {
        "error[accelerator.assembly]: device placement requires an admitted target plan".to_string()
    })?;
    if target.packages.is_empty() {
        return Err("error[accelerator.assembly]: selected target has no package closure".into());
    }
    let mut operations = BTreeMap::<(String, String), Vec<String>>::new();
    let mut kernels = Vec::new();
    let mut files = BTreeMap::new();
    let mut dtypes = BTreeSet::new();
    let mut architectures = BTreeSet::new();
    let mut provenance = BTreeSet::new();
    for region in regions {
        dtypes.insert(region.specialization.dtype.identifier().to_string());
        architectures.insert(region.specialization.architecture.clone());
        provenance.insert(format!(
            "{}:{}:{}",
            region.source.file, region.source.line, region.source.column
        ));
        validate_region_target(region, target)?;
        match &region.maintained_library_operation {
            Some(operation) => {
                let (package, operation) = split_package_operation(operation)?;
                validate_package_operation(package, operation, target)?;
                operations
                    .entry((package.to_string(), operation.to_string()))
                    .or_default()
                    .push(region.id.clone());
            }
            None => {
                let (kernel, path, bytes) = select_generated_kernel(region, request.artifacts)?;
                if files.insert(path.clone(), bytes).is_some() {
                    return Err(format!(
                        "error[accelerator.assembly]: duplicate artifact path `{path}`"
                    )
                    .into());
                }
                kernels.push(kernel);
            }
        }
    }
    validate_runtime(request, target)?;
    let operations = operations
        .into_iter()
        .map(|((package, operation), mut regions)| {
            regions.sort();
            AcceleratorStaticOperation {
                package,
                operation,
                regions,
            }
        })
        .collect::<Vec<_>>();
    kernels.sort_by(|left, right| left.region.cmp(&right.region));
    let packages = target
        .packages
        .iter()
        .map(|package| package.package.clone())
        .collect::<Vec<_>>();
    let native_libraries = target
        .packages
        .iter()
        .flat_map(|package| package.libraries.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let capabilities = required_capabilities(target);
    let static_registry = render_static_registry(&operations, &kernels);
    let registry_path = "accelerator/static_registry.rs".to_string();
    files.insert(registry_path, static_registry.as_bytes().to_vec());
    let mut manifest = AcceleratorApplicationArtifactManifest {
        schema: ACCELERATOR_APPLICATION_MANIFEST_SCHEMA.to_string(),
        application: request.placement.application.clone(),
        accelerator_selected: true,
        host_code: sorted_unique(request.host_symbols.iter().cloned()),
        operations,
        kernels,
        dtypes: dtypes.into_iter().collect(),
        architectures: architectures.into_iter().collect(),
        transfers: request
            .placement
            .transfers
            .iter()
            .map(|transfer| {
                format!(
                    "{}:{:?}->{:?}:{}",
                    transfer.value, transfer.from, transfer.to, transfer.before_operation
                )
            })
            .collect(),
        packages,
        native_libraries,
        runtime_capabilities: capabilities,
        runtime_adapters: vec!["vm-capability-worker-event-pump".to_string()],
        memory: Some(AcceleratorAssemblyMemoryBudget {
            runtime_device_bytes: request.runtime.device_memory_bytes,
            actor_device_bytes: request.actor_device_memory_bytes,
            actor_operations: request.actor_operation_limit,
        }),
        cleanup_policy: Some("actor-owned-exactly-once".to_string()),
        excluded: Vec::new(),
        provenance: provenance.into_iter().collect(),
        manifest_sha256: String::new(),
    };
    manifest.manifest_sha256 = hash_manifest(&manifest)?;
    let artifact_sha256 = hash_parts(
        std::iter::once(manifest.manifest_sha256.as_bytes()).chain(
            files
                .iter()
                .flat_map(|(path, bytes)| [path.as_bytes(), bytes.as_slice()]),
        ),
    );
    Ok(AcceleratorSpecializedArtifact {
        manifest,
        static_registry: Some(static_registry),
        files,
        artifact_sha256,
    })
}

fn validate_region_target(
    region: &AcceleratorPlacementRegion,
    target: &AcceleratorTargetPlan,
) -> AcceleratorResult<()> {
    if target.packages.iter().any(|package| {
        package
            .architecture
            .as_deref()
            .is_some_and(|architecture| architecture == region.specialization.architecture)
    }) {
        Ok(())
    } else {
        Err(format!(
            "error[accelerator.assembly]: region `{}` architecture `{}` is not admitted",
            region.id, region.specialization.architecture
        )
        .into())
    }
}

fn validate_package_operation(
    package: &str,
    operation: &str,
    target: &AcceleratorTargetPlan,
) -> AcceleratorResult<()> {
    if operation.trim().is_empty() {
        return Err("error[accelerator.assembly]: package operation must not be empty".into());
    }
    if target.packages.iter().any(|entry| entry.package == package) {
        Ok(())
    } else {
        Err(
            format!("error[accelerator.assembly]: operation package `{package}` is not admitted")
                .into(),
        )
    }
}

fn select_generated_kernel(
    region: &AcceleratorPlacementRegion,
    artifacts: &[AcceleratorAssemblyArtifact],
) -> AcceleratorResult<(AcceleratorStaticKernel, String, Vec<u8>)> {
    let matches = artifacts
        .iter()
        .filter(|artifact| {
            artifact
                .descriptor
                .kernels
                .iter()
                .any(|kernel| kernel.entrypoint == region.id)
        })
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(format!(
            "error[accelerator.assembly]: generated region `{}` requires exactly one artifact, found {}",
            region.id,
            matches.len()
        ).into());
    }
    let artifact = matches[0];
    let descriptor = artifact
        .descriptor
        .kernels
        .iter()
        .find(|kernel| kernel.entrypoint == region.id)
        .cloned()
        .ok_or_else(|| {
            format!(
                "error[accelerator.assembly]: generated region `{}` has no typed descriptor",
                region.id
            )
        })?;
    let actual = hash_bytes(&artifact.bytes);
    if actual != artifact.descriptor.artifact_sha256 {
        return Err(format!(
            "error[accelerator.assembly]: artifact digest mismatch for region `{}`",
            region.id
        )
        .into());
    }
    if artifact.descriptor.architecture != region.specialization.architecture {
        return Err(format!(
            "error[accelerator.assembly]: artifact architecture mismatch for region `{}`",
            region.id
        )
        .into());
    }
    let path = format!("accelerator/{}/{}", region.id, artifact.descriptor.artifact);
    Ok((
        AcceleratorStaticKernel {
            region: region.id.clone(),
            descriptor,
            artifact_sha256: actual,
            artifact: path.clone(),
        },
        path,
        artifact.bytes.clone(),
    ))
}

fn validate_runtime(
    request: &AcceleratorAssemblyRequest<'_>,
    target: &AcceleratorTargetPlan,
) -> AcceleratorResult<()> {
    if !request.runtime.threading {
        return Err("error[accelerator.assembly]: target lacks threading capability".into());
    }
    if !request.runtime.blocking_workers {
        return Err("error[accelerator.assembly]: target lacks blocking-worker capability".into());
    }
    if !request.runtime.cancellation {
        return Err("error[accelerator.assembly]: target lacks cancellation capability".into());
    }
    if request.runtime.device_memory_bytes < request.actor_device_memory_bytes {
        return Err(
            "error[accelerator.assembly]: actor memory budget exceeds runtime device memory".into(),
        );
    }
    for package in &target.packages {
        if let Some(driver) = &package.driver_api {
            if !request.runtime.driver_apis.contains(driver) {
                return Err(format!(
                    "error[accelerator.assembly]: target lacks driver API `{driver}`"
                )
                .into());
            }
        }
        for library in &package.libraries {
            if !request.runtime.native_libraries.contains(library) {
                return Err(format!(
                    "error[accelerator.assembly]: target lacks native library `{library}`"
                )
                .into());
            }
        }
    }
    Ok(())
}

fn required_capabilities(target: &AcceleratorTargetPlan) -> Vec<String> {
    let mut values = BTreeSet::from([
        "runtime.blocking-workers".to_string(),
        "runtime.cancellation".to_string(),
        "runtime.threading".to_string(),
    ]);
    for package in &target.packages {
        values.extend(package.capabilities.iter().cloned());
        if let Some(driver) = &package.driver_api {
            values.insert(format!("driver:{driver}"));
        }
    }
    values.into_iter().collect()
}

fn render_static_registry(
    operations: &[AcceleratorStaticOperation],
    kernels: &[AcceleratorStaticKernel],
) -> String {
    let mut source = String::from("// Generated by terlc; closed accelerator registry.\n");
    source.push_str("pub const OPERATIONS: &[(&str, &str)] = &[\n");
    for operation in operations {
        source.push_str(&format!(
            "    ({:?}, {:?}),\n",
            operation.package, operation.operation
        ));
    }
    source.push_str("];\npub const KERNELS: &[(&str, &str)] = &[\n");
    for kernel in kernels {
        source.push_str(&format!(
            "    ({:?}, {:?}),\n",
            kernel.region, kernel.descriptor.entrypoint
        ));
    }
    source.push_str("];\n");
    source
}

fn split_package_operation(value: &str) -> AcceleratorResult<(&str, &str)> {
    value.split_once(':').ok_or_else(|| {
        AcceleratorError::message(
            "split accelerator package operation",
            format!("error[accelerator.assembly]: invalid maintained operation `{value}`"),
        )
    })
}

fn accelerator_components() -> Vec<String> {
    [
        "accelerator-adapters",
        "accelerator-artifacts",
        "accelerator-descriptors",
        "accelerator-diagnostics",
        "accelerator-native-libraries",
        "accelerator-package-workers",
        "accelerator-static-registry",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn require_unique(values: &[String], kind: &str) -> AcceleratorResult<()> {
    let unique = values.iter().collect::<BTreeSet<_>>();
    if unique.len() != values.len() {
        return Err(format!("error[accelerator.assembly]: duplicate {kind}").into());
    }
    Ok(())
}

fn sorted_unique(values: impl Iterator<Item = String>) -> Vec<String> {
    values.collect::<BTreeSet<_>>().into_iter().collect()
}

fn hash_manifest(manifest: &AcceleratorApplicationArtifactManifest) -> AcceleratorResult<String> {
    let mut unhashed = manifest.clone();
    unhashed.manifest_sha256.clear();
    let bytes = serde_json::to_vec(&unhashed).map_err(|error| {
        AcceleratorError::sourced(
            "accelerator.assembly-manifest",
            "encode accelerator manifest",
            "error[accelerator.assembly]: manifest encoding failed",
            error,
        )
    })?;
    Ok(hash_parts([bytes.as_slice()]))
}

fn hash_bytes(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn hash_parts<'a>(parts: impl IntoIterator<Item = &'a [u8]>) -> String {
    let mut digest = Sha256::new();
    for part in parts {
        digest.update((part.len() as u64).to_le_bytes());
        digest.update(part);
    }
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
#[path = "assembly_test.rs"]
mod assembly_test;
