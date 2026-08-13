use std::collections::{BTreeMap, BTreeSet};

use super::*;
use crate::compiler::accelerator::{
    AcceleratorAdmissionMode, AcceleratorAdmittedPackage, AcceleratorAdmittedToolchain,
    AcceleratorArtifactSource, AcceleratorExecutionDimensions, AcceleratorIrSource,
    AcceleratorPlacementSpecialization, AcceleratorScalarType, AcceleratorSynchronizationDecision,
    AcceleratorTensorOrder, AcceleratorTransferDecision,
};

fn source() -> AcceleratorIrSource {
    AcceleratorIrSource {
        file: "src/app/Main.terl".to_string(),
        line: 10,
        column: 5,
    }
}

fn region(
    id: &str,
    placement: AcceleratorPlacement,
    maintained: Option<&str>,
) -> AcceleratorPlacementRegion {
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
        maintained_library_operation: maintained.map(str::to_string),
        fused: false,
        reason: "fixture".to_string(),
        source: source(),
    }
}

fn placement(regions: Vec<AcceleratorPlacementRegion>) -> AcceleratorPlacementPlan {
    AcceleratorPlacementPlan {
        schema: super::super::ACCELERATOR_PLACEMENT_SCHEMA,
        application: "vision".to_string(),
        architecture: "sm-86".to_string(),
        regions,
        transfers: vec![AcceleratorTransferDecision {
            value: "pixels".to_string(),
            from: AcceleratorPlacement::Host,
            to: AcceleratorPlacement::Device,
            before_operation: "region-0-operation".to_string(),
            reason: "device input".to_string(),
        }],
        synchronizations: Vec::<AcceleratorSynchronizationDecision>::new(),
        rejections: Vec::new(),
        reference_transfer_count: 1,
        optimized_transfer_count: 1,
    }
}

fn target() -> AcceleratorTargetPlan {
    AcceleratorTargetPlan {
        schema: super::super::ACCELERATOR_TARGET_PLAN_SCHEMA,
        target_triple: "x86_64-unknown-linux-gnu".to_string(),
        mode: AcceleratorAdmissionMode::ToolkitAot,
        hardware_probe_free: true,
        packages: vec![AcceleratorAdmittedPackage {
            package: "terlan-cuda".to_string(),
            source: "packages/terlan-cuda/accelerator.toml".to_string(),
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

fn runtime() -> AcceleratorRuntimeCapabilitySet {
    AcceleratorRuntimeCapabilitySet {
        driver_apis: BTreeSet::from(["cuda-driver-v1".to_string()]),
        device_memory_bytes: 1_024,
        threading: true,
        blocking_workers: true,
        cancellation: true,
        native_libraries: BTreeSet::from(["cuda-driver".to_string()]),
    }
}

fn artifact(entrypoint: &str, bytes: &[u8]) -> AcceleratorAssemblyArtifact {
    AcceleratorAssemblyArtifact {
        descriptor: AcceleratorArtifactDescriptor {
            schema: super::super::ACCELERATOR_ARTIFACT_SCHEMA.to_string(),
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
                entrypoint: entrypoint.to_string(),
                parameters: Vec::new(),
                dimensions: AcceleratorExecutionDimensions {
                    grid: [1, 1, 1],
                    block: [1, 1, 1],
                },
                shared_memory_bytes: 0,
            }],
            sources: vec![AcceleratorArtifactSource {
                entrypoint: entrypoint.to_string(),
                source: source(),
            }],
            artifact: "kernel.ptx".to_string(),
            artifact_sha256: hash_bytes(bytes),
            build_options: BTreeMap::new(),
        },
        bytes: bytes.to_vec(),
    }
}

fn assemble(
    placement: &AcceleratorPlacementPlan,
    target: Option<&AcceleratorTargetPlan>,
    artifacts: &[AcceleratorAssemblyArtifact],
    runtime: &AcceleratorRuntimeCapabilitySet,
) -> Result<AcceleratorSpecializedArtifact, String> {
    Ok(AcceleratorSpecializedArtifact::assemble(
        &AcceleratorAssemblyRequest {
            placement,
            target,
            artifacts,
            runtime,
            host_symbols: &["vision.main".to_string()],
            actor_operation_limit: 4,
            actor_device_memory_bytes: 256,
        },
    )?)
}

#[test]
fn cpu_only_artifact_excludes_the_complete_accelerator_closure() {
    let placement = placement(vec![region("host", AcceleratorPlacement::Host, None)]);
    let first = assemble(&placement, None, &[], &runtime()).expect("CPU artifact");
    let second = assemble(&placement, None, &[], &runtime()).expect("CPU artifact again");
    assert_eq!(first, second);
    assert!(!first.manifest.accelerator_selected);
    assert!(first.files.is_empty());
    assert!(first.static_registry.is_none());
    assert!(first.manifest.packages.is_empty());
    assert!(first.manifest.runtime_capabilities.is_empty());
    assert!(first.manifest.runtime_adapters.is_empty());
    assert!(first.manifest.native_libraries.is_empty());
    assert_eq!(first.manifest.excluded, accelerator_components());
}

#[test]
fn maintained_operation_selects_only_its_static_runtime_closure() {
    let placement = placement(vec![region(
        "region-0",
        AcceleratorPlacement::Device,
        Some("terlan-cuda:buffer.add"),
    )]);
    let assembled = assemble(&placement, Some(&target()), &[], &runtime()).expect("assembly");
    assert!(assembled.manifest.accelerator_selected);
    assert_eq!(assembled.manifest.operations.len(), 1);
    assert!(assembled.manifest.kernels.is_empty());
    assert_eq!(assembled.manifest.dtypes, ["f32"]);
    assert_eq!(assembled.manifest.architectures, ["sm-86"]);
    assert_eq!(assembled.manifest.packages, ["terlan-cuda"]);
    assert_eq!(assembled.manifest.native_libraries, ["cuda-driver"]);
    assert_eq!(
        assembled.manifest.runtime_adapters,
        ["vm-capability-worker-event-pump"]
    );
    assert_eq!(assembled.files.len(), 1);
    let registry = assembled.static_registry.expect("static registry");
    assert!(registry.contains("buffer.add"));
    assert!(!registry.contains("universal"));
    assert_eq!(
        assembled.manifest.cleanup_policy.as_deref(),
        Some("actor-owned-exactly-once")
    );
}

#[test]
fn generated_region_selects_one_verified_aot_artifact() {
    let placement = placement(vec![region("region-0", AcceleratorPlacement::Device, None)]);
    let artifact = artifact("region-0", b"// deterministic PTX");
    let assembled =
        assemble(&placement, Some(&target()), &[artifact], &runtime()).expect("generated assembly");
    assert!(assembled.manifest.operations.is_empty());
    assert_eq!(assembled.manifest.kernels.len(), 1);
    assert_eq!(assembled.files.len(), 2);
    assert!(assembled
        .files
        .contains_key("accelerator/region-0/kernel.ptx"));
    assert_eq!(
        assembled.manifest.kernels[0].descriptor.entrypoint,
        "region-0"
    );
}

#[test]
fn assembly_rejects_missing_target_and_runtime_capabilities() {
    let placement = placement(vec![region(
        "region-0",
        AcceleratorPlacement::Device,
        Some("terlan-cuda:buffer.add"),
    )]);
    assert!(assemble(&placement, None, &[], &runtime())
        .unwrap_err()
        .contains("admitted target"));

    for (kind, mutate) in [
        ("threading", 0_u8),
        ("blocking-worker", 1),
        ("cancellation", 2),
        ("driver API", 3),
        ("native library", 4),
        ("memory budget", 5),
    ] {
        let mut unavailable = runtime();
        match mutate {
            0 => unavailable.threading = false,
            1 => unavailable.blocking_workers = false,
            2 => unavailable.cancellation = false,
            3 => unavailable.driver_apis.clear(),
            4 => unavailable.native_libraries.clear(),
            5 => unavailable.device_memory_bytes = 128,
            _ => unreachable!(),
        }
        let error = assemble(&placement, Some(&target()), &[], &unavailable)
            .expect_err("missing capability");
        assert!(error.contains(kind), "{kind}: {error}");
    }
}

#[test]
fn malformed_artifacts_and_ambiguous_inputs_fail_closed() {
    let placement = placement(vec![region("region-0", AcceleratorPlacement::Device, None)]);
    let valid = artifact("region-0", b"ptx");
    assert!(assemble(&placement, Some(&target()), &[], &runtime())
        .unwrap_err()
        .contains("exactly one artifact"));
    assert!(assemble(
        &placement,
        Some(&target()),
        &[valid.clone(), valid.clone()],
        &runtime()
    )
    .unwrap_err()
    .contains("exactly one artifact"));
    let mut corrupt = valid;
    corrupt.bytes.push(0);
    assert!(
        assemble(&placement, Some(&target()), &[corrupt], &runtime())
            .unwrap_err()
            .contains("digest mismatch")
    );
}

#[test]
fn assembly_hashes_and_manifests_are_reproducible_and_input_sensitive() {
    let placement = placement(vec![region("region-0", AcceleratorPlacement::Device, None)]);
    let first_artifact = artifact("region-0", b"ptx-a");
    let first = assemble(
        &placement,
        Some(&target()),
        std::slice::from_ref(&first_artifact),
        &runtime(),
    )
    .expect("first");
    let second = assemble(
        &placement,
        Some(&target()),
        std::slice::from_ref(&first_artifact),
        &runtime(),
    )
    .expect("second");
    assert_eq!(first, second);
    let changed = assemble(
        &placement,
        Some(&target()),
        &[artifact("region-0", b"ptx-b")],
        &runtime(),
    )
    .expect("changed");
    assert_ne!(first.artifact_sha256, changed.artifact_sha256);
    assert_ne!(
        first.manifest.kernels[0].artifact_sha256,
        changed.manifest.kernels[0].artifact_sha256
    );
}
