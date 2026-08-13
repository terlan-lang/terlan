use std::path::Path;

use super::*;
use crate::compiler::accelerator::{
    AcceleratorDependencyClosure, AcceleratorDescriptor, AcceleratorPackageDescriptor,
};

const DESCRIPTOR: &str = r#"
schema = 1
backend = "synthetic"
device_classes = ["vector"]
artifact_formats = ["vector-object"]
dtypes = ["f32"]
layouts = ["row-major"]
address_spaces = ["host", "device"]
resource_classes = ["buffer"]
asynchronous_operations = ["execute"]
capabilities = ["accelerator.execute"]

[[targets]]
triple = "x86_64-unknown-linux-gnu"
availability = "supported"
artifact_formats = ["vector-object"]
architectures = ["vector-v1"]
driver_api = "synthetic-driver-1"
determinism = "strict"

[targets.memory]
minimum_device_bytes = 1024
minimum_pinned_host_bytes = 256
unified_memory = false

[[host_libraries]]
name = "synthetic-driver"
version = "1"
required = true

[[toolchains]]
name = "synthetic-aot"
version = "1"
artifact_formats = ["vector-object"]
required = false

[[operations]]
id = "buffer.add"
effects = ["execute"]
asynchronous = true

[[kernels]]
id = "add-f32"
artifact_format = "vector-object"
artifact = "kernels/add.vo"
symbol = "add_f32"
target_architectures = ["vector-v1"]
max_shared_memory_bytes = 0

[[kernels.parameters]]
name = "output"
dtype = "f32"
address_space = "device"
access = "write"
"#;

fn closure() -> AcceleratorDependencyClosure {
    let descriptor =
        AcceleratorDescriptor::parse(DESCRIPTOR, Path::new("synthetic/accelerator.toml"))
            .expect("descriptor");
    AcceleratorDependencyClosure::resolve(
        vec![AcceleratorPackageDescriptor {
            package: "synthetic".to_string(),
            version: "1".to_string(),
            source: "synthetic/accelerator.toml".to_string(),
            descriptor,
        }],
        Path::new("terlan.toml"),
    )
    .expect("closure")
}

fn request(mode: AcceleratorAdmissionMode) -> AcceleratorTargetRequest {
    AcceleratorTargetRequest {
        backend: "synthetic".to_string(),
        target_triple: "x86_64-unknown-linux-gnu".to_string(),
        architecture: Some("vector-v1".to_string()),
        artifact_format: "vector-object".to_string(),
        mode,
        driver_api: Some("synthetic-driver-1".to_string()),
        libraries: vec![AcceleratorLibraryBinding {
            name: "synthetic-driver".to_string(),
            version: "1".to_string(),
            identity: format!("sha256:{}", "11".repeat(32)),
        }],
        toolchains: Vec::new(),
        device_memory_bytes: Some(2048),
        pinned_host_memory_bytes: Some(512),
        unified_memory: false,
        determinism: AcceleratorDeterminism::Strict,
    }
}

fn explicit_toolchain() -> AcceleratorToolchainBinding {
    let executable = std::env::current_exe().expect("test executable");
    AcceleratorToolchainBinding {
        name: "synthetic-aot".to_string(),
        version: "1".to_string(),
        executable_sha256: accelerator_toolchain_sha256(&executable).expect("digest"),
        executable,
        target_triples: vec!["x86_64-unknown-linux-gnu".to_string()],
        artifact_formats: vec!["vector-object".to_string()],
        libraries: vec!["synthetic-driver".to_string()],
        headers: vec!["synthetic-sdk-1".to_string()],
        license: "Apache-2.0".to_string(),
    }
}

#[test]
fn check_only_admission_never_needs_driver_device_library_or_toolchain() {
    let mut request = request(AcceleratorAdmissionMode::CheckOnly);
    request.driver_api = None;
    request.libraries.clear();
    request.device_memory_bytes = None;
    request.pinned_host_memory_bytes = None;
    let plan = AcceleratorTargetPlan::admit(&closure(), &request).expect("check-only plan");
    assert!(plan.hardware_probe_free);
    assert_eq!(plan.packages.len(), 1);
    assert_eq!(
        plan.deferred_requirements,
        [
            "synthetic:driver-api:synthetic-driver-1",
            "synthetic:library:synthetic-driver"
        ]
    );
    assert!(plan.toolchains.is_empty());
}

#[test]
fn driver_only_and_aot_modes_have_distinct_toolchain_requirements() {
    let driver =
        AcceleratorTargetPlan::admit(&closure(), &request(AcceleratorAdmissionMode::DriverOnly))
            .expect("driver-only plan");
    assert!(driver.toolchains.is_empty());

    let mut aot = request(AcceleratorAdmissionMode::ToolkitAot);
    assert!(AcceleratorTargetPlan::admit(&closure(), &aot)
        .expect_err("AOT requires explicit toolchain")
        .contains("toolchain"));
    aot.toolchains.push(explicit_toolchain());
    let plan = AcceleratorTargetPlan::admit(&closure(), &aot).expect("AOT plan");
    assert_eq!(plan.toolchains.len(), 1);
    assert_eq!(plan.packages[0].toolchains, ["synthetic-aot"]);
}

#[test]
fn admission_rejects_target_architecture_driver_memory_and_policy_mismatch() {
    for mutate in [
        |request: &mut AcceleratorTargetRequest| request.backend = "cuda".to_string(),
        |request: &mut AcceleratorTargetRequest| {
            request.target_triple = "aarch64-unknown-linux-gnu".to_string()
        },
        |request: &mut AcceleratorTargetRequest| {
            request.architecture = Some("vector-v2".to_string())
        },
        |request: &mut AcceleratorTargetRequest| request.artifact_format = "ptx".to_string(),
        |request: &mut AcceleratorTargetRequest| request.driver_api = Some("wrong".to_string()),
        |request: &mut AcceleratorTargetRequest| request.device_memory_bytes = Some(1),
        |request: &mut AcceleratorTargetRequest| {
            request.determinism = AcceleratorDeterminism::BestEffort
        },
    ] {
        let mut candidate = request(AcceleratorAdmissionMode::DriverOnly);
        mutate(&mut candidate);
        assert!(AcceleratorTargetPlan::admit(&closure(), &candidate).is_err());
    }
}

#[test]
fn admission_rejects_forged_toolchains_and_duplicate_bindings() {
    let mut forged_request = request(AcceleratorAdmissionMode::ToolkitAot);
    let mut toolchain = explicit_toolchain();
    toolchain.executable_sha256 = "00".repeat(32);
    forged_request.toolchains.push(toolchain);
    assert!(AcceleratorTargetPlan::admit(&closure(), &forged_request)
        .expect_err("forged toolchain")
        .contains("toolchain-identity"));

    let mut duplicate = request(AcceleratorAdmissionMode::DriverOnly);
    duplicate.libraries.push(duplicate.libraries[0].clone());
    assert!(AcceleratorTargetPlan::admit(&closure(), &duplicate)
        .expect_err("duplicate library")
        .contains("library-duplicate"));
}
