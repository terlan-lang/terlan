//! Complete shard-local transfer of one parked generated actor.

use std::fmt;

use crate::runtime::vm::actor::{VmActorRuntimeImportFailure, VmActorRuntimeTransfer};
use crate::runtime::vm::process::VmProcessId;

use super::super::execution_runtime::{
    PureNativeActorExecutionImportFailure, PureNativeActorExecutionTransfer,
};
use super::generation_lifetime::PureNativeActorGenerationLease;
use super::PureNativeExecutionShard;

/// Complete movable state for one generated actor at a native safepoint.
#[derive(Debug)]
pub(crate) struct PureNativeActorTransfer {
    owner: VmProcessId,
    actor: VmActorRuntimeTransfer,
    execution: PureNativeActorExecutionTransfer,
    generation: PureNativeActorGenerationLease,
}

impl PureNativeActorTransfer {
    /// Returns the process authorized to import and resume this transfer.
    pub(crate) const fn owner(&self) -> VmProcessId {
        self.owner
    }

    /// Returns the exact generated continuation identity.
    pub(crate) const fn continuation_id(&self) -> u64 {
        self.execution.continuation_id()
    }

    /// Returns the source generation retained by this detached envelope.
    pub(crate) const fn source_generation(&self) -> u64 {
        self.generation.source_epoch().as_u64()
    }

    /// Returns the sealed native image required at destination admission.
    pub(crate) fn image_identity(&self) -> &str {
        self.generation.image().identity()
    }
}

/// Failed shard import retaining every component for source rollback.
#[derive(Debug)]
pub(crate) struct PureNativeActorImportFailure {
    reason: String,
    transfer: PureNativeActorTransfer,
}

impl PureNativeActorImportFailure {
    /// Creates a preflight rejection without consuming the transfer.
    pub(crate) fn rejected(reason: String, transfer: PureNativeActorTransfer) -> Self {
        Self { reason, transfer }
    }

    /// Returns the stable destination rejection.
    pub(crate) fn reason(&self) -> &str {
        &self.reason
    }

    /// Returns complete actor ownership for source restoration.
    pub(crate) fn into_transfer(self) -> PureNativeActorTransfer {
        self.transfer
    }
}

impl fmt::Display for PureNativeActorImportFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.reason)
    }
}

impl std::error::Error for PureNativeActorImportFailure {}

impl PureNativeExecutionShard {
    /// Detaches one actor only after both runtime layers publish a safepoint.
    pub(crate) fn detach_actor_state(
        &mut self,
        owner: VmProcessId,
    ) -> Result<PureNativeActorTransfer, String> {
        self.require_routable("detach_actor_state")?;
        let generation = self.generation_transfers.acquire(
            &self.boundary,
            self.boundary.sealed_image()?,
            self.generation()?,
        )?;
        let actor = self.actors.detach_actor_runtime(owner)?;
        let execution = match self.execution.detach_actor_execution(owner.as_u64()) {
            Ok(execution) => execution,
            Err(error) => {
                self.actors
                    .import_actor_runtime(actor)
                    .expect("source actor runtime can roll back before execution detaches");
                return Err(error);
            }
        };
        let actor_identity = actor.native_continuation();
        let execution_identity = (execution.request_id(), execution.continuation_id());
        if actor_identity != execution_identity {
            self.execution
                .import_actor_execution(execution)
                .expect("source execution state can roll back an identity mismatch");
            self.actors
                .import_actor_runtime(actor)
                .expect("source actor state can roll back an identity mismatch");
            return Err(format!(
                "error[execution_shard.transfer_identity]: actor continuation {actor_identity:?} does not match execution continuation {execution_identity:?}"
            ));
        }
        Ok(PureNativeActorTransfer {
            owner,
            actor,
            execution,
            generation,
        })
    }

    /// Imports one generated actor or returns it intact for source rollback.
    pub(crate) fn import_actor_state(
        &mut self,
        transfer: PureNativeActorTransfer,
    ) -> Result<(), PureNativeActorImportFailure> {
        if let Err(reason) = self.validate_actor_state_import(&transfer) {
            return Err(PureNativeActorImportFailure { reason, transfer });
        }
        let PureNativeActorTransfer {
            owner,
            actor,
            execution,
            generation,
        } = transfer;
        if let Err(failure) = self.actors.import_actor_runtime(actor) {
            return Err(actor_import_failure(owner, failure, execution, generation));
        }
        if let Err(failure) = self.execution.import_actor_execution(execution) {
            let actor = self
                .actors
                .detach_actor_runtime(owner)
                .expect("just-imported actor runtime can detach for rollback");
            return Err(execution_import_failure(owner, actor, failure, generation));
        }
        Ok(())
    }

    /// Validates both destination tables before either consumes the envelope.
    fn validate_actor_state_import(
        &self,
        transfer: &PureNativeActorTransfer,
    ) -> Result<(), String> {
        if transfer.owner != transfer.actor.owner()
            || transfer.owner.as_u64() != transfer.execution.owner_id()
        {
            return Err(
                "error[execution_shard.transfer_owner]: component owner mismatch".to_string(),
            );
        }
        let actor_identity = transfer.actor.native_continuation();
        let execution_identity = (
            transfer.execution.request_id(),
            transfer.execution.continuation_id(),
        );
        if actor_identity != execution_identity {
            return Err(
                "error[execution_shard.transfer_identity]: continuation mismatch".to_string(),
            );
        }
        let destination_epoch = self.require_routable("import_actor_state")?;
        let destination_image = self.supervisor.image().ok_or_else(|| {
            "error[execution_shard.transfer_generation]: destination image is unavailable"
                .to_string()
        })?;
        if destination_image != transfer.generation.image() {
            return Err(format!(
                "error[execution_shard.transfer_generation]: source_epoch={} source_image={} destination_epoch={} destination_image={}",
                transfer.generation.source_epoch().as_u64(),
                transfer.generation.image().identity(),
                destination_epoch.as_u64(),
                destination_image.identity()
            ));
        }
        self.actors.validate_actor_runtime_import(&transfer.actor)?;
        self.execution
            .validate_actor_execution_import(&transfer.execution)
    }
}

/// Rebuilds a complete transfer from an actor-runtime admission rejection.
fn actor_import_failure(
    owner: VmProcessId,
    failure: VmActorRuntimeImportFailure,
    execution: PureNativeActorExecutionTransfer,
    generation: PureNativeActorGenerationLease,
) -> PureNativeActorImportFailure {
    PureNativeActorImportFailure {
        reason: failure.reason().to_string(),
        transfer: PureNativeActorTransfer {
            owner,
            actor: failure.into_transfer(),
            execution,
            generation,
        },
    }
}

/// Rebuilds a complete transfer from an execution-runtime admission rejection.
fn execution_import_failure(
    owner: VmProcessId,
    actor: VmActorRuntimeTransfer,
    failure: PureNativeActorExecutionImportFailure,
    generation: PureNativeActorGenerationLease,
) -> PureNativeActorImportFailure {
    PureNativeActorImportFailure {
        reason: failure.reason().to_string(),
        transfer: PureNativeActorTransfer {
            owner,
            actor,
            execution: failure.into_transfer(),
            generation,
        },
    }
}
