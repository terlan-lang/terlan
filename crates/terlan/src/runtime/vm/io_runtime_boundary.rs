#[cfg(test)]
#[path = "io_runtime_boundary_test.rs"]
#[cfg(test)]
mod io_runtime_boundary_test;

/// Host-side work an external I/O helper may perform for the VM.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) enum VmExternalIoRuntimeRole {
    ByteProducer,
    ByteConsumer,
    NameResolver,
    CryptoHandshake,
}

/// Scheduling behavior an external I/O helper is allowed to have.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) enum VmExternalIoSchedulingPolicy {
    /// The helper may block or poll host I/O but must report readiness back to
    /// `VmIoReactorLoop` as typed VM wakeups.
    VmWakeProducerOnly,
    /// Rejected: the helper would decide actor order or runnable state itself.
    OwnsActorScheduling,
    /// Rejected: the helper would hold process continuations outside the VM.
    OwnsProcessContinuations,
    /// Rejected: the helper would call scheduler wake APIs directly.
    DirectSchedulerAccess,
}

/// VM-owned external I/O runtime boundary plan.
///
/// Inputs:
/// - Adapter role, scheduling policy, wakeup behavior, backpressure behavior,
///   and replay behavior for a host-side I/O helper.
///
/// Output:
/// - A validated plan that can be attached to VM I/O integration code.
///
/// Transformation:
/// - Allows maintained Rust libraries to perform protocol or socket work while
///   rejecting any plan that transfers actor scheduling, continuations, or
///   readiness replay ownership out of the Terlan VM.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmExternalIoRuntimePlan {
    pub(crate) name: String,
    pub(crate) role: VmExternalIoRuntimeRole,
    pub(crate) scheduling_policy: VmExternalIoSchedulingPolicy,
    pub(crate) emits_typed_vm_wakeups: bool,
    pub(crate) enforces_bounded_backpressure: bool,
    pub(crate) records_support_bundle_replay: bool,
}

#[cfg(test)]
impl VmExternalIoRuntimePlan {
    /// Creates a candidate external runtime plan.
    pub(crate) fn new(
        name: impl Into<String>,
        role: VmExternalIoRuntimeRole,
        scheduling_policy: VmExternalIoSchedulingPolicy,
    ) -> Self {
        Self {
            name: name.into(),
            role,
            scheduling_policy,
            emits_typed_vm_wakeups: false,
            enforces_bounded_backpressure: false,
            records_support_bundle_replay: false,
        }
    }

    /// Marks whether the helper returns typed wakeups to `VmIoReactorLoop`.
    pub(crate) fn with_typed_vm_wakeups(mut self, enabled: bool) -> Self {
        self.emits_typed_vm_wakeups = enabled;
        self
    }

    /// Marks whether helper queues are bounded before crossing into the VM.
    pub(crate) fn with_bounded_backpressure(mut self, enabled: bool) -> Self {
        self.enforces_bounded_backpressure = enabled;
        self
    }

    /// Marks whether readiness can be captured in support-bundle replay data.
    pub(crate) fn with_support_bundle_replay(mut self, enabled: bool) -> Self {
        self.records_support_bundle_replay = enabled;
        self
    }
}

/// Validated external I/O helper boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmExternalIoRuntimeBoundary {
    pub(crate) name: String,
    pub(crate) role: VmExternalIoRuntimeRole,
}

#[cfg(test)]
impl VmExternalIoRuntimeBoundary {
    /// Validates that an external I/O helper cannot own VM scheduling.
    pub(crate) fn validate(plan: VmExternalIoRuntimePlan) -> Result<Self, String> {
        if plan.name.trim().is_empty() {
            return Err("external I/O runtime boundary name cannot be empty".to_string());
        }
        match plan.scheduling_policy {
            VmExternalIoSchedulingPolicy::VmWakeProducerOnly => {}
            VmExternalIoSchedulingPolicy::OwnsActorScheduling => {
                return Err(format!(
                    "external I/O runtime `{}` cannot own actor scheduling",
                    plan.name
                ));
            }
            VmExternalIoSchedulingPolicy::OwnsProcessContinuations => {
                return Err(format!(
                    "external I/O runtime `{}` cannot own process continuations",
                    plan.name
                ));
            }
            VmExternalIoSchedulingPolicy::DirectSchedulerAccess => {
                return Err(format!(
                    "external I/O runtime `{}` cannot call VM scheduler wake APIs directly",
                    plan.name
                ));
            }
        }
        if !plan.emits_typed_vm_wakeups {
            return Err(format!(
                "external I/O runtime `{}` must emit typed VM wakeups",
                plan.name
            ));
        }
        if !plan.enforces_bounded_backpressure {
            return Err(format!(
                "external I/O runtime `{}` must enforce bounded backpressure",
                plan.name
            ));
        }
        if !plan.records_support_bundle_replay {
            return Err(format!(
                "external I/O runtime `{}` must record support-bundle replay metadata",
                plan.name
            ));
        }
        Ok(Self {
            name: plan.name,
            role: plan.role,
        })
    }
}
