#![forbid(unsafe_code)]

//! Emits hardware-independent accelerator target-admission plans.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::Serialize;
use terlan::compiler::accelerator::{
    AcceleratorAdmissionMode, AcceleratorDependencyClosure, AcceleratorDescriptor,
    AcceleratorDeterminism, AcceleratorPackageDescriptor, AcceleratorTargetPlan,
    AcceleratorTargetRequest,
};
use terlan::support::boundary_error::QualityResult;

/// Quality report joining CUDA and synthetic backend plans.
#[derive(Serialize)]
struct TargetAdmissionReport {
    /// Stable report schema.
    schema: &'static str,
    /// Plans produced without hardware discovery.
    plans: Vec<AcceleratorTargetPlan>,
    /// Stable adversarial classes covered by the Rust gate.
    rejection_evidence: Vec<&'static str>,
}

/// Minimal second-backend fixture proving target admission is backend-neutral.
const SYNTHETIC_DESCRIPTOR: &str = r#"
schema = 1
backend = "synthetic-vector"
device_classes = ["vector"]
artifact_formats = ["vector-object"]
dtypes = ["f32"]
layouts = ["row-major"]
address_spaces = ["host", "device"]
resource_classes = ["buffer"]
asynchronous_operations = ["execute"]
capabilities = ["accelerator.synthetic.execute"]

[[targets]]
triple = "x86_64-unknown-linux-gnu"
availability = "experimental"
artifact_formats = ["vector-object"]
architectures = ["vector-v1"]
determinism = "strict"

[[operations]]
id = "buffer.add"
effects = ["execute"]
asynchronous = true
"#;

/// Parses output and CUDA descriptor paths.
fn arguments() -> QualityResult<(PathBuf, PathBuf)> {
    let mut values = std::env::args().skip(1);
    let output = values.next().map(PathBuf::from).ok_or_else(|| {
        "usage: terlan-accelerator-target-admission <output> <cuda-descriptor>".to_string()
    })?;
    let descriptor = values.next().map(PathBuf::from).ok_or_else(|| {
        "usage: terlan-accelerator-target-admission <output> <cuda-descriptor>".to_string()
    })?;
    if values.next().is_some() {
        return Err("unexpected accelerator target-admission argument".into());
    }
    Ok((output, descriptor))
}

/// Builds one resolved single-package closure.
fn closure(
    package: &str,
    source: &str,
    provenance: &Path,
) -> QualityResult<AcceleratorDependencyClosure> {
    let descriptor =
        AcceleratorDescriptor::parse(source, provenance).map_err(|error| error.to_string())?;
    Ok(AcceleratorDependencyClosure::resolve(
        vec![AcceleratorPackageDescriptor {
            package: package.to_string(),
            version: "0.0.8".to_string(),
            source: provenance.to_string_lossy().into_owned(),
            descriptor,
        }],
        provenance,
    )
    .map_err(|error| error.to_string())?)
}

/// Creates a descriptor-only request that never probes a driver or device.
fn check_request(
    backend: &str,
    target_triple: &str,
    architecture: Option<&str>,
    artifact_format: &str,
    determinism: AcceleratorDeterminism,
) -> AcceleratorTargetRequest {
    AcceleratorTargetRequest {
        backend: backend.to_string(),
        target_triple: target_triple.to_string(),
        architecture: architecture.map(str::to_string),
        artifact_format: artifact_format.to_string(),
        mode: AcceleratorAdmissionMode::CheckOnly,
        driver_api: None,
        libraries: Vec::new(),
        toolchains: Vec::new(),
        device_memory_bytes: None,
        pinned_host_memory_bytes: None,
        unified_memory: false,
        determinism,
    }
}

/// Emits deterministic CPU-only CUDA and synthetic target plans.
fn run() -> QualityResult<()> {
    let (output, cuda_path) = arguments()?;
    let cuda_source = fs::read_to_string(&cuda_path)
        .map_err(|error| format!("cannot read {}: {error}", cuda_path.display()))?;
    let cuda = closure("terlan-cuda", &cuda_source, &cuda_path)?;
    let synthetic_path = Path::new("synthetic/accelerator.toml");
    let synthetic = closure(
        "terlan-synthetic-vector",
        SYNTHETIC_DESCRIPTOR,
        synthetic_path,
    )?;
    let plans = vec![
        AcceleratorTargetPlan::admit(
            &cuda,
            &check_request(
                "cuda",
                "x86_64-unknown-linux-gnu",
                Some("sm-30"),
                "ptx",
                AcceleratorDeterminism::BestEffort,
            ),
        )
        .map_err(|error| error.to_string())?,
        AcceleratorTargetPlan::admit(
            &synthetic,
            &check_request(
                "synthetic-vector",
                "x86_64-unknown-linux-gnu",
                Some("vector-v1"),
                "vector-object",
                AcceleratorDeterminism::Strict,
            ),
        )
        .map_err(|error| error.to_string())?,
    ];
    let report = TargetAdmissionReport {
        schema: "terlan.accelerator-target-admission.v1",
        plans,
        rejection_evidence: vec![
            "target-mismatch",
            "architecture-mismatch",
            "artifact-format-mismatch",
            "driver-api-mismatch",
            "memory-budget-mismatch",
            "determinism-mismatch",
            "missing-explicit-toolchain",
            "toolchain-digest-mismatch",
            "duplicate-binding",
        ],
    };
    let parent = output
        .parent()
        .ok_or_else(|| "target-admission output has no parent".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    fs::write(
        &output,
        serde_json::to_string_pretty(&report)
            .map_err(|error| format!("cannot encode target admission report: {error}"))?
            + "\n",
    )
    .map_err(|error| format!("cannot write {}: {error}", output.display()))?;
    Ok(())
}

/// Runs the target-admission report emitter.
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error[accelerator.target-admission]: {error}");
            ExitCode::from(1)
        }
    }
}
