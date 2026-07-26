//! Bounded admission and generation-safe replacement for capability workers.

use std::collections::BTreeSet;
use std::task::Waker;

use super::{
    VmCapabilityId, VmCapabilityRequestContext, VmCapabilityWorkerClient,
    VmCapabilityWorkerCompletion, VmCapabilityWorkerIdentity, VmCapabilityWorkerRuntime,
    VmCapabilityWorkerTerminal,
};
use crate::runtime::vm::native_boundary::deadline::VmScheduledNativeBoundaryRequest;
use crate::runtime::vm::process::VmProcessId;
use crate::terlan_native_boundary::term::NativeBoundaryTerm;
use crate::terlan_native_boundary::request::RequestId;

/// One configured logical worker slot and its bounded in-flight capacity.
pub(crate) struct VmCapabilityWorkerPoolSlot {
    /// Stable logical worker identity retained across process replacement.
    id: super::VmCapabilityWorkerId,
    /// Last process generation admitted for this logical slot.
    generation: super::VmCapabilityWorkerGeneration,
    /// Maximum requests admitted concurrently through this slot.
    concurrency_limit: u64,
    /// Current worker process, absent after failure or orderly shutdown.
    client: Option<VmCapabilityWorkerClient>,
}

impl VmCapabilityWorkerPoolSlot {
    /// Admits one worker client with an explicit positive concurrency bound.
    pub(crate) fn new(
        client: VmCapabilityWorkerClient,
        concurrency_limit: u64,
    ) -> Result<Self, String> {
        if concurrency_limit == 0 {
            return Err(
                "error[capability_worker.pool_capacity]: slot capacity must be positive"
                    .to_string(),
            );
        }
        if concurrency_limit > client.credit_limit() {
            return Err(format!(
                "error[capability_worker.pool_capacity]: slot capacity {concurrency_limit} exceeds worker credit limit {}",
                client.credit_limit()
            ));
        }
        Ok(Self {
            id: client.identity().id.clone(),
            generation: client.identity().generation,
            concurrency_limit,
            client: Some(client),
        })
    }

    /// Returns the stable logical identity of this slot.
    pub(crate) fn id(&self) -> &super::VmCapabilityWorkerId {
        &self.id
    }

    /// Returns the currently admitted process generation.
    pub(crate) const fn generation(&self) -> super::VmCapabilityWorkerGeneration {
        self.generation
    }

    /// Returns the configured in-flight request capacity.
    pub(crate) const fn concurrency_limit(&self) -> u64 {
        self.concurrency_limit
    }

    /// Returns capacity currently available without parking another actor.
    pub(crate) fn available_capacity(&self) -> u64 {
        self.client
            .as_ref()
            .map(|client| {
                self.concurrency_limit
                    .saturating_sub(client.pending_len() as u64)
            })
            .unwrap_or(0)
    }

    /// Returns whether a live worker process currently occupies this slot.
    pub(crate) const fn is_live(&self) -> bool {
        self.client.is_some()
    }

    /// Installs only the next generation of this exact logical worker slot.
    pub(crate) fn replace(&mut self, client: VmCapabilityWorkerClient) -> Result<(), String> {
        if self.client.is_some() {
            return Err(format!(
                "error[capability_worker.pool_replacement]: worker `{}` is still live",
                self.id.as_str()
            ));
        }
        if client.identity().id != self.id {
            return Err(format!(
                "error[capability_worker.pool_replacement]: expected worker `{}`, received `{}`",
                self.id.as_str(),
                client.identity().id.as_str()
            ));
        }
        let expected = self.generation.as_u64().checked_add(1).ok_or_else(|| {
            "error[capability_worker.pool_replacement]: worker generation exhausted".to_string()
        })?;
        if client.identity().generation.as_u64() != expected {
            return Err(format!(
                "error[capability_worker.pool_replacement]: worker `{}` expected generation {expected}, received {}",
                self.id.as_str(),
                client.identity().generation.as_u64()
            ));
        }
        if self.concurrency_limit > client.credit_limit() {
            return Err(format!(
                "error[capability_worker.pool_capacity]: slot capacity {} exceeds replacement credit limit {}",
                self.concurrency_limit,
                client.credit_limit()
            ));
        }
        self.generation = client.identity().generation;
        self.client = Some(client);
        Ok(())
    }

}

/// Exact pool assignment retained by a caller until completion or cancellation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmCapabilityWorkerPoolRequest {
    /// Worker process generation that accepted the request.
    pub(crate) worker: VmCapabilityWorkerIdentity,
    /// VM deadline and actor ownership created before transport publication.
    pub(crate) scheduled: VmScheduledNativeBoundaryRequest,
}

/// Exact worker assignment for a continuation already parked by its shard.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmCapabilityWorkerParkedRequest {
    /// Worker process generation that accepted the request.
    pub(crate) worker: VmCapabilityWorkerIdentity,
    /// Worker-local request identity used for completion correlation.
    pub(crate) request_id: RequestId,
    /// Actor whose generated continuation remains parked.
    pub(crate) owner: VmProcessId,
}

/// VM-owned pool selecting among bounded external capability-worker slots.
pub(crate) struct VmCapabilityWorkerPool {
    /// Stable configured slots; a failed process leaves its slot vacant.
    slots: Vec<VmCapabilityWorkerPoolSlot>,
    /// Round-robin cursor used only among currently eligible slots.
    next_slot: usize,
}

impl VmCapabilityWorkerPool {
    /// Creates a non-empty pool with unique logical worker identities.
    pub(crate) fn new(slots: Vec<VmCapabilityWorkerPoolSlot>) -> Result<Self, String> {
        if slots.is_empty() {
            return Err(
                "error[capability_worker.pool_capacity]: pool must contain a worker slot"
                    .to_string(),
            );
        }
        let mut identities = BTreeSet::new();
        for slot in &slots {
            if !identities.insert(slot.id.as_str().to_string()) {
                return Err(format!(
                    "error[capability_worker.pool_identity]: duplicate worker slot `{}`",
                    slot.id.as_str()
                ));
            }
        }
        Ok(Self {
            slots,
            next_slot: 0,
        })
    }

    /// Returns total configured capacity without counting failed slots twice.
    pub(crate) fn configured_capacity(&self) -> u64 {
        self.slots
            .iter()
            .map(VmCapabilityWorkerPoolSlot::concurrency_limit)
            .sum()
    }

    /// Returns currently available request credits across live worker slots.
    pub(crate) fn available_capacity(&self) -> u64 {
        self.slots
            .iter()
            .map(VmCapabilityWorkerPoolSlot::available_capacity)
            .sum()
    }

    /// Registers one task with every live worker transport.
    pub(crate) fn register_event_waker(&self, waker: &Waker) {
        for client in self.slots.iter().filter_map(|slot| slot.client.as_ref()) {
            client.register_event_waker(waker);
        }
    }

    /// Returns the number of current worker processes, excluding failed slots.
    pub(crate) fn live_workers(&self) -> usize {
        self.slots.iter().filter(|slot| slot.is_live()).count()
    }

    /// Requests orderly shutdown from every currently live worker process.
    pub(crate) fn shutdown_all(&self) -> Vec<String> {
        self.slots
            .iter()
            .filter_map(|slot| slot.client.as_ref())
            .filter_map(|client| client.shutdown().err())
            .collect()
    }

    /// Starts one call on an admitted slot without blocking the scheduler.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn start_call(
        &mut self,
        runtime: &mut VmCapabilityWorkerRuntime<'_>,
        owner: VmProcessId,
        context: VmCapabilityRequestContext,
        operation: impl Into<String>,
        arguments: Vec<NativeBoundaryTerm>,
        now_tick: u64,
        timeout_ticks: u64,
    ) -> Result<VmCapabilityWorkerPoolRequest, String> {
        let index = self.select_slot(&context.capability)?;
        let client = self.slots[index]
            .client
            .as_mut()
            .expect("slot selection admits only live clients");
        let worker = client.identity().clone();
        let scheduled = client.start_call(
            runtime,
            owner,
            context,
            operation,
            arguments,
            now_tick,
            timeout_ticks,
        )?;
        self.next_slot = (index + 1) % self.slots.len();
        Ok(VmCapabilityWorkerPoolRequest { worker, scheduled })
    }

    /// Starts one call whose generated continuation is already shard-parked.
    pub(crate) fn start_parked_call(
        &mut self,
        owner: VmProcessId,
        context: VmCapabilityRequestContext,
        operation: impl Into<String>,
        arguments: Vec<NativeBoundaryTerm>,
    ) -> Result<VmCapabilityWorkerParkedRequest, String> {
        let index = self.select_slot(&context.capability)?;
        let client = self.slots[index]
            .client
            .as_mut()
            .expect("slot selection admits only live clients");
        let worker = client.identity().clone();
        let request_id = client.start_parked_call(owner, context, operation, arguments)?;
        self.next_slot = (index + 1) % self.slots.len();
        Ok(VmCapabilityWorkerParkedRequest {
            worker,
            request_id,
            owner,
        })
    }

    /// Polls each live slot for an already-parked generated completion.
    pub(crate) fn poll_parked(
        &mut self,
    ) -> Result<Option<VmCapabilityWorkerCompletion>, String> {
        for offset in 0..self.slots.len() {
            let index = (self.next_slot + offset) % self.slots.len();
            let Some(client) = self.slots[index].client.as_mut() else {
                continue;
            };
            let Some(completion) = client.poll_parked()? else {
                continue;
            };
            if matches!(
                completion,
                VmCapabilityWorkerCompletion::TransportClosed { .. }
                    | VmCapabilityWorkerCompletion::TransportFailed { .. }
                    | VmCapabilityWorkerCompletion::ShutdownAcknowledged { .. }
            ) {
                self.slots[index].client = None;
            }
            self.next_slot = (index + 1) % self.slots.len();
            return Ok(Some(completion));
        }
        Ok(None)
    }

    /// Polls each live slot once and returns the first available event.
    pub(crate) fn poll(
        &mut self,
        runtime: &mut VmCapabilityWorkerRuntime<'_>,
    ) -> Result<Option<VmCapabilityWorkerCompletion>, String> {
        for offset in 0..self.slots.len() {
            let index = (self.next_slot + offset) % self.slots.len();
            let Some(client) = self.slots[index].client.as_mut() else {
                continue;
            };
            let Some(completion) = client.poll(runtime)? else {
                continue;
            };
            if matches!(
                completion,
                VmCapabilityWorkerCompletion::TransportClosed { .. }
                    | VmCapabilityWorkerCompletion::TransportFailed { .. }
                    | VmCapabilityWorkerCompletion::ShutdownAcknowledged { .. }
            ) {
                self.slots[index].client = None;
            }
            self.next_slot = (index + 1) % self.slots.len();
            return Ok(Some(completion));
        }
        Ok(None)
    }

    /// Cancels only the exact worker generation that accepted a request.
    pub(crate) fn cancel(
        &mut self,
        runtime: &mut VmCapabilityWorkerRuntime<'_>,
        request: &VmCapabilityWorkerPoolRequest,
    ) -> Result<VmCapabilityWorkerTerminal, String> {
        let client = self.exact_client_mut(&request.worker)?;
        client.cancel(runtime, request.scheduled.timer_id)
    }

    /// Cancels only the exact worker generation holding an already-parked request.
    pub(crate) fn cancel_parked(
        &mut self,
        request: &VmCapabilityWorkerParkedRequest,
    ) -> Result<(), String> {
        let client = self.exact_client_mut(&request.worker)?;
        client.cancel_parked(request.owner, request.request_id)
    }

    /// Installs the immediate next process generation into one vacant slot.
    pub(crate) fn replace(&mut self, client: VmCapabilityWorkerClient) -> Result<(), String> {
        let id = client.identity().id.clone();
        let slot = self
            .slots
            .iter_mut()
            .find(|slot| slot.id == id)
            .ok_or_else(|| {
                format!(
                    "error[capability_worker.pool_replacement]: unknown worker slot `{}`",
                    id.as_str()
                )
            })?;
        slot.replace(client)
    }

    /// Selects one capability-compatible slot with an unused local credit.
    fn select_slot(&self, capability: &VmCapabilityId) -> Result<usize, String> {
        let mut admitted = false;
        for offset in 0..self.slots.len() {
            let index = (self.next_slot + offset) % self.slots.len();
            let slot = &self.slots[index];
            let Some(client) = slot.client.as_ref() else {
                continue;
            };
            if !client.admits_capability(capability) {
                continue;
            }
            admitted = true;
            if slot.available_capacity() > 0 {
                return Ok(index);
            }
        }
        if admitted {
            Err(format!(
                "error[capability_worker.pool_full]: capability `{}` has no available worker credit",
                capability.as_str()
            ))
        } else {
            Err(format!(
                "error[capability_worker.pool_capability]: no live worker admits capability `{}`",
                capability.as_str()
            ))
        }
    }

    /// Resolves a live client only when both slot and process generation match.
    fn exact_client_mut(
        &mut self,
        identity: &VmCapabilityWorkerIdentity,
    ) -> Result<&mut VmCapabilityWorkerClient, String> {
        let slot = self
            .slots
            .iter_mut()
            .find(|slot| slot.id == identity.id)
            .ok_or_else(|| {
                format!(
                    "error[capability_worker.pool_identity]: unknown worker `{}`",
                    identity.id.as_str()
                )
            })?;
        if slot.generation != identity.generation {
            return Err(format!(
                "error[capability_worker.pool_stale_generation]: worker `{}` expected generation {}, received {}",
                slot.id.as_str(),
                slot.generation.as_u64(),
                identity.generation.as_u64()
            ));
        }
        slot.client.as_mut().ok_or_else(|| {
            format!(
                "error[capability_worker.pool_unavailable]: worker `{}` generation {} is not live",
                slot.id.as_str(),
                slot.generation.as_u64()
            )
        })
    }
}

#[cfg(test)]
#[path = "pool_test.rs"]
mod pool_test;
