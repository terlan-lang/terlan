//! Linear transfer of active timers owned by one migrating actor.

use std::fmt;

use super::{VmProcessId, VmTimer, VmTimerId, VmTimerTable};

/// Exact active timers and clock position detached for one actor owner.
#[derive(Debug)]
pub(crate) struct VmTimerTransfer {
    owner: VmProcessId,
    timers: Vec<VmTimer>,
    identity_watermark: u64,
    observed_tick: Option<u64>,
}

impl VmTimerTransfer {
    /// Returns the actor that owns every transferred timer.
    pub(crate) const fn owner(&self) -> VmProcessId {
        self.owner
    }

    /// Returns exact timer identities used to move delayed payloads with them.
    pub(crate) fn timer_ids(&self) -> impl Iterator<Item = VmTimerId> + '_ {
        self.timers.iter().map(|timer| timer.id)
    }

    /// Returns the number of active timers retained by this transfer.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.timers.len()
    }
}

/// Failed timer import retaining exact deadlines for source rollback.
#[derive(Debug)]
pub(crate) struct VmTimerImportFailure {
    reason: String,
    transfer: VmTimerTransfer,
}

impl VmTimerImportFailure {
    /// Returns the stable destination rejection.
    pub(crate) fn reason(&self) -> &str {
        &self.reason
    }

    /// Returns every timer and its clock position for source restoration.
    pub(crate) fn into_transfer(self) -> VmTimerTransfer {
        self.transfer
    }
}

impl fmt::Display for VmTimerImportFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.reason)
    }
}

impl std::error::Error for VmTimerImportFailure {}

impl VmTimerTable {
    /// Detaches every active timer owned by one unchanged actor identity.
    pub(crate) fn detach_owner_timer_state(&mut self, owner: VmProcessId) -> VmTimerTransfer {
        let identities = self
            .timers
            .iter()
            .filter_map(|(identity, timer)| (timer.owner == owner).then_some(*identity))
            .collect::<Vec<_>>();
        let timers = identities
            .into_iter()
            .map(|identity| {
                self.timers
                    .remove(&identity)
                    .expect("inventoried owner timer remains present")
            })
            .collect();
        VmTimerTransfer {
            owner,
            timers,
            identity_watermark: self.next_timer_id,
            observed_tick: self.last_clock_tick,
        }
    }

    /// Validates owner, identity, and absolute clock admission before mutation.
    pub(crate) fn validate_timer_import(&self, transfer: &VmTimerTransfer) -> Result<(), String> {
        if transfer.owner.as_u64() == 0 {
            return Err("timer transfer owner identity must be nonzero".to_string());
        }
        if !transfer.timers.is_empty() {
            let source_tick = transfer.observed_tick.unwrap_or(0);
            if let Some(destination_tick) = self.last_clock_tick {
                if source_tick != destination_tick {
                    return Err(format!(
                        "timer transfer clock mismatch: source {source_tick}, destination {destination_tick}"
                    ));
                }
            }
        }
        for timer in &transfer.timers {
            if timer.owner != transfer.owner {
                return Err("timer transfer contains a cross-actor timer".to_string());
            }
            if self.timers.contains_key(&timer.id) {
                return Err(format!(
                    "timer transfer destination already contains timer {}",
                    timer.id.as_u64()
                ));
            }
        }
        Ok(())
    }

    /// Imports active timers or returns every deadline unchanged for rollback.
    pub(crate) fn import_timer_transfer(
        &mut self,
        transfer: VmTimerTransfer,
    ) -> Result<(), VmTimerImportFailure> {
        if let Err(reason) = self.validate_timer_import(&transfer) {
            return Err(VmTimerImportFailure { reason, transfer });
        }
        self.next_timer_id = self.next_timer_id.max(transfer.identity_watermark);
        if !transfer.timers.is_empty() && self.last_clock_tick.is_none() {
            self.last_clock_tick = transfer.observed_tick;
        }
        for timer in transfer.timers {
            self.timers.insert(timer.id, timer);
        }
        self.metrics.max_active = self.metrics.max_active.max(self.timers.len());
        Ok(())
    }
}
