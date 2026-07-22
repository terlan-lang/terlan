use super::BenchmarkStatus;

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
    Available,
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
    /// - VM HTTP lane with executable capability status.
    ///
    /// Transformation:
    /// - Uses the typed capability decision instead of ad hoc string checks.
    fn vm_http() -> Self {
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
        match capability_status {
            RuntimeCapabilityStatus::Available => Self {
                name: lane.name(),
                capability: capability.name(),
                status: BenchmarkStatus::Completed,
                reason: None,
                detail: None,
            },
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
/// - Keeps the 0.0.7 VM HTTP lane explicit without depending on the removed
///   OTP HTTP benchmark.
fn runtime_capability_status(
    lane: RuntimeLaneKind,
    capability: RuntimeCapability,
) -> RuntimeCapabilityStatus {
    match (lane, capability) {
        (RuntimeLaneKind::TerlanVm, RuntimeCapability::HttpRuntime) => {
            RuntimeCapabilityStatus::Available
        }
    }
}

#[cfg(test)]
mod tests {
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
}
