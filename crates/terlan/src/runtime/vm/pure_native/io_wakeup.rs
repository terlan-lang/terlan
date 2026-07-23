//! Typed external wakeups for parked direct-native continuations.

use crate::runtime::native_image::control::TvmTransitionOperation;
use crate::runtime::native_image::TvmBoundaryType;
use crate::runtime::vm::execution_shard_protocol::{VmExecutionShardId, VmShardEpoch};
use crate::runtime::vm::process::VmProcessId;
use crate::runtime::vm::ReplValue;

use super::PureNativeSuspension;

/// Stable authority requested by one native typed-I/O suspension.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PureNativeIoWait {
    /// Execution shard that exclusively owns the continuation tables.
    shard: VmExecutionShardId,
    /// Exact admitted shard generation that parked the continuation.
    epoch: VmShardEpoch,
    /// Actor that owns the parked generated call.
    owner: VmProcessId,
    /// Shard-local native request identity.
    request_id: u64,
    /// Generated continuation entry authorized to resume.
    continuation_id: u64,
    /// Exact value type accepted from the completed VM I/O operation.
    boundary_type: TvmBoundaryType,
}

impl PureNativeIoWait {
    /// Derives an external wait token from one typed receive suspension.
    pub(crate) fn from_suspension(
        shard: VmExecutionShardId,
        epoch: VmShardEpoch,
        owner: VmProcessId,
        suspension: &PureNativeSuspension,
    ) -> Result<Self, String> {
        if suspension.owner_id() != owner.as_u64() {
            return Err(format!(
                "error[pure_native_io.owner]: actor {} cannot own suspension for actor {}",
                owner.as_u64(),
                suspension.owner_id()
            ));
        }
        if suspension.operation() != TvmTransitionOperation::Receive {
            return Err(format!(
                "error[pure_native_io.operation]: external wake requires typed Receive, found {:?}",
                suspension.operation()
            ));
        }
        let boundary_type = TvmBoundaryType::from_transition_words(suspension.arguments())
            .map_err(|error| format!("error[pure_native_io.type]: {error}"))?;
        Ok(Self {
            shard,
            epoch,
            owner,
            request_id: suspension.request_id(),
            continuation_id: suspension.continuation_id(),
            boundary_type,
        })
    }

    /// Returns the runtime value type accepted by this wait.
    pub(crate) fn boundary_type(&self) -> &TvmBoundaryType {
        &self.boundary_type
    }

    /// Returns the exact admitted generation authorized to accept completion.
    #[cfg(test)]
    pub(crate) const fn epoch(&self) -> VmShardEpoch {
        self.epoch
    }

    /// Creates one owned completion for this exact wait.
    pub(crate) fn wake(&self, value: ReplValue) -> PureNativeIoWake {
        PureNativeIoWake {
            wait: self.clone(),
            value,
        }
    }

    /// Rebinds the same continuation authority after an admitted shard migration.
    #[allow(dead_code)] // Called by the hidden MC-5 explicit migration surface.
    pub(crate) fn migrated_to(&self, shard: VmExecutionShardId, epoch: VmShardEpoch) -> Self {
        Self {
            shard,
            epoch,
            owner: self.owner,
            request_id: self.request_id,
            continuation_id: self.continuation_id,
            boundary_type: self.boundary_type.clone(),
        }
    }

    /// Validates that this token still names the supplied suspension.
    pub(super) fn validate_suspension(
        &self,
        shard: &VmExecutionShardId,
        epoch: VmShardEpoch,
        owner: VmProcessId,
        suspension: &PureNativeSuspension,
    ) -> Result<(), String> {
        let expected = Self::from_suspension(shard.clone(), epoch, owner, suspension)?;
        if self != &expected {
            return Err(format!(
                "error[pure_native_io.identity]: wake {}/epoch-{}/{}/{}/{} does not own suspension {}/epoch-{}/{}/{}/{}",
                self.shard.as_str(),
                self.epoch.as_u64(),
                self.owner.as_u64(),
                self.request_id,
                self.continuation_id,
                expected.shard.as_str(),
                expected.epoch.as_u64(),
                expected.owner.as_u64(),
                expected.request_id,
                expected.continuation_id
            ));
        }
        Ok(())
    }
}

/// Owned typed value delivered after one VM I/O operation completes.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PureNativeIoWake {
    /// Exact parked wait completed by this wake.
    wait: PureNativeIoWait,
    /// Owned public value injected into generated continuation parameters.
    value: ReplValue,
}

impl PureNativeIoWake {
    /// Returns the exact parked wait completed by this wake.
    pub(crate) fn wait(&self) -> &PureNativeIoWait {
        &self.wait
    }

    /// Borrows the typed public value supplied by the I/O service.
    pub(crate) fn value(&self) -> &ReplValue {
        &self.value
    }
}
