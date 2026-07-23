//! Canonical replay boundaries around generated scheduler execution.

use crate::runtime::vm::fixed_scheduler_control::VmFixedActorLease;
use crate::runtime::vm::fixed_scheduler_control::VmFixedSchedulerControl;
use crate::runtime::vm::fixed_scheduler_telemetry::{
    VmFixedSchedulerEventKind, VmFixedSchedulerTelemetry,
};

use super::AotSchedulerPublication;

/// Executes one actor slice and closes its replay interval on every result path.
pub(super) fn execute_interval<T>(
    telemetry: &VmFixedSchedulerTelemetry,
    lease: &VmFixedActorLease,
    execute: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let context = telemetry.begin_execution(lease)?;
    let result = execute();
    let finished = telemetry.finish_execution(context);
    match (result, finished) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(error),
        (Err(error), Err(finish_error)) => Err(format!("{error}; {finish_error}")),
    }
}

/// Releases and reclaims one actor while preserving its final owner identity.
pub(super) fn settle_terminal<T>(
    control: &VmFixedSchedulerControl<AotSchedulerPublication>,
    telemetry: &VmFixedSchedulerTelemetry,
    lease: VmFixedActorLease,
    result: Result<T, String>,
) -> Result<T, String> {
    let context = telemetry.context_for_lease(&lease)?;
    let route = lease.route();
    let settled = control
        .release(
            lease,
            crate::runtime::vm::actor_directory::VmActorLifecycle::Exiting,
        )
        .and_then(|_| control.reclaim(route))
        .and(result);
    let kind = if settled.is_ok() {
        VmFixedSchedulerEventKind::Completed
    } else {
        VmFixedSchedulerEventKind::Failed
    };
    match (settled, telemetry.record_with_context(kind, context)) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(error),
        (Err(error), Err(record_error)) => Err(format!("{error}; {record_error}")),
    }
}
