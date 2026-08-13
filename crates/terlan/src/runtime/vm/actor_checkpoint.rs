use super::*;

impl VmActorRuntime {
    /// Restores ordered mailbox checkpoint values without partial pressure writes.
    #[cfg(test)]
    pub(crate) fn restore_mailbox_checkpoint(
        &mut self,
        recipient: VmProcessId,
        payloads: Vec<ReplValue>,
    ) -> Result<Vec<u64>, String> {
        let restored =
            self.memory
                .restore_mailbox_checkpoint(&mut self.processes, recipient, payloads)?;
        self.scheduler
            .charge_memory_reductions(
                &mut self.processes,
                recipient,
                restored.pressure.requested_bytes,
            )
            .expect("validated checkpoint recipient remains live while charging reductions");
        if restored.pressure.outcome == VmMemoryPressureOutcome::HardLimitRejected {
            return Err(format!(
                "actor process {} checkpoint exceeds its VM mailbox memory hard limit",
                recipient.as_u64()
            ));
        }
        if !restored.message_ids.is_empty() {
            self.scheduler
                .wake_process(&mut self.processes, recipient)
                .expect("checkpoint recipient was validated before wake");
        }
        self.charge_actor_reductions(recipient, ACTOR_OPERATION_REDUCTIONS);
        Ok(restored.message_ids)
    }
}
