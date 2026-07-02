use super::BenchmarkStatus;

const VM_HTTP_UNAVAILABLE_REASON: &str = "terlan_vm_http_runtime_unavailable";
const VM_HTTP_UNAVAILABLE_DETAIL: &str = "Terlan VM HTTP runtime lane is not implemented yet.";

/// Runtime lane selected for capability reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimeLaneKind {
    TerlanVm,
}

impl RuntimeLaneKind {
    /// Returns the stable lane name used in benchmark reports.
    ///
    /// Inputs:
    /// - `self`: runtime lane selector.
    ///
    /// Output:
    /// - Stable machine-readable lane name.
    ///
    /// Transformation:
    /// - Maps the internal selector to the JSON/report vocabulary.
    fn name(self) -> &'static str {
        match self {
            RuntimeLaneKind::TerlanVm => "terlan-vm-http-runtime",
        }
    }
}

/// Runtime capability selected for capability reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimeCapability {
    HttpRuntime,
}

impl RuntimeCapability {
    /// Returns the stable capability name used in benchmark reports.
    ///
    /// Inputs:
    /// - `self`: runtime capability selector.
    ///
    /// Output:
    /// - Stable machine-readable capability name.
    ///
    /// Transformation:
    /// - Maps the internal selector to the JSON/report vocabulary.
    fn name(self) -> &'static str {
        match self {
            RuntimeCapability::HttpRuntime => "http_runtime",
        }
    }
}

/// Typed availability decision for one runtime capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimeCapabilityStatus {
    Unavailable {
        reason: &'static str,
        detail: &'static str,
    },
}

/// One runtime lane represented in benchmark or gate output.
#[derive(Debug, Clone)]
struct RuntimeLane {
    name: &'static str,
    capability: &'static str,
    status: BenchmarkStatus,
    reason: Option<&'static str>,
    detail: Option<&'static str>,
}

impl RuntimeLane {
    /// Returns the current VM HTTP lane status.
    ///
    /// Inputs:
    /// - None.
    ///
    /// Output:
    /// - Skipped VM HTTP lane with stable unavailable reason and detail.
    ///
    /// Transformation:
    /// - Uses the typed capability decision instead of ad hoc string checks.
    fn skipped_vm_http() -> Self {
        Self::from_capability(
            RuntimeLaneKind::TerlanVm,
            RuntimeCapability::HttpRuntime,
            runtime_capability_status(RuntimeLaneKind::TerlanVm, RuntimeCapability::HttpRuntime),
        )
    }

    /// Builds a report lane from a typed runtime capability decision.
    ///
    /// Inputs:
    /// - `lane`: runtime lane selector.
    /// - `capability`: capability selector.
    /// - `capability_status`: typed availability decision.
    ///
    /// Output:
    /// - Runtime lane report entry.
    ///
    /// Transformation:
    /// - Converts capability availability into benchmark report status fields.
    fn from_capability(
        lane: RuntimeLaneKind,
        capability: RuntimeCapability,
        capability_status: RuntimeCapabilityStatus,
    ) -> Self {
        let RuntimeCapabilityStatus::Unavailable { reason, detail } = capability_status;
        Self {
            name: lane.name(),
            capability: capability.name(),
            status: BenchmarkStatus::Skipped,
            reason: Some(reason),
            detail: Some(detail),
        }
    }
}

/// Returns the typed availability status for one runtime lane capability.
///
/// Inputs:
/// - `lane`: runtime lane selector.
/// - `capability`: capability selector.
///
/// Output:
/// - Stable capability decision for the requested lane.
///
/// Transformation:
/// - Keeps the 0.0.7 VM HTTP lane explicit while the runtime is not yet
///   implemented, without depending on the removed OTP HTTP benchmark.
fn runtime_capability_status(
    lane: RuntimeLaneKind,
    capability: RuntimeCapability,
) -> RuntimeCapabilityStatus {
    match (lane, capability) {
        (RuntimeLaneKind::TerlanVm, RuntimeCapability::HttpRuntime) => {
            RuntimeCapabilityStatus::Unavailable {
                reason: VM_HTTP_UNAVAILABLE_REASON,
                detail: VM_HTTP_UNAVAILABLE_DETAIL,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the Terlan VM HTTP placeholder is a typed capability decision.
    ///
    /// Inputs:
    /// - VM runtime lane and HTTP runtime capability.
    ///
    /// Output:
    /// - Test passes when the capability reports the stable unavailable code.
    ///
    /// Transformation:
    /// - Pins VM HTTP unavailability to an explicit runtime capability result.
    #[test]
    fn terlan_vm_http_runtime_capability_is_typed_unavailable() {
        assert_eq!(
            runtime_capability_status(RuntimeLaneKind::TerlanVm, RuntimeCapability::HttpRuntime),
            RuntimeCapabilityStatus::Unavailable {
                reason: VM_HTTP_UNAVAILABLE_REASON,
                detail: VM_HTTP_UNAVAILABLE_DETAIL,
            }
        );
    }

    /// Verifies VM HTTP lane reports use the typed capability status.
    ///
    /// Inputs:
    /// - No external runtime state.
    ///
    /// Output:
    /// - Test passes when lane fields contain the stable VM HTTP skip status.
    ///
    /// Transformation:
    /// - Converts the typed capability decision into the report shape that
    ///   future runtime benchmarks can reuse.
    #[test]
    fn runtime_report_vm_lane_uses_typed_capability_status() {
        let report = RuntimeLane::skipped_vm_http();

        assert_eq!(report.name, "terlan-vm-http-runtime");
        assert_eq!(report.capability, "http_runtime");
        assert!(matches!(report.status, BenchmarkStatus::Skipped));
        assert_eq!(report.reason, Some(VM_HTTP_UNAVAILABLE_REASON));
        assert_eq!(report.detail, Some(VM_HTTP_UNAVAILABLE_DETAIL));
    }
}
