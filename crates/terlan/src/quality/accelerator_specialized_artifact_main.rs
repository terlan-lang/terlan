#![forbid(unsafe_code)]

//! Emits CPU-only and accelerator-selected specialized artifact evidence.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::Serialize;
use sha2::{Digest, Sha256};
use terlan::compiler::accelerator::*;
use terlan::support::boundary_error::QualityResult;

/// Stable AC8 quality report.
#[derive(Serialize)]
struct SpecializedArtifactReport {
    /// Stable report schema.
    schema: &'static str,
    /// CPU-only artifact inspection.
    cpu_only: ArtifactEvidence,
    /// Accelerator-selected artifact inspection.
    selected: ArtifactEvidence,
    /// Rejected assembly classes covered by the Rust gate.
    rejection_evidence: [&'static str; 9],
    /// Whether two independent assemblies were byte-identical.
    reproducible: bool,
}

/// Persisted closure, hash, size, import, and provenance evidence for one lane.
#[derive(Serialize)]
struct ArtifactEvidence {
    /// Ordinary application manifest.
    manifest: AcceleratorApplicationArtifactManifest,
    /// Complete fixture-envelope hash.
    artifact_sha256: String,
    /// Complete fixture-envelope size.
    artifact_size_bytes: usize,
    /// Sorted symbol-like strings present in the envelope.
    symbols: Vec<String>,
    /// Sorted section-like paths present in the envelope.
    sections: Vec<String>,
    /// Sorted native imports selected by assembly.
    imports: Vec<String>,
    /// Whether every forbidden accelerator marker was absent.
    excluded_markers_absent: bool,
}

/// Parses the report output path.
fn output_path() -> QualityResult<PathBuf> {
    let mut arguments = std::env::args().skip(1);
    let output = arguments
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| "usage: terlan-accelerator-specialized-artifact <output>".to_string())?;
    if arguments.next().is_some() {
        return Err("unexpected specialized artifact argument".into());
    }
    Ok(output)
}

/// Returns one deterministic source mapping.
fn source() -> AcceleratorIrSource {
    AcceleratorIrSource {
        file: "target/quality/accelerator_specialized/Main.terl".to_string(),
        line: 12,
        column: 5,
    }
}

/// Builds one host or device placement region.
fn region(id: &str, placement: AcceleratorPlacement) -> AcceleratorPlacementRegion {
    AcceleratorPlacementRegion {
        id: id.to_string(),
        placement,
        operations: vec![format!("{id}-operation")],
        specialization: AcceleratorPlacementSpecialization {
            dtype: AcceleratorScalarType::F32,
            rank: 1,
            shape: vec![4],
            order: AcceleratorTensorOrder::RowMajor,
            alignment: 4,
            architecture: "sm-86".to_string(),
            constants: BTreeMap::new(),
            launch_dimensions: Some(AcceleratorExecutionDimensions {
                grid: [1, 1, 1],
                block: [32, 1, 1],
            }),
        },
        maintained_library_operation: None,
        fused: false,
        reason: "quality fixture".to_string(),
        source: source(),
    }
}

/// Builds one checked placement plan.
fn placement(regions: Vec<AcceleratorPlacementRegion>) -> AcceleratorPlacementPlan {
    AcceleratorPlacementPlan {
        schema: ACCELERATOR_PLACEMENT_SCHEMA,
        application: "accelerator_specialized".to_string(),
        architecture: "sm-86".to_string(),
        regions,
        transfers: Vec::new(),
        synchronizations: Vec::new(),
        rejections: Vec::new(),
        reference_transfer_count: 0,
        optimized_transfer_count: 0,
    }
}

/// Builds one target plan already admitted by AC3.
fn target() -> AcceleratorTargetPlan {
    AcceleratorTargetPlan {
        schema: ACCELERATOR_TARGET_PLAN_SCHEMA,
        target_triple: "x86_64-unknown-linux-gnu".to_string(),
        mode: AcceleratorAdmissionMode::ToolkitAot,
        hardware_probe_free: true,
        packages: vec![AcceleratorAdmittedPackage {
            package: "terlan-cuda".to_string(),
            source: "terlan-cuda/accelerator.toml".to_string(),
            backend: "cuda".to_string(),
            capabilities: vec!["accelerator.execute".to_string()],
            artifact_format: "ptx".to_string(),
            architecture: Some("sm-86".to_string()),
            driver_api: Some("cuda-driver-v1".to_string()),
            libraries: vec!["cuda-driver".to_string()],
            toolchains: vec!["llvm-nvptx".to_string()],
        }],
        toolchains: Vec::new(),
        deferred_requirements: Vec::new(),
    }
}

/// Returns all explicit capabilities available in the selected fixture.
fn runtime() -> AcceleratorRuntimeCapabilitySet {
    AcceleratorRuntimeCapabilitySet {
        driver_apis: BTreeSet::from(["cuda-driver-v1".to_string()]),
        device_memory_bytes: 1_073_741_824,
        threading: true,
        blocking_workers: true,
        cancellation: true,
        native_libraries: BTreeSet::from(["cuda-driver".to_string()]),
    }
}

/// Builds one immutable generated artifact descriptor and payload.
fn kernel_artifact() -> AcceleratorAssemblyArtifact {
    let bytes = b".version 7.0\n.target sm_86\n.visible .entry device_region() { ret; }\n";
    AcceleratorAssemblyArtifact {
        descriptor: AcceleratorArtifactDescriptor {
            schema: ACCELERATOR_ARTIFACT_SCHEMA.to_string(),
            backend: "llvm-nvptx".to_string(),
            artifact_format: "ptx".to_string(),
            architecture: "sm-86".to_string(),
            ir_sha256: "11".repeat(32),
            toolchain: AcceleratorAdmittedToolchain {
                name: "llvm-nvptx".to_string(),
                version: "14.0.0".to_string(),
                executable: "/usr/bin/llc".to_string(),
                executable_sha256: "22".repeat(32),
                license: "Apache-2.0 WITH LLVM-exception".to_string(),
            },
            kernels: vec![AcceleratorArtifactKernel {
                entrypoint: "device_region".to_string(),
                parameters: Vec::new(),
                dimensions: AcceleratorExecutionDimensions {
                    grid: [1, 1, 1],
                    block: [1, 1, 1],
                },
                shared_memory_bytes: 0,
            }],
            sources: vec![AcceleratorArtifactSource {
                entrypoint: "device_region".to_string(),
                source: source(),
            }],
            artifact: "device_region.ptx".to_string(),
            artifact_sha256: sha256(bytes),
            build_options: BTreeMap::new(),
        },
        bytes: bytes.to_vec(),
    }
}

/// Assembles one fixture from checked compiler inputs.
fn assemble(
    placement: &AcceleratorPlacementPlan,
    target: Option<&AcceleratorTargetPlan>,
    artifacts: &[AcceleratorAssemblyArtifact],
) -> QualityResult<AcceleratorSpecializedArtifact> {
    Ok(
        AcceleratorSpecializedArtifact::assemble(&AcceleratorAssemblyRequest {
            placement,
            target,
            artifacts,
            runtime: &runtime(),
            host_symbols: &["accelerator_specialized.main".to_string()],
            actor_operation_limit: 8,
            actor_device_memory_bytes: 268_435_456,
        })
        .map_err(|error| error.to_string())?,
    )
}

/// Encodes one deterministic artifact envelope used for closure inspection.
fn envelope(artifact: &AcceleratorSpecializedArtifact) -> QualityResult<Vec<u8>> {
    let mut bytes = serde_json::to_vec(&artifact.manifest)
        .map_err(|error| format!("failed to encode artifact manifest: {error}"))?;
    for (path, payload) in &artifact.files {
        bytes.extend_from_slice(&(path.len() as u64).to_le_bytes());
        bytes.extend_from_slice(path.as_bytes());
        bytes.extend_from_slice(&(payload.len() as u64).to_le_bytes());
        bytes.extend_from_slice(payload);
    }
    Ok(bytes)
}

/// Inspects one assembled fixture without executing it.
fn evidence(artifact: AcceleratorSpecializedArtifact) -> QualityResult<ArtifactEvidence> {
    let envelope = envelope(&artifact)?;
    let forbidden = [
        "cuda-driver",
        "terlan-cuda",
        "vm-capability-worker-event-pump",
        ".ptx",
        "accelerator.execute",
    ];
    let excluded_markers_absent = forbidden.iter().all(|marker| {
        !envelope
            .windows(marker.len())
            .any(|bytes| bytes == marker.as_bytes())
    });
    let symbols = artifact
        .static_registry
        .as_deref()
        .map(|_| vec!["accelerator.static_registry".to_string()])
        .unwrap_or_else(|| artifact.manifest.host_code.clone());
    Ok(ArtifactEvidence {
        artifact_sha256: sha256(&envelope),
        artifact_size_bytes: envelope.len(),
        symbols,
        sections: artifact.files.keys().cloned().collect(),
        imports: artifact.manifest.native_libraries.clone(),
        excluded_markers_absent,
        manifest: artifact.manifest,
    })
}

/// Computes one lowercase SHA-256 identity.
fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Writes deterministic CPU and accelerator artifact fixtures and their report.
fn run() -> QualityResult<()> {
    let output = output_path()?;
    let cpu_plan = placement(vec![region("host_region", AcceleratorPlacement::Host)]);
    let selected_plan = placement(vec![region("device_region", AcceleratorPlacement::Device)]);
    let target = target();
    let kernel = kernel_artifact();
    let first_cpu = assemble(&cpu_plan, None, &[])?;
    let second_cpu = assemble(&cpu_plan, None, &[])?;
    let first_selected = assemble(&selected_plan, Some(&target), std::slice::from_ref(&kernel))?;
    let second_selected = assemble(&selected_plan, Some(&target), std::slice::from_ref(&kernel))?;
    let reproducible = envelope(&first_cpu)? == envelope(&second_cpu)?
        && envelope(&first_selected)? == envelope(&second_selected)?;
    let report = SpecializedArtifactReport {
        schema: "terlan.accelerator-specialized-artifact.v1",
        cpu_only: evidence(first_cpu)?,
        selected: evidence(first_selected)?,
        rejection_evidence: [
            "missing-target",
            "missing-driver",
            "insufficient-memory",
            "missing-threading",
            "missing-blocking-worker",
            "missing-cancellation",
            "missing-native-library",
            "ambiguous-kernel",
            "artifact-digest-mismatch",
        ],
        reproducible,
    };
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }
    let bytes = serde_json::to_vec_pretty(&report)
        .map_err(|error| format!("failed to encode specialized artifact report: {error}"))?;
    fs::write(&output, bytes)
        .map_err(|error| format!("failed to write {}: {error}", output.display()))?;
    write_fixture(
        output.parent().unwrap_or_else(|| Path::new(".")),
        "cpu-only.bin",
        &second_cpu,
    )?;
    write_fixture(
        output.parent().unwrap_or_else(|| Path::new(".")),
        "accelerator-selected.bin",
        &second_selected,
    )?;
    Ok(())
}

/// Persists one deterministic artifact fixture for independent inspection.
fn write_fixture(
    directory: &Path,
    name: &str,
    artifact: &AcceleratorSpecializedArtifact,
) -> QualityResult<()> {
    fs::write(directory.join(name), envelope(artifact)?)
        .map_err(|error| format!("failed to write {name}: {error}"))?;
    Ok(())
}

/// Emits a stable diagnostic and nonzero status on failure.
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
