//! Victim-owned scheduler queue claims for bounded work stealing.

use super::super::actor_directory::VmActorStealClaim;
use super::super::scheduler_topology::VmSchedulerId;
use super::super::work_stealing::{VmSchedulerWorkSnapshot, VmStealPlan};
use super::{
    VmProcessId, VmProcessState, VmProcessTable, VmScheduler, VmSchedulerClass,
    VmSchedulerQueueTransition,
};

/// Linear scheduler queue ownership removed from one victim for stealing.
#[derive(Debug)]
#[must_use = "a scheduler steal claim must be transferred or explicitly rolled back"]
pub(crate) struct VmSchedulerStealClaim {
    actor: VmActorStealClaim,
    process: VmProcessId,
    class: VmSchedulerClass,
    enqueued_tick: u64,
    wait_ticks: u64,
}

/// Linear bounded group of claims detached by one victim owner command.
#[derive(Debug)]
#[must_use = "a scheduler steal batch must be imported or explicitly rolled back"]
pub(crate) struct VmSchedulerStealBatch {
    claims: Vec<VmSchedulerStealClaim>,
}

impl VmSchedulerStealBatch {
    /// Returns the number of actors still controlled by this batch.
    pub(crate) fn len(&self) -> usize {
        self.claims.len()
    }

    /// Returns whether this batch owns no actor claims.
    pub(crate) fn is_empty(&self) -> bool {
        self.claims.is_empty()
    }
}

impl VmSchedulerStealClaim {
    /// Returns the exact process removed from the victim queue.
    pub(crate) const fn process_id(&self) -> VmProcessId {
        self.process
    }

    /// Returns the scheduling class retained for destination publication.
    pub(crate) const fn class(&self) -> VmSchedulerClass {
        self.class
    }

    /// Returns the original enqueue tick retained across rollback or transfer.
    pub(crate) const fn enqueued_tick(&self) -> u64 {
        self.enqueued_tick
    }

    /// Returns wait already accumulated before victim detachment.
    pub(crate) const fn wait_ticks(&self) -> u64 {
        self.wait_ticks
    }
}

/// Destination rejection retaining the exact linear claim for victim rollback.
#[derive(Debug)]
pub(crate) struct VmSchedulerStealImportFailure {
    reason: String,
    pending: VmSchedulerStealBatch,
    imported: usize,
}

impl VmSchedulerStealImportFailure {
    /// Returns the stable destination rejection.
    pub(crate) fn reason(&self) -> &str {
        &self.reason
    }

    /// Returns the number of claims already published before rejection.
    pub(crate) fn imported(&self) -> usize {
        self.imported
    }

    /// Returns every unconsumed claim for exact source restoration.
    pub(crate) fn into_pending(self) -> VmSchedulerStealBatch {
        self.pending
    }
}

impl VmScheduler {
    /// Publishes one live queue snapshot under this scheduler's owner identity.
    pub(crate) fn work_snapshot(
        &self,
        scheduler: VmSchedulerId,
        accepting: bool,
    ) -> Result<VmSchedulerWorkSnapshot, String> {
        if self.owner != scheduler.owner_word() {
            return Err(format!(
                "error[vm.work_stealing.snapshot_owner]: scheduler {} does not own this queue",
                scheduler.index()
            ));
        }
        let runnable = std::array::from_fn(|index| self.queues[index].len());
        let oldest_wait_ticks = std::array::from_fn(|index| {
            self.queues[index]
                .iter()
                .filter_map(|process| self.enqueued_at.get(process))
                .map(|enqueued| self.tick.saturating_sub(*enqueued))
                .max()
                .unwrap_or(0)
        });
        let snapshot = VmSchedulerWorkSnapshot::new(scheduler, runnable, oldest_wait_ticks);
        Ok(if accepting {
            snapshot
        } else {
            snapshot.stopped()
        })
    }

    /// Claims the newest eligible actor from one class on the victim owner.
    pub(crate) fn claim_stealable_process(
        &mut self,
        processes: &VmProcessTable,
        class: VmSchedulerClass,
    ) -> Result<Option<VmSchedulerStealClaim>, String> {
        let Some(process) = self.queues[class.queue_index()].back().copied() else {
            return Ok(None);
        };
        if !self.queued.contains(&process) || self.classes.get(&process) != Some(&class) {
            return Err(format!(
                "error[vm.work_stealing.queue]: process {} has inconsistent victim membership",
                process.as_u64()
            ));
        }
        if processes.get(process).map(|process| &process.state) != Some(&VmProcessState::Runnable) {
            return Err(format!(
                "error[vm.work_stealing.state]: process {} is not runnable",
                process.as_u64()
            ));
        }
        let actor = processes.claim_actor_for_steal(process)?;
        let removed = self.queues[class.queue_index()]
            .pop_back()
            .expect("validated victim queue remains nonempty");
        debug_assert_eq!(removed, process);
        self.queued.remove(&process);
        let enqueued_tick = self.enqueued_at.remove(&process).unwrap_or(self.tick);
        self.classes.remove(&process);
        self.metrics
            .queue_transitions
            .push(VmSchedulerQueueTransition {
                tick: self.tick,
                pid: process.as_u64(),
                action: "steal-claim",
                class,
                queue_len: self.queued_len(),
            });
        Ok(Some(VmSchedulerStealClaim {
            actor,
            process,
            class,
            enqueued_tick,
            wait_ticks: self.tick.saturating_sub(enqueued_tick),
        }))
    }

    /// Restores a rejected claim to the exact victim queue tail and wait age.
    pub(crate) fn abort_steal_claim(
        &mut self,
        processes: &VmProcessTable,
        claim: VmSchedulerStealClaim,
    ) -> Result<(), String> {
        if self.queued.contains(&claim.process) || self.classes.contains_key(&claim.process) {
            return Err(format!(
                "error[vm.work_stealing.rollback]: process {} victim placement changed",
                claim.process.as_u64()
            ));
        }
        processes.abort_actor_steal(claim.actor)?;
        self.classes.insert(claim.process, claim.class);
        self.queued.insert(claim.process);
        self.queues[claim.class.queue_index()].push_back(claim.process);
        self.enqueued_at.insert(claim.process, claim.enqueued_tick);
        self.metrics
            .queue_transitions
            .push(VmSchedulerQueueTransition {
                tick: self.tick,
                pid: claim.process.as_u64(),
                action: "steal-abort",
                class: claim.class,
                queue_len: self.queued_len(),
            });
        Ok(())
    }

    /// Claims at most `maximum` actors from one exact scheduling class.
    pub(crate) fn claim_stealable_batch(
        &mut self,
        processes: &VmProcessTable,
        class: VmSchedulerClass,
        maximum: usize,
    ) -> Result<VmSchedulerStealBatch, String> {
        if maximum == 0 {
            return Err("error[vm.work_stealing.batch]: claim bound is zero".to_string());
        }
        let mut claims = Vec::with_capacity(maximum.min(self.queued_len()));
        while claims.len() < maximum {
            match self.claim_stealable_process(processes, class) {
                Ok(Some(claim)) => claims.push(claim),
                Ok(None) => break,
                Err(reason) => {
                    let rollback =
                        self.abort_steal_batch(processes, VmSchedulerStealBatch { claims });
                    return Err(match rollback {
                        Ok(()) => reason,
                        Err(rollback) => format!("{reason}; {rollback}"),
                    });
                }
            }
        }
        Ok(VmSchedulerStealBatch { claims })
    }

    /// Restores a detached batch in reverse claim order to reconstruct its tail.
    pub(crate) fn abort_steal_batch(
        &mut self,
        processes: &VmProcessTable,
        mut batch: VmSchedulerStealBatch,
    ) -> Result<(), String> {
        while let Some(claim) = batch.claims.pop() {
            self.abort_steal_claim(processes, claim)?;
        }
        Ok(())
    }

    /// Publishes one victim claim into this destination's runnable queue.
    pub(crate) fn complete_steal_claim(
        &mut self,
        processes: &VmProcessTable,
        claim: VmSchedulerStealClaim,
    ) -> Result<(), VmSchedulerStealImportFailure> {
        let reject = |reason, claim| VmSchedulerStealImportFailure {
            reason,
            pending: VmSchedulerStealBatch {
                claims: vec![claim],
            },
            imported: 0,
        };
        if self.queued.contains(&claim.process) || self.classes.contains_key(&claim.process) {
            return Err(reject(
                format!(
                    "error[vm.work_stealing.destination]: process {} already has placement",
                    claim.process.as_u64()
                ),
                claim,
            ));
        }
        if processes.get(claim.process).map(|process| &process.state)
            != Some(&VmProcessState::Runnable)
        {
            return Err(reject(
                format!(
                    "error[vm.work_stealing.destination]: process {} is not runnable",
                    claim.process.as_u64()
                ),
                claim,
            ));
        }
        let VmSchedulerStealClaim {
            actor,
            process,
            class,
            enqueued_tick,
            wait_ticks,
        } = claim;
        if let Err((reason, actor)) = processes.complete_actor_steal(actor) {
            return Err(reject(
                reason,
                VmSchedulerStealClaim {
                    actor,
                    process,
                    class,
                    enqueued_tick,
                    wait_ticks,
                },
            ));
        }
        let enqueued_tick = self.tick.saturating_sub(wait_ticks);
        self.classes.insert(process, class);
        self.queued.insert(process);
        self.queues[class.queue_index()].push_back(process);
        self.enqueued_at.insert(process, enqueued_tick);
        self.metrics
            .queue_transitions
            .push(VmSchedulerQueueTransition {
                tick: self.tick,
                pid: process.as_u64(),
                action: "steal-import",
                class,
                queue_len: self.queued_len(),
            });
        Ok(())
    }

    /// Imports a bounded batch or returns every claim not yet published.
    pub(crate) fn complete_steal_batch(
        &mut self,
        processes: &VmProcessTable,
        batch: VmSchedulerStealBatch,
    ) -> Result<usize, VmSchedulerStealImportFailure> {
        let mut claims = batch.claims.into_iter();
        let mut imported = 0;
        while let Some(claim) = claims.next() {
            if let Err(failure) = self.complete_steal_claim(processes, claim) {
                let reason = failure.reason;
                let mut pending = failure.pending.claims;
                pending.extend(claims);
                return Err(VmSchedulerStealImportFailure {
                    reason,
                    pending: VmSchedulerStealBatch { claims: pending },
                    imported,
                });
            }
            imported += 1;
        }
        Ok(imported)
    }
}

/// Executes one bounded policy plan through victim and destination queue owners.
pub(crate) fn transfer_steal_batch(
    victim_id: VmSchedulerId,
    victim: &mut VmScheduler,
    thief_id: VmSchedulerId,
    thief: &mut VmScheduler,
    processes: &VmProcessTable,
    plan: VmStealPlan,
) -> Result<usize, String> {
    if plan.victim() != victim_id || plan.thief() != thief_id {
        return Err(
            "error[vm.work_stealing.plan]: scheduler identities do not match owners".to_string(),
        );
    }
    if victim.owner != victim_id.owner_word() || thief.owner != thief_id.owner_word() {
        return Err(
            "error[vm.work_stealing.owner]: scheduler mutator owner does not match plan"
                .to_string(),
        );
    }
    let batch = victim.claim_stealable_batch(processes, plan.class(), plan.maximum_actors())?;
    match thief.complete_steal_batch(processes, batch) {
        Ok(transferred) => Ok(transferred),
        Err(failure) => {
            let reason = failure.reason().to_string();
            victim
                .abort_steal_batch(processes, failure.into_pending())
                .map_err(|rollback| format!("{reason}; {rollback}"))?;
            Err(reason)
        }
    }
}
