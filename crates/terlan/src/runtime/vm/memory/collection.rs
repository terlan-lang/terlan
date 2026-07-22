use super::VmMemoryAccountant;
use crate::runtime::vm::process::{VmProcessId, VmProcessState, VmProcessTable};

/// Deterministic result of reconciling one process heap with traced live data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmMemoryCollection {
    pub(crate) pid: VmProcessId,
    pub(crate) previous_bytes: usize,
    pub(crate) protected_bytes: usize,
    pub(crate) traced_value_bytes: usize,
    pub(crate) retained_bytes: usize,
    pub(crate) reclaimed_bytes: usize,
}

impl VmMemoryAccountant {
    /// Reclaims unreferenced process heap bytes without releasing owned roots.
    ///
    /// `traced_value_bytes` is the retained size produced by the VM value
    /// tracer. Mailbox payloads, resources, and shared allocations are added by
    /// the accountant because their ownership cannot be inferred from a value
    /// stack alone.
    pub(crate) fn collect_process_heap(
        &mut self,
        processes: &mut VmProcessTable,
        pid: VmProcessId,
        traced_value_bytes: usize,
    ) -> Result<VmMemoryCollection, String> {
        let process = processes
            .get(pid)
            .ok_or_else(|| format!("missing process {} for VM memory accounting", pid.as_u64()))?;
        if matches!(process.state, VmProcessState::Exited(_)) {
            return Err(format!(
                "exited process {} cannot own VM heap bytes",
                pid.as_u64()
            ));
        }
        let previous_bytes = process.heap_bytes;
        let mailbox_bytes = process.mailbox_accounted_bytes()?;
        let protected_bytes = self.protected_process_bytes(pid, mailbox_bytes)?;
        let retained_bytes = protected_bytes
            .checked_add(traced_value_bytes)
            .ok_or_else(|| {
                format!(
                    "process {} collected heap retained byte overflow",
                    pid.as_u64()
                )
            })?;
        if retained_bytes > previous_bytes {
            return Err(format!(
                "process {} collected heap retains {} bytes from {} accounted bytes",
                pid.as_u64(),
                retained_bytes,
                previous_bytes
            ));
        }
        let reclaimed_bytes = previous_bytes - retained_bytes;
        self.release_heap(processes, pid, reclaimed_bytes)?;
        Ok(VmMemoryCollection {
            pid,
            previous_bytes,
            protected_bytes,
            traced_value_bytes,
            retained_bytes,
            reclaimed_bytes,
        })
    }

    fn protected_process_bytes(
        &self,
        pid: VmProcessId,
        mailbox_bytes: usize,
    ) -> Result<usize, String> {
        let resource_bytes = checked_owned_sum(
            self.resource_ownership
                .values()
                .filter(|ownership| ownership.owner == pid.as_u64())
                .map(|ownership| ownership.logical_bytes),
            pid,
            "resource",
        )?;
        let shared_bytes = checked_owned_sum(
            self.shared_allocations
                .values()
                .filter(|allocation| allocation.owners.contains(&pid.as_u64()))
                .map(|allocation| allocation.logical_bytes),
            pid,
            "shared allocation",
        )?;
        mailbox_bytes
            .checked_add(resource_bytes)
            .and_then(|bytes| bytes.checked_add(shared_bytes))
            .ok_or_else(|| format!("process {} protected heap root byte overflow", pid.as_u64()))
    }
}

fn checked_owned_sum(
    mut bytes: impl Iterator<Item = usize>,
    pid: VmProcessId,
    kind: &str,
) -> Result<usize, String> {
    bytes.try_fold(0usize, |total, bytes| {
        total
            .checked_add(bytes)
            .ok_or_else(|| format!("process {} {kind} ownership byte overflow", pid.as_u64()))
    })
}
