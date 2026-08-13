use super::*;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn probe_reports_unavailable_without_toolkit_or_driver() {
    let probe = CudaProbe {
        driver_available: false,
        device_available: false,
        toolkit_available: false,
        libtorch_cuda_available: false,
        nvcc_available: false,
        cuda_root_available: false,
    };

    assert_eq!(probe.status(), CudaAvailabilityStatus::Unavailable);
    assert_eq!(
        probe.summary().direct_cuda_reason_codes(),
        ["cuda-device-unavailable", "cuda-driver-unavailable"]
    );
}

#[test]
fn probe_reports_available_with_toolkit_and_driver() {
    let probe = CudaProbe {
        driver_available: true,
        device_available: true,
        toolkit_available: true,
        libtorch_cuda_available: false,
        nvcc_available: true,
        cuda_root_available: false,
    };

    assert_eq!(probe.status(), CudaAvailabilityStatus::Available);
}

#[test]
fn probe_keeps_driver_device_toolkit_and_libtorch_independent() {
    let driver_only = CudaProbe {
        driver_available: true,
        device_available: true,
        toolkit_available: false,
        libtorch_cuda_available: true,
        nvcc_available: false,
        cuda_root_available: false,
    };
    assert_eq!(driver_only.status(), CudaAvailabilityStatus::Unavailable);
    assert!(driver_only.summary().direct_cuda_reason_codes().is_empty());

    let toolkit_only = CudaProbe {
        driver_available: false,
        device_available: false,
        toolkit_available: true,
        libtorch_cuda_available: false,
        nvcc_available: true,
        cuda_root_available: false,
    };
    assert_eq!(toolkit_only.status(), CudaAvailabilityStatus::Unavailable);
    assert_eq!(
        toolkit_only.summary().direct_cuda_reason_codes(),
        ["cuda-device-unavailable", "cuda-driver-unavailable"]
    );
}

#[test]
fn package_execution_diagnostic_uses_shared_sorted_reason_codes() {
    let unavailable = CudaProbe {
        driver_available: false,
        device_available: false,
        toolkit_available: false,
        libtorch_cuda_available: true,
        nvcc_available: false,
        cuda_root_available: false,
    }
    .summary();
    validate_cuda_package_execution_readiness(&unavailable)
        .expect("external package gate owns typed skip");

    let available = CudaProbe {
        driver_available: true,
        device_available: true,
        toolkit_available: true,
        libtorch_cuda_available: false,
        nvcc_available: true,
        cuda_root_available: false,
    }
    .summary();
    validate_cuda_package_execution_readiness(&available)
        .expect("external package gate owns real execution");
}

#[test]
fn toolkit_root_requires_headers_and_compiler() {
    let root = TempRepo::new("cuda_toolkit_root");
    fs::create_dir_all(root.path().join("include")).expect("create include directory");
    fs::create_dir_all(root.path().join("bin")).expect("create bin directory");
    fs::write(root.path().join("include/cuda.h"), "/* fixture */").expect("write CUDA header");

    assert!(!cuda_root_has_toolkit(root.path()));
    fs::write(root.path().join("bin/nvcc"), "fixture").expect("write nvcc fixture");
    assert!(cuda_root_has_toolkit(root.path()));
}

#[test]
fn libtorch_cuda_probe_accepts_platform_library_layouts() {
    for relative in [
        "lib/libtorch_cuda.so",
        "lib/libtorch_cuda.dylib",
        "lib/torch_cuda.lib",
        "bin/torch_cuda.dll",
    ] {
        let root = TempRepo::new("libtorch_cuda_root");
        let library = root.path().join(relative);
        fs::create_dir_all(library.parent().expect("library parent"))
            .expect("create library directory");
        fs::write(&library, "fixture").expect("write CUDA library fixture");
        assert!(libtorch_root_has_cuda(root.path()), "{relative}");
    }
}

#[test]
fn availability_report_is_deterministic_and_keeps_cuda_lanes_separate() {
    let root = TempRepo::new("cuda_availability_report");
    let summary = CudaProbe {
        driver_available: true,
        device_available: true,
        toolkit_available: false,
        libtorch_cuda_available: true,
        nvcc_available: false,
        cuda_root_available: false,
    }
    .summary();

    write_status_report(root.path(), &summary).expect("write first report");
    let path = root.path().join(STATUS_REPORT);
    let first = fs::read(&path).expect("read first report");
    write_status_report(root.path(), &summary).expect("write second report");
    assert_eq!(first, fs::read(&path).expect("read second report"));

    let report: serde_json::Value = serde_json::from_slice(&first).expect("parse status report");
    assert_eq!(report["gate_result"], "passed");
    assert_eq!(report["direct_cuda"]["execution_disposition"], "run");
    assert_eq!(report["direct_cuda"]["reason_codes"], json!([]));
    assert_eq!(report["pytorch_cuda"]["execution_disposition"], "run");
    assert_eq!(report["pytorch_cuda"]["reason_codes"], json!([]));
}

#[test]
fn availability_report_sorts_reason_codes() {
    let root = TempRepo::new("cuda_availability_reason_order");
    let summary = CudaProbe {
        driver_available: false,
        device_available: false,
        toolkit_available: false,
        libtorch_cuda_available: false,
        nvcc_available: false,
        cuda_root_available: false,
    }
    .summary();

    write_status_report(root.path(), &summary).expect("write report");
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(root.path().join(STATUS_REPORT)).expect("read report"))
            .expect("parse report");
    assert_eq!(
        report["direct_cuda"]["reason_codes"],
        json!(["cuda-device-unavailable", "cuda-driver-unavailable"])
    );
    assert_eq!(
        report["pytorch_cuda"]["reason_codes"],
        json!([
            "cuda-device-unavailable",
            "cuda-driver-unavailable",
            "libtorch-cuda-unavailable"
        ])
    );
}

#[test]
fn availability_rejects_core_cuda_dependency() {
    let root = TempRepo::new("cuda_package_core_dependency");
    root.write_manifest(
        r#"[package]
name = "terlan"

[dependencies]
cudarc = "0.17"
"#,
    );

    let error = validate_no_core_cuda_dependency(root.path()).expect_err("cuda dependency");

    assert!(error.contains("direct CUDA dependency `cudarc` is forbidden"));
}

#[test]
fn availability_accepts_core_without_cuda_dependency() {
    let root = TempRepo::new("cuda_package_no_core_dependency");
    root.write_manifest(
        r#"[package]
name = "terlan"

[dependencies]
serde = "1"
"#,
    );

    validate_no_core_cuda_dependency(root.path()).expect("no CUDA dependency");
}

struct TempRepo {
    path: std::path::PathBuf,
}

impl TempRepo {
    fn new(name: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let path = env::temp_dir().join(format!("{name}_{}_{}", std::process::id(), unique));
        fs::create_dir_all(path.join("crates/terlan")).expect("create temp repo");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn write_manifest(&self, contents: &str) {
        fs::write(self.path.join(CORE_MANIFEST), contents).expect("write manifest");
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
