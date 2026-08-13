use std::collections::BTreeMap;
use std::env;
use std::path::Path;
use std::process::Command;

use serde::Deserialize;
use serde_json::json;

use crate::terlan_quality::QualityResult;

const CORE_MANIFEST: &str = "crates/terlan/Cargo.toml";
const STATUS_REPORT: &str = "target/quality/cuda-package-availability-status.json";
const CUDA_DEPENDENCY_MARKERS: &[&str] = &["cuda", "cudarc", "cust", "rustacuda"];

/// Summary produced by the CUDA package availability gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CudaPackageAvailabilitySummary {
    pub status: CudaAvailabilityStatus,
    pub driver_available: bool,
    pub device_available: bool,
    pub toolkit_available: bool,
    pub libtorch_cuda_available: bool,
    pub nvcc_available: bool,
    pub cuda_root_available: bool,
}

/// CUDA availability state observed by the default-safe gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CudaAvailabilityStatus {
    Available,
    Unavailable,
}

impl CudaAvailabilityStatus {
    /// Returns a stable user-facing status label.
    pub fn as_str(self) -> &'static str {
        match self {
            CudaAvailabilityStatus::Available => "available",
            CudaAvailabilityStatus::Unavailable => "unavailable",
        }
    }
}

/// Runs the default-safe CUDA availability gate.
///
/// Inputs:
/// - `root`: Terlan golden repository root.
/// - Host environment and `PATH`.
///
/// Output:
/// - Success summary even when CUDA is unavailable.
/// - Failure only when the core compiler crate grows a direct CUDA dependency.
///
/// Transformation:
/// - Enforces CUDA as an optional external package capability, then probes the
///   local machine for CUDA toolkit/driver indicators without requiring them.
pub fn run_cuda_package_availability(root: &Path) -> QualityResult<CudaPackageAvailabilitySummary> {
    validate_no_core_cuda_dependency(root)?;
    let probe = CudaProbe::from_environment();
    let summary = probe.summary();
    write_status_report(root, &summary)?;
    Ok(summary)
}

/// Runs the opt-in CUDA package execution gate.
///
/// Inputs:
/// - `root`: Terlan golden repository root.
/// - Host environment and `PATH`.
///
/// Output:
/// - Success after the core-dependency policy and capability probe pass.
/// - External package execution or a typed CPU-only skip is owned by the
///   package gate invoked from the Make target.
///
/// Transformation:
/// - Reuses the default availability probe. The sibling `terlan-cuda` gate
///   consumes this state and writes the execution report.
pub fn run_cuda_package_check(root: &Path) -> QualityResult<CudaPackageAvailabilitySummary> {
    let summary = run_cuda_package_availability(root)?;
    validate_cuda_package_execution_readiness(&summary)?;
    Ok(summary)
}

fn validate_no_core_cuda_dependency(root: &Path) -> QualityResult<()> {
    let path = root.join(CORE_MANIFEST);
    let text = std::fs::read_to_string(&path)
        .map_err(|err| format!("{}: failed to read manifest: {err}", path.display()))?;
    let manifest: CargoManifest = basic_toml::from_str(&text)
        .map_err(|err| format!("{}: invalid TOML: {err}", path.display()))?;
    let mut diagnostics = Vec::new();
    for dependency in manifest.dependencies.keys() {
        let normalized = dependency.to_lowercase();
        if CUDA_DEPENDENCY_MARKERS
            .iter()
            .any(|marker| normalized.contains(marker))
        {
            diagnostics.push(format!(
                "{CORE_MANIFEST}: direct CUDA dependency `{dependency}` is forbidden; CUDA must live behind an external package/native boundary"
            ));
        }
    }
    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(render_failure(&diagnostics))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CudaProbe {
    driver_available: bool,
    device_available: bool,
    toolkit_available: bool,
    libtorch_cuda_available: bool,
    nvcc_available: bool,
    cuda_root_available: bool,
}

impl CudaProbe {
    fn from_environment() -> Self {
        let driver_available = command_available("nvidia-smi");
        let device_available = driver_available && cuda_device_available();
        let nvcc_available = command_available("nvcc");
        let cuda_root_available = environment_cuda_root()
            .as_deref()
            .is_some_and(cuda_root_has_toolkit);
        Self {
            driver_available,
            device_available,
            toolkit_available: nvcc_available || cuda_root_available,
            libtorch_cuda_available: environment_libtorch_root()
                .as_deref()
                .is_some_and(libtorch_root_has_cuda),
            nvcc_available,
            cuda_root_available,
        }
    }

    fn summary(self) -> CudaPackageAvailabilitySummary {
        CudaPackageAvailabilitySummary {
            status: self.status(),
            driver_available: self.driver_available,
            device_available: self.device_available,
            toolkit_available: self.toolkit_available,
            libtorch_cuda_available: self.libtorch_cuda_available,
            nvcc_available: self.nvcc_available,
            cuda_root_available: self.cuda_root_available,
        }
    }

    fn status(self) -> CudaAvailabilityStatus {
        if self.driver_available && self.device_available && self.toolkit_available {
            CudaAvailabilityStatus::Available
        } else {
            CudaAvailabilityStatus::Unavailable
        }
    }
}

impl CudaPackageAvailabilitySummary {
    fn direct_cuda_reason_codes(&self) -> Vec<&'static str> {
        let mut missing = Vec::new();
        if !self.driver_available {
            missing.push("cuda-driver-unavailable");
        }
        if !self.device_available {
            missing.push("cuda-device-unavailable");
        }
        missing.sort_unstable();
        missing
    }

    fn pytorch_cuda_reason_codes(&self) -> Vec<&'static str> {
        let mut missing = Vec::new();
        if !self.driver_available {
            missing.push("cuda-driver-unavailable");
        }
        if !self.device_available {
            missing.push("cuda-device-unavailable");
        }
        if !self.libtorch_cuda_available {
            missing.push("libtorch-cuda-unavailable");
        }
        missing.sort_unstable();
        missing
    }
}

fn validate_cuda_package_execution_readiness(
    _summary: &CudaPackageAvailabilitySummary,
) -> QualityResult<()> {
    Ok(())
}

fn command_available(command: &str) -> bool {
    Command::new(command)
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn cuda_device_available() -> bool {
    Command::new("nvidia-smi")
        .args(["--query-gpu=index", "--format=csv,noheader"])
        .output()
        .map(|output| output.status.success() && !output.stdout.iter().all(u8::is_ascii_whitespace))
        .unwrap_or(false)
}

fn environment_cuda_root() -> Option<std::path::PathBuf> {
    env::var_os("CUDA_HOME")
        .or_else(|| env::var_os("CUDA_PATH"))
        .map(Into::into)
}

fn cuda_root_has_toolkit(root: &Path) -> bool {
    root.join("include/cuda.h").is_file()
        && (root.join("bin/nvcc").is_file() || root.join("bin/nvcc.exe").is_file())
}

fn environment_libtorch_root() -> Option<std::path::PathBuf> {
    env::var_os("LIBTORCH")
        .or_else(|| env::var_os("LIBTORCH_DIR"))
        .map(Into::into)
}

fn libtorch_root_has_cuda(root: &Path) -> bool {
    [
        "lib/libtorch_cuda.so",
        "lib/libtorch_cuda.dylib",
        "lib/torch_cuda.lib",
        "bin/torch_cuda.dll",
    ]
    .iter()
    .any(|relative| root.join(relative).is_file())
}

fn write_status_report(root: &Path, summary: &CudaPackageAvailabilitySummary) -> QualityResult<()> {
    let path = root.join(STATUS_REPORT);
    let direct_missing = summary.direct_cuda_reason_codes();
    let pytorch_missing = summary.pytorch_cuda_reason_codes();
    let report = json!({
        "schema": "terlan.cuda-package-availability-status.v1",
        "gate_result": "passed",
        "observations": {
            "driver_available": summary.driver_available,
            "device_available": summary.device_available,
            "toolkit_available": summary.toolkit_available,
            "libtorch_cuda_available": summary.libtorch_cuda_available,
            "nvcc_available": summary.nvcc_available,
            "cuda_root_available": summary.cuda_root_available,
        },
        "direct_cuda": {
            "ready": direct_missing.is_empty(),
            "execution_disposition": if direct_missing.is_empty() { "run" } else { "skip" },
            "reason_codes": direct_missing,
        },
        "pytorch_cuda": {
            "ready": pytorch_missing.is_empty(),
            "execution_disposition": if pytorch_missing.is_empty() { "run" } else { "skip" },
            "reason_codes": pytorch_missing,
        },
    });
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| {
            format!(
                "{}: failed to create report directory: {err}",
                parent.display()
            )
        })?;
    }
    let mut bytes = serde_json::to_vec_pretty(&report)
        .map_err(|err| format!("{}: failed to encode status report: {err}", path.display()))?;
    bytes.push(b'\n');
    std::fs::write(&path, bytes)
        .map_err(|err| format!("{}: failed to write status report: {err}", path.display()))
}

fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[cuda-package-availability] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[derive(Debug, Deserialize)]
struct CargoManifest {
    #[serde(default)]
    dependencies: BTreeMap<String, serde::de::IgnoredAny>,
}

#[cfg(test)]
#[path = "cuda_package_availability_test.rs"]
mod tests;
