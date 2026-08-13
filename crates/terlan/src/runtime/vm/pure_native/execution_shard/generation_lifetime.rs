//! Native generation references retained across scheduler ownership transfer.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use crate::runtime::vm::execution_shard_protocol::{VmSealedShardImage, VmShardEpoch};

use super::super::{
    PureNativeBoundary, VmNativeGenerationReferenceClass, VmNativeGenerationReferenceSnapshot,
};
use super::PureNativeExecutionShard;

/// Shared count of actor envelopes detached from one source generation.
#[derive(Debug, Default)]
pub(super) struct PureNativeGenerationTransferTracker {
    outstanding: Arc<AtomicUsize>,
}

impl PureNativeGenerationTransferTracker {
    /// Acquires one checked lease while retaining the exact executable mapping.
    pub(super) fn acquire(
        &self,
        boundary: &PureNativeBoundary,
        image: VmSealedShardImage,
        source_epoch: VmShardEpoch,
    ) -> Result<PureNativeActorGenerationLease, String> {
        let retained_boundary = boundary.fork_empty()?;
        self.outstanding
            .try_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                count.checked_add(1)
            })
            .map_err(|_| {
                "error[execution_shard.transfer_generation]: transfer reference count exhausted"
                    .to_string()
            })?;
        Ok(PureNativeActorGenerationLease {
            source_epoch,
            image,
            _retained_boundary: retained_boundary,
            outstanding: Arc::clone(&self.outstanding),
        })
    }

    /// Returns the number of transfer envelopes retaining this generation.
    pub(super) fn count(&self) -> usize {
        self.outstanding.load(Ordering::Acquire)
    }
}

/// Linear lease retaining one source image while an actor has no shard owner.
#[derive(Debug)]
pub(super) struct PureNativeActorGenerationLease {
    source_epoch: VmShardEpoch,
    image: VmSealedShardImage,
    _retained_boundary: PureNativeBoundary,
    outstanding: Arc<AtomicUsize>,
}

impl PureNativeActorGenerationLease {
    /// Returns the source shard epoch at transfer detachment.
    pub(super) const fn source_epoch(&self) -> VmShardEpoch {
        self.source_epoch
    }

    /// Returns the exact sealed image required at destination admission.
    pub(super) const fn image(&self) -> &VmSealedShardImage {
        &self.image
    }
}

impl Drop for PureNativeActorGenerationLease {
    /// Releases exactly one source-generation transfer reference.
    fn drop(&mut self) {
        let result = self
            .outstanding
            .try_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                count.checked_sub(1)
            });
        debug_assert!(
            result.is_ok(),
            "actor transfer generation lease cannot underflow"
        );
    }
}

impl PureNativeExecutionShard {
    /// Pins one runtime reference that cannot be inferred from actor tables.
    #[cfg(test)]
    pub(crate) fn pin_generation_reference(
        &mut self,
        class: VmNativeGenerationReferenceClass,
    ) -> Result<(), String> {
        let count = self.generation_pins.entry(class).or_default();
        *count = count.checked_add(1).ok_or_else(|| {
            format!(
                "error[execution_shard.generation_pin]: {} reference count overflowed",
                class.name()
            )
        })?;
        Ok(())
    }

    /// Releases one externally tracked generation reference exactly once.
    #[cfg(test)]
    pub(crate) fn release_generation_reference(
        &mut self,
        class: VmNativeGenerationReferenceClass,
    ) -> Result<(), String> {
        let Some(count) = self.generation_pins.get_mut(&class) else {
            return Err(format!(
                "error[execution_shard.generation_pin]: {} has no reference to release",
                class.name()
            ));
        };
        *count -= 1;
        if *count == 0 {
            self.generation_pins.remove(&class);
        }
        Ok(())
    }

    /// Captures every runtime reference that prevents generation unload.
    pub(crate) fn generation_references(&self) -> VmNativeGenerationReferenceSnapshot {
        let processes = self.actors.processes().snapshots();
        let live = processes
            .iter()
            .filter(|process| {
                !matches!(
                    process.state,
                    crate::runtime::vm::process::VmProcessState::Exited(_)
                )
            })
            .collect::<Vec<_>>();
        let parked_continuations = self
            .execution
            .pending_continuation_count()
            .max(self.actors.pending_native_continuation_count());
        let mut snapshot = VmNativeGenerationReferenceSnapshot::new();
        snapshot.record(VmNativeGenerationReferenceClass::NativeFrame, live.len());
        snapshot.record(
            VmNativeGenerationReferenceClass::ParkedContinuation,
            parked_continuations,
        );
        snapshot.record(
            VmNativeGenerationReferenceClass::ActorTransfer,
            self.generation_transfers.count(),
        );
        snapshot.record(
            VmNativeGenerationReferenceClass::ActorHeap,
            self.execution.managed_ref().actor_count(),
        );
        snapshot.record(
            VmNativeGenerationReferenceClass::MailboxFragment,
            live.iter().map(|process| process.mailbox_messages).sum(),
        );
        snapshot.record(
            VmNativeGenerationReferenceClass::Timer,
            self.actors.timer_snapshots().len(),
        );
        snapshot.record(
            VmNativeGenerationReferenceClass::Resource,
            self.actors.resource_snapshots().len(),
        );
        snapshot.record(VmNativeGenerationReferenceClass::AsyncCapabilityCallback, 0);
        snapshot.record(VmNativeGenerationReferenceClass::Debugger, 0);
        snapshot.record(VmNativeGenerationReferenceClass::CrashMetadata, 0);
        for (class, count) in &self.generation_pins {
            snapshot.add(*class, *count);
        }
        snapshot
    }
}
