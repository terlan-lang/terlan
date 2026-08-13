//! Linear transfer of one actor's managed and parked-continuation state.

use std::fmt;

use crate::runtime::native_image::managed::ManagedActorTransfer;

use super::{PendingNativeContinuation, PureNativeExecutionRuntime};
use crate::runtime::vm::pure_native::PureNativeSuspension;

/// Complete execution-runtime state detached for one parked actor.
#[derive(Debug)]
pub(crate) struct PureNativeActorExecutionTransfer {
    owner_id: u64,
    continuation: PendingNativeContinuation,
    resident_suspension: Option<PureNativeSuspension>,
    managed: ManagedActorTransfer,
}

impl PureNativeActorExecutionTransfer {
    /// Returns the actor authorized to import this execution state.
    pub(crate) const fn owner_id(&self) -> u64 {
        self.owner_id
    }

    /// Returns the exact parked native request identity.
    pub(crate) const fn request_id(&self) -> u64 {
        self.continuation.request_id
    }

    /// Returns the exact compiler-generated continuation identity.
    pub(crate) const fn continuation_id(&self) -> u64 {
        self.continuation.continuation_id
    }

    /// Returns managed actor state without exposing allocator internals.
    #[cfg(test)]
    pub(crate) fn managed(&self) -> &ManagedActorTransfer {
        &self.managed
    }
}

/// Failed execution-state import that preserves rollback ownership.
#[derive(Debug)]
pub(crate) struct PureNativeActorExecutionImportFailure {
    reason: String,
    transfer: Box<PureNativeActorExecutionTransfer>,
}

impl PureNativeActorExecutionImportFailure {
    /// Returns the stable destination rejection.
    pub(crate) fn reason(&self) -> &str {
        &self.reason
    }

    /// Returns the complete state so the source runtime can restore it.
    pub(crate) fn into_transfer(self) -> PureNativeActorExecutionTransfer {
        *self.transfer
    }
}

impl fmt::Display for PureNativeActorExecutionImportFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.reason)
    }
}

impl std::error::Error for PureNativeActorExecutionImportFailure {}

impl PureNativeExecutionRuntime {
    /// Detaches one parked continuation and every managed root it can reach.
    pub(crate) fn detach_actor_execution(
        &mut self,
        owner_id: u64,
    ) -> Result<PureNativeActorExecutionTransfer, String> {
        let continuation = self.continuations.get(&owner_id).ok_or_else(|| {
            format!(
                "error[execution_shard.transfer_safepoint]: actor {owner_id} has no parked continuation"
            )
        })?;
        if continuation
            .managed
            .as_ref()
            .is_some_and(|captures| captures.owner_id() != owner_id)
        {
            return Err(
                "error[execution_shard.transfer_roots]: cross-actor continuation roots".to_string(),
            );
        }
        if continuation.completions.iter().any(|frame| {
            frame
                .managed
                .as_ref()
                .is_some_and(|captures| captures.owner_id() != owner_id)
        }) {
            return Err(
                "error[execution_shard.transfer_roots]: cross-actor completion roots".to_string(),
            );
        }
        let managed = self.managed.detach_actor(owner_id)?;
        let continuation = self
            .continuations
            .remove(&owner_id)
            .expect("validated parked continuation remains present");
        let resident_suspension = self.resident_suspensions.remove(&owner_id);
        Ok(PureNativeActorExecutionTransfer {
            owner_id,
            continuation,
            resident_suspension,
            managed,
        })
    }

    /// Verifies destination admission before either runtime is mutated further.
    pub(crate) fn validate_actor_execution_import(
        &self,
        transfer: &PureNativeActorExecutionTransfer,
    ) -> Result<(), String> {
        if transfer.owner_id == 0 {
            return Err("error[execution_shard.transfer_owner]: owner is zero".to_string());
        }
        if transfer.managed.owner_id() != transfer.owner_id {
            return Err(
                "error[execution_shard.transfer_owner]: managed owner does not match continuation"
                    .to_string(),
            );
        }
        if transfer
            .continuation
            .managed
            .as_ref()
            .is_some_and(|captures| captures.owner_id() != transfer.owner_id)
        {
            return Err(
                "error[execution_shard.transfer_roots]: cross-actor continuation roots".to_string(),
            );
        }
        if transfer.continuation.completions.iter().any(|frame| {
            frame
                .managed
                .as_ref()
                .is_some_and(|captures| captures.owner_id() != transfer.owner_id)
        }) {
            return Err(
                "error[execution_shard.transfer_roots]: cross-actor completion roots".to_string(),
            );
        }
        if self.continuations.contains_key(&transfer.owner_id) {
            return Err(format!(
                "error[execution_shard.transfer_collision]: actor {} already owns a continuation",
                transfer.owner_id
            ));
        }
        if self.resident_suspensions.contains_key(&transfer.owner_id) {
            return Err(format!(
                "error[execution_shard.transfer_collision]: actor {} already owns a resident suspension",
                transfer.owner_id
            ));
        }
        if transfer
            .resident_suspension
            .as_ref()
            .is_some_and(|suspension| {
                suspension.owner_id() != transfer.owner_id
                    || suspension.request_id() != transfer.continuation.request_id
                    || suspension.continuation_id() != transfer.continuation.continuation_id
            })
        {
            return Err(
                "error[execution_shard.transfer_owner]: resident suspension does not match continuation"
                    .to_string(),
            );
        }
        self.managed.validate_actor_import(&transfer.managed)
    }

    /// Imports preflighted execution state or returns it intact for rollback.
    pub(crate) fn import_actor_execution(
        &mut self,
        transfer: PureNativeActorExecutionTransfer,
    ) -> Result<(), PureNativeActorExecutionImportFailure> {
        if let Err(reason) = self.validate_actor_execution_import(&transfer) {
            return Err(PureNativeActorExecutionImportFailure {
                reason,
                transfer: Box::new(transfer),
            });
        }
        let PureNativeActorExecutionTransfer {
            owner_id,
            continuation,
            resident_suspension,
            managed,
        } = transfer;
        if let Err(failure) = self.managed.import_actor(managed) {
            let reason = failure.reason().to_string();
            return Err(PureNativeActorExecutionImportFailure {
                reason,
                transfer: Box::new(PureNativeActorExecutionTransfer {
                    owner_id,
                    continuation,
                    resident_suspension,
                    managed: failure.into_transfer(),
                }),
            });
        }
        self.next_request_id = self.next_request_id.max(continuation.request_id);
        self.continuations.insert(owner_id, continuation);
        if let Some(suspension) = resident_suspension {
            self.resident_suspensions.insert(owner_id, suspension);
        }
        Ok(())
    }
}
