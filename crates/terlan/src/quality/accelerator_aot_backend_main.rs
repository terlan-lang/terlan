#![forbid(unsafe_code)]

//! Emits deterministic LLVM NVPTX backend selection and artifact evidence.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use serde::Serialize;
use terlan::compiler::accelerator::{
    accelerator_toolchain_sha256, AcceleratorAdmittedToolchain, AcceleratorAotBackend,
    AcceleratorAotRequest, AcceleratorArtifactDescriptor, AcceleratorExecutionDimensions,
    AcceleratorIrModule, AcceleratorIrSource, AcceleratorKernelSelection, LlvmNvptxBackend,
};
use terlan::compiler::hir::resolve_syntax_module_output;
use terlan::compiler::syntax::parse_module_as_syntax_output;
use terlan::compiler::typeck::{
    lower_syntax_module_output_to_core, type_check_syntax_module_output,
};
use terlan::support::boundary_error::QualityResult;

/// Stable AC5 quality report.
#[derive(Serialize)]
struct AcceleratorAotReport {
    /// Stable report schema.
    schema: &'static str,
    /// Selected maintained backend and reviewed criteria.
    backend_selection: BackendSelection,
    /// Versioned package-loadable artifact descriptor.
    artifact: AcceleratorArtifactDescriptor,
    /// True when a second request reused the cache.
    cache_hit_verified: bool,
    /// True when isolated builds emitted byte-identical artifacts.
    isolated_builds_reproducible: bool,
    /// Rejection classes covered by compiler tests.
    rejected_cases: Vec<&'static str>,
}

/// Recorded maintained-backend selection criteria.
#[derive(Serialize)]
struct BackendSelection {
    /// Compiler-owned adapter identity.
    implementation: &'static str,
    /// Maintained external project.
    project: &'static str,
    /// Exact executable version line.
    version: String,
    /// SPDX license expression.
    license: &'static str,
    /// Explicit executable path.
    executable: String,
    /// Immutable executable digest.
    executable_sha256: String,
    /// Supported target lane.
    target: &'static str,
    /// Reproducibility result for the selected lane.
    reproducibility: &'static str,
    /// Reviewed alternatives and reasons they were not selected first.
    evaluated_alternatives: Vec<BackendAlternative>,
}

/// One evaluated maintained backend alternative.
#[derive(Serialize)]
struct BackendAlternative {
    /// Project identity.
    project: &'static str,
    /// Current decision.
    status: &'static str,
    /// Selection rationale.
    reason: &'static str,
}

/// Parses report and explicit LLVM executable paths.
fn arguments() -> QualityResult<(PathBuf, PathBuf)> {
    let mut values = std::env::args().skip(1);
    let report = values
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| "usage: terlan-accelerator-aot-backend <report> <llc>".to_string())?;
    let llc = values
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| "usage: terlan-accelerator-aot-backend <report> <llc>".to_string())?;
    if values.next().is_some() {
        return Err("unexpected accelerator AOT backend argument".into());
    }
    Ok((report, llc))
}

/// Produces checked AcceleratorIR from the public compiler pipeline.
fn fixture_ir() -> QualityResult<AcceleratorIrModule> {
    let source = "\
module accelerator_aot_report.\n\
pub choose(left: Int, right: Int): Int ->\n\
    if { left > right -> left + 2; true -> right * 3 }.\n";
    let syntax = parse_module_as_syntax_output(source).map_err(|error| format!("{error:?}"))?;
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    if !diagnostics.is_empty() {
        return Err(format!("AOT fixture diagnostics: {diagnostics:#?}").into());
    }
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    Ok(AcceleratorIrModule::lower(
        &core,
        &[AcceleratorKernelSelection {
            function: "choose".to_string(),
            specializations: BTreeMap::new(),
            buffer_parameters: BTreeMap::new(),
            dimensions: AcceleratorExecutionDimensions {
                grid: [1, 1, 1],
                block: [32, 1, 1],
            },
            shared_memory_bytes: 0,
            synchronization_points: Vec::new(),
            math_operations: BTreeSet::new(),
            source: AcceleratorIrSource {
                file: "target/quality/accelerator_aot_report.terl".to_string(),
                line: 2,
                column: 1,
            },
        }],
    )
    .map_err(|error| error.to_string())?)
}

/// Returns the exact first version line from the explicit executable.
fn toolchain_version(executable: &Path) -> QualityResult<String> {
    let output = Command::new(executable)
        .arg("--version")
        .env_clear()
        .output()
        .map_err(|error| format!("cannot execute {}: {error}", executable.display()))?;
    if !output.status.success() {
        return Err(format!("{} --version failed", executable.display()).into());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout
        .lines()
        .next()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .ok_or_else(|| "LLVM version output is empty".to_string())?;
    Ok(line
        .split_whitespace()
        .last()
        .filter(|value| value.bytes().any(|byte| byte.is_ascii_digit()))
        .map(str::to_string)
        .ok_or_else(|| "LLVM semantic version is missing".to_string())?)
}

/// Compiles the fixture twice in isolation and writes the AC5 report.
fn run() -> QualityResult<()> {
    let (report_path, llc) = arguments()?;
    let executable = fs::canonicalize(&llc)
        .map_err(|error| format!("cannot resolve {}: {error}", llc.display()))?;
    let version = toolchain_version(&executable)?;
    let digest = accelerator_toolchain_sha256(&executable).map_err(|error| error.to_string())?;
    let toolchain = AcceleratorAdmittedToolchain {
        name: "llvm-nvptx".to_string(),
        version: version.clone(),
        executable: executable.to_string_lossy().into_owned(),
        executable_sha256: digest.clone(),
        license: "Apache-2.0 WITH LLVM-exception".to_string(),
    };
    let ir = fixture_ir()?;
    let root = report_path
        .parent()
        .ok_or_else(|| "AOT report path has no parent".to_string())?
        .join("accelerator-aot-backend");
    let first_directory = root.join("first");
    let second_directory = root.join("second");
    if root.exists() {
        fs::remove_dir_all(&root)
            .map_err(|error| format!("cannot reset {}: {error}", root.display()))?;
    }
    let options = BTreeMap::from([("optimization".to_string(), "2".to_string())]);
    let backend = LlvmNvptxBackend;
    let compile = |output_directory: &Path| {
        backend.compile(&AcceleratorAotRequest {
            ir: &ir,
            architecture: "sm-30",
            toolchain: &toolchain,
            build_options: options.clone(),
            output_directory,
        })
    };
    let first = compile(&first_directory).map_err(|error| error.to_string())?;
    let cached = compile(&first_directory).map_err(|error| error.to_string())?;
    let second = compile(&second_directory).map_err(|error| error.to_string())?;
    let report = AcceleratorAotReport {
        schema: "terlan.accelerator-aot-backend.v1",
        backend_selection: BackendSelection {
            implementation: "llvm-nvptx",
            project: "LLVM NVPTX",
            version,
            license: "Apache-2.0 WITH LLVM-exception",
            executable: executable.to_string_lossy().into_owned(),
            executable_sha256: digest,
            target: "nvptx64-nvidia-cuda/sm-30/PTX",
            reproducibility: "byte-identical-isolated-builds",
            evaluated_alternatives: vec![
                BackendAlternative {
                    project: "NVIDIA NVRTC",
                    status: "deferred",
                    reason: "runtime compiler conflicts with the AOT-only contract",
                },
                BackendAlternative {
                    project: "NVIDIA nvcc",
                    status: "deferred",
                    reason: "toolkit source compilation is not backend-neutral IR lowering",
                },
                BackendAlternative {
                    project: "Cranelift",
                    status: "unsupported",
                    reason: "no maintained NVPTX backend",
                },
            ],
        },
        artifact: first.descriptor,
        cache_hit_verified: cached.cache_hit,
        isolated_builds_reproducible: first.bytes == second.bytes,
        rejected_cases: vec![
            "unadmitted-toolchain",
            "invalid-architecture",
            "unsupported-scalar",
            "invalid-ir",
            "toolchain-failure",
            "malformed-ptx",
            "missing-entrypoint",
            "static-loop-unroll-limit",
        ],
    };
    fs::create_dir_all(
        report_path
            .parent()
            .ok_or_else(|| "AOT report path has no parent".to_string())?,
    )
    .map_err(|error| error.to_string())?;
    fs::write(
        &report_path,
        serde_json::to_string_pretty(&report).map_err(|error| error.to_string())? + "\n",
    )
    .map_err(|error| format!("cannot write {}: {error}", report_path.display()))?;
    Ok(())
}

/// Runs the maintained-backend quality report emitter.
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error[accelerator.aot-report]: {error}");
            ExitCode::from(1)
        }
    }
}
