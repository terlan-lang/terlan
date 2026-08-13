use super::*;

/// Verifies the Terlan VM HTTP lane is a typed executable capability.
///
/// Inputs:
/// - VM runtime lane and HTTP runtime capability.
///
/// Output:
/// - Test passes when the capability reports available.
///
/// Transformation:
/// - Pins VM HTTP availability to an explicit runtime capability result.
#[test]
fn terlan_vm_http_runtime_capability_is_available() {
    assert_eq!(
        runtime_capability_status(RuntimeLaneKind::TerlanVm, RuntimeCapability::HttpRuntime),
        RuntimeCapabilityStatus::Available
    );
}

/// Verifies VM HTTP lane reports use the typed capability status.
///
/// Inputs:
/// - No external runtime state.
///
/// Output:
/// - Test passes when lane fields contain the executable VM HTTP status.
///
/// Transformation:
/// - Converts the typed capability decision into the report shape that
///   runtime benchmarks can reuse.
#[test]
fn runtime_report_vm_lane_uses_typed_capability_status() {
    let report = RuntimeLane::vm_http();

    assert_eq!(report.name, "terlan-vm-http-runtime");
    assert_eq!(report.capability, "http_runtime");
    assert!(matches!(report.status, BenchmarkStatus::Completed));
    assert_eq!(report.reason, None);
    assert_eq!(report.detail, None);
}
