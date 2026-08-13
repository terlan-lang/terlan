use super::*;

#[cfg(test)]
pub(super) fn opaque_value(kind: &'static str) -> VmValueSizeError {
    VmValueSizeError::OpaqueValue { kind }
}

pub(super) fn add_sequence_storage(
    total: &mut usize,
    slots: usize,
) -> Result<(), VmValueSizeError> {
    checked_add_size(total, LOGICAL_SEQUENCE_HEADER_BYTES)?;
    let bytes = slots
        .checked_mul(LOGICAL_VALUE_SLOT_BYTES)
        .ok_or(VmValueSizeError::Overflow)?;
    checked_add_size(total, bytes)
}

pub(super) fn add_logical_string(total: &mut usize, value: &str) -> Result<(), VmValueSizeError> {
    checked_add_size(total, LOGICAL_STRING_HEADER_BYTES)?;
    checked_add_size(total, value.len())
}

pub(super) fn checked_add_size(total: &mut usize, bytes: usize) -> Result<(), VmValueSizeError> {
    *total = total.checked_add(bytes).ok_or(VmValueSizeError::Overflow)?;
    Ok(())
}

/// Runs one memory mutation under the process table's scoped actor ownership.
pub(super) fn with_live_process_mut<R>(
    processes: &mut VmProcessTable,
    pid: VmProcessId,
    mutate: impl FnOnce(&mut super::super::process::VmProcess) -> R,
) -> Result<R, String> {
    require_live_process(processes, pid)?;
    processes.with_process_control_mutator(pid, mutate)
}

/// Validates a live process before memory ownership is read or changed.
pub(super) fn require_live_process(
    processes: &VmProcessTable,
    pid: VmProcessId,
) -> Result<(), String> {
    let process = processes
        .get(pid)
        .ok_or_else(|| format!("missing process {} for VM memory accounting", pid.as_u64()))?;
    if matches!(process.state, VmProcessState::Exited(_)) {
        return Err(format!(
            "exited process {} cannot own VM heap bytes",
            pid.as_u64()
        ));
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn stale_shared_allocation(allocation: VmSharedAllocationId) -> String {
    format!("stale VM shared allocation {}", allocation.as_u64())
}
