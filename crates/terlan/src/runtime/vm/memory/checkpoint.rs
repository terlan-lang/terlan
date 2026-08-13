use super::VmMemoryAccountant;
#[cfg(test)]
use super::{logical_value_bytes, VmMemoryPressureDecision, VmMemoryPressureOutcome};
#[cfg(test)]
use crate::runtime::vm::{
    process::{VmProcessId, VmProcessTable},
    ReplValue,
};

/// Result of restoring an ordered mailbox checkpoint under one heap reservation.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmAccountedMailboxRestore {
    pub(crate) message_ids: Vec<u64>,
    pub(crate) pressure: VmMemoryPressureDecision,
}

impl VmMemoryAccountant {
    /// Restores checkpointed mailbox values atomically under VM memory pressure.
    #[cfg(test)]
    pub(crate) fn restore_mailbox_checkpoint(
        &mut self,
        processes: &mut VmProcessTable,
        recipient: VmProcessId,
        payloads: Vec<ReplValue>,
    ) -> Result<VmAccountedMailboxRestore, String> {
        processes.validate_send(recipient, recipient)?;
        let measured = payloads
            .into_iter()
            .map(|payload| {
                logical_value_bytes(&payload)
                    .map(|logical_bytes| (payload, logical_bytes))
                    .map_err(|error| error.to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        let total_bytes = measured.iter().try_fold(0usize, |total, (_, bytes)| {
            total.checked_add(*bytes).ok_or_else(|| {
                "error[vm_memory_checkpoint_size_overflow]: mailbox checkpoint exceeds usize"
                    .to_string()
            })
        })?;
        let pressure = self.account_heap(processes, recipient, total_bytes)?;
        if pressure.outcome == VmMemoryPressureOutcome::HardLimitRejected {
            return Ok(VmAccountedMailboxRestore {
                message_ids: Vec::new(),
                pressure,
            });
        }
        let mut message_ids = Vec::with_capacity(measured.len());
        for (payload, logical_bytes) in measured {
            message_ids.push(processes.send_accounted(
                recipient,
                recipient,
                payload,
                logical_bytes,
            )?);
        }
        Ok(VmAccountedMailboxRestore {
            message_ids,
            pressure,
        })
    }
}
