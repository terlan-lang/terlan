#[cfg(test)]
use super::BenchmarkStatus;

/// Runtime lane selected for capability reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(test)]
enum RuntimeLaneKind {
    TerlanVm,
}

#[cfg(test)]
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
#[cfg(test)]
enum RuntimeCapability {
    HttpRuntime,
}

#[cfg(test)]
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
#[cfg(test)]
enum RuntimeCapabilityStatus {
    Available,
}

/// One runtime lane represented in benchmark or gate output.
#[derive(Debug, Clone)]
#[cfg(test)]
struct RuntimeLane {
    name: &'static str,
    capability: &'static str,
    status: BenchmarkStatus,
    reason: Option<&'static str>,
    detail: Option<&'static str>,
}

#[cfg(test)]
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
#[cfg(test)]
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
#[path = "http_runtime_lane_test.rs"]
mod tests;
